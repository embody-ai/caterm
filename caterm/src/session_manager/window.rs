use std::collections::BTreeMap;

use anyhow::{Result, bail};

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
    const MIN_PANE_SIZE: u16 = 10;

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

    pub fn rebalance_pane_sizes(&mut self) {
        if self.panes.is_empty() {
            return;
        }

        let pane_ids: Vec<u64> = self.pane_ids_in_index_order();
        let count = pane_ids.len() as u16;
        let base = 100 / count;
        let mut remainder = 100 % count;

        for pane_id in pane_ids {
            if let Some(pane) = self.panes.get_mut(&pane_id) {
                pane.size = base
                    + if remainder > 0 {
                        remainder -= 1;
                        1
                    } else {
                        0
                    };
            }
        }
    }

    pub fn resize_pane(&mut self, pane_id: u64, delta: i16) -> Result<()> {
        if self.panes.len() < 2 {
            bail!("cannot resize a window with fewer than 2 panes");
        }
        if delta == 0 {
            return Ok(());
        }

        let pane_ids = self.pane_ids_in_index_order();
        let target_index = pane_ids
            .iter()
            .position(|id| *id == pane_id)
            .ok_or_else(|| anyhow::anyhow!("pane {pane_id} not found"))?;
        let partner_index = if target_index + 1 < pane_ids.len() {
            target_index + 1
        } else {
            target_index.saturating_sub(1)
        };
        let partner_id = pane_ids[partner_index];

        let target_size = self
            .panes
            .get(&pane_id)
            .map(|pane| pane.size)
            .ok_or_else(|| anyhow::anyhow!("pane {pane_id} not found"))?;
        let partner_size = self
            .panes
            .get(&partner_id)
            .map(|pane| pane.size)
            .ok_or_else(|| anyhow::anyhow!("pane {partner_id} not found"))?;

        let delta = if delta > 0 {
            let max_growth = partner_size.saturating_sub(Self::MIN_PANE_SIZE) as i16;
            delta.min(max_growth)
        } else {
            let max_shrink = target_size.saturating_sub(Self::MIN_PANE_SIZE) as i16;
            delta.max(-max_shrink)
        };

        if delta == 0 {
            bail!(
                "resize would exceed the minimum pane size of {}",
                Self::MIN_PANE_SIZE
            );
        }

        if let Some(target) = self.panes.get_mut(&pane_id) {
            target.size = (target.size as i16 + delta) as u16;
        }
        if let Some(partner) = self.panes.get_mut(&partner_id) {
            partner.size = (partner.size as i16 - delta) as u16;
        }

        Ok(())
    }

    fn pane_ids_in_index_order(&self) -> Vec<u64> {
        let mut panes: Vec<(u32, u64)> = self
            .panes
            .values()
            .map(|pane| (pane.index, pane.id))
            .collect();
        panes.sort_by_key(|(index, _)| *index);
        panes.into_iter().map(|(_, id)| id).collect()
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
