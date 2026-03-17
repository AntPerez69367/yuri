use crate::common::types::Legend;
use serde::{Serialize, Deserialize};

/// Achievement legends display. Pre-allocated to MAX size.
pub use crate::common::constants::entity::player::MAX_LEGENDS;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerLegends {
    /// Legend entries — slot-indexed, pre-allocated.
    /// Empty slot has icon = 0.
    pub legends: Vec<Legend>,
}

impl Default for PlayerLegends {
    fn default() -> Self {
        Self {
            legends: vec![Legend::default(); MAX_LEGENDS],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_preallocated() {
        let l = PlayerLegends::default();
        assert_eq!(l.legends.len(), MAX_LEGENDS);
    }
}
