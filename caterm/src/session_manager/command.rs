use serde::{Deserialize, Serialize};

use super::error::classify_error;
use super::protocol::PROTOCOL_VERSION;
use super::snapshot::{PaneSnapshot, ServerSnapshot, SessionSnapshot, WindowSnapshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResponse {
    pub protocol_version: u32,
    pub ok: bool,
    pub result: Option<CommandResult>,
    pub error: Option<String>,
    pub error_code: Option<String>,
}

impl CommandResponse {
    pub fn success(result: CommandResult) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            ok: true,
            result: Some(result),
            error: None,
            error_code: None,
        }
    }

    pub fn failure(error: String) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            ok: false,
            result: None,
            error_code: Some(classify_error(&error).to_string()),
            error: Some(error),
        }
    }
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
