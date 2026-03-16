use crate::common::types::{BankData, Item};

/// Equipment, inventory, banks, and currency. Pre-allocated to MAX sizes.
pub const MAX_EQUIP: usize = 15;
pub const MAX_INVENTORY: usize = 52;
pub const MAX_BANK_SLOTS: usize = 255;

#[derive(Debug, Clone)]
pub struct PlayerInventory {
    /// Equipped items — slot-indexed, pre-allocated.
    /// Empty slot has id = 0.
    pub equip: Vec<Item>,
    /// Carried items — slot-indexed, pre-allocated.
    /// Empty slot has id = 0.
    pub inventory: Vec<Item>,
    /// Bank storage — slot-indexed, pre-allocated.
    /// Empty slot has item_id = 0.
    pub banks: Vec<BankData>,
    pub money: u32,
    pub bank_money: u32,
    pub max_inv: u8,
    pub max_slots: u32,
}

impl Default for PlayerInventory {
    fn default() -> Self {
        Self {
            equip: vec![Item::default(); MAX_EQUIP],
            inventory: vec![Item::default(); MAX_INVENTORY],
            banks: vec![BankData::default(); MAX_BANK_SLOTS],
            money: 0,
            bank_money: 0,
            max_inv: 0,
            max_slots: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_preallocated() {
        let inv = PlayerInventory::default();
        assert_eq!(inv.equip.len(), MAX_EQUIP);
        assert_eq!(inv.inventory.len(), MAX_INVENTORY);
        assert_eq!(inv.banks.len(), MAX_BANK_SLOTS);
    }

    #[test]
    fn empty_slot_has_zero_id() {
        let inv = PlayerInventory::default();
        assert_eq!(inv.equip[0].id, 0);
        assert_eq!(inv.inventory[0].id, 0);
        assert_eq!(inv.banks[0].item_id, 0);
    }
}
