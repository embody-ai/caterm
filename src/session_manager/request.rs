use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::event::ServerEvent;

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
    SendInput {
        session: String,
        window: String,
        pane: String,
        data: String,
    },
    List,
    Stop,
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerResponse {
    pub ok: bool,
    pub events: Vec<ServerEvent>,
}
