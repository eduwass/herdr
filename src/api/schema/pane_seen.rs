use serde::{Deserialize, Serialize};

/// Acknowledge the agent state actually observed by a remote reader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneMarkSeenParams {
    pub pane_id: String,
    pub terminal_id: String,
    pub state_change_seq: u64,
}
