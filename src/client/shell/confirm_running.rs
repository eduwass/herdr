use super::*;
use crate::api::schema::{Method, PaneProcessInfoParams, PaneTarget, ResponseResult, TabTarget};

#[derive(Debug)]
pub(super) struct ClientPaneClose {
    endpoint_id: ClientEndpointId,
    boot_id: String,
    pane_id: String,
    tab_id: Option<String>,
}

impl ClientShellState {
    pub(super) fn prepare_running_close(
        &mut self,
        method: &Method,
        outcome: &mut ClientShellInput,
    ) -> bool {
        if !self.config.confirm_close_running {
            return false;
        }
        let Some(snapshot) = self.snapshot.as_deref() else {
            return false;
        };
        let (pane_id, tab_id) = match method {
            Method::PaneClose(target) => (target.pane_id.clone(), None),
            Method::TabClose(target) => {
                let mut panes = snapshot
                    .panes
                    .iter()
                    .filter(|pane| pane.tab_id == target.tab_id);
                let Some(pane) = panes.next() else {
                    return true;
                };
                if panes.next().is_some() {
                    return false;
                }
                (pane.pane_id.clone(), Some(target.tab_id.clone()))
            }
            _ => return false,
        };
        let close = ClientPaneClose {
            endpoint_id: self.active_endpoint_id.clone(),
            boot_id: snapshot.boot_id.clone(),
            pane_id: pane_id.clone(),
            tab_id,
        };
        self.push_endpoint_method_with_kind(
            Method::PaneProcessInfo(PaneProcessInfoParams {
                pane_id: Some(pane_id),
            }),
            PendingEndpointKind::CloseProcessInfo(Box::new(close)),
            outcome,
        );
        true
    }

    fn running_close_target_exists(&self, close: &ClientPaneClose) -> bool {
        self.active_endpoint_id == close.endpoint_id
            && self.snapshot.as_deref().is_some_and(|snapshot| {
                snapshot.boot_id == close.boot_id
                    && snapshot
                        .panes
                        .iter()
                        .any(|pane| pane.pane_id == close.pane_id)
                    && close.tab_id.as_ref().is_none_or(|tab_id| {
                        let mut panes = snapshot.panes.iter().filter(|pane| &pane.tab_id == tab_id);
                        panes
                            .next()
                            .is_some_and(|pane| pane.pane_id == close.pane_id)
                            && panes.next().is_none()
                    })
            })
    }

    fn dispatch_running_close(&mut self, close: ClientPaneClose, outcome: &mut ClientShellInput) {
        if !self.running_close_target_exists(&close) {
            return;
        }
        let method = match close.tab_id {
            Some(tab_id) => Method::TabClose(TabTarget { tab_id }),
            None => Method::PaneClose(PaneTarget {
                pane_id: close.pane_id,
            }),
        };
        self.push_endpoint_method_with_kind(method, PendingEndpointKind::Generic, outcome);
    }

    pub(super) fn complete_running_close_probe(
        &mut self,
        close: ClientPaneClose,
        result: Result<ResponseResult, ClientShellEndpointError>,
    ) -> (bool, Vec<ClientShellAction>) {
        if !self.running_close_target_exists(&close) || self.overlay.is_some() {
            return (false, Vec::new());
        }
        let info = match result {
            Ok(ResponseResult::PaneProcessInfo { process_info })
                if process_info.pane_id == close.pane_id =>
            {
                process_info
            }
            Err(_) => return (true, Vec::new()),
            _ => {
                self.endpoint_error =
                    Some("Cannot inspect the close target's foreground process".into());
                return (true, Vec::new());
            }
        };
        let process = info
            .foreground_processes
            .iter()
            .find(|process| Some(process.pid) == info.foreground_process_group_id)
            .or_else(|| info.foreground_processes.first());
        let Some(command) = process.map(|process| process.name.as_str()) else {
            self.endpoint_error = Some("Cannot determine whether the close target is idle".into());
            return (true, Vec::new());
        };
        let mut outcome = ClientShellInput::default();
        if is_idle_shell_command(command) {
            self.dispatch_running_close(close, &mut outcome);
        } else {
            self.overlay = Some(ClientShellOverlay::ConfirmClose(
                ClientConfirmCloseOverlay {
                    workspace_id: String::new(),
                    title: if close.tab_id.is_some() {
                        "Close tab?"
                    } else {
                        "Close pane?"
                    }
                    .into(),
                    detail: format!("{} — terminate {command}", close.pane_id),
                    running_close: Some(Box::new(close)),
                },
            ));
        }
        (true, outcome.actions)
    }

    pub(super) fn accept_close_overlay(&mut self, outcome: &mut ClientShellInput) {
        let Some(ClientShellOverlay::ConfirmClose(confirm)) = self.overlay.take() else {
            return;
        };
        if let Some(close) = confirm.running_close {
            self.dispatch_running_close(*close, outcome);
        } else {
            self.push_endpoint_method(
                Method::WorkspaceClose(crate::api::schema::WorkspaceCloseParams {
                    workspace_id: confirm.workspace_id,
                    close_group: true,
                }),
                outcome,
            );
        }
        outcome.repaint = true;
    }
}

fn is_idle_shell_command(command: &str) -> bool {
    matches!(
        command
            .trim()
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "sh" | "bash"
            | "zsh"
            | "fish"
            | "nu"
            | "ksh"
            | "csh"
            | "tcsh"
            | "elvish"
            | "pwsh"
            | "powershell"
            | "powershell.exe"
            | "cmd"
            | "cmd.exe"
    )
}
