use std::collections::BTreeMap;

use super::window::Window;

pub struct Session {
    pub id: u64,
    pub name: String,
    pub windows: BTreeMap<u64, Window>,
    pub active_window_id: Option<u64>,
}

impl Session {
    pub fn next_window_index(&self) -> u32 {
        self.windows
            .values()
            .map(|window| window.index)
            .max()
            .map_or(0, |index| index + 1)
    }
}
