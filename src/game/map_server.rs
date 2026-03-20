//! Map server utility functions.
use crate::common::traits::LegacyEntity;
use crate::database::get_pool;
use crate::database::{blocking_run_async, boards as db_boards};
use crate::game::lua::coroutine::purge_player;
use crate::game::lua::dispatch::dispatch;
use crate::game::mob::MobSpawnData;
use crate::game::npc::{NpcEntity, NPC_ID, NPC_START_NUM};
use crate::game::pc::{MapSessionData, PlayerEntity, FLAG_MAIL, U_FLAG_UNPHYSICAL};
use crate::game::scripting::types::floor::FloorItemData;
use crate::servers::map::packet::ClientPacket;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::game::map_parse::packet::{rfifob, rfifop, wfifohead, wfifop, wfifoset};

use crate::core::request_shutdown;
use crate::database::board_db;
use crate::game::map_char::intif_save_impl::sl_intif_save;
use crate::game::scripting::sl_exec;
use crate::network::crypt::encrypt;
use crate::session::{get_session_manager, session_call_parse};
use crate::session::{
    session_exists, session_get_client_ip, session_get_data, session_get_eof, session_set_eof,
    SessionId,
};

// ---------------------------------------------------------------------------
// In-game time globals.
// ---------------------------------------------------------------------------

/// Current in-game hour (0–23).  Incremented by `change_time_char` every game hour.
pub static cur_time: AtomicI32 = AtomicI32::new(0);

/// Current in-game day within the current season (1–91).
pub static cur_day: AtomicI32 = AtomicI32::new(0);

/// Current in-game season (1–4).
pub static cur_season: AtomicI32 = AtomicI32::new(0);

/// Current in-game year.
pub static cur_year: AtomicI32 = AtomicI32::new(0);

/// Previous in-game hour; used by `map_weather` to detect hour transitions.
pub static old_time: AtomicI32 = AtomicI32::new(0);

// ---------------------------------------------------------------------------
// Network / session globals.
// ---------------------------------------------------------------------------

/// File descriptor for the char-server connection.
pub static char_fd: AtomicI32 = AtomicI32::new(0);

/// File descriptor for the map network socket (map listen port).
pub static map_fd: AtomicI32 = AtomicI32::new(0);

/// Online user list (count + per-slot char-id array).
pub struct UserlistData {
    pub user_count: u32,
    pub user: [u32; 10000],
}

static USERLIST: OnceLock<Mutex<UserlistData>> = OnceLock::new();

#[inline]
pub fn userlist() -> std::sync::MutexGuard<'static, UserlistData> {
    USERLIST
        .get_or_init(|| {
            Mutex::new(UserlistData {
                user_count: 0,
                user: [0u32; 10000],
            })
        })
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Authentication-attempt counter.
pub static auth_n: AtomicI32 = AtomicI32::new(0);

// ---------------------------------------------------------------------------
// Floor item ID pool.
// ---------------------------------------------------------------------------

/// Upper bound on simultaneously active floor items.
const MAX_FLOORITEM: usize = 100_000_000;

/// Bitmap tracking which floor item slots are in use (1 = occupied, 0 = free).
/// Grown on demand by `map_additem`; cleared by `map_clritem`.
/// Mutex is only for the `Sync` bound required by `OnceLock`; the game loop is single-threaded
/// and never actually contends.
static OBJECT_SLOTS: OnceLock<Mutex<Vec<u8>>> = OnceLock::new();

#[inline]
fn object_slots() -> std::sync::MutexGuard<'static, Vec<u8>> {
    OBJECT_SLOTS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Free all floor item ID slots.
pub fn map_clritem() {
    object_slots().clear();
}

/// Remove a floor item from the world by its ID.
///
/// Unlinks from the block grid, then removes the Arc from `ITEM_MAP`.
/// The block-grid unlink must come first: `map_delblock` needs a valid pointer.
/// With Arc<RwLock<T>>, removing from the map just drops one reference count —
/// the data stays alive until all Arc holders are dropped.
///
/// # Safety
/// `id` must be a valid floor item ID currently registered in `ITEM_MAP`.
/// Must be called on the game thread (single-threaded game loop).
pub unsafe fn map_delitem(id: u32) {
    use crate::game::block::map_delblock_id;

    // Clone the Arc (releases item_map Mutex), then read-lock for the bl.m.
    let Some(arc) = map_id2fl_ref(id) else { return };
    let m = arc.read().m;

    // Unlink from block grid.
    map_delblock_id(id, m);

    // Remove from ITEM_MAP — drops one Arc reference.
    item_map().remove(&id);

    let idx = id.wrapping_sub(crate::game::mob::FLOORITEM_START_NUM) as usize;
    let mut slots = object_slots();
    if idx < slots.len() {
        slots[idx] = 0;
    }
}

/// Assign an ID to a new floor item and insert it into the world.
///
/// Scans the bitmap for the first free slot, grows the bitmap if necessary,
/// assigns the item's ID, takes ownership of the `Box<FloorItemData>` via `Box::from_raw`,
/// registers it in `ITEM_MAP`, and links it into the block grid.
///
/// # Safety
/// - `fl` must be a valid non-null pointer to a `FloorItemData`,
///   allocated via `Box` (i.e., `Box::into_raw`), with `m`/`x`/`y` already set.
/// - Caller must not use `fl` after this call — ownership transfers to `ITEM_MAP`.
/// - Must be called on the game thread (single-threaded game loop).
pub unsafe fn map_additem(fl: *mut crate::game::scripting::types::floor::FloorItemData) {
    let mut slots = object_slots();

    // Find first free slot.
    let i = slots.iter().position(|&b| b == 0).unwrap_or(slots.len());

    if i >= MAX_FLOORITEM {
        tracing::error!("map_additem: floor item capacity exceeded ({MAX_FLOORITEM})");
        // Drop the Box to avoid leaking the allocation — caller transferred
        // ownership to us via Box::into_raw, so we must reclaim it on all paths.
        unsafe {
            drop(Box::from_raw(fl));
        }
        return;
    }

    // Grow bitmap if the free slot is at or beyond the current length.
    if i >= slots.len() {
        let new_n = i + 256;
        slots.resize(new_n, 0);
    }

    slots[i] = 1;
    drop(slots); // release lock before calling map_addiddb_item / map_addblock

    let id = (i as u32).wrapping_add(crate::game::mob::FLOORITEM_START_NUM);
    (*fl).id = id;
    (*fl).bl_type = crate::game::mob::BL_ITEM as u8;
    // Take ownership of the Box<FloorItemData> into ITEM_MAP.
    let item_box = Box::from_raw(fl);
    map_addiddb_item(id, item_box);
    // After map_addiddb_item, the data lives inside Arc<RwLock<FloorItemData>>.
    // Read the Arc to get coordinates for the grid insert.
    let arc_clone = map_id2fl_ref(id).expect("just inserted");
    let fi = arc_clone.read();
    crate::game::block::map_addblock_id(fi.id, fi.bl_type, fi.m, fi.x, fi.y);
}

/// Set the IP address and port for a map slot.
///
/// Returns 0 on success, 1 if `id` is out of range.
///
///
/// # Safety
/// `crate::database::map_db::raw_map_ptr()` must be a valid initialized pointer (non-null, pointing to at
/// least `MAP_SLOTS` slots). Call only after `map_init` has completed.
pub unsafe fn map_setmapip(id: i32, ip: u32, port: u16) -> i32 {
    if id < 0 || id as usize >= crate::database::map_db::MAP_SLOTS {
        return 1;
    }
    (*crate::database::map_db::raw_map_ptr().add(id as usize)).ip = ip;
    (*crate::database::map_db::raw_map_ptr().add(id as usize)).port = port;
    0
}

// ---------------------------------------------------------------------------
// ID database — entity lookup by ID and name
// ---------------------------------------------------------------------------

// ── Typed entity storage ──────────────────────────────────────────────────
// Each map owns its entities via Arc<RwLock<T>>. The Arc allows callers to
// hold a reference-counted handle without lifetime issues; the RwLock
// provides runtime borrow-checking (read/write locking). Lookup functions
// return Arc<RwLock<T>> — callers acquire read or write guards as needed.
//
// The game loop is single-threaded; the outer Mutex satisfies
// OnceLock<T>: Sync but never actually contends. The RwLock on each entity
// similarly won't contend in practice, but structurally enforces correct
// access patterns and is ready for future multi-threading.

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
fn item_map() -> std::sync::MutexGuard<'static, HashMap<u32, Arc<RwLock<FloorItemData>>>> {
    ITEM_MAP
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Iterate all online players safely. Collects Arc handles under the lock,
/// then releases the lock and iterates — eliminating any risk of dangling
/// pointers if a player is removed mid-iteration (Arc keeps data alive).
pub fn for_each_player<F: FnMut(&mut crate::game::pc::MapSessionData)>(mut f: F) {
    let arcs: Vec<Arc<PlayerEntity>> = { player_map().values().map(Arc::clone).collect() };
    for arc in arcs {
        let mut guard = arc.write();
        f(&mut guard);
    }
}

pub fn map_initiddb() {}
pub fn map_termiddb() {
    player_map().clear();
    mob_map().clear();
    npc_map().clear();
    item_map().clear();
}

/// Return position and entity type for any entity ID, using typed lookups.
pub fn entity_position(id: u32) -> Option<(crate::common::types::Point, u8)> {
    use crate::common::types::Point;
    use crate::game::mob::{FLOORITEM_START_NUM, MOB_START_NUM, NPC_START_NUM};
    if id < MOB_START_NUM {
        if let Some(arc) = map_id2sd_pc(id) {
            let sd = arc.read();
            return Some((Point::new(sd.m, sd.x, sd.y), sd.bl_type));
        }
    } else if id >= NPC_START_NUM {
        if let Some(arc) = map_id2npc_ref(id) {
            let nd = arc.read();
            return Some((Point::new(nd.m, nd.x, nd.y), nd.bl_type));
        }
    } else if id >= FLOORITEM_START_NUM {
        if let Some(arc) = map_id2fl_ref(id) {
            let fi = arc.read();
            return Some((Point::new(fi.m, fi.x, fi.y), fi.bl_type));
        }
    } else {
        if let Some(arc) = map_id2mob_ref(id) {
            let mob = arc.read();
            return Some((Point::new(mob.m, mob.x, mob.y), mob.bl_type));
        }
    }
    None
}

/// Convert `Box<T>` into `Arc<RwLock<T>>` without stack-allocating T.
///
/// `parking_lot::RawRwLock::INIT` is all-zero bits, so a zeroed
/// `Box<RwLock<T>>` is a valid unlocked lock.  We then copy T directly
/// from the source Box into the RwLock's data slot and convert to Arc.
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

/// Insert a player — takes ownership of the Box, wrapping it in Arc<RwLock>.
pub fn map_addiddb_player(
    id: u32,
    fd: crate::session::SessionId,
    sd: Box<crate::game::pc::MapSessionData>,
) {
    let name = sd.player.identity.name.clone();
    let gm_level = sd.player.identity.gm_level;
    let arc = Arc::from(PlayerEntity::new(id, fd, name, gm_level, sd));
    player_map().insert(id, arc);
}

/// Insert a mob — takes ownership of the Box, wrapping it in Arc<RwLock>.
pub fn map_addiddb_mob(id: u32, mob: Box<crate::game::mob::MobSpawnData>) {
    mob_map().insert(id, unsafe { box_into_arc_rwlock(mob) });
}

/// Insert an NPC — takes ownership of the Box, wrapping it in Arc<NpcEntity>.
pub fn map_addiddb_npc(id: u32, entity: Arc<NpcEntity>) {
    npc_map().insert(id, entity);
}

/// Insert a floor item — takes ownership of the Box, wrapping it in Arc<RwLock>.
pub fn map_addiddb_item(id: u32, item: Box<crate::game::scripting::types::floor::FloorItemData>) {
    item_map().insert(id, unsafe { box_into_arc_rwlock(item) });
}

/// Remove an entity from the typed maps by ID.
///
/// With Arc<RwLock<T>>, this is safe — removing from the map drops one Arc
/// reference, but any existing Arc holders keep the data alive until they
/// are also dropped.
pub fn map_deliddb(id: u32) {
    use crate::game::mob::{FLOORITEM_START_NUM, MOB_START_NUM, NPC_START_NUM};
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

// ── Arc<RwLock<T>> entity lookups ──────────────────────────────────────────
// These functions return Arc<RwLock<T>> — callers acquire read or write
// guards as needed via .read() / .write().
// The Arc handle can be held independently of the HashMap lock, so callers
// can safely call back into game code without holding the entity map lock.
// ────────────────────────────────────────────────────────────────────────────

/// Typed lookup — returns `Arc<PlayerEntity>` if id is a player.
#[must_use]
#[inline]
pub fn find_player_by_id(id: u32) -> Option<Arc<PlayerEntity>> {
    player_map().get(&id).cloned()
}

pub fn find_mob_by_id(id: u32) -> Option<Arc<RwLock<MobSpawnData>>> {
    mob_map().get(&id).cloned()
}

pub fn find_npc_by_id(id: u32) -> Option<Arc<NpcEntity>> {
    npc_map().get(&id).cloned()
}

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
///
/// Each variant holds an `Arc<RwLock<T>>` handle. Callers use `with_bl_mut`
/// (callback pattern) since a reference through a lock guard cannot be returned.
pub enum GameEntity {
    Player(Arc<PlayerEntity>),
    Mob(Arc<RwLock<MobSpawnData>>),
    Npc(Arc<NpcEntity>),
    Item(Arc<RwLock<FloorItemData>>),
}

/// Extension trait for ergonomic entity access on `Option<Arc<RwLock<T>>>`.
///
/// Reduces boilerplate at call sites by combining the Option unwrap and lock
/// acquisition into a single method call with a callback.
pub trait EntityLock<T> {
    /// Acquire a write lock and call `f` with `&mut T`. Returns `None` if
    /// the Option is `None`.
    fn with_mut<R, F: FnOnce(&mut T) -> R>(&self, f: F) -> Option<R>;
    /// Acquire a read lock and call `f` with `&T`. Returns `None` if
    /// the Option is `None`.
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
/// Returns None if the id is not registered in any typed map.
#[must_use]
pub fn map_id2entity(id: u32) -> Option<GameEntity> {
    use crate::game::mob::{FLOORITEM_START_NUM, MOB_START_NUM, NPC_START_NUM};
    if id < MOB_START_NUM {
        return map_id2sd_pc(id).map(GameEntity::Player);
    }
    if id >= NPC_START_NUM {
        return map_id2npc_ref(id).map(GameEntity::Npc);
    }
    // id is in [MOB_START_NUM, NPC_START_NUM) — either mob or floor item
    if id >= FLOORITEM_START_NUM {
        return map_id2fl_ref(id).map(GameEntity::Item);
    }
    map_id2mob_ref(id).map(GameEntity::Mob)
}

/// Floor item lookup — returns raw pointer via read lock on the `Arc<RwLock<FloorItemData>>`.
/// Prefer `map_id2fl_ref` at new call sites.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn map_id2fl(id: u32) -> *mut std::ffi::c_void {
    map_id2fl_ref(id)
        .map(|arc| {
            &*arc.read() as *const crate::game::scripting::types::floor::FloorItemData
                as *mut crate::game::scripting::types::floor::FloorItemData
                as *mut std::ffi::c_void
        })
        .unwrap_or(std::ptr::null_mut())
}

/// Remove a mob from MOB_MAP (called from free_onetime).
pub fn mob_map_remove(id: u32) {
    mob_map().remove(&id);
}

/// Find a player session by name (case-insensitive).
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn map_name2sd(name: *const i8) -> *mut MapSessionData {
    use crate::session::{session_exists, session_get_data, session_get_eof, SessionId};
    if name.is_null() {
        return std::ptr::null_mut();
    }
    let target = std::ffi::CStr::from_ptr(name).to_string_lossy();
    for i in 0..crate::session::get_fd_max() {
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

/// Find an NPC by name (case-insensitive). Iterates NPC ID range.
///
/// Find an NPC by display name (case-insensitive).
pub fn find_npc_by_display_name(name: &str) -> Option<Arc<NpcEntity>> {
    let max_npc_id = NPC_ID.load(Ordering::Relaxed);
    for id in NPC_START_NUM..=max_npc_id {
        if let Some(arc) = map_id2npc_ref(id) {
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
    if name.is_null() {
        return std::ptr::null_mut();
    }
    let max_npc_id = NPC_ID.load(Ordering::Relaxed);
    for id in NPC_START_NUM..=max_npc_id {
        if let Some(arc) = map_id2npc_ref(id) {
            let nd = arc.read();
            if libc::strcasecmp(nd.npc_name.as_ptr(), name) == 0 {
                return &*nd as *const crate::game::npc::NpcData as *mut crate::game::npc::NpcData
                    as *mut std::ffi::c_void;
            }
        }
    }
    std::ptr::null_mut()
}

/// Reload the map registry for a single map.
///
/// Loads the global player registry from the database.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn map_loadregistry(id: i32) -> i32 {
    crate::database::map_db::map_loadregistry(id)
}

/// Read a game-global registry value by name (case-insensitive).
pub fn map_readglobalgamereg(reg: &str) -> i32 {
    let gr = gamereg();
    if gr.registry.is_null() {
        return 0;
    }
    let ckey = std::ffi::CString::new(reg).unwrap_or_default();
    for i in 0..gr.registry_num as usize {
        let entry = unsafe { &*gr.registry.add(i) };
        if unsafe { reg_str_eq(&entry.str, ckey.as_ptr()) } {
            return entry.val;
        }
    }
    0
}

/// Game loop callback — runs Lua cron hooks based on wall-clock seconds.
///
/// Called every 1000 ms from the Tokio select! loop.
/// Must be called on the Lua-owning thread (LocalSet).
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn map_cronjob() {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    if t.is_multiple_of(60) {
        cron("cronJobMin");
    }
    if t.is_multiple_of(300) {
        cron("cronJob5Min");
    }
    if t.is_multiple_of(1800) {
        cron("cronJob30Min");
    }
    if t.is_multiple_of(3600) {
        cron("cronJobHour");
    }
    if t.is_multiple_of(86400) {
        cron("cronJobDay");
    }
    cron("cronJobSec");
}

/// Dispatch a Lua event with a single entity ID argument.
fn sl_doscript_simple(root: &str, method: Option<&str>, id: u32) -> bool {
    dispatch(root, method, &[id])
}

#[inline]
fn cron(name: &str) {
    dispatch(name, None, &[]);
}

// ---------------------------------------------------------------------------
// Session state helpers
// ---------------------------------------------------------------------------

/// Returns 1 if `sd` is non-null and has an active session.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn isPlayerActive(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() {
        return 0;
    }
    let fd = (*sd).fd;
    if fd.raw() == 0 {
        return 0;
    }
    if !session_exists(fd) {
        tracing::warn!(
            "[map] isPlayerActive: player exists but session does not ({})",
            (*sd).player.identity.name
        );
        return 0;
    }
    1
}

/// Returns 1 if `sd` has a live session with no EOF flag set.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn isActive(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() {
        return 0;
    }
    let fd = (*sd).fd;
    if !session_exists(fd) {
        return 0;
    }
    if session_get_eof(fd) != 0 {
        return 0;
    }
    1
}

// ---------------------------------------------------------------------------
// Online status
// ---------------------------------------------------------------------------

/// Updates `Character.ChaOnline`/`ChaLastIP`.
///
/// Returns `true` if the login Lua hook should be fired (character exists and val != 0).
/// The caller is responsible for firing the hook AFTER this future completes so that
/// any DB calls inside the hook do not re-enter DB_RUNTIME (which would deadlock).
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn mmo_setonline(id: u32, val: i32) -> bool {
    // Extract all data from raw pointers BEFORE any .await so no raw pointers
    // cross yield points (required for the future to be Send).
    let addr = {
        let Some(arc) = map_id2sd_pc(id) else {
            return false;
        };
        let sd = arc.read();
        let fd = sd.fd;
        // session_get_client_ip returns IP in network byte order (sin_addr.s_addr).
        let raw_ip = session_get_client_ip(fd);
        format!(
            "{}.{}.{}.{}",
            raw_ip & 0xff,
            (raw_ip >> 8) & 0xff,
            (raw_ip >> 16) & 0xff,
            (raw_ip >> 24) & 0xff,
        )
    };

    // Check character exists.
    let pool = get_pool();
    let exists: bool =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM `Character` WHERE `ChaId` = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap_or(0)
            > 0;

    // Update online status + last IP regardless of whether character was found in SELECT.
    let pool = get_pool();
    let _ =
        sqlx::query("UPDATE `Character` SET `ChaOnline` = ?, `ChaLastIP` = ? WHERE `ChaId` = ?")
            .bind(val)
            .bind(&addr)
            .bind(id)
            .execute(pool)
            .await;

    exists && val != 0
}

// ---------------------------------------------------------------------------
// Block grid helpers — map_canmove, map_addmob
// ---------------------------------------------------------------------------

/// Returns 1 if the cell `(x, y)` on map `m` is passable, 0 otherwise.
///
/// The `pass` tile array stores the char-ID of the player occupying each cell
/// (non-zero means occupied). A cell with a player is treated as blocked unless
/// that player has `uFlag_unphysical` set.
///
///
/// # Safety
/// `m` must be a valid loaded map index. `x` and `y` must be within bounds.
pub unsafe fn map_canmove(m: i32, x: i32, y: i32) -> i32 {
    // read_pass(m, x, y) expands to map[m].pass[x + y * map[m].xs]
    let slot = &*crate::database::map_db::raw_map_ptr().add(m as usize);
    let pass_val = *slot.pass.add(x as usize + y as usize * slot.xs as usize);

    if pass_val != 0 {
        // A player ID is stored in the pass cell. Look them up.
        let blocked = match map_id2sd_pc(pass_val as u32) {
            None => true, // not in player map → treat as blocking
            Some(arc) => (arc.read().uFlags & U_FLAG_UNPHYSICAL) == 0,
        };
        if blocked {
            // Cell is occupied by a physical player — blocked.
            return 1;
        }
    }

    0
}

/// Insert a new mob spawn record for the map/position of `sd` into the
/// `Spawns<serverid>` DB table.
///
///
/// # Safety
/// `sd` must be a valid, non-null `MapSessionData` pointer.
pub async unsafe fn map_addmob(
    sd: *mut MapSessionData,
    id: u32,
    start: i32,
    end: i32,
    replace: u32,
) -> i32 {
    let m = (*sd).m as i32;
    let x = (*sd).x as i32;
    let y = (*sd).y as i32;
    let sid = crate::config::config().server_id;

    let sql = format!(
        "INSERT INTO `Spawns{sid}` \
         (`SpnMapId`, `SpnX`, `SpnY`, `SpnMobId`, `SpnLastDeath`, \
          `SpnStartTime`, `SpnEndTime`, `SpnMobIdReplace`) \
         VALUES(?, ?, ?, ?, 0, ?, ?, ?)"
    );

    let _ = sqlx::query(&sql)
        .bind(m)
        .bind(x)
        .bind(y)
        .bind(id)
        .bind(start)
        .bind(end)
        .bind(replace)
        .execute(get_pool())
        .await;

    0
}

// ---------------------------------------------------------------------------
// Board / N-Mail packet constants
// ---------------------------------------------------------------------------

use crate::common::constants::world::{BOARD_CAN_DEL, BOARD_CAN_WRITE};

// ---------------------------------------------------------------------------
// nmail_sendmessage — sends a notification message packet to the player's fd.
//
// Packet layout (pre-encryption):
//   [0]     = 0xAA  (magic)
//   [1..2]  = SWAP16(len+5)  (big-endian total payload len)
//   [3]     = 0x31  (packet id)
//   [4]     = 0x03  (sub-id)
//   [5]     = other (byte)
//   [6]     = type  (byte)
//   [7]     = strlen(message)  (byte)
//   [8..]   = message (null-terminated)
//   [len+7] = 0x07  (terminator)
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn nmail_sendmessage(
    sd: *mut MapSessionData,
    message: *const i8,
    other: i32,
    r#type: i32,
) -> i32 {
    if isPlayerActive(sd) == 0 {
        return 0;
    }

    let fd = (*sd).fd;
    if !session_exists(fd) {
        return 0;
    }

    let msg_len = libc_strlen(message);

    wfifohead(fd, 65535 + 3);
    let p0 = wfifop(fd, 0);
    if p0.is_null() {
        return 0;
    }

    *p0 = 0xAA_u8;
    *wfifop(fd, 3) = 0x31_u8;
    *wfifop(fd, 4) = 0x03_u8;
    *wfifop(fd, 5) = other as u8;
    *wfifop(fd, 6) = r#type as u8;
    *wfifop(fd, 7) = msg_len as u8;
    // copy message bytes (replicating C strcpy, without the null — it is overwritten by the sentinel).
    // C does: len = strlen(message); len++ — effective length is N+1.
    std::ptr::copy_nonoverlapping(message as *const u8, wfifop(fd, 8), msg_len);
    *wfifop(fd, msg_len + 8) = 0x07_u8; // 0x07 sentinel at [8+N] (matches C: strcpy null is overwritten)
                                        // big-endian packet length field at [1..2]: (N+1) + 5 = N + 6
    let size_be = ((msg_len as u16) + 6).to_be();
    (wfifop(fd, 1) as *mut u16).write_unaligned(size_be);

    let enc_len = encrypt(fd) as usize;
    wfifoset(fd, enc_len);
    0
}

// ---------------------------------------------------------------------------
// boards_delete — forwards delete request to char-server (packet 0x3008).
//
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn boards_delete(sd: *mut MapSessionData, board: i32) -> i32 {
    if sd.is_null() {
        return 0;
    }

    // Read the post id from the player's recv buffer (big-endian u16 at offset 8).
    let post = {
        let p = rfifop((*sd).fd, 8) as *const u16;
        if p.is_null() {
            return 0;
        }
        u16::from_be(p.read_unaligned()) as i32
    };

    let name = (*sd).player.identity.name.clone();
    let gm_level = (*sd).player.identity.gm_level as u16;
    let can_delete = (*sd).board_candel as u16;

    let result = blocking_run_async(async move {
        db_boards::delete_post(
            get_pool(),
            board as u16,
            post as u16,
            &name,
            gm_level,
            can_delete,
        )
        .await
    });

    // Format client response (matches 0x3808 handler).
    // other=7 tells the client to refresh the board after delete.
    let (msg, r#type) = match result {
        0 => (c"The message has been deleted.", 1),
        1 => (c"You can only delete your own messages.", 0),
        _ => (c"Something went wrong. Please try again later.", 0),
    };
    nmail_sendmessage(sd, msg.as_ptr(), 7, r#type);
    0
}

// ---------------------------------------------------------------------------
// boards_showposts — sets board flags on `sd`, then forwards to char-server.
//
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn boards_showposts(sd: *mut MapSessionData, board: i32) -> i32 {
    if sd.is_null() {
        return 0;
    }

    (*sd).board_canwrite = 0;
    (*sd).board_candel = 0;
    (*sd).boardnameval = 0;

    if board == 0 {
        // Board 0 == NMail — always writable/deletable
        (*sd).board_canwrite = 1;
        (*sd).board_candel = 1;
    } else {
        (*sd).board = board;
        let bd = &*board_db::search(board);
        if bd.script != 0 {
            let yname = crate::game::scripting::carray_to_str(&bd.yname);
            sl_doscript_simple(yname, Some("check"), (*sd).id);
        } else {
            (*sd).board_canwrite = 1;
        }
        if (*sd).player.identity.gm_level == 99 {
            (*sd).board_canwrite = 1;
            (*sd).board_candel = 1;
        }
    }

    let mut flags: i32 = 0;
    if (*sd).board_canwrite != 0 {
        if (*sd).board_canwrite == 6 {
            flags = 6; // special write flag
        } else {
            flags |= BOARD_CAN_WRITE;
        }
    }
    if (*sd).board_candel != 0 {
        flags |= BOARD_CAN_DEL;
    }

    let fd = (*sd).fd;
    let name = (*sd).player.identity.name.clone();
    let bcount = (*sd).bcount as u32;
    let popup = (*sd).board_popup as u8;

    // Query DB directly instead of sending 0x3009 to char server.
    let rows = blocking_run_async(async move {
        db_boards::list_posts(get_pool(), board as u32, bcount * 20, &name).await
    });

    tracing::debug!(
        "[map] [boards] showposts: board={} flags={} rows={}",
        board,
        flags,
        rows.len()
    );

    // Compute flags1/flags2 (matches char server's 0x3009 handler).
    let flags1: u8 = if popup != 0 && board != 0 {
        if flags == 6 {
            6
        } else if flags & BOARD_CAN_WRITE == 0 {
            0
        } else {
            2
        }
    } else {
        if flags == 6 {
            6
        } else if flags & BOARD_CAN_WRITE == 0 {
            1
        } else {
            3
        }
    };
    let flags2: u8 = if board == 0 { 4 } else { 2 };

    let board_display_name = board_db::board_name(board);

    let mut pkt_out = ClientPacket::board(flags2);
    pkt_out.put_u8(flags1);
    pkt_out.put_u16_be(board as u16);
    pkt_out.put_str(&board_display_name);

    if rows.is_empty() {
        pkt_out.put_u8(0);
    } else {
        pkt_out.put_u8(rows.len() as u8);

        for row in &rows {
            pkt_out.put_u8(row.color as u8);
            pkt_out.put_u16_be(row.post_id as u16);

            // Compose user string: "{bn_name} {user}" for boards, just "{user}" for nmail.
            let composed_user = if board != 0 && row.board_name != 0 {
                let bn = board_db::bn_name(row.board_name as i32);
                format!("{} {}", bn, row.user)
            } else {
                row.user.clone()
            };
            pkt_out.put_str(&composed_user);

            pkt_out.put_u8(row.month as u8);
            pkt_out.put_u8(row.day as u8);
            pkt_out.put_str(&row.topic);
        }
    }

    tracing::debug!(
        "[map] [boards] showposts: sending client packet fd={} buf_len={}",
        fd,
        pkt_out.buf.len()
    );
    pkt_out.send(fd);

    // Advance pagination counter.
    (*sd).bcount += 1;
    0
}

// ---------------------------------------------------------------------------
// boards_readpost — sets board flags and forwards read-post request.
//
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn boards_readpost(sd: *mut MapSessionData, board: i32, post: i32) -> i32 {
    if board != 0 {
        (*sd).board = board;
        let bd = &*board_db::search(board);
        if bd.script != 0 {
            let yname = crate::game::scripting::carray_to_str(&bd.yname);
            sl_doscript_simple(yname, Some("check"), (*sd).id);
        } else {
            (*sd).board_canwrite = 1;
        }
        if (*sd).player.identity.gm_level == 99 {
            (*sd).board_canwrite = 1;
            (*sd).board_candel = 1;
        }
    }

    let mut flags: i32 = 0;
    if (*sd).board_canwrite != 0 {
        flags |= BOARD_CAN_WRITE;
    }
    if (*sd).board_candel != 0 {
        flags |= BOARD_CAN_DEL;
    }

    let fd = (*sd).fd;
    let name = (*sd).player.identity.name.clone();

    // Query DB directly instead of sending 0x300A to char server.
    let content = blocking_run_async(async move {
        db_boards::read_post(get_pool(), board as u32, post as u32, &name).await
    });

    let Some(content) = content else {
        return 0;
    };

    // Mark mail as read and potentially clear mail flag (matches char server 0x300A handler).
    if board == 0 {
        let name2 = (*sd).player.identity.name.clone();
        let post_id = content.post_id;
        let clear_flag = blocking_run_async(async move {
            db_boards::mark_mail_read(get_pool(), post_id, &name2).await
        });
        if clear_flag {
            (*sd).flags &= !FLAG_MAIL;
        }
    }

    // Build client response (matches 0x380F handler in packet.rs).
    // post_type and buttons are computed by the char server's 0x300A handler:
    let post_type: u8 = if board == 0 { 5 } else { 3 };
    let buttons: u8 = if board == 0 || (flags & BOARD_CAN_WRITE) != 0 {
        3
    } else {
        1
    };

    let mut pkt_out = ClientPacket::board(post_type);
    // Overwrite [4] — C code does NOT set [4]=3 for readpost.
    pkt_out.buf[4] = 0;
    pkt_out.put_u8(buttons);
    pkt_out.put_u8(if board == 0 { 1 } else { 0 });
    pkt_out.put_u16_be(content.post_id as u16);
    pkt_out.put_str(&content.user);
    pkt_out.put_u8(content.month as u8);
    pkt_out.put_u8(content.day as u8);
    pkt_out.put_str(&content.topic);
    pkt_out.put_str_u16_be(&content.body);

    tracing::debug!(
        "[map] [boards] readpost: sending client packet fd={} buf_len={}",
        fd,
        pkt_out.buf.len()
    );
    pkt_out.send(fd);
    0
}

// ---------------------------------------------------------------------------
// boards_post — reads post data from the player's recv buffer, validates it,
// and forwards to char-server (packet 0x300C).
//
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn boards_post(sd: *mut MapSessionData, board: i32) -> i32 {
    if sd.is_null() {
        return 0;
    }

    let fd = (*sd).fd;

    let topiclen = rfifob(fd, 8) as usize;
    if topiclen > 52 {
        let mut name_buf = [0u8; 16];
        let name_bytes = (*sd).player.identity.name.as_bytes();
        let n = name_bytes.len().min(15);
        name_buf[..n].copy_from_slice(&name_bytes[..n]);
        clif_Hacker(
            name_buf.as_mut_ptr() as *mut i8,
            c"Board hacking: TOPIC HACK".as_ptr(),
        );
        return 0;
    }

    let postlen = {
        let p = rfifop(fd, topiclen + 9) as *const u16;
        if p.is_null() {
            return 0;
        }
        u16::from_be(p.read_unaligned()) as usize
    };
    if postlen > 4000 {
        let mut name_buf = [0u8; 16];
        let name_bytes = (*sd).player.identity.name.as_bytes();
        let n = name_bytes.len().min(15);
        name_buf[..n].copy_from_slice(&name_bytes[..n]);
        clif_Hacker(
            name_buf.as_mut_ptr() as *mut i8,
            c"Board hacking: POST(BODY) HACK".as_ptr(),
        );
        return 0;
    }

    if topiclen == 0 {
        nmail_sendmessage(sd, c"Post must contain subject.".as_ptr(), 6, 0);
        return 0;
    }
    if postlen == 0 {
        nmail_sendmessage(sd, c"Post must contain a body.".as_ptr(), 6, 0);
        return 0;
    }

    // Extract topic and post body from the recv buffer.
    let mut topic_buf = [0u8; 53];
    let mut post_buf = [0u8; 4001];
    std::ptr::copy_nonoverlapping(rfifop(fd, 9), topic_buf.as_mut_ptr(), topiclen);
    std::ptr::copy_nonoverlapping(rfifop(fd, topiclen + 11), post_buf.as_mut_ptr(), postlen);

    let name = (*sd).player.identity.name.clone();
    let mut nval = (*sd).boardnameval as i32;
    if (*sd).player.identity.gm_level != 0 {
        nval = 1;
    }

    let topic_str = std::str::from_utf8(&topic_buf[..topiclen])
        .unwrap_or("")
        .to_owned();
    let post_str = std::str::from_utf8(&post_buf[..postlen])
        .unwrap_or("")
        .to_owned();

    // Call DB directly instead of sending 0x300C to char server.
    let result = blocking_run_async(async move {
        db_boards::create_board_post(get_pool(), board as u32, nval, &name, &topic_str, &post_str)
            .await
    });

    // Format client response (matches 0x380B handler).
    let (msg, r#type) = match result {
        0 => (c"Your message has been posted.", 1),
        _ => (c"Something went wrong. Please try again later.", 0),
    };
    nmail_sendmessage(sd, msg.as_ptr(), 6, r#type);
    0
}

// ---------------------------------------------------------------------------
// nmail_read — body is entirely commented out in C; stub that returns 0.
//
// The original SQL implementation was removed long ago (left as commented-out
// code). This function is kept as a noop stub.
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn nmail_read(_sd: *mut MapSessionData, _post: i32) -> i32 {
    0
}

// ---------------------------------------------------------------------------
// nmail_luascript — inserts a Lua-mail record and runs `sl_exec`.
//
// Uses sqlx for database access.
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn nmail_luascript(sd: *mut MapSessionData, to: i32, topic: i32, msg: i32) -> i32 {
    let fd = (*sd).fd;
    let mut message = [0i8; 4000];

    std::ptr::copy_nonoverlapping(
        rfifop(fd, (to + topic + 12) as usize) as *const i8,
        message.as_mut_ptr(),
        msg as usize,
    );

    let cha_name = (*sd).player.identity.name.clone();
    let body = std::ffi::CStr::from_ptr(message.as_ptr())
        .to_str()
        .unwrap_or("")
        .to_owned();

    let ok = sqlx::query(
            "INSERT INTO `Mail` (`MalChaName`, `MalChaNameDestination`, `MalBody`) VALUES (?, 'Lua', ?)"
        )
        .bind(cha_name)
        .bind(body)
        .execute(get_pool())
        .await
        .is_ok();
    if !ok {
        return 0;
    }

    sl_exec(sd, message.as_mut_ptr());
    0
}

// ---------------------------------------------------------------------------
// nmail_poemscript — validates, deduplicates, and inserts a poem board post.
//
// Uses sqlx for database access.
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn nmail_poemscript(
    sd: *mut MapSessionData,
    topic: *const i8,
    message: *const i8,
) -> i32 {
    use chrono::Datelike as _;

    // Use chrono::Local::now() to match C's localtime(&t) behaviour.
    // month0() is 0-based (January = 0), matching C's tm_mon.
    // day()   is 1-based (1..=31),      matching C's tm_mday.
    let now = chrono::Local::now();
    let month = now.month0() as i32;
    let day = now.day() as i32;

    let char_id = (*sd).player.identity.id as i32;

    // Check whether the player already submitted a poem this cycle.
    let already_submitted = sqlx::query_scalar::<_, Option<u32>>(
        "SELECT `BrdId` FROM `Boards` WHERE `BrdBnmId` = '19' AND `BrdChaId` = ? LIMIT 1",
    )
    .bind(char_id)
    .fetch_optional(get_pool())
    .await
    .ok()
    .flatten()
    .is_some();

    if already_submitted {
        nmail_sendmessage(sd, c"You have already submitted a poem.".as_ptr(), 6, 1);
        return 0;
    }

    // topic and message are *const i8 passed by C caller — convert to owned Strings.
    let topic_str = std::ffi::CStr::from_ptr(topic)
        .to_str()
        .unwrap_or("")
        .to_owned();
    let message_str = std::ffi::CStr::from_ptr(message)
        .to_str()
        .unwrap_or("")
        .to_owned();

    // Find the current maximum board position.
    let boardpos: u32 = sqlx::query_scalar::<_, Option<u32>>(
        "SELECT MAX(`BrdPosition`) FROM `Boards` WHERE `BrdBnmId` = '19'",
    )
    .fetch_optional(get_pool())
    .await
    .ok()
    .flatten()
    .flatten()
    .unwrap_or(0);

    let ok = sqlx::query(
            "INSERT INTO `Boards` (`BrdBnmId`, `BrdChaName`, `BrdChaId`, `BrdTopic`, `BrdPost`, `BrdMonth`, `BrdDay`, `BrdPosition`) VALUES ('19', 'Anonymous', ?, ?, ?, ?, ?, ?)"
        )
        .bind(char_id)
        .bind(topic_str)
        .bind(message_str)
        .bind(month)
        .bind(day)
        .bind(boardpos.saturating_add(1))
        .execute(get_pool())
        .await
        .is_ok();
    if !ok {
        return 1;
    }

    nmail_sendmessage(sd, c"Poem submitted.".as_ptr(), 6, 1);
    0
}

// ---------------------------------------------------------------------------
// nmail_sendmailcopy — forwards a copy-to-self mail to the char-server.
//
// Packet 0x300F:
//   [0..1]     = 0x300F
//   [2..3]     = sd->fd
//   [4..19]    = from name (16 bytes)
//   [20..35]   = to_user (16 bytes, C copies up to 16 chars)
//   [72..123]  = topic (52 bytes)
//   [124..4123]= message (4000 bytes)
// Total: 4124 bytes.
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn nmail_sendmailcopy(
    sd: *mut MapSessionData,
    to_user: *const i8,
    topic: *const i8,
    message: *const i8,
) -> i32 {
    if libc_strlen(to_user) > 16 || libc_strlen(topic) > 52 || libc_strlen(message) > 4000 {
        return 0;
    }

    let from = (*sd).player.identity.name.clone();
    let to_str = std::ffi::CStr::from_ptr(to_user)
        .to_string_lossy()
        .into_owned();
    let topic_str = std::ffi::CStr::from_ptr(topic)
        .to_string_lossy()
        .into_owned();
    let msg_str = std::ffi::CStr::from_ptr(message)
        .to_string_lossy()
        .into_owned();

    // Call DB directly instead of sending 0x300F to char server.
    // No client response for copy-to-self mail.
    blocking_run_async(async move {
        let _ = db_boards::nmail_insert(get_pool(), &from, &to_str, &topic_str, &msg_str).await;
    });
    0
}

// ---------------------------------------------------------------------------
// nmail_write — parses incoming mail write packet, dispatches to Lua/poem/mail.
//
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn nmail_write(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() {
        return 0;
    }
    let fd = (*sd).fd;

    let tolen = rfifob(fd, 8) as usize;
    if tolen > 52 {
        let mut name_buf = [0u8; 16];
        let name_bytes = (*sd).player.identity.name.as_bytes();
        let n = name_bytes.len().min(15);
        name_buf[..n].copy_from_slice(&name_bytes[..n]);
        clif_Hacker(name_buf.as_mut_ptr() as *mut i8, c"NMAIL: To User".as_ptr());
        return 0;
    }
    let topiclen = rfifob(fd, tolen + 9) as usize;
    if topiclen > 52 {
        let mut name_buf = [0u8; 16];
        let name_bytes = (*sd).player.identity.name.as_bytes();
        let n = name_bytes.len().min(15);
        name_buf[..n].copy_from_slice(&name_bytes[..n]);
        clif_Hacker(name_buf.as_mut_ptr() as *mut i8, c"NMAIL: Topic".as_ptr());
        return 0;
    }
    let messagelen = {
        let p = rfifop(fd, tolen + topiclen + 10) as *const u16;
        if p.is_null() {
            return 0;
        }
        u16::from_be(p.read_unaligned()) as usize
    };
    if messagelen > 4000 {
        let mut name_buf = [0u8; 16];
        let name_bytes = (*sd).player.identity.name.as_bytes();
        let n = name_bytes.len().min(15);
        name_buf[..n].copy_from_slice(&name_bytes[..n]);
        clif_Hacker(name_buf.as_mut_ptr() as *mut i8, c"NMAIL: Message".as_ptr());
        return 0;
    }

    let mut to_user = [0i8; 52];
    let mut topic = [0i8; 52];
    let mut message = [0i8; 4000];

    std::ptr::copy_nonoverlapping(rfifop(fd, 9) as *const i8, to_user.as_mut_ptr(), tolen);
    std::ptr::copy_nonoverlapping(
        rfifop(fd, tolen + 10) as *const i8,
        topic.as_mut_ptr(),
        topiclen,
    );
    std::ptr::copy_nonoverlapping(
        rfifop(fd, topiclen + tolen + 12) as *const i8,
        message.as_mut_ptr(),
        messagelen,
    );
    let send_copy = rfifob(fd, topiclen + tolen + 12 + messagelen) as i32;

    // Case: "lua" — run Lua script mail
    let to_user_cstr = std::ffi::CStr::from_ptr(to_user.as_ptr());
    let to_user_lower = to_user_cstr.to_string_lossy().to_ascii_lowercase();

    if to_user_lower == "lua" {
        std::ptr::copy_nonoverlapping(
            message.as_ptr(),
            (*sd).mail.as_mut_ptr(),
            messagelen.min((*sd).mail.len()),
        );
        (*sd).luaexec = 0;
        sl_doscript_simple("canRunLuaMail", None, (*sd).id);
        if (*sd).player.identity.gm_level == 99 || (*sd).luaexec != 0 {
            nmail_luascript(sd, tolen as i32, topiclen as i32, messagelen as i32).await;
            nmail_sendmessage(sd, c"LUA script ran!".as_ptr(), 6, 1);
            return 0; // only return if we actually handled the Lua mail
        }
        // permission denied — fall through to poems/standard mail
    }

    // Case: "poems" / "poem"
    if to_user_lower == "poems" || to_user_lower == "poem" {
        if map_readglobalgamereg("poemAccept") == 0 {
            nmail_sendmessage(
                sd,
                c"Currently not accepting poem submissions.".as_ptr(),
                6,
                0,
            );
            return 0;
        }

        std::ptr::copy_nonoverlapping(
            message.as_ptr(),
            (*sd).mail.as_mut_ptr(),
            messagelen.min((*sd).mail.len()),
        );

        if topiclen == 0 {
            nmail_sendmessage(sd, c"Mail must contain a subject.".as_ptr(), 6, 0);
            return 0;
        }
        if messagelen == 0 {
            nmail_sendmessage(sd, c"Mail must contain a body.".as_ptr(), 6, 0);
            return 0;
        }

        nmail_poemscript(sd, topic.as_ptr(), message.as_ptr()).await;
        return 0;
    }

    // Standard mail
    if topiclen == 0 {
        nmail_sendmessage(sd, c"Mail must contain a subject.".as_ptr(), 6, 0);
        return 0;
    }
    if messagelen == 0 {
        nmail_sendmessage(sd, c"Mail must contain a body.".as_ptr(), 6, 0);
        return 0;
    }

    nmail_sendmail(sd, to_user.as_ptr(), topic.as_ptr(), message.as_ptr());

    if send_copy != 0 {
        // Build "[To NAME] original_topic" (truncated to 51 chars + null).
        let to_str = std::ffi::CStr::from_ptr(to_user.as_ptr()).to_string_lossy();
        let tp_str = std::ffi::CStr::from_ptr(topic.as_ptr()).to_string_lossy();
        let mut a_topic = format!("[To {}] {}", to_str, tp_str);
        a_topic.truncate(51);
        let a_topic_c = std::ffi::CString::new(a_topic).unwrap_or_default();
        let self_name_c =
            std::ffi::CString::new((*sd).player.identity.name.as_str()).unwrap_or_default();
        nmail_sendmailcopy(
            sd,
            self_name_c.as_ptr(),
            a_topic_c.as_ptr(),
            message.as_ptr(),
        );
    }

    0
}

// ---------------------------------------------------------------------------
// nmail_sendmail — forwards a mail message to the char-server (packet 0x300D).
//
// Packet layout is identical to nmail_sendmailcopy (0x300F) but uses 0x300D.
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn nmail_sendmail(
    sd: *mut MapSessionData,
    to_user: *const i8,
    topic: *const i8,
    message: *const i8,
) -> i32 {
    if libc_strlen(to_user) > 16 || libc_strlen(topic) > 52 || libc_strlen(message) > 4000 {
        return 0;
    }

    let from = (*sd).player.identity.name.clone();
    let to_str = std::ffi::CStr::from_ptr(to_user)
        .to_string_lossy()
        .into_owned();
    let topic_str = std::ffi::CStr::from_ptr(topic)
        .to_string_lossy()
        .into_owned();
    let msg_str = std::ffi::CStr::from_ptr(message)
        .to_string_lossy()
        .into_owned();

    // Call DB directly instead of sending 0x300D to char server.
    let result = blocking_run_async(async move {
        db_boards::nmail_insert(get_pool(), &from, &to_str, &topic_str, &msg_str).await
    });

    // Format client response (matches 0x380C handler).
    let (msg, r#type) = match result {
        0 => (c"Your message has been sent.", 1),
        2 => (c"User does not exist.", 0),
        _ => (c"Something went wrong. Please try again later.", 0),
    };
    nmail_sendmessage(sd, msg.as_ptr(), 6, r#type);
    0
}

// ---------------------------------------------------------------------------
// map_changepostcolor — SQL UPDATE to set board post highlight color.
//
// Uses sqlx for database access.
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn map_changepostcolor(board: i32, post: i32, color: i32) -> i32 {
    sqlx::query(
        "UPDATE `Boards` SET `BrdHighlighted` = ? WHERE `BrdBnmId` = ? AND `BrdPosition` = ?",
    )
    .bind(color)
    .bind(board)
    .bind(post)
    .execute(get_pool())
    .await
    .ok();
    0
}

// ---------------------------------------------------------------------------
// map_getpostcolor — SQL SELECT to retrieve board post highlight color.
//
// Uses sqlx for database access.
// ---------------------------------------------------------------------------

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn map_getpostcolor(board: i32, post: i32) -> i32 {
    sqlx::query_scalar::<_, Option<i32>>(
        "SELECT `BrdHighlighted` FROM `Boards` WHERE `BrdBnmId` = ? AND `BrdPosition` = ?",
    )
    .bind(board)
    .bind(post)
    .fetch_optional(get_pool())
    .await
    .ok()
    .flatten()
    .flatten()
    .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// libc_strlen — safe strlen wrapper for *const i8 inputs.
// Used by length-check guards in nmail_sendmail/nmail_sendmailcopy.
// ---------------------------------------------------------------------------

#[inline]
unsafe fn libc_strlen(s: *const i8) -> usize {
    if s.is_null() {
        return 0;
    }
    std::ffi::CStr::from_ptr(s).to_bytes().len()
}

// ---------------------------------------------------------------------------
// clif_Hacker — declare here (not in existing extern block).
// ---------------------------------------------------------------------------

use crate::game::client::handlers::clif_Hacker;

// ---------------------------------------------------------------------------
// Language / message table — map_msg[] and lang_read
// ---------------------------------------------------------------------------
//
/// Number of named message slots.
///
/// Enum values (0-based): MAP_WHISPFAIL…MAP_ERRSUMMON = 29 entries, then MSG_MAX = 30.
pub const MSG_MAX: usize = 30;

/// One message entry in the language table.
///
/// ```c
/// struct map_msg_data { char message[256]; int len; };
/// ```
#[repr(C)]
pub struct MapMsgData {
    pub message: [i8; 256],
    pub len: i32,
}

/// The global language message table.
///
/// Entries are populated by `lang_read`.  The `message` field is a
/// null-terminated, fixed-length C string stored directly in the struct
/// (no heap allocation); `len` caches `strlen(message)` capped at 255.
/// Set once at startup, read-only thereafter.
static MAP_MSG: OnceLock<Box<[MapMsgData; MSG_MAX]>> = OnceLock::new();

/// Access the global language message table. Returns a reference to the
/// table, which is valid for the lifetime of the process.
/// If `lang_read` has not been called, returns a zeroed default table.
#[inline]
pub fn map_msg() -> &'static [MapMsgData; MSG_MAX] {
    MAP_MSG.get_or_init(|| {
        const ZERO: MapMsgData = MapMsgData {
            message: [0; 256],
            len: 0,
        };
        Box::new([ZERO; MSG_MAX])
    })
}

/// Mapping from the string key used in the lang file to the `map_msg` slot index.
///
/// Default message database used when no lang file is loaded.
static LANG_KEY_MAP: &[(&str, usize)] = &[
    ("MAP_WHISPFAIL", 0),
    ("MAP_ERRGHOST", 1),
    ("MAP_ERRITMLEVEL", 2),
    ("MAP_ERRITMMIGHT", 3),
    ("MAP_ERRITMGRACE", 4),
    ("MAP_ERRITMWILL", 5),
    ("MAP_ERRITMSEX", 6),
    ("MAP_ERRITMFULL", 7),
    ("MAP_ERRITMMAX", 8),
    ("MAP_ERRITMPATH", 9),
    ("MAP_ERRITMMARK", 10),
    ("MAP_ERRITM2H", 11),
    ("MAP_ERRMOUNT", 12),
    ("MAP_EQHELM", 13),
    ("MAP_EQWEAP", 14),
    ("MAP_EQARMOR", 15),
    ("MAP_EQSHIELD", 16),
    ("MAP_EQLEFT", 17),
    ("MAP_EQRIGHT", 18),
    ("MAP_EQSUBLEFT", 19),
    ("MAP_EQSUBRIGHT", 20),
    ("MAP_EQFACEACC", 21),
    ("MAP_EQCROWN", 22),
    ("MAP_EQMANTLE", 23),
    ("MAP_EQNECKLACE", 24),
    ("MAP_EQBOOTS", 25),
    ("MAP_EQCOAT", 26),
    ("MAP_ERRVITA", 27),
    ("MAP_ERRMANA", 28),
    ("MAP_ERRSUMMON", 29),
];

/// Parse the language config file and populate `map_msg[]`.
///
/// The file format is line-based:
/// - Lines starting with `//` are comments and are skipped.
/// - Non-comment lines are parsed as `KEY: value` (separated by `: `).
/// - The key is matched case-insensitively against the known `MAP_*`/`MSG_*` names.
/// - Matching entries are written into `map_msg[index].message` (truncated to 255
///   bytes) and `map_msg[index].len` is set accordingly.
///
///
/// Returns 0 on success, 1 if the file cannot be opened.
///
/// # Safety
/// `cfg_file` must be a valid, non-null, null-terminated C string.  This
/// function must only be called from the game thread (no concurrent access to
/// `map_msg`).
pub unsafe fn lang_read(cfg_file: *const i8) -> i32 {
    use std::io::BufRead as _;

    let path = std::ffi::CStr::from_ptr(cfg_file).to_string_lossy();

    let file = match std::fs::File::open(path.as_ref()) {
        Ok(f) => f,
        Err(_) => {
            println!("CFG_ERR: Language file ({path}) not found.");
            return 1;
        }
    };

    const ZERO: MapMsgData = MapMsgData {
        message: [0; 256],
        len: 0,
    };
    let mut msgs = Box::new([ZERO; MSG_MAX]);

    for line in std::io::BufReader::new(file).lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        // Skip comment lines.
        if line.starts_with("//") {
            continue;
        }

        // Parse `KEY: value` — split on the first `: ` only.
        let Some(colon_pos) = line.find(": ") else {
            continue;
        };
        let key = &line[..colon_pos];
        // Value is everything after `: `, stripping any trailing \r\n.
        let value = line[colon_pos + 2..].trim_end_matches(['\r', '\n']);

        // Look up the key (case-insensitive).
        let key_up = key.to_ascii_uppercase();
        let Some(&(_, idx)) = LANG_KEY_MAP.iter().find(|(k, _)| *k == key_up.as_str()) else {
            continue;
        };

        // Copy the value into the fixed message buffer, truncating at 255 bytes.
        let bytes = value.as_bytes();
        let copy_len = bytes.len().min(255);
        let slot = &mut msgs[idx];
        // Zero the whole buffer first (matches strncpy semantics for short strings).
        slot.message = [0; 256];
        for (i, &b) in bytes[..copy_len].iter().enumerate() {
            slot.message[i] = b as i8;
        }
        slot.message[copy_len] = 0; // null terminator (already zero, but be explicit)
        slot.len = copy_len as i32;
    }

    // Set the global table (only first call wins; subsequent calls are no-ops).
    let _ = MAP_MSG.set(msgs);

    println!("[map] [lang_read] file={path}");
    0
}

// ---------------------------------------------------------------------------
// In-game time functions.
// ---------------------------------------------------------------------------

/// Advance the in-game clock by one hour and broadcast the new time to all
/// connected players.
///
/// Call order: `cur_time` wraps 0–23; each full day advances `cur_day` (1–91);
/// each full season (91 days) advances `cur_season` (1–4); each four seasons
/// advances `cur_year`.  After updating globals the new values are written to
/// the `Time` table and `clif_sendtime` is called for every active session.
///
///
/// # Safety
/// Must be called on the game thread.  `crate::session::get_fd_max()` must reflect the current session table bounds.
pub async unsafe fn change_time_char(_id: i32, _n: i32) -> i32 {
    let t = cur_time.fetch_add(1, Ordering::Relaxed) + 1;

    if t == 24 {
        cur_time.store(0, Ordering::Relaxed);
        let d = cur_day.fetch_add(1, Ordering::Relaxed) + 1;
        if d == 92 {
            cur_day.store(1, Ordering::Relaxed);
            let s = cur_season.fetch_add(1, Ordering::Relaxed) + 1;
            if s == 5 {
                cur_season.store(1, Ordering::Relaxed);
                cur_year.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    // Broadcast updated time to all active sessions.
    for i in 0..crate::session::get_fd_max() {
        let fd = SessionId::from_raw(i);
        if session_exists(fd) {
            if let Some(sd) = session_get_data(fd) {
                crate::game::map_parse::player_state::clif_sendtime(&sd);
            }
        }
    }

    // Persist updated time to the database.
    let (t, d, s, y) = (
        cur_time.load(Ordering::Relaxed),
        cur_day.load(Ordering::Relaxed),
        cur_season.load(Ordering::Relaxed),
        cur_year.load(Ordering::Relaxed),
    );
    sqlx::query("UPDATE `Time` SET `TimHour` = ?, `TimDay` = ?, `TimSeason` = ?, `TimYear` = ?")
        .bind(t)
        .bind(d)
        .bind(s)
        .bind(y)
        .execute(get_pool())
        .await
        .ok();

    0
}

/// Load in-game time from the database and initialise `cur_time`, `cur_day`,
/// `cur_season`, `cur_year`, and `old_time`.
///
/// Reads the first row of the `Time` table.  If the query fails or no row is
/// returned the globals are left at their previous values (zero on startup).
///
///
/// # Safety
/// Must be called on the game thread.
pub async unsafe fn get_time_thing() -> i32 {
    #[derive(sqlx::FromRow)]
    struct TimeRow {
        #[sqlx(rename = "TimHour")]
        hour: u32,
        #[sqlx(rename = "TimDay")]
        day: u32,
        #[sqlx(rename = "TimSeason")]
        season: u32,
        #[sqlx(rename = "TimYear")]
        year: u32,
    }

    if let Some(row) = sqlx::query_as::<_, TimeRow>(
        "SELECT `TimHour`, `TimDay`, `TimSeason`, `TimYear` FROM `Time` LIMIT 1",
    )
    .fetch_optional(get_pool())
    .await
    .ok()
    .flatten()
    {
        old_time.store(row.hour as i32, Ordering::Relaxed);
        cur_time.store(row.hour as i32, Ordering::Relaxed);
        cur_day.store(row.day as i32, Ordering::Relaxed);
        cur_season.store(row.season as i32, Ordering::Relaxed);
        cur_year.store(row.year as i32, Ordering::Relaxed);
    }

    0
}

/// Record the current UNIX timestamp as the server start time in the `UpTime`
/// table (row `UtmId = 3`).
///
/// Deletes the existing row then inserts the current `time(NULL)` value.
///
///
/// # Safety
/// Must be called on the game thread.
pub async unsafe fn uptime() -> i32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i32)
        .unwrap_or(0);

    let pool = get_pool();
    sqlx::query("DELETE FROM `UpTime` WHERE `UtmId` = '3'")
        .execute(pool)
        .await
        .ok();
    sqlx::query("INSERT INTO `UpTime`(`UtmId`, `UtmValue`) VALUES('3', ?)")
        .bind(now)
        .execute(pool)
        .await
        .ok();

    0
}

// ---------------------------------------------------------------------------
// Object flags — static object collision-flag table loaded from static_objects.tbl
//
// Each byte encodes directional movement flags (OBJ_UP / OBJ_RIGHT / OBJ_DOWN /
// OBJ_LEFT) for its corresponding object.
// ---------------------------------------------------------------------------

/// Static object flag table populated by `object_flag_init`.
///
/// Bitmap of directional collision flags for each floor item object slot.
static OBJECT_FLAGS: std::sync::OnceLock<Box<[u8]>> = std::sync::OnceLock::new();

/// Access the object collision flags table.
/// Returns `None` if `object_flag_init` hasn't been called yet.
pub fn object_flags() -> Option<&'static [u8]> {
    OBJECT_FLAGS.get().map(|b| b.as_ref())
}

/// Load the static object flag table from `static_objects.tbl`.
///
/// Reads a binary file whose format is:
/// - 4 bytes: total object count (`num`, little-endian `i32`)
/// - 1 byte: initial flag (consumed before the loop)
/// - Then `num` records, each:
///   - 1 byte: count of tile IDs that follow
///   - `count` × 2 bytes: tile IDs
///   - 5 bytes: reserved/padding
///   - 1 byte: flag byte for this object
///
/// Allocates the object flags table with `num + 1` zero-initialised bytes.
/// The actual per-object flag assignment is intentionally left commented out,
/// preserving the original C behaviour (table allocated but entries stay zero).
///
///
/// # Safety
/// and point to a null-terminated string.
pub unsafe fn object_flag_init() -> i32 {
    let filename = b"static_objects.tbl\0";
    let dir_bytes = crate::config::config().data_dir.as_bytes();

    // Build full path: data_dir + filename (without the extra NUL added by CString).
    let mut path_bytes: Vec<u8> = Vec::with_capacity(dir_bytes.len() + filename.len() - 1);
    path_bytes.extend_from_slice(dir_bytes);
    path_bytes.extend_from_slice(&filename[..filename.len() - 1]);
    let path_cstr = match std::ffi::CString::new(path_bytes) {
        Ok(s) => s,
        Err(_) => {
            tracing::error!("[map] [object_flag_init] path contains interior nul byte");
            std::process::exit(1);
        }
    };

    let path_str = path_cstr.to_string_lossy();
    println!(
        "[map] [object_flag_init] reading static obj table path={}",
        path_str
    );

    let fi = libc::fopen(path_cstr.as_ptr(), c"rb".as_ptr());
    if fi.is_null() {
        tracing::error!(
            "[map] [error] cannot read static object table path={}",
            path_str
        );
        std::process::exit(1);
    }

    let mut num: i32 = 0;
    libc::fread(std::ptr::addr_of_mut!(num).cast(), 4, 1, fi);

    // Allocate object flags with num+1 zero-initialised bytes.
    let flags_vec: Vec<u8> = vec![0u8; (num as usize) + 1];
    let _ = OBJECT_FLAGS.set(flags_vec.into_boxed_slice());

    let mut flag: i8 = 0;
    libc::fread(std::ptr::addr_of_mut!(flag).cast(), 1, 1, fi);

    let mut _z: i32 = 1;
    while libc::feof(fi) == 0 {
        let mut count: i8 = 0;
        libc::fread(std::ptr::addr_of_mut!(count).cast(), 1, 1, fi);
        let mut remaining = count;
        while remaining != 0 {
            let mut tile: i16 = 0;
            libc::fread(std::ptr::addr_of_mut!(tile).cast(), 2, 1, fi);
            remaining -= 1;
        }

        let mut nothing = [0u8; 5];
        libc::fread(nothing.as_mut_ptr().cast(), 5, 1, fi);
        libc::fread(std::ptr::addr_of_mut!(flag).cast(), 1, 1, fi);
        // flag assignment intentionally omitted, matching original behavior
        _z += 1;
    }

    libc::fclose(fi);
    0
}

// ---------------------------------------------------------------------------
// map_src linked-list
//
// The C implementation used a `struct map_src_list` singly-linked list with
// heap-allocated nodes.  Replaced here with a `Vec<MapSrcEntry>` for safety.
// `map_src_clear` frees the list; `map_src_add` appends one parsed entry.
//
// C (currently unused in the codebase, but retained for ABI completeness).
// ---------------------------------------------------------------------------

// Retained for ABI compatibility — map_src_add/map_src_clear are declared in
#[allow(dead_code)]
/// One entry in the map source list (equivalent to C `struct map_src_list`).
#[derive(Debug)]
struct MapSrcEntry {
    id: i32,
    pvp: i32,
    spell: i32,
    sweeptime: u32,
    title: [u8; 64],
    cantalk: u8,
    show_ghosts: u8,
    region: u8,
    indoor: u8,
    warpout: u8,
    bind: u8,
    bgm: u16,
    bgmtype: u16,
    light: u16,
    weather: u16,
    mapfile: Vec<u8>,
}

// Retained for ABI compatibility — map_src_add/map_src_clear are declared in

#[allow(dead_code)]
/// The parsed map source list.
static MAP_SRC_LIST: OnceLock<Mutex<Vec<MapSrcEntry>>> = OnceLock::new();

#[inline]
fn map_src_list() -> std::sync::MutexGuard<'static, Vec<MapSrcEntry>> {
    MAP_SRC_LIST
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Free all entries in the map source list.
pub fn map_src_clear() -> i32 {
    map_src_list().clear();
    0
}

/// Parse one CSV line and append it to the map source list.
///
/// Expected format (matching the C `sscanf` format string):
/// ```text
/// map_id,title,bgm,pvp,spell,light,weather,sweeptime,cantalk,showghosts,
/// region,indoor, warpout,bind,mapfile
/// ```
/// (Note the C format has a leading space before `warpout`: `", %c"`.)
///
/// Returns `0` on success, `-1` if fewer than 13 fields can be parsed.
///
///
/// # Safety
/// `r1` must be a valid, non-null, null-terminated C string.
/// Must be called on the game thread.
pub unsafe fn map_src_add(r1: *const i8) -> i32 {
    let line = std::ffi::CStr::from_ptr(r1).to_string_lossy();

    // Split on commas, matching the C sscanf format (15 fields max).
    // Format: map_id,title,bgm,pvp,spell,light,weather,sweeptime,
    //         cantalk,showghosts,region,indoor, warpout,bind,mapfile
    // The title field may contain spaces but not commas ([^,] in sscanf).
    let parts: Vec<&str> = line.splitn(15, ',').collect();
    if parts.len() < 13 {
        return -1;
    }

    let map_id: i32 = match parts[0].trim().parse() {
        Ok(v) => v,
        Err(_) => return -1,
    };
    let map_title = parts[1];
    let map_bgm: u16 = match parts[2].trim().parse() {
        Ok(v) => v,
        Err(_) => return -1,
    };
    let pvp: i32 = match parts[3].trim().parse() {
        Ok(v) => v,
        Err(_) => return -1,
    };
    let spell: i32 = match parts[4].trim().parse() {
        Ok(v) => v,
        Err(_) => return -1,
    };
    let light: u16 = match parts[5].trim().parse() {
        Ok(v) => v,
        Err(_) => return -1,
    };
    let weather: u16 = match parts[6].trim().parse() {
        Ok(v) => v,
        Err(_) => return -1,
    };
    let sweeptime: u32 = match parts[7].trim().parse() {
        Ok(v) => v,
        Err(_) => return -1,
    };
    // Single-character fields (%c in C sscanf) — read first byte only.
    let cantalk = parts[8].trim().as_bytes().first().copied().unwrap_or(0);
    let showghosts = parts[9].trim().as_bytes().first().copied().unwrap_or(0);
    let region = parts[10].trim().as_bytes().first().copied().unwrap_or(0);
    let indoor = parts[11].trim().as_bytes().first().copied().unwrap_or(0);
    // C format has a leading space before warpout: `", %c"` — trim handles it.
    let warpout = parts[12].trim().as_bytes().first().copied().unwrap_or(0);
    if parts.len() < 14 {
        return -1;
    }
    let bind = parts[13].trim().as_bytes().first().copied().unwrap_or(0);
    let map_file = if parts.len() >= 15 {
        parts[14].trim()
    } else {
        ""
    };

    let mut title_buf = [0u8; 64];
    let title_bytes = map_title.as_bytes();
    let copy_len = title_bytes.len().min(63);
    title_buf[..copy_len].copy_from_slice(&title_bytes[..copy_len]);

    let entry = MapSrcEntry {
        id: map_id,
        pvp,
        spell,
        sweeptime,
        title: title_buf,
        cantalk,
        show_ghosts: showghosts,
        region,
        indoor,
        warpout,
        bind,
        bgm: map_bgm,
        bgmtype: 0, // not populated from CSV; C calloc'd struct also left this as 0
        light,
        weather,
        mapfile: map_file.as_bytes().to_vec(),
    };

    map_src_list().push(entry);
    0
}

// ---------------------------------------------------------------------------
// gamereg — game-global registry
//
// Server-wide key/value integer store backed by the `GameRegistry<serverid>` table.
// ---------------------------------------------------------------------------

use crate::common::constants::world::MAX_GAMEREG;

/// Game-global registry entry.
///
/// ```c
/// struct game_data {
///     struct global_reg *registry;
///     int registry_num;
/// };
/// ```
#[repr(C)]
pub struct GameData {
    pub registry: *mut crate::database::map_db::GlobalReg,
    pub registry_num: i32,
}

// SAFETY: `gamereg` is only accessed on the single-threaded game loop.
// No Rust code takes shared references to it across threads.
unsafe impl Send for GameData {}
unsafe impl Sync for GameData {}

impl std::fmt::Debug for GameData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameData").finish_non_exhaustive()
    }
}

/// The game-wide registry global.
///
/// Populated by `map_loadgameregistry` and mutated by `map_setglobalgamereg`.
static GAMEREG: OnceLock<Mutex<GameData>> = OnceLock::new();

#[inline]
pub fn gamereg() -> std::sync::MutexGuard<'static, GameData> {
    GAMEREG
        .get_or_init(|| {
            Mutex::new(GameData {
                registry: std::ptr::null_mut(),
                registry_num: 0,
            })
        })
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Allocate a zeroed array of `GlobalReg` entries of the given length via the
/// global allocator.  The caller is responsible for freeing via the same allocator.
fn alloc_zeroed_gamereg_registry(len: usize) -> *mut crate::database::map_db::GlobalReg {
    use crate::database::map_db::GlobalReg;
    let layout = std::alloc::Layout::array::<GlobalReg>(len).expect("GlobalReg layout overflow");
    let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
    if ptr.is_null() {
        std::alloc::handle_alloc_error(layout);
    }
    ptr as *mut GlobalReg
}

/// ASCII case-insensitive comparison of a `GlobalReg.str` field against a C string.
///
/// Returns `true` if the two null-terminated byte sequences are equal ignoring ASCII case.
/// Equivalent to `strcasecmp` used in the C registry search loops.
unsafe fn reg_str_eq(arr: &[i8; 64], cstr: *const i8) -> bool {
    if cstr.is_null() {
        return false;
    }
    for (i, arr_byte) in arr.iter().enumerate().take(64) {
        let a = *arr_byte as u8;
        let b = *cstr.add(i) as u8;
        if !a.eq_ignore_ascii_case(&b) {
            return false;
        }
        if a == 0 {
            return true; // both null-terminated at the same position
        }
    }
    false
}

/// Copy a C string into a `[i8; 64]` field, null-terminating. Truncates at 63 chars.
unsafe fn copy_cstr_to_reg_str(dest: &mut [i8; 64], src: *const i8) {
    let mut i = 0usize;
    while i < 63 {
        let b = *src.add(i);
        dest[i] = b;
        if b == 0 {
            return;
        }
        i += 1;
    }
    dest[63] = 0; // ensure null termination
}

// ---------------------------------------------------------------------------
// map_registrysave — persist one map registry slot to the `MapRegistry` table.
//
//
// Logic:
//   - SELECT MrgPosition WHERE MrgMapId=m AND MrgIdentifier=str → save_id (-1 if not found)
//   - If found:
//       val==0 → DELETE WHERE MrgMapId=m AND MrgIdentifier=str
//       val!=0 → UPDATE SET MrgIdentifier=str, MrgValue=val WHERE MrgMapId=m AND MrgPosition=save_id
//   - If not found:
//       val>0  → INSERT (MrgMapId, MrgIdentifier, MrgValue, MrgPosition)
// ---------------------------------------------------------------------------

/// Persist one map registry slot at index `i` on map `m` to the `MapRegistry` table.
///
///
/// # Safety
/// `crate::database::map_db::raw_map_ptr()` must be a valid initialised pointer.  `m` must be a
/// loaded map index and `i` must be within `[0, MAX_MAPREG)`.
pub async unsafe fn map_registrysave(m: i32, i: i32) -> i32 {
    use crate::database::map_db::{GlobalReg, MAP_SLOTS, MAX_MAPREG};

    if m < 0 || m as usize >= MAP_SLOTS {
        return 0;
    }
    if i < 0 || i as usize >= MAX_MAPREG {
        return 0;
    }

    // Extract all data into owned/Copy values before the first .await to ensure
    // the future is Send (raw pointer refs cannot cross await points safely).
    let (identifier, val, m_u32, i_u32) = {
        let slot = &mut *crate::database::map_db::raw_map_ptr().add(m as usize);
        if slot.registry.is_null() {
            return 0;
        }

        let p: &GlobalReg = &*slot.registry.add(i as usize);

        // Read the identifier (null-terminated i8 array) into a Rust String.
        let identifier = {
            let bytes: &[u8] = std::slice::from_raw_parts(p.str.as_ptr() as *const u8, 64);
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(64);
            String::from_utf8_lossy(&bytes[..end]).into_owned()
        };
        let val = p.val;
        (identifier, val, m as u32, i as u32)
    }; // slot and p refs dropped here

    // SELECT existing position.
    let save_id: Option<u32> = sqlx::query_scalar::<_, u32>(
        "SELECT MrgPosition FROM MapRegistry \
             WHERE MrgMapId = ? AND MrgIdentifier = ?",
    )
    .bind(m_u32)
    .bind(identifier.clone())
    .fetch_optional(get_pool())
    .await
    .unwrap_or(None);

    match save_id {
        Some(pos) => {
            if val == 0 {
                // Delete the entry — value cleared.
                let _ = sqlx::query(
                    "DELETE FROM MapRegistry \
                         WHERE MrgMapId = ? AND MrgIdentifier = ?",
                )
                .bind(m_u32)
                .bind(identifier.clone())
                .execute(get_pool())
                .await;
            } else {
                // Update in-place.
                let _ = sqlx::query(
                    "UPDATE MapRegistry SET MrgIdentifier = ?, MrgValue = ? \
                         WHERE MrgMapId = ? AND MrgPosition = ?",
                )
                .bind(identifier.clone())
                .bind(val)
                .bind(m_u32)
                .bind(pos)
                .execute(get_pool())
                .await;
            }
        }
        None => {
            if val > 0 {
                // Insert new row.
                let _ = sqlx::query(
                    "INSERT INTO MapRegistry \
                         (MrgMapId, MrgIdentifier, MrgValue, MrgPosition) \
                         VALUES (?, ?, ?, ?)",
                )
                .bind(m_u32)
                .bind(identifier)
                .bind(val)
                .bind(i_u32)
                .execute(get_pool())
                .await;
            }
        }
    }

    0
}

// ---------------------------------------------------------------------------
// map_setglobalreg — set a map-level registry key/value in memory and persist.
//
//
// Uses the `map_isloaded` guard (registry != null), then:
//   1. Linear search for an existing entry with the same name (strcasecmp).
//   2. If found: update val, persist, clear str if val==0.
//   3. If not found: reuse the first empty slot, or extend registry_num if capacity allows.
// ---------------------------------------------------------------------------

/// Set a key/value pair in the per-map registry for map `m`, then persist to DB.
///
///
/// # Safety
/// `crate::database::map_db::raw_map_ptr()` must be a valid initialised pointer.  `m` must be within
/// `[0, MAP_SLOTS)`.  `reg` must be a valid non-null null-terminated C string.
pub async unsafe fn map_setglobalreg(m: i32, reg: *const i8, val: i32) -> i32 {
    use crate::database::map_db::MAP_SLOTS;

    if reg.is_null() {
        return 0;
    }
    if m < 0 || m as usize >= MAP_SLOTS {
        return 0;
    }

    // Determine the save index and apply all in-memory updates in a sync block,
    // releasing all raw pointer references before the first .await point.
    // Returns: Some((save_idx, clear_str)) where clear_str means entry.str should
    // be zeroed after the DB persist, or None if nothing to save.
    let save_info: Option<(i32, bool)> = {
        let slot = &mut *crate::database::map_db::raw_map_ptr().add(m as usize);
        if slot.registry.is_null() {
            return 0;
        }
        let num = slot.registry_num as usize;

        // Search for an existing entry.
        let mut exist: Option<usize> = None;
        for idx in 0..num {
            let entry = &*slot.registry.add(idx);
            if reg_str_eq(&entry.str, reg) {
                exist = Some(idx);
                break;
            }
        }

        if let Some(idx) = exist {
            let entry = &mut *slot.registry.add(idx);
            entry.val = val;
            Some((idx as i32, val == 0))
        } else {
            // Search for an empty slot to reuse.
            let mut reuse_idx: Option<usize> = None;
            for idx in 0..num {
                let entry = &*slot.registry.add(idx);
                if entry.str[0] == 0 {
                    reuse_idx = Some(idx);
                    break;
                }
            }
            if let Some(idx) = reuse_idx {
                let entry = &mut *slot.registry.add(idx);
                copy_cstr_to_reg_str(&mut entry.str, reg);
                entry.val = val;
                Some((idx as i32, false))
            } else if num < crate::database::map_db::MAX_MAPREG {
                let new_num = num + 1;
                slot.registry_num = new_num as i32;
                let entry = &mut *slot.registry.add(num);
                copy_cstr_to_reg_str(&mut entry.str, reg);
                entry.val = val;
                Some((num as i32, false))
            } else {
                None
            }
        }
    }; // all raw pointer refs dropped here

    if let Some((save_idx, clear_str)) = save_info {
        map_registrysave(m, save_idx).await;
        if clear_str {
            // Clear the entry str after persisting val==0.
            let slot = &mut *crate::database::map_db::raw_map_ptr().add(m as usize);
            if !slot.registry.is_null() {
                let entry = &mut *slot.registry.add(save_idx as usize);
                entry.str = [0i8; 64];
            }
        }
    }

    0
}

/// Send-safe async function for persisting a map registry entry by name.
///
/// Updates the in-memory registry for map `m` and persists to DB. All parameters
/// are owned/Copy types so the future is `Send` — safe to `.await` from Lua callback
/// boundaries via `blocking_run_async`.
///
/// # Safety
/// `crate::database::map_db::raw_map_ptr()` must be a valid initialised pointer and `m` within
/// `[0, MAP_SLOTS)`.
pub async unsafe fn map_setglobalreg_str(m: i32, reg_name: String, val: i32) -> i32 {
    use crate::database::map_db::MAP_SLOTS;
    if m < 0 || m as usize >= MAP_SLOTS {
        return 0;
    }

    let save_info: Option<(i32, bool)> = {
        let slot = &mut *crate::database::map_db::raw_map_ptr().add(m as usize);
        if slot.registry.is_null() {
            return 0;
        }
        let num = slot.registry_num as usize;

        // Search for an existing entry (case-insensitive compare with owned String).
        let mut exist: Option<usize> = None;
        for idx in 0..num {
            let entry = &*slot.registry.add(idx);
            let bytes: &[u8] = std::slice::from_raw_parts(entry.str.as_ptr() as *const u8, 64);
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(64);
            let entry_name = std::str::from_utf8_unchecked(&bytes[..end]);
            if entry_name.eq_ignore_ascii_case(&reg_name) {
                exist = Some(idx);
                break;
            }
        }

        if let Some(idx) = exist {
            let entry = &mut *slot.registry.add(idx);
            entry.val = val;
            Some((idx as i32, val == 0))
        } else {
            let mut reuse_idx: Option<usize> = None;
            for idx in 0..num {
                let entry = &*slot.registry.add(idx);
                if entry.str[0] == 0 {
                    reuse_idx = Some(idx);
                    break;
                }
            }
            if let Some(idx) = reuse_idx {
                let entry = &mut *slot.registry.add(idx);
                let bytes = reg_name.as_bytes();
                let n = bytes.len().min(63);
                for (i, &b) in bytes[..n].iter().enumerate() {
                    entry.str[i] = b as i8;
                }
                entry.str[n] = 0;
                entry.val = val;
                Some((idx as i32, false))
            } else if num < crate::database::map_db::MAX_MAPREG {
                slot.registry_num = (num + 1) as i32;
                let entry = &mut *slot.registry.add(num);
                let bytes = reg_name.as_bytes();
                let n = bytes.len().min(63);
                for (i, &b) in bytes[..n].iter().enumerate() {
                    entry.str[i] = b as i8;
                }
                entry.str[n] = 0;
                entry.val = val;
                Some((num as i32, false))
            } else {
                None
            }
        }
    }; // all raw pointer refs dropped here

    if let Some((save_idx, clear_str)) = save_info {
        map_registrysave(m, save_idx).await;
        if clear_str {
            let slot = &mut *crate::database::map_db::raw_map_ptr().add(m as usize);
            if !slot.registry.is_null() {
                let entry = &mut *slot.registry.add(save_idx as usize);
                entry.str = [0i8; 64];
            }
        }
    }
    0
}

/// Send-safe async function for persisting a game-global registry entry by name.
///
/// Updates the in-memory `gamereg` and persists to DB. All parameters are owned/Copy
/// Set a game-global registry value by name and persist to DB.
/// Uses owned `String` so the future is `Send`.
pub async fn map_setglobalgamereg_str(reg_name: String, val: i32) -> i32 {
    // SAFETY: raw pointer access to the GlobalReg array — single-threaded, guarded by Mutex.
    let save_info: Option<(i32, bool)> = unsafe {
        let mut gr = gamereg();
        if gr.registry.is_null() {
            return 0;
        }
        let num = gr.registry_num as usize;

        let mut exist: Option<usize> = None;
        for idx in 0..num {
            let entry = &*gr.registry.add(idx);
            let bytes: &[u8] = std::slice::from_raw_parts(entry.str.as_ptr() as *const u8, 64);
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(64);
            let entry_name = std::str::from_utf8_unchecked(&bytes[..end]);
            if entry_name.eq_ignore_ascii_case(&reg_name) {
                exist = Some(idx);
                break;
            }
        }

        if let Some(idx) = exist {
            let entry = &mut *gr.registry.add(idx);
            if entry.val == val {
                return 0;
            } // value unchanged — skip save
            entry.val = val;
            Some((idx as i32, val == 0))
        } else {
            let mut reuse_idx: Option<usize> = None;
            for idx in 0..num {
                let entry = &*gr.registry.add(idx);
                if entry.str[0] == 0 {
                    reuse_idx = Some(idx);
                    break;
                }
            }
            if let Some(idx) = reuse_idx {
                let entry = &mut *gr.registry.add(idx);
                let bytes = reg_name.as_bytes();
                let n = bytes.len().min(63);
                for (i, &b) in bytes[..n].iter().enumerate() {
                    entry.str[i] = b as i8;
                }
                entry.str[n] = 0;
                entry.val = val;
                Some((idx as i32, false))
            } else if num < MAX_GAMEREG {
                gr.registry_num = (num + 1) as i32;
                let entry = &mut *gr.registry.add(num);
                let bytes = reg_name.as_bytes();
                let n = bytes.len().min(63);
                for (i, &b) in bytes[..n].iter().enumerate() {
                    entry.str[i] = b as i8;
                }
                entry.str[n] = 0;
                entry.val = val;
                Some((num as i32, false))
            } else {
                None
            }
        }
    }; // MutexGuard dropped here — safe to await below

    if let Some((save_idx, clear_str)) = save_info {
        (unsafe { map_savegameregistry(save_idx) }).await;
        if clear_str {
            let gr = gamereg();
            if !gr.registry.is_null() {
                unsafe {
                    let entry = &mut *gr.registry.add(save_idx as usize);
                    entry.str = [0i8; 64];
                }
            }
        }
    }
    0
}

// ---------------------------------------------------------------------------
// map_readglobalreg — read a map-level registry value from memory.
//
// ---------------------------------------------------------------------------

/// Return the value for registry key `reg` on map `m`, or 0 if not found.
///
///
/// # Safety
/// `crate::database::map_db::raw_map_ptr()` must be a valid initialised pointer.  `m` must be within
/// `[0, MAP_SLOTS)`.  `reg` must be a valid non-null null-terminated C string.
pub unsafe fn map_readglobalreg(m: i32, reg: *const i8) -> i32 {
    use crate::database::map_db::MAP_SLOTS;

    if m < 0 || m as usize >= MAP_SLOTS {
        return 0;
    }
    let slot = &*crate::database::map_db::raw_map_ptr().add(m as usize);
    if slot.registry.is_null() {
        return 0;
    }

    let num = slot.registry_num as usize;
    for idx in 0..num {
        let entry = &*slot.registry.add(idx);
        if reg_str_eq(&entry.str, reg) {
            return entry.val;
        }
    }
    0
}

// ---------------------------------------------------------------------------
// map_loadgameregistry — load game-global registry from `GameRegistry<id>` table.
//
//
// Allocates gamereg.registry, queries all rows, copies them into the array.
// ---------------------------------------------------------------------------

/// Load the game-global registry from the `GameRegistry<serverid>` table.
///
///
/// # Safety
/// Must be called on the game thread after the database pool is initialised.
pub async unsafe fn map_loadgameregistry() -> i32 {
    #[derive(sqlx::FromRow)]
    struct GrgRow {
        #[sqlx(rename = "GrgIdentifier")]
        grg_identifier: String,
        #[sqlx(rename = "GrgValue")]
        grg_value: u32, // INT UNSIGNED in schema
    }

    let sid = crate::config::config().server_id;
    let limit = MAX_GAMEREG as u32;

    {
        let mut gr = gamereg();
        gr.registry_num = 0;

        // Free previous registry if reload.
        if !gr.registry.is_null() {
            let layout =
                std::alloc::Layout::array::<crate::database::map_db::GlobalReg>(MAX_GAMEREG)
                    .expect("layout computation is infallible for MAX_GAMEREG = 5000");
            std::alloc::dealloc(gr.registry as *mut u8, layout);
            gr.registry = std::ptr::null_mut();
        }

        gr.registry = alloc_zeroed_gamereg_registry(MAX_GAMEREG);
    }

    let sql = format!("SELECT GrgIdentifier, GrgValue FROM `GameRegistry{sid}` LIMIT {limit}");

    let rows_opt = match sqlx::query_as::<_, GrgRow>(&sql)
        .fetch_all(get_pool())
        .await
    {
        Ok(rows) => Some(
            rows.into_iter()
                .map(|r| (r.grg_identifier, r.grg_value as i32))
                .collect::<Vec<_>>(),
        ),
        Err(e) => {
            tracing::error!("[map] map_loadgameregistry failed: {e:#}");
            None
        }
    };

    let rows = match rows_opt {
        Some(r) => r,
        None => return 0,
    };

    let count = rows.len().min(MAX_GAMEREG);
    let mut gr = gamereg();
    gr.registry_num = count as i32;

    for (i, (identifier, val)) in rows.iter().take(count).enumerate() {
        let entry = &mut *gr.registry.add(i);
        let bytes = identifier.as_bytes();
        let copy_len = bytes.len().min(63);
        std::ptr::copy_nonoverlapping(
            bytes.as_ptr() as *const i8,
            entry.str.as_mut_ptr(),
            copy_len,
        );
        entry.str[copy_len] = 0;
        entry.val = *val;
    }

    tracing::info!("[map] [load_game_registry] count={count}");
    0
}

// ---------------------------------------------------------------------------
// map_savegameregistry — persist one game-global registry slot to DB.
//
//
// Logic:
//   - SELECT GrgId WHERE GrgIdentifier=str → save_id (0 if not found)
//   - If found (save_id != 0):
//       val==0 → DELETE WHERE GrgIdentifier=str
//       val!=0 → UPDATE SET GrgIdentifier=str, GrgValue=val WHERE GrgId=save_id
//   - If not found (save_id==0):
//       val>0  → INSERT (GrgIdentifier, GrgValue)
// ---------------------------------------------------------------------------

/// Persist one game-global registry slot at index `i` to `GameRegistry<serverid>`.
///
///
/// # Safety
/// Must be called on the game thread.  `i` must be within `[0, registry_num)`.
/// `gamereg.registry` must be a valid allocated array.
pub async unsafe fn map_savegameregistry(i: i32) -> i32 {
    // Extract all data into owned/Copy types before the first .await to ensure
    // the future is Send (raw pointer refs cannot cross await points).
    let (sid, identifier, val) = {
        let gr = gamereg();
        if gr.registry.is_null() {
            return 0;
        }
        if i < 0 || i as usize >= gr.registry_num as usize {
            return 0;
        }
        let sid = crate::config::config().server_id;
        let entry = &*gr.registry.add(i as usize);
        let identifier = {
            let bytes: &[u8] = std::slice::from_raw_parts(entry.str.as_ptr() as *const u8, 64);
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(64);
            String::from_utf8_lossy(&bytes[..end]).into_owned()
        };
        let val = entry.val;
        (sid, identifier, val)
    }; // entry ref dropped here

    // SELECT existing GrgId.
    let id2 = identifier.clone();
    let save_id: Option<u32> = sqlx::query_scalar::<_, u32>(&format!(
        "SELECT GrgId FROM `GameRegistry{sid}` WHERE GrgIdentifier = ?",
    ))
    .bind(id2)
    .fetch_optional(get_pool())
    .await
    .unwrap_or(None);

    match save_id {
        Some(grg_id) if grg_id != 0 => {
            if val == 0 {
                let id2 = identifier.clone();
                let _ = sqlx::query(&format!(
                    "DELETE FROM `GameRegistry{sid}` WHERE GrgIdentifier = ?",
                ))
                .bind(id2)
                .execute(get_pool())
                .await;
            } else {
                let id2 = identifier.clone();
                let _ = sqlx::query(&format!(
                    "UPDATE `GameRegistry{sid}` \
                         SET GrgIdentifier = ?, GrgValue = ? \
                         WHERE GrgId = ?",
                ))
                .bind(id2)
                .bind(val)
                .bind(grg_id)
                .execute(get_pool())
                .await;
            }
        }
        _ => {
            if val > 0 {
                let _ = sqlx::query(&format!(
                    "INSERT INTO `GameRegistry{sid}` \
                         (GrgIdentifier, GrgValue) VALUES (?, ?)",
                ))
                .bind(identifier)
                .bind(val)
                .execute(get_pool())
                .await;
            }
        }
    }

    0
}

// ---------------------------------------------------------------------------
// map_setglobalgamereg — set a game-global registry key/value and persist.
//
//
// Same three-phase logic as map_setglobalreg but operates on `gamereg`.
// Uses MAX_GLOBALREG as the capacity limit (== MAX_GAMEREG == 5000).
// ---------------------------------------------------------------------------

/// Set a key/value pair in the game-global registry, then persist to DB.
///
///
/// # Safety
/// Must be called on the game thread.  `reg` must be a valid non-null
/// null-terminated C string.  `gamereg.registry` must be initialised.
pub async unsafe fn map_setglobalgamereg(reg: *const i8, val: i32) -> i32 {
    if reg.is_null() {
        return 0;
    }

    // Perform all in-memory updates in a sync block, releasing raw pointer refs
    // before the first .await point.
    // Returns Some((save_idx, clear_str)) or None if nothing to save.
    let save_info: Option<(i32, bool)> = {
        let mut gr = gamereg();
        if gr.registry.is_null() {
            return 0;
        }
        let num = gr.registry_num as usize;

        // Search for an existing entry (strcasecmp).
        let mut exist: Option<usize> = None;
        for idx in 0..num {
            let entry = &*gr.registry.add(idx);
            if reg_str_eq(&entry.str, reg) {
                exist = Some(idx);
                break;
            }
        }

        if let Some(idx) = exist {
            let entry = &mut *gr.registry.add(idx);
            if entry.val == val {
                return 0;
            } // value unchanged — skip save
            entry.val = val;
            Some((idx as i32, val == 0))
        } else {
            // Reuse an empty slot.
            let mut reuse_idx: Option<usize> = None;
            for idx in 0..num {
                let entry = &*gr.registry.add(idx);
                if entry.str[0] == 0 {
                    reuse_idx = Some(idx);
                    break;
                }
            }
            if let Some(idx) = reuse_idx {
                let entry = &mut *gr.registry.add(idx);
                copy_cstr_to_reg_str(&mut entry.str, reg);
                entry.val = val;
                Some((idx as i32, false))
            } else if num < MAX_GAMEREG {
                gr.registry_num = (num + 1) as i32;
                let entry = &mut *gr.registry.add(num);
                copy_cstr_to_reg_str(&mut entry.str, reg);
                entry.val = val;
                Some((num as i32, false))
            } else {
                None
            }
        }
    }; // MutexGuard dropped here — safe to await below

    if let Some((save_idx, clear_str)) = save_info {
        map_savegameregistry(save_idx).await;
        if clear_str {
            let gr = gamereg();
            if !gr.registry.is_null() {
                let entry = &mut *gr.registry.add(save_idx as usize);
                entry.str = [0i8; 64];
            }
        }
    }

    0
}

// ---------------------------------------------------------------------------
// map_registrydelete — no-op stub for ABI completeness.
//
// ---------------------------------------------------------------------------

/// Look up a character's name by ID.
/// Returns `"None"` for id=0, empty string if not found.
pub async fn map_id2name(id: u32) -> String {
    if id == 0 {
        return "None".to_string();
    }
    if let Some(pe) = map_id2sd_pc(id) {
        return pe.name.clone();
    }
    sqlx::query_scalar::<_, String>("SELECT `ChaName` FROM `Character` WHERE `ChaId`=?")
        .bind(id)
        .fetch_optional(get_pool())
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Trigger the mapWeather Lua hook when the in-game hour changes.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub fn map_weather(_id: i32, _n: i32) -> bool {
    let ot = old_time.load(Ordering::Relaxed);
    let ct = cur_time.load(Ordering::Relaxed);
    if ot != ct {
        old_time.store(ct, Ordering::Relaxed);
        dispatch("mapWeather", None, &[]);
    }
    false
}

/// Save all online character sessions to the char server.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn map_savechars(_none: i32, _nonetoo: i32) -> i32 {
    for x in 0..crate::session::get_fd_max() {
        let fd = SessionId::from_raw(x);
        if !session_exists(fd) {
            continue;
        }
        if session_get_eof(fd) != 0 {
            continue;
        }
        if let Some(sd) = session_get_data(fd) {
            sl_intif_save(&mut *sd.write() as *mut MapSessionData);
        }
    }
    0
}

/// No-op stub — `map_registrydelete` was commented out in C and has no current callers.
/// Retained for ABI completeness.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
#[allow(dead_code)]
pub unsafe fn map_registrydelete(_m: i32, _i: i32) -> i32 {
    0
}

// ---------------------------------------------------------------------------
// map_lastdeath_mob — record a mob's last-death time in the Spawns table.
//
//
// SQL: UPDATE `Spawns<serverid>` SET SpnLastDeath=last_death
//      WHERE SpnX=startx AND SpnY=starty AND SpnMapId=bl.m AND SpnId=id
// ---------------------------------------------------------------------------

/// Record the mob's last-death timestamp in the `Spawns<serverid>` DB table.
///
///
/// # Safety
/// `p` must be a valid non-null pointer to a `MobSpawnData` struct.
/// Must be called on the game thread after the DB pool is initialised.
pub async unsafe fn map_lastdeath_mob(p: *mut crate::game::mob::MobSpawnData) -> i32 {
    if p.is_null() {
        return 0;
    }

    // Extract all data into Copy values before the first .await so the future is Send.
    let (last_death, startx, starty, map_id, mob_id, sid) = {
        let last_death = (*p).last_death;
        let startx = (*p).startx as i32;
        let starty = (*p).starty as i32;
        let map_id = (*p).m as i32;
        let mob_id = (*p).id as i32;
        let sid = crate::config::config().server_id;
        (last_death, startx, starty, map_id, mob_id, sid)
    }; // p ref dropped here

    let sql = format!(
        "UPDATE `Spawns{sid}` \
         SET SpnLastDeath = ? \
         WHERE SpnX = ? AND SpnY = ? AND SpnMapId = ? AND SpnId = ?",
    );

    if let Err(e) = sqlx::query(&sql)
        .bind(last_death)
        .bind(startx)
        .bind(starty)
        .bind(map_id)
        .bind(mob_id)
        .execute(get_pool())
        .await
    {
        tracing::error!("[map] map_lastdeath_mob failed: {e:#}");
    }

    0
}

// ---------------------------------------------------------------------------
// hasCoref
// ---------------------------------------------------------------------------

/// Returns 1 if the player `sd` has an active co-reference or is contained
/// inside another player that is still in the ID database.  Returns 0 otherwise.
///
///
/// # Safety
/// `sd` must be a valid non-null pointer to a `MapSessionData` that is
/// currently registered in the game world.  Must be called on the game thread.
pub unsafe fn hasCoref(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() {
        return 0;
    }
    // Direct coref: this player is already flagged.
    if (*sd).coref != 0 {
        return 1;
    }
    // Container coref: the container player must still be in the ID database.
    if (*sd).coref_container != 0 && map_id2sd_pc((*sd).coref_container).is_some() {
        return 1;
    }
    0
}

// ---------------------------------------------------------------------------
// map_do_term
// ---------------------------------------------------------------------------

/// Shuts down the map server: save characters, free all map tile/grid
/// allocations, and terminate all subsystem databases.
///
///
/// # Safety
/// Must be called exactly once at shutdown, on the game thread, after all
/// clients have been disconnected.
pub unsafe fn map_do_term() {
    use crate::database::map_db::WarpList;
    use crate::database::map_db::{GlobalReg, MAP_SLOTS, MAX_MAPREG};

    map_savechars(0, 0);
    map_clritem();
    map_termiddb();

    // Free per-slot tile arrays (Rust Vec alloc) and block grid arrays.
    if !crate::database::map_db::raw_map_ptr().is_null() {
        let slots =
            std::slice::from_raw_parts_mut(crate::database::map_db::raw_map_ptr(), MAP_SLOTS);
        for slot in slots.iter_mut() {
            let cells = slot.xs as usize * slot.ys as usize;
            let bcells = slot.bxs as usize * slot.bys as usize;

            if !slot.tile.is_null() && cells > 0 {
                drop(Vec::from_raw_parts(slot.tile, cells, cells));
                slot.tile = std::ptr::null_mut();
            }
            if !slot.obj.is_null() && cells > 0 {
                drop(Vec::from_raw_parts(slot.obj, cells, cells));
                slot.obj = std::ptr::null_mut();
            }
            if !slot.map.is_null() && cells > 0 {
                drop(Vec::from_raw_parts(slot.map, cells, cells));
                slot.map = std::ptr::null_mut();
            }
            if !slot.pass.is_null() && cells > 0 {
                drop(Vec::from_raw_parts(slot.pass, cells, cells));
                slot.pass = std::ptr::null_mut();
            }
            // block/block_mob are no longer allocated (block_grid handles spatial indexing).
            if !slot.warp.is_null() && bcells > 0 {
                drop(Vec::<*mut WarpList>::from_raw_parts(
                    slot.warp, bcells, bcells,
                ));
                slot.warp = std::ptr::null_mut();
            }
            if !slot.registry.is_null() {
                let layout = std::alloc::Layout::array::<GlobalReg>(MAX_MAPREG)
                    .expect("GlobalReg layout overflow");
                std::alloc::dealloc(slot.registry as *mut u8, layout);
                slot.registry = std::ptr::null_mut();
            }
        }
    }

    crate::game::block::map_termblock();
    crate::database::item_db::term();
    crate::database::magic_db::term();
    crate::database::class_db::term();
    println!("[map] Map Server Shutdown");
}

// ---------------------------------------------------------------------------
// Map server globals.
// ---------------------------------------------------------------------------

/// Party/group member ID table. Flat 2-D: groups[256][256] = 65536 elements.
static GROUPS: OnceLock<Mutex<Box<[u32; 65536]>>> = OnceLock::new();

#[inline]
pub fn groups() -> std::sync::MutexGuard<'static, Box<[u32; 65536]>> {
    GROUPS
        .get_or_init(|| Mutex::new(Box::new([0u32; 65536])))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// File descriptor for the logging socket (unused in current build; kept for ABI).
pub static log_fd: AtomicI32 = AtomicI32::new(0);

/// Maximum map ID seen during load; used by map scan loops.
pub static map_max: AtomicI32 = AtomicI32::new(0);

/// Map server public IP string (dotted-decimal, e.g. "127.0.0.1").
pub static MAP_IP_S: OnceLock<[u8; 16]> = OnceLock::new();

/// Logging server IP string (dotted-decimal).
pub static LOG_IP_S: OnceLock<[u8; 16]> = OnceLock::new();

/// Hour value from the previous cron-job tick; used to detect hour changes.
pub static oldHour: AtomicI32 = AtomicI32::new(0);

/// Minute value from the previous cron-job tick; used to detect minute changes.
pub static oldMinute: AtomicI32 = AtomicI32::new(0);

/// Timer ID returned by timer_insert for the cron-job callback.
pub static cronjobtimer: AtomicI32 = AtomicI32::new(0);

// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------

/// Reload all map data (tile, registry) and notify all online players.
///
///
/// # Safety
/// Must be called on the game thread. `maps_dir` and `server_id` are read from
/// `crate::config::config()`.
pub unsafe fn map_reload() -> i32 {
    use crate::database::map_db::map_reload;

    let cfg = crate::config::config();
    let serverid = cfg.server_id;
    if map_reload(&cfg.maps_dir, serverid) != 0 {
        tracing::error!("[map] map_reload failed");
        return -1;
    }

    let n = crate::database::map_db::map_n.load(Ordering::Relaxed) as usize;
    // Map IDs are sparse — must iterate all slots, not just 0..map_n.
    for i in 0..crate::database::map_db::MAP_SLOTS {
        // map_isloaded(i): registry pointer is non-null iff the map was loaded.
        let slot = &*crate::database::map_db::raw_map_ptr().add(i);
        if !slot.registry.is_null() {
            // TODO: broadcast viewport refresh to all players on this map
            // (sl_updatepeople was a no-op stub — implement when needed)
        }
    }

    tracing::info!("[map] Map reload finished. {} maps loaded", n);
    0
}

// ---------------------------------------------------------------------------

//
// Countdown broadcast/disconnect function. Called every 250 ms by timer_insert
// (set up in gm_command.rs shutdown handler). Uses two module-level statics
// to replace C's `static int reset` and `static int diff` local statics.
//
// ---------------------------------------------------------------------------

/// Running countdown value (milliseconds remaining until shutdown).
static RESET_TIMER_REMAINING: AtomicI32 = AtomicI32::new(0);

/// Accumulated elapsed ms since the last broadcast.
static RESET_TIMER_DIFF: AtomicI32 = AtomicI32::new(0);

/// Shutdown countdown timer callback.
///
/// `v1` — initial countdown in ms (only used on first call when `reset == 0`).
/// `v2` — elapsed ms since the last call (timer interval, typically 250).
///
/// Returns 1 when shutdown is triggered, 0 otherwise.
///
///
/// # Safety
/// Must be called on the game thread. Accesses the global session table and
/// `crate::session::get_fd_max()`. Both are single-threaded game globals.
pub unsafe fn map_reset_timer(v1: i32, v2: i32) -> i32 {
    let mut remaining = RESET_TIMER_REMAINING.load(Ordering::Relaxed);
    let mut diff = RESET_TIMER_DIFF.load(Ordering::Relaxed);

    if remaining == 0 {
        remaining = v1;
    }

    remaining -= v2;
    diff += v2;
    RESET_TIMER_REMAINING.store(remaining, Ordering::Relaxed);
    RESET_TIMER_DIFF.store(diff, Ordering::Relaxed);

    if remaining <= 250 {
        let msg = c"Chaos is rising up. Please re-enter in a few seconds.";
        crate::game::map_parse::chat::clif_broadcast(msg.as_ptr(), -1);
    }

    if remaining <= 0 {
        // Disconnect all active sessions, then request shutdown.
        for x in 0..crate::session::get_fd_max() {
            let fd = SessionId::from_raw(x);
            if session_exists(fd) {
                let sd = session_get_data(fd);
                if let Some(ref sd_arc) = sd {
                    if session_get_eof(fd) == 0 {
                        let player_id = sd_arc.id;
                        crate::database::blocking_run_async(crate::database::assert_send(
                            async move {
                                if let Some(pe) = map_id2sd_pc(player_id) {
                                    crate::game::client::handlers::clif_handle_disconnect(&pe)
                                        .await;
                                }
                            },
                        ));
                        // session_call_parse is now async; schedule it on the LocalSet.
                        // map_reset_timer is a sync TimerFn so it cannot .await directly.
                        // The session eof flag (set below) ensures clif_parse sees the
                        // disconnect state when the spawned task runs.
                        tokio::task::spawn_local(session_call_parse(fd));
                        if let Some(s) = get_session_manager().get_session(fd) {
                            if let Ok(mut session) = s.try_lock() {
                                session.flush_read_buffer();
                            }
                        }
                        session_set_eof(fd, 1);
                    }
                }
            }
        }
        request_shutdown();
        RESET_TIMER_REMAINING.store(0, Ordering::Relaxed);
        RESET_TIMER_DIFF.store(0, Ordering::Relaxed);
        return 1;
    }

    if remaining <= 60_000 {
        if diff >= 10_000 {
            let msg = format!("Reset in {} seconds\0", remaining / 1000);
            crate::game::map_parse::chat::clif_broadcast(msg.as_ptr() as *const i8, -1);
            RESET_TIMER_DIFF.store(0, Ordering::Relaxed);
        }
    } else if remaining <= 3_600_000 {
        if diff >= 300_000 {
            let msg = format!("Reset in {} minutes\0", remaining / 60_000);
            crate::game::map_parse::chat::clif_broadcast(msg.as_ptr() as *const i8, -1);
            RESET_TIMER_DIFF.store(0, Ordering::Relaxed);
        }
    } else if remaining > 3_600_000 && diff >= 3_600_000 {
        let msg = format!("Reset in {} hours\0", remaining / 3_600_000);
        crate::game::map_parse::chat::clif_broadcast(msg.as_ptr() as *const i8, -1);
        RESET_TIMER_DIFF.store(0, Ordering::Relaxed);
    }

    0
}
