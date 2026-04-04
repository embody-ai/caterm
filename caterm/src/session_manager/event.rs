use serde::{Deserialize, Serialize};

use super::snapshot::{PaneSnapshot, ServerSnapshot, SessionSnapshot, WindowSnapshot};

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
