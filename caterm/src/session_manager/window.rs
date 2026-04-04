use std::collections::BTreeMap;

use super::pane::Pane;
use super::snapshot::WindowSnapshot;

#[derive(Debug, Clone, Copy)]
pub enum WindowLayout {
    Single,
    Tiled,
    Horizontal,
    Vertical,
}

pub struct Window {
    pub id: u64,
    pub index: u32,
    pub name: String,
    pub layout: WindowLayout,
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

    pub fn sync_layout(&mut self) {
        self.layout = if self.panes.len() <= 1 {
            WindowLayout::Single
        } else if matches!(
            self.layout,
            WindowLayout::Horizontal | WindowLayout::Vertical
        ) {
            self.layout
        } else {
            WindowLayout::Tiled
        };
    }

    pub fn set_split_layout(&mut self, layout: WindowLayout) {
        self.layout = if self.panes.len() <= 1 {
            WindowLayout::Single
        } else {
            layout
        };
    }

    pub fn snapshot(&self) -> WindowSnapshot {
        WindowSnapshot {
            id: self.id,
            index: self.index,
            name: self.name.clone(),
            layout: self.layout.as_str().to_string(),
            active_pane_id: self.active_pane_id,
            active_pane_index: self
                .active_pane_id
                .and_then(|id| self.panes.get(&id))
                .map(|pane| pane.index),
            panes: self.panes.values().map(Pane::snapshot).collect(),
        }
    }
}

impl WindowLayout {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Tiled => "tiled",
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }
}
