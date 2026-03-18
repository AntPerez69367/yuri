use crate::common::player::combat::PlayerCombat;
use crate::common::player::inventory::PlayerInventory;
use crate::common::player::registries::PlayerRegistries;
use crate::common::types::{Item, Point};

/// Anything that participates in combat. Returns the shared PlayerCombat struct.
pub trait Combatant {
    fn combat(&self) -> &PlayerCombat;
    fn combat_mut(&mut self) -> &mut PlayerCombat;
    fn is_alive(&self) -> bool;
}

/// Anything with equipment/inventory.
pub trait InventoryHolder {
    fn equip(&self) -> &[Item];
    fn inventory(&self) -> &[Item];
    fn inventory_mut(&mut self) -> &mut [Item];
    fn money(&self) -> u32;
    fn set_money(&mut self, val: u32);
}

/// Anything with a position on the map.
pub trait Spatial {
    fn id(&self) -> u32;
    fn position(&self) -> Point;
    fn set_position(&self, p: Point);
    fn map_id(&self) -> u16;
}

/// Anything that can be targeted by scripts via registry variables.
pub trait ScriptTarget {
    fn get_reg(&self, key: &str) -> Option<i32>;
    fn set_reg(&mut self, key: &str, val: i32);
}

// ── Implementations for player sub-structs ──

impl Combatant for PlayerCombat {
    fn combat(&self) -> &PlayerCombat { self }
    fn combat_mut(&mut self) -> &mut PlayerCombat { self }
    fn is_alive(&self) -> bool { self.state >= 0 }
}


impl InventoryHolder for PlayerInventory {
    fn equip(&self) -> &[Item] { &self.equip }
    fn inventory(&self) -> &[Item] { &self.inventory }
    fn inventory_mut(&mut self) -> &mut [Item] { &mut self.inventory }
    fn money(&self) -> u32 { self.money }
    fn set_money(&mut self, val: u32) { self.money = val; }
}



impl ScriptTarget for PlayerRegistries {
    fn get_reg(&self, key: &str) -> Option<i32> {
        self.global_reg.get(key).copied()
    }
    fn set_reg(&mut self, key: &str, val: i32) {
        self.global_reg.insert(key.to_owned(), val);
    }
}

// LegacyEntity trait to help with breaking up god structs
pub trait LegacyEntity {
    type Data;
    fn read_legacy(&self) -> parking_lot::RwLockReadGuard<'_, Self::Data>;
    fn write_legacy(&self) -> parking_lot::RwLockWriteGuard<'_, Self::Data>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::player::{PlayerCombat, PlayerRegistries};

    #[test]
    fn player_combat_implements_combatant() {
        let mut c = PlayerCombat { hp: 100, max_hp: 100, ..Default::default() };
        assert!(c.is_alive());
        let stats = c.combat();
        assert_eq!(stats.hp, 100);
        c.combat_mut().hp = 50;
        assert_eq!(c.combat().hp, 50);
    }

    #[test]
    fn player_registries_implements_script_target() {
        let mut r = PlayerRegistries::default();
        assert_eq!(ScriptTarget::get_reg(&r, "x"), None);
        ScriptTarget::set_reg(&mut r, "x", 42);
        assert_eq!(ScriptTarget::get_reg(&r, "x"), Some(42));
    }
}
