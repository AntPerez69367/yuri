pub mod identity;
pub mod combat;
pub mod progression;
pub mod spells;
pub mod inventory;
pub mod appearance;
pub mod social;
pub mod registries;
pub mod legends;
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
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

    #[test]
    fn bincode_roundtrip() {
        let mut p = PlayerData::default();
        p.identity.id = 12345;
        p.identity.name = "TestPlayer".to_string();
        p.combat.hp = 100;
        p.combat.max_hp = 200;
        p.progression.level = 50;
        p.inventory.money = 9999;
        p.registries.global_reg.insert("test_key".to_string(), 42);
        p.social.clan = 7;
        p.legends.legends[0].icon = 5;

        let bytes = bincode::serialize(&p).expect("serialize");
        let p2: PlayerData = bincode::deserialize(&bytes).expect("deserialize");

        assert_eq!(p2.identity.id, 12345);
        assert_eq!(p2.identity.name, "TestPlayer");
        assert_eq!(p2.combat.hp, 100);
        assert_eq!(p2.combat.max_hp, 200);
        assert_eq!(p2.progression.level, 50);
        assert_eq!(p2.inventory.money, 9999);
        assert_eq!(p2.registries.global_reg.get("test_key"), Some(&42));
        assert_eq!(p2.social.clan, 7);
        assert_eq!(p2.legends.legends[0].icon, 5);
    }
}
