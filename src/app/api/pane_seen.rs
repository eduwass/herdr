use super::responses::{encode_error, encode_success};
use crate::api::schema::{PaneMarkSeenParams, ResponseResult};
use crate::app::App;

impl App {
    pub(super) fn handle_pane_mark_seen(
        &mut self,
        id: String,
        params: PaneMarkSeenParams,
    ) -> String {
        let Some((ws_idx, pane_id)) = self.parse_pane_id(&params.pane_id) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some(pane) = self.state.workspaces[ws_idx].pane_state(pane_id) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some(terminal) = self.state.terminals.get(&pane.attached_terminal_id) else {
            return encode_error(id, "pane_not_found", "terminal not found");
        };
        if pane.attached_terminal_id.as_str() != params.terminal_id
            || terminal.last_agent_state_change_seq.unwrap_or(0) != params.state_change_seq
        {
            return encode_error(
                id,
                "stale_agent_state",
                "refresh the agent state before acknowledging it",
            );
        }
        let changed = !pane.seen;
        if let Some(pane) = self.state.workspaces[ws_idx].pane_state_mut(pane_id) {
            pane.seen = true;
        }
        if changed {
            self.state.mark_session_dirty();
            self.emit_pane_updated(ws_idx, pane_id);
        }
        encode_success(id, ResponseResult::Ok {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::schema::{Method, Request};

    #[test]
    fn mark_seen_is_scoped_versioned_and_does_not_focus() {
        let (_, rx) = tokio::sync::mpsc::unbounded_channel();
        let hub = crate::api::EventHub::default();
        let mut app = App::new(
            &crate::config::Config::default(),
            crate::app::AppPolicy::TEST,
            None,
            rx,
            hub.clone(),
        );
        let mut workspace = crate::workspace::Workspace::test_new("seen");
        let first = workspace.tabs[0].root_pane;
        let second = workspace.test_split(ratatui::layout::Direction::Horizontal);
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        app.state.active = None;
        let focused = app.state.workspaces[0].focused_pane_id();
        for pane in [first, second] {
            app.state.workspaces[0].pane_state_mut(pane).unwrap().seen = false;
        }
        let terminal_id = app.state.workspaces[0].terminal_id(first).unwrap().clone();
        app.state
            .terminals
            .get_mut(&terminal_id)
            .unwrap()
            .last_agent_state_change_seq = Some(7);
        let params = PaneMarkSeenParams {
            pane_id: app.public_pane_id(0, first).unwrap(),
            terminal_id: terminal_id.as_str().to_owned(),
            state_change_seq: 7,
        };
        let invoke = |app: &mut App, params| -> serde_json::Value {
            let request = Request {
                id: "seen".into(),
                method: Method::PaneMarkSeen(params),
            };
            let encoded = serde_json::to_string(&request).unwrap();
            assert!(encoded.contains("pane.mark_seen"));
            let request: Request = serde_json::from_str(&encoded).unwrap();
            assert!(crate::api::request_changes_ui(&request));
            assert_eq!(
                crate::api::api_method_name(&request.method),
                "pane.mark_seen"
            );
            serde_json::from_str(&app.handle_api_request(request)).unwrap()
        };
        let mut missing = params.clone();
        missing.pane_id = "missing".into();
        assert_eq!(invoke(&mut app, missing)["error"]["code"], "pane_not_found");
        let mut stale = params.clone();
        stale.state_change_seq = 6;
        assert_eq!(
            invoke(&mut app, stale)["error"]["code"],
            "stale_agent_state"
        );
        let mut replaced = params.clone();
        replaced.terminal_id = "old-terminal".into();
        assert_eq!(
            invoke(&mut app, replaced)["error"]["code"],
            "stale_agent_state"
        );
        assert!(!app.state.workspaces[0].pane_state(first).unwrap().seen);
        assert_eq!(invoke(&mut app, params.clone())["result"]["type"], "ok");
        assert!(app.state.workspaces[0].pane_state(first).unwrap().seen);
        assert!(!app.state.workspaces[0].pane_state(second).unwrap().seen);
        assert_eq!(app.state.active, None);
        assert_eq!(app.state.workspaces[0].focused_pane_id(), focused);
        let events = hub.events_after(0).len();
        assert!(events > 0);
        assert_eq!(invoke(&mut app, params.clone())["result"]["type"], "ok");
        assert_eq!(hub.events_after(0).len(), events);
        // A later completion remains unread even if an old acknowledgement is retried.
        app.state
            .terminals
            .get_mut(&terminal_id)
            .unwrap()
            .last_agent_state_change_seq = Some(8);
        app.state.workspaces[0].pane_state_mut(first).unwrap().seen = false;
        assert_eq!(
            invoke(&mut app, params)["error"]["code"],
            "stale_agent_state"
        );
        assert!(!app.state.workspaces[0].pane_state(first).unwrap().seen);
    }
}
