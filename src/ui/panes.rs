use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use super::scrollbar::{render_pane_scrollbar, should_show_scrollbar};
use super::widgets::panel_contrast_fg;
use crate::app::state::Palette;
use crate::app::{AppState, Mode};
use crate::layout::PaneInfo;
use crate::terminal::{TerminalRuntime, TerminalRuntimeRegistry};

pub(crate) fn pane_is_scrolled_back(rt: &TerminalRuntime) -> bool {
    rt.scroll_metrics()
        .is_some_and(|metrics| metrics.offset_from_bottom > 0)
}

fn truncate_label(text: &str, max_width: usize) -> String {
    let len = text.chars().count();
    if len <= max_width {
        return text.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_string();
    }
    let prefix: String = text.chars().take(max_width.saturating_sub(1)).collect();
    format!("{prefix}…")
}

fn pane_border_title(label: &str, pane_width: u16) -> Option<String> {
    let label = label.trim();
    if label.is_empty() || pane_width <= 4 {
        return None;
    }
    let max_label_width = pane_width.saturating_sub(4) as usize;
    Some(format!(" {} ", truncate_label(label, max_label_width)))
}

fn stable_terminal_inner_rect(pane_inner: Rect) -> Rect {
    if pane_inner.width <= 4 {
        return pane_inner;
    }

    Rect::new(
        pane_inner.x,
        pane_inner.y,
        pane_inner.width.saturating_sub(1),
        pane_inner.height,
    )
}

fn pane_inner_rect(area: Rect, framed: bool) -> Rect {
    if framed {
        Block::default().borders(Borders::ALL).inner(area)
    } else {
        area
    }
}

fn runtime_for_tab_pane<'a>(
    terminal_runtimes: &'a TerminalRuntimeRegistry,
    tab: &'a crate::workspace::Tab,
    pane_id: crate::layout::PaneId,
) -> Option<(&'a crate::terminal::TerminalId, &'a TerminalRuntime)> {
    let terminal_id = tab.terminal_id(pane_id)?;
    #[cfg(test)]
    if let Some(runtime) = tab.runtimes.get(&pane_id) {
        return Some((terminal_id, runtime));
    }
    terminal_runtimes
        .get(terminal_id)
        .map(|runtime| (terminal_id, runtime))
}

fn popup_inner_rect(
    tab: &crate::workspace::Tab,
    popup_area: Rect,
    relative_area: Rect,
) -> Option<(crate::layout::PaneId, Rect)> {
    let popup = tab.popup.as_ref()?;
    let area = popup_area_for_spec(&popup.spec, popup_area, relative_area);
    let (_, inner, _) = popup.spec.rects(area)?;
    Some((popup.pane_id, inner))
}

fn popup_area_for_spec(
    spec: &crate::workspace::ResolvedPopupSpec,
    popup_area: Rect,
    relative_area: Rect,
) -> Rect {
    let popup_area = if popup_area.width > 0 && popup_area.height > 0 {
        popup_area
    } else {
        relative_area
    };
    match spec.effective(popup_area).position {
        crate::api::schema::PopupPosition::TotalCenter => popup_area,
        crate::api::schema::PopupPosition::RelativeCenter => relative_area,
    }
}

fn resize_tab_popup_pane(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    tab: &crate::workspace::Tab,
    popup_area: Rect,
    relative_area: Rect,
    cell_size: crate::kitty_graphics::HostCellSize,
) {
    let Some((pane_id, inner_rect)) = popup_inner_rect(tab, popup_area, relative_area) else {
        return;
    };
    let Some((terminal_id, rt)) = runtime_for_tab_pane(terminal_runtimes, tab, pane_id) else {
        return;
    };
    if app.direct_attach_resize_locks.contains(terminal_id) {
        return;
    }
    rt.resize(
        inner_rect.height,
        inner_rect.width,
        cell_size.width_px,
        cell_size.height_px,
    );
}

fn stable_scrollbar_gutter(rt: &TerminalRuntime, pane_inner: Rect) -> (Rect, Option<Rect>) {
    let inner_rect = stable_terminal_inner_rect(pane_inner);
    if inner_rect == pane_inner {
        return (inner_rect, None);
    }
    let gutter = Rect::new(
        pane_inner.x + pane_inner.width.saturating_sub(1),
        pane_inner.y,
        1,
        pane_inner.height,
    );
    let scrollbar_rect = rt
        .scroll_metrics()
        .filter(|metrics| should_show_scrollbar(*metrics))
        .map(|_| gutter);

    (inner_rect, scrollbar_rect)
}

/// Resize every visible runtime in a tab to the geometry it would receive if the tab were selected.
pub(super) fn resize_tab_panes(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    tab: &crate::workspace::Tab,
    area: Rect,
    popup_area: Rect,
    cell_size: crate::kitty_graphics::HostCellSize,
) {
    let multi_pane = tab.layout.pane_count() > 1;
    resize_tab_popup_pane(app, terminal_runtimes, tab, popup_area, area, cell_size);

    if tab.zoomed {
        let focused_id = tab.layout.focused();
        if let Some((terminal_id, rt)) = runtime_for_tab_pane(terminal_runtimes, tab, focused_id) {
            let pane_inner = pane_inner_rect(area, multi_pane);
            let inner_rect = stable_terminal_inner_rect(pane_inner);
            if !app.direct_attach_resize_locks.contains(terminal_id) {
                rt.resize(
                    inner_rect.height,
                    inner_rect.width,
                    cell_size.width_px,
                    cell_size.height_px,
                );
            }
        }
        return;
    }

    for info in tab.layout.panes(area) {
        let pane_inner = if multi_pane {
            Block::default().borders(Borders::ALL).inner(info.rect)
        } else {
            area
        };

        if let Some((terminal_id, rt)) = runtime_for_tab_pane(terminal_runtimes, tab, info.id) {
            let inner_rect = stable_terminal_inner_rect(pane_inner);
            if !app.direct_attach_resize_locks.contains(terminal_id) {
                rt.resize(
                    inner_rect.height,
                    inner_rect.width,
                    cell_size.width_px,
                    cell_size.height_px,
                );
            }
        }
    }
}

/// Compute pane layout info and optionally resize pane runtimes to match.
pub(super) fn compute_pane_infos(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    area: Rect,
    popup_area: Rect,
    resize_panes: bool,
    cell_size: crate::kitty_graphics::HostCellSize,
) -> Vec<PaneInfo> {
    let Some(ws_idx) = app.active else {
        return Vec::new();
    };
    let Some(ws) = app.workspaces.get(ws_idx) else {
        return Vec::new();
    };

    let multi_pane = ws.layout.pane_count() > 1;
    let terminal_active = app.mode == Mode::Terminal;

    if resize_panes {
        if let Some(tab) = ws.active_tab() {
            resize_tab_popup_pane(app, terminal_runtimes, tab, popup_area, area, cell_size);
        }
    }

    if ws.zoomed {
        let focused_id = ws.layout.focused();
        let pane_inner = pane_inner_rect(area, multi_pane);
        let mut inner_rect = pane_inner;
        let mut scrollbar_rect = None;
        if let Some(rt) = app.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, focused_id) {
            (inner_rect, scrollbar_rect) = stable_scrollbar_gutter(rt, pane_inner);
            if resize_panes
                && ws.terminal_id(focused_id).is_some_and(|terminal_id| {
                    !app.direct_attach_resize_locks.contains(terminal_id)
                })
            {
                rt.resize(
                    inner_rect.height,
                    inner_rect.width,
                    cell_size.width_px,
                    cell_size.height_px,
                );
            }
        }
        return vec![PaneInfo {
            id: focused_id,
            rect: area,
            inner_rect,
            scrollbar_rect,
            is_focused: true,
        }];
    }

    let mut pane_infos = ws.layout.panes(area);

    for info in &mut pane_infos {
        let pane_inner = if multi_pane {
            let border_set = if info.is_focused && terminal_active {
                ratatui::symbols::border::THICK
            } else {
                ratatui::symbols::border::PLAIN
            };
            let block = Block::default()
                .borders(Borders::ALL)
                .border_set(border_set);
            block.inner(info.rect)
        } else {
            area
        };

        let mut inner_rect = pane_inner;
        let mut scrollbar_rect = None;
        if let Some(rt) = app.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, info.id) {
            (inner_rect, scrollbar_rect) = stable_scrollbar_gutter(rt, pane_inner);
            if resize_panes
                && ws.terminal_id(info.id).is_some_and(|terminal_id| {
                    !app.direct_attach_resize_locks.contains(terminal_id)
                })
            {
                rt.resize(
                    inner_rect.height,
                    inner_rect.width,
                    cell_size.width_px,
                    cell_size.height_px,
                );
            }
        }

        info.inner_rect = inner_rect;
        info.scrollbar_rect = scrollbar_rect;
    }

    if let Some(tab) = ws.active_tab() {
        if let Some(popup) = &tab.popup {
            let area = popup_area_for_spec(&popup.spec, popup_area, area);
            if let Some((outer, inner, _)) = popup.spec.rects(area) {
                let mut inner_rect = inner;
                let mut scrollbar_rect = None;
                if let Some(rt) =
                    app.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, popup.pane_id)
                {
                    (inner_rect, scrollbar_rect) = stable_scrollbar_gutter(rt, inner);
                }
                pane_infos.push(PaneInfo {
                    id: popup.pane_id,
                    rect: outer,
                    inner_rect,
                    scrollbar_rect,
                    is_focused: tab.popup_focused,
                });
            }
        }
    }

    pane_infos
}

pub(super) fn render_panes(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
    popup_area: Rect,
) {
    let Some(ws_idx) = app.active else {
        render_empty(app, frame, area);
        return;
    };
    let Some(ws) = app.workspaces.get(ws_idx) else {
        render_empty(app, frame, area);
        return;
    };

    let multi_pane = ws.layout.pane_count() > 1;
    // A focused popup owns the cursor; suppress the tiled pane cursor meanwhile.
    let popup_has_focus = ws.active_popup_focused();
    let terminal_active = app.mode == Mode::Terminal && !popup_has_focus;

    for info in &app.view.pane_infos {
        if let Some(rt) = app.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, info.id) {
            if multi_pane {
                let (border_style, border_set) = if info.is_focused && terminal_active {
                    (
                        Style::default().fg(app.palette.accent),
                        ratatui::symbols::border::THICK,
                    )
                } else if info.is_focused {
                    (
                        Style::default().fg(app.palette.accent),
                        ratatui::symbols::border::PLAIN,
                    )
                } else {
                    (
                        Style::default().fg(app.palette.overlay0),
                        ratatui::symbols::border::PLAIN,
                    )
                };

                let mut block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(border_style)
                    .border_set(border_set);
                if let Some(title) = ws
                    .pane_state(info.id)
                    .and_then(|pane| app.terminals.get(&pane.attached_terminal_id))
                    .and_then(|terminal| {
                        terminal.border_label(app.show_agent_labels_on_pane_borders)
                    })
                    .and_then(|label| pane_border_title(&label, info.rect.width))
                {
                    block = block.title(Line::from(Span::styled(title, border_style)));
                }
                frame.render_widget(block, info.rect);
            }

            let show_cursor = info.is_focused
                && terminal_active
                && !pane_is_scrolled_back(rt)
                && app.pane_exposes_host_cursor(ws_idx, info.id);
            rt.render(frame, info.inner_rect, show_cursor);
            render_pane_scrollbar(app, frame, info, rt);

            let should_dim = !info.is_focused && multi_pane && !terminal_active;
            if should_dim {
                let inner = info.inner_rect;
                let buf = frame.buffer_mut();
                for y in inner.y..inner.y + inner.height {
                    for x in inner.x..inner.x + inner.width {
                        let cell = &mut buf[(x, y)];
                        cell.set_style(cell.style().add_modifier(Modifier::DIM));
                    }
                }
            }

            render_selection_highlight(
                &app.selection,
                frame,
                info.id,
                info.inner_rect,
                rt.scroll_metrics(),
                &app.palette,
                app.host_terminal_theme,
            );
            render_copy_mode_cursor(app, frame, info);
        }
    }

    render_popup(app, terminal_runtimes, frame, popup_area, area, ws_idx, ws);
}

fn render_popup(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    popup_area: Rect,
    relative_area: Rect,
    ws_idx: usize,
    ws: &crate::workspace::Workspace,
) {
    let Some(tab) = ws.active_tab() else {
        return;
    };
    let Some(popup) = &tab.popup else {
        return;
    };
    let spec = &popup.spec;
    let area = popup_area_for_spec(spec, popup_area, relative_area);
    let Some((outer, inner, effective)) = spec.rects(area) else {
        return;
    };

    // Clear the region (and the border/padding band) so tiled content doesn't
    // bleed through, then paint the popup background.
    frame.render_widget(Clear, outer);
    if effective.bg != ratatui::style::Color::Reset {
        frame.render_widget(
            Block::default().style(Style::default().bg(effective.bg)),
            outer,
        );
    }

    if effective.border {
        let border_set = match effective.border_style {
            crate::api::schema::PopupBorderStyle::Single => ratatui::symbols::border::PLAIN,
            crate::api::schema::PopupBorderStyle::Double => ratatui::symbols::border::DOUBLE,
            crate::api::schema::PopupBorderStyle::Rounded => ratatui::symbols::border::ROUNDED,
            crate::api::schema::PopupBorderStyle::Thick => ratatui::symbols::border::THICK,
        };
        let mut block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(effective.border_color))
            .border_set(border_set)
            .style(Style::default().bg(effective.bg));
        if let Some(title) = effective
            .title
            .as_deref()
            .and_then(|label| pane_border_title(label, outer.width))
        {
            block = block.title(Line::from(Span::styled(
                title,
                Style::default().fg(effective.border_color),
            )));
        }
        frame.render_widget(block, outer);
    }

    if let Some(rt) = app.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, popup.pane_id) {
        let show_cursor =
            tab.popup_focused && app.mode == Mode::Terminal && !pane_is_scrolled_back(rt);
        rt.render(frame, inner, show_cursor);
    }
}

fn render_copy_mode_cursor(app: &AppState, frame: &mut Frame, info: &PaneInfo) {
    if app.mode != Mode::Copy {
        return;
    }
    let Some(copy_mode) = app.copy_mode else {
        return;
    };
    if copy_mode.pane_id != info.id
        || copy_mode.cursor_row >= info.inner_rect.height
        || copy_mode.cursor_col >= info.inner_rect.width
    {
        return;
    }

    let x = info.inner_rect.x + copy_mode.cursor_col;
    let y = info.inner_rect.y + copy_mode.cursor_row;
    let cell = &mut frame.buffer_mut()[(x, y)];
    cell.set_style(
        Style::default()
            .fg(panel_contrast_fg(&app.palette))
            .bg(app.palette.accent)
            .add_modifier(Modifier::BOLD),
    );
}

fn render_selection_highlight(
    selection: &Option<crate::selection::Selection>,
    frame: &mut Frame,
    pane_id: crate::layout::PaneId,
    inner: Rect,
    scroll_metrics: Option<crate::pane::ScrollMetrics>,
    p: &Palette,
    host_theme: crate::terminal_theme::TerminalTheme,
) {
    if let Some(sel) = selection {
        if sel.is_visible() && sel.pane_id == pane_id {
            let buf = frame.buffer_mut();
            let style = automatic_selection_style(p, host_theme);
            for y in 0..inner.height {
                for x in 0..inner.width {
                    if sel.contains(y, x, scroll_metrics) {
                        let cell = &mut buf[(inner.x + x, inner.y + y)];
                        cell.set_style(style);
                    }
                }
            }
        }
    }
}

type Rgb = (u8, u8, u8);

fn automatic_selection_style(
    p: &Palette,
    host_theme: crate::terminal_theme::TerminalTheme,
) -> Style {
    let bg = automatic_selection_bg(p, host_theme);
    Style::reset().fg(selection_fg_for_bg(bg, p)).bg(bg)
}

fn automatic_selection_bg(p: &Palette, host_theme: crate::terminal_theme::TerminalTheme) -> Color {
    let Some(background) = host_theme.background.map(terminal_theme_to_rgb) else {
        return selection_palette_background(p);
    };

    let target = if relative_luminance(background) < 0.5 {
        (255, 255, 255)
    } else {
        (0, 0, 0)
    };
    let selected = mix_rgb(background, target, 0.28);
    Color::Rgb(selected.0, selected.1, selected.2)
}

fn selection_palette_background(p: &Palette) -> Color {
    if p.panel_bg == Color::Reset {
        p.surface_dim
    } else {
        p.panel_bg
    }
}

fn terminal_theme_to_rgb(color: crate::terminal_theme::RgbColor) -> Rgb {
    (color.r, color.g, color.b)
}

fn selection_fg_for_bg(bg: Color, p: &Palette) -> Color {
    color_to_rgb(bg)
        .map(|bg| {
            if relative_luminance(bg) < 0.5 {
                Color::White
            } else {
                Color::Black
            }
        })
        .unwrap_or_else(|| panel_contrast_fg(p))
}

fn mix_rgb(base: Rgb, target: Rgb, amount: f32) -> Rgb {
    fn channel(base: u8, target: u8, amount: f32) -> u8 {
        (f32::from(base) + (f32::from(target) - f32::from(base)) * amount).round() as u8
    }
    (
        channel(base.0, target.0, amount),
        channel(base.1, target.1, amount),
        channel(base.2, target.2, amount),
    )
}

fn relative_luminance(color: Rgb) -> f32 {
    fn channel(value: u8) -> f32 {
        let value = f32::from(value) / 255.0;
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(color.0) + 0.7152 * channel(color.1) + 0.0722 * channel(color.2)
}

fn color_to_rgb(color: Color) -> Option<Rgb> {
    match color {
        Color::Reset => None,
        Color::Black => Some((0, 0, 0)),
        Color::Red => Some((128, 0, 0)),
        Color::Green => Some((0, 128, 0)),
        Color::Yellow => Some((128, 128, 0)),
        Color::Blue => Some((0, 0, 128)),
        Color::Magenta => Some((128, 0, 128)),
        Color::Cyan => Some((0, 128, 128)),
        Color::Gray => Some((192, 192, 192)),
        Color::DarkGray => Some((128, 128, 128)),
        Color::LightRed => Some((255, 0, 0)),
        Color::LightGreen => Some((0, 255, 0)),
        Color::LightYellow => Some((255, 255, 0)),
        Color::LightBlue => Some((0, 0, 255)),
        Color::LightMagenta => Some((255, 0, 255)),
        Color::LightCyan => Some((0, 255, 255)),
        Color::White => Some((255, 255, 255)),
        Color::Rgb(r, g, b) => Some((r, g, b)),
        Color::Indexed(_) => None,
    }
}

fn render_empty(app: &AppState, frame: &mut Frame, area: Rect) {
    let p = &app.palette;
    let lines = vec![
        Line::from(""),
        Line::from(""),
        Line::from(Span::styled(
            "  No workspaces yet",
            Style::default().fg(p.overlay0),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  A workspace is one project context.",
            Style::default().fg(p.overlay1),
        )),
        Line::from(Span::styled(
            "  Its root pane (top-left) sets the default repo or folder name.",
            Style::default().fg(p.overlay1),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Press ", Style::default().fg(p.overlay0)),
            Span::styled(
                app.keybinds
                    .new_workspace
                    .label()
                    .unwrap_or_else(|| "unset".to_string()),
                Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" to create one", Style::default().fg(p.overlay0)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(p.surface_dim)),
        ),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::PaneId;
    use crate::selection::Selection;
    use crate::terminal::TerminalRuntime;
    use crate::workspace::Workspace;

    #[test]
    fn pane_border_title_trims_and_truncates() {
        assert_eq!(
            pane_border_title(" claude ", 20).as_deref(),
            Some(" claude ")
        );
        assert_eq!(pane_border_title("", 20), None);
        assert_eq!(pane_border_title("abcdef", 8).as_deref(), Some(" abc… "));
        assert_eq!(pane_border_title("abcdef", 4), None);
    }

    #[tokio::test]
    async fn pane_scrollbar_gutter_is_reserved_before_scrollback_exists() {
        let mut app = AppState::test_new();
        let mut workspace = Workspace::test_new("test");
        let root_pane = workspace.tabs[0].root_pane;
        workspace.tabs[0].runtimes.insert(
            root_pane,
            TerminalRuntime::test_with_scrollback_bytes(40, 8, 1024, b"ready\n"),
        );
        app.workspaces = vec![workspace];
        app.active = Some(0);

        let area = Rect::new(10, 3, 40, 8);
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let infos = compute_pane_infos(
            &app,
            &terminal_runtimes,
            area,
            area,
            false,
            crate::kitty_graphics::HostCellSize::default(),
        );
        let info = &infos[0];

        assert_eq!(info.rect, area);
        assert_eq!(info.scrollbar_rect, None);
        assert_eq!(info.inner_rect, Rect::new(10, 3, 39, 8));
    }

    #[tokio::test]
    async fn zoomed_pane_scrollbar_gutter_is_reserved_before_scrollback_exists() {
        let mut app = AppState::test_new();
        let mut workspace = Workspace::test_new("test");
        workspace.zoomed = true;
        let root_pane = workspace.tabs[0].root_pane;
        workspace.tabs[0].runtimes.insert(
            root_pane,
            TerminalRuntime::test_with_scrollback_bytes(40, 8, 1024, b"ready\n"),
        );
        app.workspaces = vec![workspace];
        app.active = Some(0);

        let area = Rect::new(10, 3, 40, 8);
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let infos = compute_pane_infos(
            &app,
            &terminal_runtimes,
            area,
            area,
            false,
            crate::kitty_graphics::HostCellSize::default(),
        );
        let info = &infos[0];

        assert_eq!(info.rect, area);
        assert_eq!(info.scrollbar_rect, None);
        assert_eq!(info.inner_rect, Rect::new(10, 3, 39, 8));
    }

    #[tokio::test]
    async fn zoomed_multi_pane_keeps_border_space() {
        let mut app = AppState::test_new();
        let mut workspace = Workspace::test_new("test");
        let focused_pane = workspace.test_split(ratatui::layout::Direction::Horizontal);
        workspace.zoomed = true;
        workspace.tabs[0].runtimes.insert(
            focused_pane,
            TerminalRuntime::test_with_scrollback_bytes(40, 8, 1024, b"ready\n"),
        );
        app.workspaces = vec![workspace];
        app.active = Some(0);

        let area = Rect::new(10, 3, 40, 8);
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let infos = compute_pane_infos(
            &app,
            &terminal_runtimes,
            area,
            area,
            false,
            crate::kitty_graphics::HostCellSize::default(),
        );
        let info = &infos[0];

        assert_eq!(info.id, focused_pane);
        assert_eq!(info.rect, area);
        assert_eq!(info.scrollbar_rect, None);
        assert_eq!(info.inner_rect, Rect::new(11, 4, 37, 6));
    }

    #[tokio::test]
    async fn popup_runtime_resizes_from_full_terminal_area_not_split_pane() {
        let mut app = AppState::test_new();
        let mut workspace = Workspace::test_new("test");
        workspace.test_split(ratatui::layout::Direction::Horizontal);
        let popup_pane = PaneId::alloc();
        let spec = crate::workspace::ResolvedPopupSpec::from_spec(
            &crate::api::schema::PopupSpec {
                width: Some(crate::api::schema::PopupDimension::Percent(80)),
                height: Some(crate::api::schema::PopupDimension::Percent(60)),
                border: Some(false),
                ..Default::default()
            },
            app.palette.accent,
            app.palette.panel_bg,
        );
        let (new_pane, replaced) = workspace.tabs[0]
            .spawn_popup_argv_command(
                popup_pane,
                2,
                2,
                None,
                &["sh".into(), "-c".into(), "sleep 60".into()],
                &crate::pane::PaneLaunchEnv::default(),
                app.pane_scrollback_limit_bytes,
                app.host_terminal_theme,
                spec,
            )
            .expect("popup should spawn");
        assert!(replaced.is_none());
        new_pane.runtime.shutdown();
        workspace.tabs[0].runtimes.insert(
            popup_pane,
            TerminalRuntime::test_with_screen_bytes(2, 2, b""),
        );
        app.workspaces = vec![workspace];
        app.active = Some(0);

        let area = Rect::new(26, 1, 74, 39);
        let popup_area = Rect::new(0, 0, 100, 40);
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let _ = compute_pane_infos(
            &app,
            &terminal_runtimes,
            area,
            popup_area,
            true,
            crate::kitty_graphics::HostCellSize::default(),
        );

        let popup_runtime = app.workspaces[0].tabs[0]
            .runtimes
            .get(&popup_pane)
            .expect("popup runtime");
        assert_eq!(popup_runtime.current_size(), (24, 80));
    }

    #[tokio::test]
    async fn relative_center_popup_runtime_resizes_from_terminal_area() {
        let mut app = AppState::test_new();
        let mut workspace = Workspace::test_new("test");
        let popup_pane = PaneId::alloc();
        let spec = crate::workspace::ResolvedPopupSpec::from_spec(
            &crate::api::schema::PopupSpec {
                width: Some(crate::api::schema::PopupDimension::Percent(80)),
                height: Some(crate::api::schema::PopupDimension::Percent(60)),
                position: Some(crate::api::schema::PopupPosition::RelativeCenter),
                border: Some(false),
                ..Default::default()
            },
            app.palette.accent,
            app.palette.panel_bg,
        );
        let (new_pane, replaced) = workspace.tabs[0]
            .spawn_popup_argv_command(
                popup_pane,
                2,
                2,
                None,
                &["sh".into(), "-c".into(), "sleep 60".into()],
                &crate::pane::PaneLaunchEnv::default(),
                app.pane_scrollback_limit_bytes,
                app.host_terminal_theme,
                spec,
            )
            .expect("popup should spawn");
        assert!(replaced.is_none());
        new_pane.runtime.shutdown();
        workspace.tabs[0].runtimes.insert(
            popup_pane,
            TerminalRuntime::test_with_screen_bytes(2, 2, b""),
        );
        app.workspaces = vec![workspace];
        app.active = Some(0);

        let area = Rect::new(26, 1, 74, 39);
        let popup_area = Rect::new(0, 0, 100, 40);
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let _ = compute_pane_infos(
            &app,
            &terminal_runtimes,
            area,
            popup_area,
            true,
            crate::kitty_graphics::HostCellSize::default(),
        );

        let popup_runtime = app.workspaces[0].tabs[0]
            .runtimes
            .get(&popup_pane)
            .expect("popup runtime");
        assert_eq!(popup_runtime.current_size(), (23, 59));
    }

    #[tokio::test]
    async fn tiny_pane_does_not_reserve_scrollbar_gutter() {
        let mut app = AppState::test_new();
        let mut workspace = Workspace::test_new("test");
        let root_pane = workspace.tabs[0].root_pane;
        workspace.tabs[0].runtimes.insert(
            root_pane,
            TerminalRuntime::test_with_scrollback_bytes(4, 8, 1024, b"ready\n"),
        );
        app.workspaces = vec![workspace];
        app.active = Some(0);

        let area = Rect::new(10, 3, 4, 8);
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let infos = compute_pane_infos(
            &app,
            &terminal_runtimes,
            area,
            area,
            false,
            crate::kitty_graphics::HostCellSize::default(),
        );
        let info = &infos[0];

        assert_eq!(info.rect, area);
        assert_eq!(info.scrollbar_rect, None);
        assert_eq!(info.inner_rect, area);
    }

    #[tokio::test]
    async fn pane_scrollbar_reserves_last_column_from_terminal_area() {
        let mut app = AppState::test_new();
        let mut workspace = Workspace::test_new("test");
        let root_pane = workspace.tabs[0].root_pane;
        workspace.tabs[0].runtimes.insert(
            root_pane,
            TerminalRuntime::test_with_scrollback_bytes(
                40,
                8,
                1024,
                b"one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n",
            ),
        );
        app.workspaces = vec![workspace];
        app.active = Some(0);

        let area = Rect::new(10, 3, 40, 8);
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let infos = compute_pane_infos(
            &app,
            &terminal_runtimes,
            area,
            area,
            false,
            crate::kitty_graphics::HostCellSize::default(),
        );
        let info = &infos[0];

        assert_eq!(info.rect, area);
        assert_eq!(info.scrollbar_rect, Some(Rect::new(49, 3, 1, 8)));
        assert_eq!(info.inner_rect, Rect::new(10, 3, 39, 8));
    }

    #[test]
    fn selection_highlight_uses_one_uniform_style() {
        let palette = Palette::catppuccin();
        let host_theme = crate::terminal_theme::TerminalTheme {
            foreground: None,
            background: Some(crate::terminal_theme::RgbColor {
                r: 12,
                g: 14,
                b: 16,
            }),
        };
        let expected_style = automatic_selection_style(&palette, host_theme);
        let selection = Some(Selection::range(PaneId::from_raw(1), 0, 0, 2, None));
        let backend = ratatui::backend::TestBackend::new(4, 1);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let buf = frame.buffer_mut();
                buf[(0, 0)].set_style(
                    Style::default()
                        .fg(Color::Rgb(10, 220, 120))
                        .bg(Color::Black),
                );
                buf[(1, 0)].set_style(
                    Style::default()
                        .fg(Color::Rgb(220, 180, 40))
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                );
                buf[(2, 0)].set_style(Style::default().fg(Color::Blue).bg(Color::Reset));
                render_selection_highlight(
                    &selection,
                    frame,
                    PaneId::from_raw(1),
                    Rect::new(0, 0, 4, 1),
                    None,
                    &palette,
                    host_theme,
                );
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let first = buffer[(0, 0)].style();
        let second = buffer[(1, 0)].style();
        let third = buffer[(2, 0)].style();

        assert_eq!(first.fg, expected_style.fg);
        assert_eq!(second.fg, expected_style.fg);
        assert_eq!(third.fg, expected_style.fg);
        assert_eq!(first.bg, expected_style.bg);
        assert_eq!(second.bg, expected_style.bg);
        assert_eq!(third.bg, expected_style.bg);
        assert_eq!(first.add_modifier, expected_style.add_modifier);
        assert_eq!(second.add_modifier, expected_style.add_modifier);
        assert_eq!(third.add_modifier, expected_style.add_modifier);
        assert!(!second.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn automatic_selection_background_uses_host_background() {
        let bg = automatic_selection_bg(
            &Palette::terminal(),
            crate::terminal_theme::TerminalTheme {
                foreground: Some(crate::terminal_theme::RgbColor {
                    r: 230,
                    g: 230,
                    b: 230,
                }),
                background: Some(crate::terminal_theme::RgbColor {
                    r: 12,
                    g: 14,
                    b: 16,
                }),
            },
        );

        let Color::Rgb(r, g, b) = bg else {
            panic!("selection background should resolve to rgb");
        };
        assert!(relative_luminance((r, g, b)) > relative_luminance((12, 14, 16)));
    }
}
