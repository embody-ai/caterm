use std::env;
use std::path::Path;

pub const DEFAULT_COLS: u16 = 120;
pub const DEFAULT_ROWS: u16 = 32;

pub struct DaemonConfig {
    pub shell: String,
    pub cols: u16,
    pub rows: u16,
    pub relay: Option<RelayClientConfig>,
}

#[derive(Debug, Clone)]
pub struct RelayClientConfig {
    pub url: String,
    pub session_id: String,
    pub api_key: Option<String>,
}

impl DaemonConfig {
    pub fn from_env() -> Self {
        Self {
            shell: resolve_shell(),
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
            relay: resolve_relay(),
        }
    }
}

fn resolve_shell() -> String {
    if let Ok(shell) = env::var("CATERM_SHELL") {
        if !shell.trim().is_empty() {
            return shell;
        }
    }

    if let Ok(shell) = env::var("SHELL") {
        if Path::new(&shell).exists() {
            return shell;
        }
    }

    "/bin/bash".to_string()
}

fn resolve_relay() -> Option<RelayClientConfig> {
    let url = env::var("CATERM_RELAY_URL").ok()?;
    let url = url.trim();
    if url.is_empty() {
        return None;
    }

    let session_id = env::var("CATERM_RELAY_SESSION_ID").ok()?;
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return None;
    }

    let api_key = env::var("CATERM_RELAY_API_KEY")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());

    Some(RelayClientConfig {
        url: url.to_string(),
        session_id: session_id.to_string(),
        api_key,
    })
}
