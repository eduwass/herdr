use super::*;

fn running_close_state() -> ClientShellState {
    let mut config = Config::default();
    config.ui.confirm_close_running = true;
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&config));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    state
}

fn request_running_close(state: &mut ClientShellState, tab: bool) -> String {
    let mut outcome = ClientShellInput::default();
    let method = if tab {
        crate::api::schema::Method::TabClose(crate::api::schema::TabTarget {
            tab_id: "tab_1".into(),
        })
    } else {
        crate::api::schema::Method::PaneClose(crate::api::schema::PaneTarget {
            pane_id: "pane_1".into(),
        })
    };
    state.push_endpoint_method(method, &mut outcome);
    let [ClientShellAction::Endpoint { request, .. }] = &outcome.actions[..] else {
        panic!("process probe")
    };
    assert!(matches!(
        request.method,
        crate::api::schema::Method::PaneProcessInfo(_)
    ));
    request.id.clone()
}

fn process_info(idle: bool) -> crate::api::schema::ResponseResult {
    crate::api::schema::ResponseResult::PaneProcessInfo {
        process_info: crate::api::schema::PaneProcessInfo {
            pane_id: "pane_1".into(),
            shell_pid: Some(100),
            foreground_process_group_id: Some(if idle { 100 } else { 200 }),
            tty: None,
            foreground_processes: vec![crate::api::schema::PaneProcessInfoProcess {
                pid: if idle { 100 } else { 200 },
                name: if idle { "zsh" } else { "python" }.into(),
                argv0: None,
                argv: None,
                cmdline: None,
                cwd: None,
            }],
        },
    }
}

#[test]
fn running_close_records_target_and_accepts_keyboard_and_mouse() {
    for tab in [false, true] {
        let mut state = running_close_state();
        let id = request_running_close(&mut state, tab);
        assert!(state
            .handle_endpoint_result("boot-1", &id, Ok(process_info(false)))
            .1
            .is_empty());
        assert!(matches!(
            state.overlay,
            Some(ClientShellOverlay::ConfirmClose(_))
        ));
        state.snapshot.as_mut().unwrap().focused_pane_id = Some("unrelated-pane".into());
        let outcome = if tab {
            state.compose(106, 20).unwrap();
            let hit = state.hits.overlay_primary;
            state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: hit.x,
                row: hit.y,
                modifiers: KeyModifiers::empty(),
            })])
        } else {
            state.handle_input_bytes(b"\r")
        };
        let [ClientShellAction::Endpoint { request, .. }] = &outcome.actions[..] else {
            panic!("close target")
        };
        if tab {
            assert!(
                matches!(&request.method, crate::api::schema::Method::TabClose(target) if target.tab_id == "tab_1")
            );
        } else {
            assert!(
                matches!(&request.method, crate::api::schema::Method::PaneClose(target) if target.pane_id == "pane_1")
            );
        }
    }
}

#[test]
fn running_close_confirms_an_exec_replaced_shell_and_rejects_unknown_identity() {
    for tab in [false, true] {
        for unknown in [false, true] {
            let mut state = running_close_state();
            let id = request_running_close(&mut state, tab);
            let mut result = process_info(false);
            let crate::api::schema::ResponseResult::PaneProcessInfo { process_info } = &mut result
            else {
                panic!("process info");
            };
            process_info.shell_pid = process_info.foreground_process_group_id;
            if unknown {
                process_info.foreground_processes.clear();
            }
            let (_, actions) = state.handle_endpoint_result("boot-1", &id, Ok(result));
            assert!(actions.is_empty());
            if unknown {
                assert!(state.endpoint_error.is_some());
            } else {
                assert!(matches!(
                    state.overlay,
                    Some(ClientShellOverlay::ConfirmClose(_))
                ));
            }
        }
    }
}

#[test]
fn running_close_is_idle_aware_and_fails_closed_on_stale_target_or_probe_error() {
    let mut state = running_close_state();
    let id = request_running_close(&mut state, false);
    let (_, actions) = state.handle_endpoint_result("boot-1", &id, Ok(process_info(true)));
    assert_eq!(actions.len(), 1);
    assert!(state.overlay.is_none());
    for stale in ["removed", "rebooted", "error", "cancel"] {
        let mut state = running_close_state();
        let id = request_running_close(&mut state, false);
        if stale == "error" {
            assert!(state
                .handle_endpoint_result(
                    "boot-1",
                    &id,
                    Err(ClientShellEndpointError {
                        code: Some("endpoint_timeout".into()),
                        message: "timeout".into(),
                    })
                )
                .1
                .is_empty());
        } else {
            state.handle_endpoint_result("boot-1", &id, Ok(process_info(false)));
            match stale {
                "removed" => state.snapshot.as_mut().unwrap().panes.clear(),
                "rebooted" => state.snapshot.as_mut().unwrap().boot_id = "boot-2".into(),
                _ => {
                    state.handle_input_bytes(b"\x1b");
                }
            }
        }
        assert!(!state.handle_input_bytes(b"\r").actions.iter().any(|action| matches!(
            action,
            ClientShellAction::Endpoint { request, .. }
                if matches!(request.method, crate::api::schema::Method::PaneClose(_) | crate::api::schema::Method::TabClose(_))
        )), "stale close dispatched: {stale}");
    }
}

#[test]
fn move_to_new_tab_menu_captures_the_original_pane() {
    let mut state = running_close_state();
    let mut other = state.snapshot.as_ref().unwrap().panes[0].clone();
    other.pane_id = "pane_2".into();
    state.snapshot.as_mut().unwrap().panes.push(other);
    state.open_pane_context_menu("pane_1".into(), 40, 4);
    let Some(ClientShellOverlay::ContextMenu(menu)) = state.overlay.as_ref() else {
        panic!("menu")
    };
    let index = menu
        .items()
        .iter()
        .position(|item| item.action == ClientContextMenuAction::MoveToNewTab)
        .unwrap();
    state.snapshot.as_mut().unwrap().focused_pane_id = Some("pane_2".into());
    let mut outcome = ClientShellInput::default();
    state.activate_context_menu_item(index, &mut outcome);
    let [ClientShellAction::Endpoint { request, .. }] = &outcome.actions[..] else {
        panic!("move")
    };
    assert!(
        matches!(&request.method, crate::api::schema::Method::PaneMove(params)
        if params.pane_id == "pane_1" && matches!(&params.destination, crate::api::schema::PaneMoveDestination::NewTab { workspace_id, .. } if workspace_id.as_deref() == Some("ws_1")))
    );
}

#[test]
fn double_right_click_zooms_after_opening_the_normal_pane_menu() {
    let mut state = running_close_state();
    state.config.pane_double_right_click_zoom = true;
    state.compose(106, 20).unwrap();
    let hit = state.hits.panes[0].inner_rect;
    let mouse = crossterm::event::MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: hit.x + 1,
        row: hit.y + 1,
        modifiers: KeyModifiers::empty(),
    };
    state.handle_raw_events(vec![RawInputEvent::Mouse(mouse)]);
    assert!(matches!(
        state.overlay,
        Some(ClientShellOverlay::ContextMenu(_))
    ));
    let outcome = state.handle_raw_events(vec![RawInputEvent::Mouse(mouse)]);
    let [ClientShellAction::Endpoint { request, .. }] = &outcome.actions[..] else {
        panic!("zoom")
    };
    assert!(
        matches!(&request.method, crate::api::schema::Method::PaneZoom(params) if params.pane_id.as_deref() == Some("pane_1"))
    );
    assert!(state.overlay.is_none());
}

#[test]
fn tab_overflow_controls_scroll_the_client_owned_tab_bar() {
    let mut snapshot = snapshot();
    snapshot.tabs.extend((2..=8).map(|number| ClientShellTab {
        tab_id: format!("tab_{number}"),
        workspace_id: "ws_1".into(),
        number,
        label: number.to_string(),
        custom_label: false,
        zoomed: false,
        focused: false,
        agent_status: AgentStatus::Idle,
    }));
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot));
    state.set_pane_surface(surface());
    state.compose(80, 20).expect("overflow tab bar");

    assert!(state.hits.tab_scroll_right.width > 0);
    let scroll_right = state.hits.tab_scroll_right;
    let outcome =
        state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: scroll_right.x + 1,
            row: scroll_right.y,
            modifiers: KeyModifiers::empty(),
        })]);
    assert!(outcome.repaint);
    assert_eq!(state.tab_scroll, 1);

    let mut update = state.snapshot.as_deref().expect("snapshot").clone();
    update.focused_tab_id = Some("tab_8".into());
    for tab in &mut update.tabs {
        tab.focused = tab.tab_id == "tab_8";
    }
    state.set_snapshot(Box::new(update));
    state.compose(80, 20).expect("focused overflow tab");
    assert!(state.hits.tabs.iter().any(|(_, tab_id)| tab_id == "tab_8"));

    state.compose(300, 20).expect("tabs without overflow");
    assert_eq!(state.tab_scroll, 0);
    assert_eq!(state.hits.tabs.len(), 8);
    state.compose(80, 20).expect("focused tab after narrowing");
    assert!(state.hits.tabs.iter().any(|(_, tab_id)| tab_id == "tab_8"));
}

#[test]
fn focused_workspace_change_reveals_new_workspace_in_full_sidebar() {
    let mut initial = snapshot();
    let template = initial.workspaces[0].clone();
    initial.workspaces = (1..=12)
        .map(|number| ClientShellWorkspace {
            workspace_id: format!("ws_{number}"),
            number,
            label: format!("space-{number}"),
            branch: None,
            focused: number == 1,
            ..template.clone()
        })
        .collect();

    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(initial));
    state.set_pane_surface(surface());
    state.compose(106, 20).expect("full sidebar");
    assert!(state.hits.workspace_max_scroll > 0);
    assert!(state
        .hits
        .workspaces
        .iter()
        .all(|hit| hit.workspace_id != "ws_12"));

    let mut update = state.snapshot.as_deref().expect("snapshot").clone();
    update.revision = 2;
    update.focused_workspace_id = Some("ws_12".into());
    for workspace in &mut update.workspaces {
        workspace.focused = workspace.workspace_id == "ws_12";
    }
    let mut updated_surface = surface();
    updated_surface.projection_revision = 2;
    state.set_snapshot(Box::new(update));
    state.set_pane_surface(updated_surface);
    state.compose(106, 2).expect("zero-height workspace body");
    assert!(state.reveal_focused_workspace);
    state.compose(106, 20).expect("updated full sidebar");

    assert!(state
        .hits
        .workspaces
        .iter()
        .any(|hit| hit.workspace_id == "ws_12"));
}

#[test]
fn client_owned_sidebar_dividers_resize_live() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    state.compose(106, 30).expect("expanded sidebar");
    let workspace_body = state.hits.workspace_body;
    let needless_scroll =
        state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: workspace_body.x,
            row: workspace_body.y,
            modifiers: KeyModifiers::empty(),
        })]);
    assert_eq!(state.hits.workspace_max_scroll, 0);
    assert_eq!(state.workspace_scroll, 0);
    assert!(!needless_scroll.repaint);
    let width_divider = state.hits.sidebar_divider;
    state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: width_divider.x,
        row: width_divider.y + 2,
        modifiers: KeyModifiers::empty(),
    })]);
    let resize =
        state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: 31,
            row: width_divider.y + 2,
            modifiers: KeyModifiers::empty(),
        })]);
    assert_eq!(state.sidebar_width, 32);
    assert!(state.sidebar_width_manual);
    assert!(resize.repaint);
    assert!(resize.resize);
    state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 31,
        row: width_divider.y + 2,
        modifiers: KeyModifiers::empty(),
    })]);

    state.set_pane_surface(surface());
    state.compose(106, 30).expect("resized sidebar");
    let section_divider = state.hits.sidebar_section_divider;
    state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: section_divider.x + 2,
        row: section_divider.y,
        modifiers: KeyModifiers::empty(),
    })]);
    let split = state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: section_divider.x + 2,
        row: 20,
        modifiers: KeyModifiers::empty(),
    })]);
    assert!(state.sidebar_section_split > 0.6);
    assert!(split.repaint);
    assert!(!split.resize);
}

#[test]
fn context_menus_capture_stable_targets_and_route_actions() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    state.compose(106, 20).expect("composed frame");

    let workspace = state.hits.workspaces[0].rect;
    let open_workspace_menu =
        state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column: workspace.x + 2,
            row: workspace.y,
            modifiers: KeyModifiers::empty(),
        })]);
    assert!(open_workspace_menu.actions.is_empty());
    assert!(matches!(
        state.overlay,
        Some(ClientShellOverlay::ContextMenu(ClientContextMenuOverlay {
            target: ClientContextMenuTarget::Workspace { ref workspace_id, .. },
            ..
        })) if workspace_id == "ws_1"
    ));
    let workspace_items = match state.overlay.as_ref() {
        Some(ClientShellOverlay::ContextMenu(menu)) => menu.items(),
        _ => panic!("workspace context menu"),
    };
    assert!(workspace_items
        .iter()
        .any(|item| item.action == ClientContextMenuAction::NewWorktree));
    state.compose(106, 20).expect("workspace context menu");
    let rename = state.hits.context_menu_rows[0].0;
    state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rename.x + 1,
        row: rename.y,
        modifiers: KeyModifiers::empty(),
    })]);
    assert!(matches!(
        state.overlay,
        Some(ClientShellOverlay::Rename(ClientRenameOverlay {
            target: ClientRenameTarget::Workspace { ref workspace_id },
            ..
        })) if workspace_id == "ws_1"
    ));

    state.overlay = None;
    state.compose(106, 20).expect("composed frame");
    let pane = state.hits.panes[0].rect;
    state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: pane.x + 1,
        row: pane.y,
        modifiers: KeyModifiers::empty(),
    })]);
    state.compose(106, 20).expect("pane context menu");
    let split_index = match state.overlay.as_ref() {
        Some(ClientShellOverlay::ContextMenu(menu)) => menu
            .items()
            .iter()
            .position(|item| item.action == ClientContextMenuAction::SplitRight)
            .expect("split right item"),
        _ => panic!("pane context menu"),
    };
    let split = state.hits.context_menu_rows[split_index].0;
    let outcome =
        state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: split.x + 1,
            row: split.y,
            modifiers: KeyModifiers::empty(),
        })]);
    let [ClientShellAction::Endpoint { request, .. }] = &outcome.actions[..] else {
        panic!("pane split context action should use endpoint API");
    };
    assert!(matches!(
        &request.method,
        crate::api::schema::Method::PaneSplit(params)
            if params.target_pane_id.as_deref() == Some("pane_1")
                && params.direction == crate::api::schema::SplitDirection::Right
    ));
}

#[test]
fn global_menu_opens_from_sidebar_and_routes_client_actions() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    state.compose(106, 30).expect("shell frame");
    let launcher = state.hits.global_launcher;
    assert_ne!(launcher, Rect::default());

    let open = state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: launcher.x,
        row: launcher.y,
        modifiers: KeyModifiers::empty(),
    })]);
    assert!(open.repaint);
    let menu = state.compose(106, 30).expect("global menu");
    let text = menu
        .cells
        .chunks(menu.width as usize)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("settings"));
    assert!(text.contains("keybinds"));
    assert!(text.contains("reload config"));
    assert!(text.contains("detach"));

    let keybinds = state.hits.global_menu_rows[1].0;
    let help = state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: keybinds.x,
        row: keybinds.y,
        modifiers: KeyModifiers::empty(),
    })]);
    assert!(help.actions.is_empty());
    assert!(matches!(state.overlay, Some(ClientShellOverlay::Help(_))));

    state.overlay = Some(ClientShellOverlay::GlobalMenu(ClientGlobalMenuOverlay {
        highlighted: 3,
    }));
    let detach = state.handle_input_bytes(b"\r");
    assert!(detach.detach);
    assert!(state.overlay.is_none());
}

#[test]
fn new_tab_overlay_owns_text_cursor_and_submits_public_api_request() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    let mut open = ClientShellInput::default();
    state.record_binding(
        crate::input::KeybindMatch::Action(crate::input::KeybindAction::NewTab),
        &mut open,
    );
    assert!(open.actions.is_empty());
    let frame = state.compose(106, 20).expect("new tab overlay");
    let text = frame
        .cells
        .chunks(frame.width as usize)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("new tab"));
    assert!(text.contains("save"));
    let restored = frame.to_ratatui_buffer().expect("overlay frame");
    assert!(!restored
        .cell((26, 7))
        .expect("overlay title cell")
        .modifier
        .contains(Modifier::DIM));
    assert!(frame.cursor.as_ref().is_some_and(|cursor| cursor.visible));

    assert!(state.handle_input_bytes(b"logs").actions.is_empty());
    let create = state.handle_input_bytes(b"\r");
    let [ClientShellAction::Endpoint { request, .. }] = &create.actions[..] else {
        panic!("new tab save should use endpoint API");
    };
    assert!(matches!(
        &request.method,
        crate::api::schema::Method::TabCreate(params)
            if params.workspace_id.as_deref() == Some("ws_1")
                && params.label.as_deref() == Some("logs")
    ));
    assert!(state.overlay.is_none());
}

#[test]
fn close_confirmation_error_becomes_client_owned_overlay_and_stable_group_close() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    let mut close = ClientShellInput::default();
    state.record_binding(
        crate::input::KeybindMatch::Action(crate::input::KeybindAction::ClosePane),
        &mut close,
    );
    let [ClientShellAction::Endpoint { request, .. }] = &close.actions[..] else {
        panic!("pane close should use endpoint API");
    };
    let request_id = request.id.clone();
    assert!(
        state
            .handle_endpoint_result(
                "boot-1",
                &request_id,
                Err(ClientShellEndpointError {
                    code: Some("confirmation_required".into()),
                    message: "confirmation required".into(),
                }),
            )
            .0
    );
    let frame = state.compose(106, 20).expect("confirmation overlay");
    let text = frame
        .cells
        .chunks(frame.width as usize)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Close workspace?"));
    assert!(text.contains("1 pane"));

    let confirm = state.handle_input_bytes(b"\r");
    let [ClientShellAction::Endpoint { request, .. }] = &confirm.actions[..] else {
        panic!("confirmation should use endpoint API");
    };
    assert!(matches!(
        &request.method,
        crate::api::schema::Method::WorkspaceClose(params)
            if params.workspace_id == "ws_1" && params.close_group
    ));
}
