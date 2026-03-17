use crate::common::types::SkillInfo;
use serde::{Serialize, Deserialize};

/// Spell book and active effects. Pre-allocated to MAX sizes.
pub use crate::common::constants::entity::player::{MAX_SPELLS, MAX_MAGIC_TIMERS};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerSpells {
    /// Known spells — slot-indexed, pre-allocated to MAX_SPELLS.
    /// Empty slot = 0.
    pub skills: Vec<u16>,
    /// Active spell/aether effects — pre-allocated to MAX_MAGIC_TIMERS.
    /// Empty slot has id = 0.
    pub dura_aether: Vec<SkillInfo>,
}

impl Default for PlayerSpells {
    fn default() -> Self {
        Self {
            skills: vec![0u16; MAX_SPELLS],
            dura_aether: vec![SkillInfo::default(); MAX_MAGIC_TIMERS],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_preallocated() {
        let s = PlayerSpells::default();
        assert_eq!(s.skills.len(), MAX_SPELLS);
        assert_eq!(s.dura_aether.len(), MAX_MAGIC_TIMERS);
    }

    #[test]
    fn skills_indexed_by_slot() {
        let mut s = PlayerSpells::default();
        s.skills[0] = 42;
        assert_eq!(s.skills[0], 42);
        assert_eq!(s.skills[1], 0);
    }
}
