use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct ClientOptions {
    pub socket_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum SessionRequest {
    CreateSession {
        name: Option<String>,
    },
    CreateWindow {
        session: String,
        name: Option<String>,
    },
    CreatePane {
        session: String,
        window: String,
        name: Option<String>,
    },
    DeleteSession {
        target: String,
    },
    DeleteWindow {
        session: String,
        target: String,
    },
    DeletePane {
        session: String,
        window: String,
        target: String,
    },
    List,
    Stop,
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionResponse {
    pub ok: bool,
    pub message: String,
}
