//! Typed entity storage — lookup, insertion, and removal for all game entities.
//!
//! Each entity type lives in its own `HashMap` behind a `Mutex` (for `Sync`)
//! and is wrapped in `Arc<RwLock<T>>` (or `Arc<T>` for strangler wrappers).

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock};

use parking_lot::RwLock;

use crate::common::traits::LegacyEntity;
use crate::common::types::Point;
use crate::game::lua::coroutine::purge_player;
use crate::game::mob::{MobSpawnData, FLOORITEM_START_NUM, MOB_START_NUM};
use crate::game::npc::{NpcEntity, NPC_ID, NPC_START_NUM};
use crate::game::pc::{MapSessionData, PlayerEntity};
use crate::game::scripting::types::floor::FloorItemData;
use crate::session::{get_fd_max, session_exists, session_get_data, session_get_eof, SessionId};

// ── Typed entity maps ────────────────────────────────────────────────────
// Each map owns its entities via Arc<RwLock<T>>. The Arc allows callers to
// hold a reference-counted handle without lifetime issues; the RwLock
// provides runtime borrow-checking (read/write locking). Lookup functions
// return Arc<RwLock<T>> — callers acquire read or write guards as needed.
//
// The game loop is single-threaded; the outer Mutex satisfies
// OnceLock<T>: Sync but never actually contends.

static PLAYER_MAP: OnceLock<Mutex<HashMap<u32, Arc<PlayerEntity>>>> = OnceLock::new();
static MOB_MAP: OnceLock<Mutex<HashMap<u32, Arc<RwLock<MobSpawnData>>>>> = OnceLock::new();
static NPC_MAP: OnceLock<Mutex<HashMap<u32, Arc<NpcEntity>>>> = OnceLock::new();
static ITEM_MAP: OnceLock<Mutex<HashMap<u32, Arc<RwLock<FloorItemData>>>>> = OnceLock::new();

#[inline]
fn player_map() -> std::sync::MutexGuard<'static, HashMap<u32, Arc<PlayerEntity>>> {
    PLAYER_MAP
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

#[inline]
fn mob_map() -> std::sync::MutexGuard<'static, HashMap<u32, Arc<RwLock<MobSpawnData>>>> {
    MOB_MAP
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

#[inline]
fn npc_map() -> std::sync::MutexGuard<'static, HashMap<u32, Arc<NpcEntity>>> {
    NPC_MAP
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

#[inline]
pub(crate) fn item_map() -> std::sync::MutexGuard<'static, HashMap<u32, Arc<RwLock<FloorItemData>>>> {
    ITEM_MAP
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

// ── Initialization / teardown ────────────────────────────────────────────

pub fn map_initiddb() {}

pub fn map_termiddb() {
    player_map().clear();
    mob_map().clear();
    npc_map().clear();
    item_map().clear();
}

// ── Insertion ────────────────────────────────────────────────────────────

/// Insert a player — takes ownership of the Box, wrapping it in Arc.
pub fn map_addiddb_player(id: u32, fd: SessionId, sd: Box<MapSessionData>) {
    let name = sd.player.identity.name.clone();
    let gm_level = sd.player.identity.gm_level;
    let arc = Arc::from(PlayerEntity::new(id, fd, name, gm_level, sd));
    player_map().insert(id, arc);
}

/// Insert a mob — takes ownership of the Box, wrapping it in Arc<RwLock>.
pub fn map_addiddb_mob(id: u32, mob: Box<MobSpawnData>) {
    mob_map().insert(id, unsafe { box_into_arc_rwlock(mob) });
}

/// Insert an NPC — takes an already-constructed Arc<NpcEntity>.
pub fn map_addiddb_npc(id: u32, entity: Arc<NpcEntity>) {
    npc_map().insert(id, entity);
}

/// Insert a floor item — takes ownership of the Box, wrapping it in Arc<RwLock>.
pub fn map_addiddb_item(id: u32, item: Box<FloorItemData>) {
    item_map().insert(id, unsafe { box_into_arc_rwlock(item) });
}

/// Convert `Box<T>` into `Arc<RwLock<T>>` without stack-allocating T.
///
/// # Safety
/// Relies on `parking_lot::RawRwLock::INIT` being all-zero bits.
unsafe fn box_into_arc_rwlock<T>(b: Box<T>) -> Arc<RwLock<T>> {
    let src: *mut T = Box::into_raw(b);
    let rwlock_box: Box<RwLock<T>> = Box::new_zeroed().assume_init();
    std::ptr::copy_nonoverlapping(src, rwlock_box.data_ptr(), 1);
    std::alloc::dealloc(src as *mut u8, std::alloc::Layout::for_value(&*src));
    Arc::from(rwlock_box)
}

// ── Removal ──────────────────────────────────────────────────────────────

/// Remove an entity from the typed maps by ID.
pub fn map_deliddb(id: u32) {
    if id == 0 {
        return;
    }
    if id < MOB_START_NUM {
        purge_player(id);
        player_map().remove(&id);
    } else if id >= NPC_START_NUM {
        npc_map().remove(&id);
    } else if id >= FLOORITEM_START_NUM {
        item_map().remove(&id);
    } else {
        mob_map().remove(&id);
    }
}

/// Remove a mob from MOB_MAP (called from free_onetime).
pub fn mob_map_remove(id: u32) {
    mob_map().remove(&id);
}

// ── Lookups ──────────────────────────────────────────────────────────────

#[must_use]
#[inline]
pub fn find_player_by_id(id: u32) -> Option<Arc<PlayerEntity>> {
    player_map().get(&id).cloned()
}

#[must_use]
#[inline]
pub fn find_mob_by_id(id: u32) -> Option<Arc<RwLock<MobSpawnData>>> {
    mob_map().get(&id).cloned()
}

#[must_use]
#[inline]
pub fn find_npc_by_id(id: u32) -> Option<Arc<NpcEntity>> {
    npc_map().get(&id).cloned()
}

#[must_use]
#[inline]
pub fn find_item_by_id(id: u32) -> Option<Arc<RwLock<FloorItemData>>> {
    item_map().get(&id).cloned()
}

// TODO: phase out — use find_player_by_id
pub fn map_id2sd_pc(id: u32) -> Option<Arc<PlayerEntity>> {
    find_player_by_id(id)
}

// TODO: phase out — use find_mob_by_id
pub fn map_id2mob_ref(id: u32) -> Option<Arc<RwLock<MobSpawnData>>> {
    find_mob_by_id(id)
}

// TODO: phase out — use find_npc_by_id
pub fn map_id2npc_ref(id: u32) -> Option<Arc<NpcEntity>> {
    find_npc_by_id(id)
}

// TODO: phase out — use find_item_by_id
pub fn map_id2fl_ref(id: u32) -> Option<Arc<RwLock<FloorItemData>>> {
    find_item_by_id(id)
}

/// Polymorphic entity reference — used by code that handles any entity type.
pub enum GameEntity {
    Player(Arc<PlayerEntity>),
    Mob(Arc<RwLock<MobSpawnData>>),
    Npc(Arc<NpcEntity>),
    Item(Arc<RwLock<FloorItemData>>),
}

/// Extension trait for ergonomic entity access on `Option<Arc<RwLock<T>>>`.
pub trait EntityLock<T> {
    fn with_mut<R, F: FnOnce(&mut T) -> R>(&self, f: F) -> Option<R>;
    fn with_ref<R, F: FnOnce(&T) -> R>(&self, f: F) -> Option<R>;
}

impl<T> EntityLock<T> for Option<Arc<RwLock<T>>> {
    fn with_mut<R, F: FnOnce(&mut T) -> R>(&self, f: F) -> Option<R> {
        self.as_ref().map(|arc| f(&mut *arc.write()))
    }
    fn with_ref<R, F: FnOnce(&T) -> R>(&self, f: F) -> Option<R> {
        self.as_ref().map(|arc| f(&*arc.read()))
    }
}

/// Look up any entity by id, dispatching by id range.
#[must_use]
pub fn map_id2entity(id: u32) -> Option<GameEntity> {
    if id < MOB_START_NUM {
        return map_id2sd_pc(id).map(GameEntity::Player);
    }
    if id >= NPC_START_NUM {
        return map_id2npc_ref(id).map(GameEntity::Npc);
    }
    if id >= FLOORITEM_START_NUM {
        return map_id2fl_ref(id).map(GameEntity::Item);
    }
    map_id2mob_ref(id).map(GameEntity::Mob)
}

// TODO: dead code — replace callers with find_item_by_id
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn map_id2fl(id: u32) -> *mut std::ffi::c_void {
    map_id2fl_ref(id)
        .map(|arc| {
            &*arc.read() as *const FloorItemData as *mut FloorItemData as *mut std::ffi::c_void
        })
        .unwrap_or(std::ptr::null_mut())
}

// ── Iteration ────────────────────────────────────────────────────────────

/// Iterate all online players safely. Collects Arc handles under the lock,
/// then releases the lock and iterates.
pub fn for_each_player<F: FnMut(&mut MapSessionData)>(mut f: F) {
    let arcs: Vec<Arc<PlayerEntity>> = { player_map().values().map(Arc::clone).collect() };
    for arc in arcs {
        let mut guard = arc.write();
        f(&mut guard);
    }
}

// ── Position lookup ──────────────────────────────────────────────────────

/// Return position and entity type for any entity ID, using typed lookups.
pub fn entity_position(id: u32) -> Option<(Point, u8)> {
    if id < MOB_START_NUM {
        if let Some(arc) = find_player_by_id(id) {
            let sd = arc.read();
            return Some((Point::new(sd.m, sd.x, sd.y), sd.bl_type));
        }
    } else if id >= NPC_START_NUM {
        if let Some(arc) = find_npc_by_id(id) {
            let nd = arc.read();
            return Some((Point::new(nd.m, nd.x, nd.y), nd.bl_type));
        }
    } else if id >= FLOORITEM_START_NUM {
        if let Some(arc) = find_item_by_id(id) {
            let fi = arc.read();
            return Some((Point::new(fi.m, fi.x, fi.y), fi.bl_type));
        }
    } else {
        if let Some(arc) = find_mob_by_id(id) {
            let mob = arc.read();
            return Some((Point::new(mob.m, mob.x, mob.y), mob.bl_type));
        }
    }
    None
}

// ── Name-based lookups ───────────────────────────────────────────────────

/// Find an NPC by display name (case-insensitive).
pub fn find_npc_by_display_name(name: &str) -> Option<Arc<NpcEntity>> {
    let max_npc_id = NPC_ID.load(Ordering::Relaxed);
    for id in NPC_START_NUM..=max_npc_id {
        if let Some(arc) = find_npc_by_id(id) {
            if arc.npc_name.eq_ignore_ascii_case(name) {
                return Some(arc);
            }
        }
    }
    None
}

// TODO: dead code — replace callers with find_npc_by_display_name
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn map_name2npc(name: *const i8) -> *mut std::ffi::c_void {
    use crate::game::npc::NpcData;
    if name.is_null() {
        return std::ptr::null_mut();
    }
    let max_npc_id = NPC_ID.load(Ordering::Relaxed);
    for id in NPC_START_NUM..=max_npc_id {
        if let Some(arc) = find_npc_by_id(id) {
            let nd = arc.read();
            if libc::strcasecmp(nd.npc_name.as_ptr(), name) == 0 {
                return &*nd as *const NpcData as *mut NpcData as *mut std::ffi::c_void;
            }
        }
    }
    std::ptr::null_mut()
}

/// Find a player session by name (case-insensitive).
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn map_name2sd(name: *const i8) -> *mut MapSessionData {
    if name.is_null() {
        return std::ptr::null_mut();
    }
    let target = std::ffi::CStr::from_ptr(name).to_string_lossy();
    for i in 0..get_fd_max() {
        let fd = SessionId::from_raw(i);
        if !session_exists(fd) {
            continue;
        }
        if session_get_eof(fd) != 0 {
            continue;
        }
        let sd = match session_get_data(fd) {
            Some(a) => a,
            None => continue,
        };
        if sd.read().player.identity.name.eq_ignore_ascii_case(&target) {
            return &mut *sd.write() as *mut MapSessionData;
        }
    }
    std::ptr::null_mut()
}
