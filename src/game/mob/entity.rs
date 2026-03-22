//! Mob entity types, ID statics, and shared helpers.

#![allow(non_snake_case)]

use std::sync::atomic::{AtomicU32, AtomicU8, AtomicU64, Ordering};
use parking_lot::{MappedRwLockReadGuard, MappedRwLockWriteGuard, RwLockReadGuard, RwLockWriteGuard};

use crate::common::constants::entity::mob::{
    MAX_GLOBALMOBREG, MAX_INVENTORY, MAX_MAGIC_TIMERS, MAX_THREATCOUNT,
    MOBOT_START_NUM, MOB_START_NUM,
};
use crate::common::constants::entity::npc::NPC_START_NUM;
use crate::common::traits::{LegacyEntity, Spatial};
use crate::common::types::{Item, Point, SkillInfo};
use crate::database::map_db::GlobalReg;
use crate::database::mob_db::MobDbData;
use crate::game::lua::dispatch::dispatch;
use crate::database::magic_db;
use crate::game::map_parse::combat::clif_sendanimation_inner;
use crate::game::pc::MapSessionData;
use crate::game::types::GfxViewer;

use crate::database::map_db::get_map_ptr as ffi_get_map_ptr;
use crate::game::block::AreaType;
use crate::game::block_grid;

// ─── MobEntity ───────────────────────────────────────────────────────────────

pub struct MobEntity {
    pub id: u32,
    pub pos_atomic: AtomicU64,
    pub legacy: parking_lot::RwLock<MobSpawnData>,
}

impl LegacyEntity for MobEntity {
    type Data = MobSpawnData;

    #[inline]
    fn read(&self) -> MappedRwLockReadGuard<'_, Self::Data> {
        RwLockReadGuard::map(self.legacy.read(), |d| d)
    }

    #[inline]
    fn write(&self) -> MappedRwLockWriteGuard<'_, Self::Data> {
        RwLockWriteGuard::map(self.legacy.write(), |d| d)
    }
}

impl Spatial for MobEntity {
    fn id(&self) -> u32 {
        self.id
    }

    fn position(&self) -> Point {
        Point::from_u64(self.pos_atomic.load(Ordering::Relaxed))
    }

    fn set_position(&self, p: Point) {
        self.pos_atomic.store(p.to_u64(), Ordering::Relaxed);
    }

    fn map_id(&self) -> u16 {
        self.position().m
    }
}

// ─── ThreatTable ─────────────────────────────────────────────────────────────

/// Mob threat entry: which player and how much threat they have generated.
#[repr(C)]
pub struct ThreatTable {
    pub user: u32,
    pub amount: u32,
}

// ─── MobSpawnData ─────────────────────────────────────────────────────────────

/// Mob spawn data (spawn parameters and state for a single mob instance).
///
/// Field order and types MUST exactly match C. Verify size with:
/// `cargo test mob_spawn_data_size -- --nocapture`
///
/// Layout:
/// ```text
/// offset  field                    size
///      0  (entity header fields)      48
///     48  da[200]                  9600  (200 * SkillInfo@48)
///   9648  inventory[52]           45760  (52 * Item@880)
///  55408  data*                       8  (pointer)
///  55416  threat[50]                400  (50 * ThreatTable@8)
///  55816  registry[50]             3400  (50 * GlobalReg@68)
///  59216  gfx                        72  (GfxViewer)
///  59288  startm..look               12  (6 * u16)
///  59300  miss, protection            4  (2 * i16)
///  59304  id..exp                    72  (18 * u32)
///  59376  ac..will                   44  (11 * i32)
///  59420  state..look_color           9  (9 * u8)
///  59429  clone..charstate            5  (5 * i8)  -> compiler pads 3 bytes here
///  59437  sleep..invis               20  (5 * f32) -- offset 59437 is wrong after pad
/// ```
/// (Use the size test to verify total = 61120.)
#[repr(C)]
pub struct MobSpawnData {
    pub id: u32,
    pub graphic_id: u32,
    pub graphic_color: u32,
    pub m: u16,
    pub x: u16,
    pub y: u16,
    pub bl_type: u8,
    pub subtype: u8,
    pub da: [SkillInfo; MAX_MAGIC_TIMERS],
    pub inventory: [Item; MAX_INVENTORY],
    pub data: *mut MobDbData,
    pub threat: [ThreatTable; MAX_THREATCOUNT],
    pub registry: [GlobalReg; MAX_GLOBALMOBREG],
    pub gfx: GfxViewer,
    pub startm: u16,
    pub startx: u16,
    pub starty: u16,
    pub prev_x: u16,
    pub prev_y: u16,
    pub look: u16,
    pub miss: i16,
    pub protection: i16,
    pub mobid: u32,
    pub current_vita: u32,
    pub current_mana: u32,
    pub target: u32,
    pub attacker: u32,
    pub owner: u32,
    pub confused_target: u32,
    pub timer: u32,
    pub last_death: u32,
    pub rangeTarget: u32,
    pub ranged: u32,
    pub newmove: u32,
    pub newatk: u32,
    pub lastvita: u32,
    pub maxvita: u32,
    pub maxmana: u32,
    pub replace: u32,
    pub mindam: u32,
    pub maxdam: u32,
    pub amnesia: u32,
    pub exp: u32,
    pub ac: i32,
    pub side: i32,
    pub time_: i32,
    pub spawncheck: i32,
    pub num: i32,
    pub crit: i32,
    pub critchance: i32,
    pub critmult: i32,
    pub snare: i32,
    pub lastaction: i32,
    pub hit: i32,
    pub might: i32,
    pub grace: i32,
    pub will: i32,
    pub state: u8,
    pub canmove: u8,
    pub onetime: u8,
    pub paralyzed: u8,
    pub blind: u8,
    pub confused: u8,
    pub summon: u8,
    pub returning: u8,
    pub look_color: u8,
    pub clone: i8,
    pub start: i8,
    pub end: i8,
    pub block: i8,
    pub charstate: i8,
    // compiler inserts 3 bytes of padding here to align f32 to 4 bytes
    pub sleep: f32,
    pub deduction: f32,
    pub damage: f32,
    pub dmgshield: f32,
    pub invis: f32,
    // compiler inserts padding here to align f64 to 8 bytes
    pub dmgdealt: f64,
    pub dmgtaken: f64,
    pub maxdmg: f64,
    pub dmgindtable: [[f64; 2]; MAX_THREATCOUNT],
    pub dmggrptable: [[f64; 2]; MAX_THREATCOUNT],
    pub cursed: u8,
}

// SAFETY: MobSpawnData contains raw pointers to C-managed entities.
// All access is gated behind unsafe blocks.
unsafe impl Send for MobSpawnData {}
unsafe impl Sync for MobSpawnData {}

// ─── Mutable globals ──────────────────────────────────────────────────────────

pub static MOB_ID: AtomicU32 = AtomicU32::new(MOB_START_NUM);
pub static MAX_NORMAL_ID: AtomicU32 = AtomicU32::new(MOB_START_NUM);
pub static CMOB_ID: AtomicU32 = AtomicU32::new(0);
pub static MOB_SPAWN_MAX: AtomicU32 = AtomicU32::new(MOB_START_NUM);
pub static MOB_SPAWN_START: AtomicU32 = AtomicU32::new(MOB_START_NUM);
pub static MOB_ONETIME_MAX: AtomicU32 = AtomicU32::new(MOBOT_START_NUM);
pub static MOB_ONETIME_START: AtomicU32 = AtomicU32::new(MOBOT_START_NUM);
pub static MIN_TIMER: AtomicU32 = AtomicU32::new(1000);
pub(super) static TIMERCHECK: AtomicU8 = AtomicU8::new(0);

// ─── Shared helpers ──────────────────────────────────────────────────────────

/// Helper: get magic yname as `&str` by spell ID (for sl_doscript calls).
/// The returned `String` is owned; callers can borrow it.
#[inline]
pub(super) fn magicdb_yname_str(id: i32) -> String {
    let arc = magic_db::search(id);
    crate::game::scripting::carray_to_str(&arc.yname).to_owned()
}

/// Helper: get magic display name pointer by spell ID.
#[inline]
pub(super) fn magicdb_name(id: i32) -> *const i8 {
    magic_db::search(id).name.as_ptr()
}

/// Helper: get magic dispel threshold by spell ID.
#[inline]
pub(super) fn magicdb_dispel(id: i32) -> i32 {
    magic_db::search(id).dispell as i32
}

/// Map mob subtype to its AI script root name.
pub(super) fn ai_script_name(data: &MobDbData) -> &str {
    match data.subtype {
        0 => "mob_ai_basic",
        1 => "mob_ai_normal",
        2 => "mob_ai_hard",
        3 => "mob_ai_boss",
        5 => "mob_ai_ghost",
        _ => crate::game::scripting::carray_to_str(&data.yname),
    }
}

/// Legacy raw-pointer player lookup for deeply unsafe code paths in mob.rs.
pub(super) fn map_id2sd_mob(id: u32) -> *mut MapSessionData {
    match crate::game::map_server::map_id2sd_pc(id) {
        Some(arc) => {
            let ptr = &mut *arc.write() as *mut MapSessionData;
            ptr
        }
        None => std::ptr::null_mut(),
    }
}

/// Helper: broadcast animation removal to nearby PCs via block_grid.
pub(super) unsafe fn broadcast_animation_to_pcs(mob: &MobSpawnData, anim: i32) {
    let m = mob.m as usize;
    if let Some(grid) = block_grid::get_grid(m) {
        let slot = &*ffi_get_map_ptr(mob.m);
        let ids = block_grid::ids_in_area(
            grid,
            mob.x as i32,
            mob.y as i32,
            AreaType::Area,
            slot.xs as i32,
            slot.ys as i32,
        );
        for id in ids {
            if let Some(sd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                let sd_guard = sd_arc.read();
                clif_sendanimation_inner(
                    sd_guard.fd,
                    sd_guard.player.appearance.setting_flags,
                    anim,
                    mob.id,
                    -1,
                );
            }
        }
    }
}

// ─── Lua dispatch helpers ────────────────────────────────────────────────────

/// Dispatch a Lua event with a single entity-ID argument.
pub(super) fn sl_doscript_simple(root: &str, method: Option<&str>, id: u32) -> bool {
    dispatch(root, method, &[id])
}

/// Dispatch a Lua event with two entity-ID arguments.
pub(super) fn sl_doscript_2(root: &str, method: Option<&str>, id1: u32, id2: u32) -> bool {
    dispatch(root, method, &[id1, id2])
}

// ─── Mob ID management ────────────────────────────────────────────────────────

pub fn mob_get_new_id() -> u32 {
    MOB_ID.fetch_add(1, Ordering::Relaxed)
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_get_free_id() -> u32 {
    let mut x = MOB_ONETIME_START.load(Ordering::Relaxed);
    loop {
        if x >= NPC_START_NUM {
            tracing::warn!("[mob] mob_get_free_id: onetime range exhausted");
            return 0;
        }
        let omax = MOB_ONETIME_MAX.load(Ordering::Relaxed);
        if x == omax {
            if omax >= NPC_START_NUM {
                tracing::warn!("[mob] mob_get_free_id: onetime range full");
                return 0;
            }
            MOB_ONETIME_MAX.store(omax + 1, Ordering::Relaxed);
        }
        if crate::game::map_server::entity_position(x).is_none() {
            return x;
        }
        x += 1;
    }
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn free_onetime(mob: *mut MobSpawnData) -> i32 {
    if mob.is_null() {
        return 0;
    }
    (*mob).data = std::ptr::null_mut();
    let id = (*mob).id;
    crate::game::map_server::mob_map_remove(id);
    // Box drop handles deallocation -- no libc::free needed.
    // The compaction loop exits early when an unoccupied slot is found.
    // It only compacts MOB_ONETIME_MAX when called for the top-of-range mob.
    // compact onetime range downward
    let mut x = MOB_ONETIME_START.load(Ordering::Relaxed);
    loop {
        let omax = MOB_ONETIME_MAX.load(Ordering::Relaxed);
        if x > omax {
            break;
        }
        if crate::game::map_server::entity_position(x).is_none() {
            return 0;
        }
        if x == omax {
            crate::game::map_server::map_deliddb(x);
            MOB_ONETIME_MAX.store(omax - 1, Ordering::Relaxed);
        }
        x += 1;
    }
    0
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn mob_spawn_data_size() {
        const EXPECTED: usize = 61088;
        assert_eq!(
            size_of::<MobSpawnData>(),
            EXPECTED,
            "MobSpawnData size mismatch -- check field types and padding"
        );
        println!("MobSpawnData = {} bytes", size_of::<MobSpawnData>());
        println!("SkillInfo    = {} bytes", size_of::<SkillInfo>());
        println!("ThreatTable  = {} bytes", size_of::<ThreatTable>());
        println!("Item         = {} bytes", size_of::<Item>());
        println!("GlobalReg    = {} bytes", size_of::<GlobalReg>());
        println!("GfxViewer    = {} bytes", size_of::<GfxViewer>());
    }
}
