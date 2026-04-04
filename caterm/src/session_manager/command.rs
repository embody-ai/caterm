use serde::{Deserialize, Serialize};

use super::snapshot::{PaneSnapshot, ServerSnapshot, SessionSnapshot, WindowSnapshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResponse {
    pub ok: bool,
    pub result: Option<CommandResult>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandResult {
    SessionCreated {
        session: SessionSnapshot,
        initial_window: WindowSnapshot,
        initial_pane: PaneSnapshot,
    },
    WindowCreated {
        session_id: u64,
        window: WindowSnapshot,
        initial_pane: PaneSnapshot,
    },
    PaneCreated {
        session_id: u64,
        window_id: u64,
        pane: PaneSnapshot,
    },
    PaneSplit {
        session_id: u64,
        window_id: u64,
        layout: String,
        pane: PaneSnapshot,
    },
    WindowSelected {
        session_id: u64,
        window: WindowSnapshot,
    },
    PaneSelected {
        session_id: u64,
        window_id: u64,
        pane: PaneSnapshot,
    },
    SessionRenamed {
        session: SessionSnapshot,
    },
    WindowRenamed {
        session_id: u64,
        window: WindowSnapshot,
    },
    PaneRenamed {
        session_id: u64,
        window_id: u64,
        pane: PaneSnapshot,
    },
    PaneResized {
        session_id: u64,
        window_id: u64,
        pane: PaneSnapshot,
    },
    SessionDeleted {
        session_id: u64,
    },
    WindowDeleted {
        session_id: u64,
        window_id: u64,
    },
    PaneDeleted {
        session_id: u64,
        window_id: u64,
        pane_id: u64,
    },
    InputAccepted {
        session_id: u64,
        window_id: u64,
        pane_id: u64,
    },
    SessionList {
        snapshot: ServerSnapshot,
    },
    Stopped,
    Pong,
}
