use serde::{Deserialize, Serialize};

use super::error::classify_error;
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
    ActiveWindowChanged {
        session_id: u64,
        window_id: u64,
    },
    ActivePaneChanged {
        session_id: u64,
        window_id: u64,
        pane_id: u64,
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
    PtyOutputSnapshot {
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
        code: String,
        message: String,
    },
}

impl ServerEvent {
    pub fn error(message: impl Into<String>) -> Self {
        let message = message.into();
        Self::Error {
            code: classify_error(&message).to_string(),
            message,
        }
    }
}
