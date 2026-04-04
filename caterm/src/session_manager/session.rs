use std::collections::BTreeMap;

use super::snapshot::SessionSnapshot;
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

    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            id: self.id,
            name: self.name.clone(),
            active_window_id: self.active_window_id,
            active_window_index: self
                .active_window_id
                .and_then(|id| self.windows.get(&id))
                .map(|window| window.index),
            windows: self.windows.values().map(Window::snapshot).collect(),
        }
    }
}
