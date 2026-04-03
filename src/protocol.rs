use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    AgentStarted { shell: String, cols: u16, rows: u16 },
    PtyOutput { data: String },
    PtyExited { exit_code: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    PtyInput { data: String },
    Resize { cols: u16, rows: u16 },
    Shutdown,
}
