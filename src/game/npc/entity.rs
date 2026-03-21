//! NPC entity definitions, trait impls, and ID management.

use parking_lot::RwLock;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::common::constants::entity::npc::{F1_NPC, NPCT_START_NUM, NPC_START_NUM};
use crate::common::constants::entity::player::MAX_GLOBALNPCREG;
use crate::common::player::inventory::MAX_EQUIP;
use crate::common::traits::{LegacyEntity, Spatial};
use crate::common::types::Item;
use crate::config::Point;
use crate::database::map_db::GlobalReg;
use crate::game::types::GfxViewer;

pub struct NpcEntity {
    // level -1: Atomic Snapshot
    pub id: u32,
    pub pos_atomic: AtomicU64,

    // level 0: Identity
    pub name: String,
    pub npc_name: String,

    // level 1: Legacy
    pub legacy: RwLock<NpcData>,
}

impl LegacyEntity for NpcEntity {
    type Data = NpcData;

    #[inline]
    fn read(&self) -> parking_lot::MappedRwLockReadGuard<'_, Self::Data> {
        parking_lot::RwLockReadGuard::map(self.legacy.read(), |d| d)
    }

    #[inline]
    fn write(&self) -> parking_lot::MappedRwLockWriteGuard<'_, Self::Data> {
        parking_lot::RwLockWriteGuard::map(self.legacy.write(), |d| d)
    }
}

impl Spatial for NpcEntity {
    fn id(&self) -> u32 {
        self.id
    }

    fn position(&self) -> Point {
        let pos = self.pos_atomic.load(Ordering::Relaxed);
        Point::from_u64(pos)
    }

    fn set_position(&self, p: Point) {
        let pos = p.to_u64();
        self.pos_atomic.store(pos, Ordering::Relaxed);
    }

    fn map_id(&self) -> u16 {
        self.position().m
    }
}
///
/// # Layout
///
/// NPC entity data. Layout verified by `npc_data_size` and `npc_data_offsets` tests.
#[repr(C)]
pub struct NpcData {
    pub id: u32,
    pub prev_x: u16,
    pub prev_y: u16,
    pub graphic_id: u32,
    pub graphic_color: u32,
    pub m: u16,
    pub x: u16,
    pub y: u16,
    pub bl_type: u8,
    pub subtype: u8,
    pub equip: [Item; MAX_EQUIP],
    pub registry: [GlobalReg; MAX_GLOBALNPCREG],
    pub gfx: GfxViewer,
    pub actiontime: u32,
    pub owner: u32,
    pub duration: u32,
    pub lastaction: u32,
    pub time: u32,
    pub duratime: u32,
    pub item_look: u32,
    pub item_owner: u32,
    pub item_color: u32,
    pub item_id: u32,
    pub item_slot: u32,
    pub item_pos: u32,
    pub item_amount: u32,
    pub item_dura: u32,
    pub name: [i8; 64],
    pub npc_name: [i8; 64],
    pub itemreal_name: [i8; 64],
    pub state: i8,
    pub side: i8,
    pub canmove: i8,
    pub npctype: i8,
    pub clone: i8,
    pub shop_npc: i8,
    pub repair_npc: i8,
    pub bank_npc: i8,
    pub receive_item: i8,
    pub retdist: i8,
    pub _pad: [u8; 2],
    pub movetimer: u32,
    pub movetime: u32,
    pub sex: u16,
    pub face: u16,
    pub face_color: u16,
    pub hair: u16,
    pub hair_color: u16,
    pub armor_color: u16,
    pub skin_color: u16,
    pub startm: u16,
    pub startx: u16,
    pub starty: u16,
    pub returning: u8,
    // 3 bytes trailing padding added automatically by repr(C) to align struct to 8 bytes
}
// SAFETY: NpcData contains raw pointers (next/prev) that are legacy block-list links.
// All access is gated behind unsafe blocks; no Rust code aliases these pointers.
unsafe impl Send for NpcData {}
unsafe impl Sync for NpcData {}

// NPC ID counters.
pub static NPC_ID: AtomicU32 = AtomicU32::new(NPC_START_NUM);
pub static NPCTEMP_ID: AtomicU32 = AtomicU32::new(NPCT_START_NUM);

/// Returns an available NPC ID, allocating a new one if needed.
///
/// Scans from `NPC_START_NUM` upward for a slot not present in the ID
/// database.  When the scan reaches `NPC_ID` it bumps the high-water mark
/// and returns it.
///
/// # Safety
///
/// Caller must hold the server-wide lock; mutates the `NPC_ID` global and
/// calls `entity_position` to check if an ID slot is occupied.
pub unsafe fn npc_get_new_npcid() -> u32 {
    let mut x = NPC_START_NUM;
    loop {
        let cur = NPC_ID.load(Ordering::Relaxed);
        if x > cur {
            break;
        }
        if x == cur {
            NPC_ID.store(cur + 1, Ordering::Relaxed);
        }
        if crate::game::map_server::entity_position(x).is_none() {
            return x;
        }
        x += 1;
    }
    NPC_ID.fetch_add(1, Ordering::Relaxed);
    NPC_ID.load(Ordering::Relaxed)
}

/// Returns an available temp NPC ID.
///
/// Scans from `NPCT_START_NUM` upward for a free slot, bumping
/// `NPCTEMP_ID` when the high-water mark is reached.
pub fn npc_get_new_npctempid() -> u32 {
    let mut x = NPCT_START_NUM;
    loop {
        let cur = NPCTEMP_ID.load(Ordering::Relaxed);
        if x > cur {
            break;
        }
        if x == cur {
            NPCTEMP_ID.store(cur + 1, Ordering::Relaxed);
        }
        if crate::game::map_server::entity_position(x).is_none() {
            return x;
        }
        x += 1;
    }
    NPCTEMP_ID.fetch_add(1, Ordering::Relaxed);
    NPCTEMP_ID.load(Ordering::Relaxed)
}

/// Decrements the temp NPC ID counter when a temp NPC is removed.
///
/// Only acts when `id` falls in the temp-NPC range and is not the sentinel
/// `F1_NPC` value.  Returns `0` unconditionally.
///
/// # Safety
///
/// Mutates the `NPCTEMP_ID` global; caller must hold the server-wide lock.
pub unsafe fn npc_idlower(id: i32) -> i32 {
    let id_u = id as u32;
    if id_u >= NPCT_START_NUM && id_u != F1_NPC {
        let cur = NPCTEMP_ID.load(Ordering::Relaxed);
        NPCTEMP_ID.store(cur.saturating_sub(1), Ordering::Relaxed);
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npc_data_size() {
        assert_eq!(
            std::mem::size_of::<NpcData>(),
            20388,
            "NpcData size mismatch"
        );
    }

    #[test]
    fn npc_data_offsets() {
        assert_eq!(std::mem::offset_of!(NpcData, equip), 24);
        assert_eq!(std::mem::offset_of!(NpcData, registry), 13224);
        assert_eq!(std::mem::offset_of!(NpcData, gfx), 20024);
        assert_eq!(std::mem::offset_of!(NpcData, id), 0);
        assert_eq!(std::mem::offset_of!(NpcData, name), 20152);
        assert_eq!(std::mem::offset_of!(NpcData, movetimer), 20356);
        assert_eq!(std::mem::offset_of!(NpcData, sex), 20364);
    }

    #[test]
    fn npc_data_canmove_offset() {
        assert_eq!(std::mem::offset_of!(NpcData, canmove), 20346);
    }

    #[test]
    fn npc_idlower_decrements_temp() {
        use std::sync::atomic::Ordering;
        unsafe {
            let orig = NPCTEMP_ID.load(Ordering::Relaxed);
            NPCTEMP_ID.store(NPCT_START_NUM + 5, Ordering::Relaxed);
            npc_idlower((NPCT_START_NUM + 1) as i32);
            assert_eq!(NPCTEMP_ID.load(Ordering::Relaxed), NPCT_START_NUM + 4);
            NPCTEMP_ID.store(orig, Ordering::Relaxed);
        }
    }

    #[test]
    fn npc_idlower_ignores_regular_npc() {
        use std::sync::atomic::Ordering;
        unsafe {
            let orig = NPCTEMP_ID.load(Ordering::Relaxed);
            NPCTEMP_ID.store(NPCT_START_NUM + 5, Ordering::Relaxed);
            // NPC_START_NUM is not a temp NPC — counter should not change
            npc_idlower(NPC_START_NUM as i32);
            assert_eq!(NPCTEMP_ID.load(Ordering::Relaxed), NPCT_START_NUM + 5);
            NPCTEMP_ID.store(orig, Ordering::Relaxed);
        }
    }
}
