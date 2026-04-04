use std::collections::BTreeMap;

use super::pane::Pane;

pub struct Window {
    pub id: u64,
    pub index: u32,
    pub name: String,
    pub panes: BTreeMap<u64, Pane>,
    pub active_pane_id: Option<u64>,
}

impl Window {
    pub fn matches_target(&self, target: &str) -> bool {
        self.name == target || self.index.to_string() == target
    }

    pub fn next_pane_index(&self) -> u32 {
        self.panes
            .values()
            .map(|pane| pane.index)
            .max()
            .map_or(0, |index| index + 1)
    }
}
