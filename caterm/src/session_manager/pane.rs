use tokio::sync::mpsc;

use crate::pty::PtySession;

use super::snapshot::PaneSnapshot;

pub struct Pane {
    pub id: u64,
    pub index: u32,
    pub name: String,
    pub size: u16,
    pub shell: String,
    pub pty: PtySession,
    pub output_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    pub output_history: String,
    pub exit_code: Option<u32>,
}

impl Pane {
    const MAX_OUTPUT_HISTORY_BYTES: usize = 8192;

    pub fn matches_target(&self, target: &str) -> bool {
        self.name == target || self.index.to_string() == target
    }

    pub fn snapshot(&self) -> PaneSnapshot {
        PaneSnapshot {
            id: self.id,
            index: self.index,
            name: self.name.clone(),
            size: self.size,
            shell: self.shell.clone(),
            exit_code: self.exit_code,
        }
    }

    pub fn push_output_history(&mut self, chunk: &str) {
        self.output_history.push_str(chunk);
        if self.output_history.len() > Self::MAX_OUTPUT_HISTORY_BYTES {
            let trim_to = self.output_history.len() - Self::MAX_OUTPUT_HISTORY_BYTES;
            self.output_history.drain(..trim_to);
        }
    }
}
