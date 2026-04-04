use tokio::sync::mpsc;

use crate::pty::PtySession;

use super::snapshot::PaneSnapshot;

pub struct Pane {
    pub id: u64,
    pub index: u32,
    pub name: String,
    pub shell: String,
    pub pty: PtySession,
    pub output_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    pub exit_code: Option<u32>,
}

impl Pane {
    pub fn matches_target(&self, target: &str) -> bool {
        self.name == target || self.index.to_string() == target
    }

    pub fn snapshot(&self) -> PaneSnapshot {
        PaneSnapshot {
            id: self.id,
            index: self.index,
            name: self.name.clone(),
            shell: self.shell.clone(),
            exit_code: self.exit_code,
        }
    }
}
