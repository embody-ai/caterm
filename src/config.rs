use std::env;
use std::path::Path;

pub const DEFAULT_COLS: u16 = 120;
pub const DEFAULT_ROWS: u16 = 32;

pub struct DaemonConfig {
    pub shell: String,
    pub cols: u16,
    pub rows: u16,
}

impl DaemonConfig {
    pub fn from_env() -> Self {
        Self {
            shell: resolve_shell(),
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
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
