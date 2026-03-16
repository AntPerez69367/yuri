pub mod identity;
pub mod combat;
pub mod progression;
pub mod spells;
pub mod inventory;
pub mod appearance;
pub mod social;
pub mod registries;
pub mod legends;
pub mod bridge;

pub use identity::PlayerIdentity;
pub use combat::PlayerCombat;
pub use progression::PlayerProgression;
pub use spells::PlayerSpells;
pub use inventory::PlayerInventory;
pub use appearance::PlayerAppearance;
pub use social::PlayerSocial;
pub use registries::PlayerRegistries;
pub use legends::PlayerLegends;

/// Decomposed player persistence data. Replaces MmoCharStatus.
///
/// Each sub-struct owns its domain logic. Cross-domain operations
/// use split borrows:
/// ```ignore
/// let PlayerData { ref mut combat, ref inventory, ref progression, .. } = *player;
/// combat.calc_stats(inventory, progression);
/// ```
#[derive(Debug, Clone, Default)]
pub struct PlayerData {
    pub identity: PlayerIdentity,
    pub combat: PlayerCombat,
    pub progression: PlayerProgression,
    pub spells: PlayerSpells,
    pub inventory: PlayerInventory,
    pub appearance: PlayerAppearance,
    pub social: PlayerSocial,
    pub registries: PlayerRegistries,
    pub legends: PlayerLegends,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_player_data() {
        let p = PlayerData::default();
        assert_eq!(p.identity.id, 0);
        assert_eq!(p.combat.hp, 0);
        assert_eq!(p.progression.level, 0);
        assert_eq!(p.inventory.equip.len(), inventory::MAX_EQUIP);
        assert_eq!(p.spells.skills.len(), spells::MAX_SPELLS);
        assert!(p.registries.global_reg.is_empty());
        assert_eq!(p.legends.legends.len(), legends::MAX_LEGENDS);
    }
}
