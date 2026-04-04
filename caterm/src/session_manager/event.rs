use serde::{Deserialize, Serialize};

use super::protocol::PROTOCOL_VERSION;
use super::snapshot::{PaneSnapshot, ServerSnapshot, SessionSnapshot, WindowSnapshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub protocol_version: u32,
    pub event: ServerEvent,
}

impl EventEnvelope {
    pub fn new(event: ServerEvent) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            event,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    SessionList {
        sessions: Vec<SessionSnapshot>,
    },
    SessionCreated {
        session: SessionSnapshot,
    },
    WindowCreated {
        session_id: u64,
        window: WindowSnapshot,
    },
    PaneCreated {
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
    PtyOutput {
        session_id: u64,
        window_id: u64,
        pane_id: u64,
        data: String,
    },
    PaneExited {
        session_id: u64,
        window_id: u64,
        pane_id: u64,
        exit_code: u32,
    },
    Snapshot {
        snapshot: ServerSnapshot,
    },
    Pong,
    Error {
        message: String,
    },
}
