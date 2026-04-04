use crate::pty::PtySession;

pub struct Pane {
    pub id: u64,
    pub index: u32,
    pub name: String,
    pub shell: String,
    pub pty: PtySession,
}

impl Pane {
    pub fn matches_target(&self, target: &str) -> bool {
        self.name == target || self.index.to_string() == target
    }
}
