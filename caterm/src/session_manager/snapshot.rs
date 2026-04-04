use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSnapshot {
    pub active_session_id: Option<u64>,
    pub active_window_id: Option<u64>,
    pub active_pane_id: Option<u64>,
    pub sessions: Vec<SessionSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub id: u64,
    pub name: String,
    pub active_window_id: Option<u64>,
    pub active_window_index: Option<u32>,
    pub windows: Vec<WindowSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowSnapshot {
    pub id: u64,
    pub index: u32,
    pub name: String,
    pub layout: String,
    pub active_pane_id: Option<u64>,
    pub active_pane_index: Option<u32>,
    pub panes: Vec<PaneSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneSnapshot {
    pub id: u64,
    pub index: u32,
    pub name: String,
    pub shell: String,
    pub exit_code: Option<u32>,
}
