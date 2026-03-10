//! Block grid management and spatial query types.
//!
//! Provides entity lifecycle functions (`map_addblock`, `map_delblock`,
//! `map_moveblock`) that delegate to the safe `BlockGrid` in `block_grid.rs`.
//!
//! # Safety
//! All public functions are `unsafe` because they dereference raw pointers into
//! the map grid. Callers must ensure:
//! - The `map` global is initialized (via `rust_map_init` + `map_initblock`).
//! - `m` is a valid, loaded map slot index.
#![allow(non_upper_case_globals)]
#![allow(static_mut_refs)]

use crate::database::map_db::{BlockList, MAP_SLOTS};
use crate::game::mob::{BL_MOB, MOB_DEAD, MobSpawnData};

// Module-level map pointer override.
// In production, this is null and `map_ptr()` delegates to `raw_map_ptr()`.
// In tests, this can be set via `test_set_map()` to inject a custom map without
// touching the global OnceLock.
// SAFETY: Only written in test code from a single test thread (guarded by MAP_MUTEX).
static mut map: *mut crate::database::map_db::MapData = std::ptr::null_mut();

/// Returns the live map pointer.
/// Checks the module-local override first (used by tests); otherwise delegates
/// to `map_db::raw_map_ptr()` (backed by `OnceLock<MapPtr>`).
#[inline(always)]
unsafe fn map_ptr() -> *mut crate::database::map_db::MapData {
    if !map.is_null() { map } else { crate::database::map_db::raw_map_ptr() }
}

// ─── Area type ───────────────────────────────────────────────────────────────

/// Spatial query shape for block-grid traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AreaType {
    /// Fixed 18x16 window centred on (x, y).  Covers +/-19 columns and +/-17 rows
    /// (NX+1 = 19, NY+1 = 17) so that the full client viewport is always included.
    Area,
    /// Only the cells outside the current viewport that have just scrolled into view
    /// (the "corner" strips).  Used for incremental send of entities to clients.
    Corner,
    /// The clamped 18x16 viewport window (same size as `Area` but shifted so it
    /// never exceeds the map boundary).
    SameArea,
    /// The entire map.
    SameMap,
}

// ─── map_is_loaded ───────────────────────────────────────────────────────────

/// Return `true` if map slot `m` is loaded (has a non-null registry pointer).
///
/// # Safety
/// `map` global must be initialized.
pub unsafe fn map_is_loaded(m: i32) -> bool {
    if m < 0 || map_ptr().is_null() {
        return false;
    }
    let m_idx = m as usize;
    if m_idx >= MAP_SLOTS {
        return false;
    }
    let slot = &*map_ptr().add(m_idx);
    !slot.registry.is_null()
}

// ─── is_alive ────────────────────────────────────────────────────────────────

/// Return `true` if the entity is alive and visible:
/// - Mobs: `state != MOB_DEAD`.
/// - PCs: not dead (`status.state != 1`) and not stealthed (`optFlags & OPT_FLAG_STEALTH == 0`).
/// - All other entity types: always `true`.
///
/// # Safety
/// `bl` must be a valid, aligned pointer to a live `BlockList` (or a struct
/// that begins with `BlockList` as its first field, such as `MapSessionData`
/// or `MobSpawnData`).
pub unsafe fn is_alive(bl: *mut BlockList) -> bool {
    use crate::game::mob::BL_PC;
    use crate::game::pc::{MapSessionData, OPT_FLAG_STEALTH, PC_DIE};

    if bl.is_null() {
        return false;
    }
    let b = &*bl;
    let bl_type = b.bl_type as i32;

    if bl_type == BL_MOB {
        let mob = bl as *mut MobSpawnData;
        (*mob).state != MOB_DEAD
    } else if bl_type == BL_PC {
        let sd = bl as *mut MapSessionData;
        let dead = (*sd).status.state == PC_DIE as i8;
        let stealth = ((*sd).optFlags & OPT_FLAG_STEALTH) != 0;
        !dead && !stealth
    } else {
        true
    }
}

// ─── map_initblock ──────────────────────────────────────────────────────────

/// Initialize the block grid for all loaded map slots.
/// Allocates warp arrays and creates safe block grids.
pub unsafe fn map_initblock() {
    crate::game::block_grid::init_grids();
    if map_ptr().is_null() { return; }
    let slots = std::slice::from_raw_parts_mut(map_ptr(), crate::database::map_db::MAP_SLOTS);
    for (n, slot) in slots.iter_mut().enumerate() {
        if slot.bxs == 0 || slot.bys == 0 { continue; }
        let cells = slot.bxs as usize * slot.bys as usize;
        // Allocate warp array (still used by warp system).
        slot.warp = alloc_ptr_array::<crate::database::map_db::WarpList>(cells);
        crate::game::block_grid::create_grid(n, slot.xs, slot.ys);
    }
}

fn alloc_ptr_array<T>(len: usize) -> *mut *mut T {
    let mut v: Vec<*mut T> = vec![std::ptr::null_mut(); len];
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Free block grid arrays for all map slots (no-op, matches C).
pub unsafe fn map_termblock() {}

// ─── map_addblock ───────────────────────────────────────────────────────────

/// Insert `bl` into the block grid.
pub unsafe fn map_addblock(bl: *mut crate::database::map_db::BlockList) -> i32 {
    if bl.is_null() { return 1; }
    let bl = &mut *bl;

    let m = bl.m as usize;
    if m >= crate::database::map_db::MAP_SLOTS || map_ptr().is_null() {
        tracing::error!("[map_addblock] invalid map id id={} m={m}", bl.id);
        return 1;
    }
    let slot = &*map_ptr().add(m);

    if slot.registry.is_null() {
        tracing::error!("[map_addblock] map not loaded id={} m={m}", bl.id);
        return 1;
    }

    let x = bl.x as i32;
    let y = bl.y as i32;
    if x < 0 || x >= slot.xs as i32 || y < 0 || y >= slot.ys as i32 {
        tracing::error!("[map_addblock] out-of-bounds m={m} x={x} y={y} xs={} ys={} id={}", slot.xs, slot.ys, bl.id);
        return 1;
    }

    if let Some(g) = crate::game::block_grid::get_grid_mut(m) {
        g.add(bl.id, bl.x, bl.y, bl.bl_type);
    }

    0
}

// ─── map_delblock ───────────────────────────────────────────────────────────

/// Remove `bl` from the block grid.
pub unsafe fn map_delblock(bl: *mut crate::database::map_db::BlockList) -> i32 {
    if bl.is_null() { return 0; }
    let bl = &mut *bl;

    let m = bl.m as usize;
    if let Some(g) = crate::game::block_grid::get_grid_mut(m) {
        g.remove(bl.id, bl.x, bl.y, bl.bl_type);
    }

    0
}

// ─── map_moveblock ──────────────────────────────────────────────────────────

/// Remove `bl` from current cell, update coords, re-insert.
pub unsafe fn map_moveblock(bl: *mut crate::database::map_db::BlockList, x1: i32, y1: i32) -> i32 {
    if bl.is_null() { return 0; }
    let b = &mut *bl;
    let m = b.m as usize;
    let old_x = b.x;
    let old_y = b.y;
    let new_x = x1 as u16;
    let new_y = y1 as u16;

    if let Some(g) = crate::game::block_grid::get_grid_mut(m) {
        g.move_entity(b.id, old_x, old_y, new_x, new_y);
    }

    b.x = new_x;
    b.y = new_y;
    0
}

// ─── Helper: map user count ─────────────────────────────────────────────────

/// Return the number of players on map `m`, using the block grid's user_count.
pub fn map_user_count(m: usize) -> i32 {
    crate::game::block_grid::get_grid(m).map(|g| g.user_count).unwrap_or(0)
}

// ─── Test helpers (pub(crate) so sibling test modules can reuse them) ─────────

/// Set the module-level `map` static to `ptr`.  Only available in test builds.
/// Used by sibling modules (e.g. `game::scripting::object_collect`) that call
/// block functions and need to inject a test map.
///
/// # Safety
/// The caller must ensure `ptr` stays valid for the lifetime of the test and
/// that no other thread concurrently reads the `map` global.
#[cfg(test)]
pub(crate) unsafe fn test_set_map(ptr: *mut crate::database::map_db::MapData) {
    map = ptr;
}

/// Allocate a zeroed `MapData` on the heap, set up minimum block-grid fields
/// for a `xs x ys` map, and return the `Box`.  The caller must eventually free
/// it via `test_free_map`.
///
/// # Safety
/// Uses raw allocation.  The returned `Box` must not be dropped normally -- pass
/// it to `test_free_map` to release the inner arrays before dropping the box.
#[cfg(test)]
pub(crate) unsafe fn test_make_map(xs: u16, ys: u16) -> Box<crate::database::map_db::MapData> {
    use crate::database::map_db::{GlobalReg, MapData, WarpList, BLOCK_SIZE};

    let layout = std::alloc::Layout::new::<MapData>();
    let raw = std::alloc::alloc_zeroed(layout) as *mut MapData;
    assert!(!raw.is_null());
    let mut slot = Box::from_raw(raw);

    slot.xs = xs;
    slot.ys = ys;
    slot.bxs = ((xs as usize + BLOCK_SIZE - 1) / BLOCK_SIZE) as u16;
    slot.bys = ((ys as usize + BLOCK_SIZE - 1) / BLOCK_SIZE) as u16;

    let cells = slot.bxs as usize * slot.bys as usize;

    let warp_layout = std::alloc::Layout::array::<*mut WarpList>(cells).unwrap();
    slot.warp = std::alloc::alloc_zeroed(warp_layout) as *mut *mut WarpList;

    let reg_layout = std::alloc::Layout::array::<GlobalReg>(1).unwrap();
    slot.registry = std::alloc::alloc_zeroed(reg_layout) as *mut GlobalReg;

    // Initialize block_grid for map slot 0 so test_insert_in_block works.
    crate::game::block_grid::init_grids();
    crate::game::block_grid::create_grid(0, xs, ys);

    slot
}

/// Free the inner arrays of a test `MapData` box (produced by `test_make_map`),
/// then leak the box so Rust does not double-free.  Only available in test builds.
///
/// # Safety
/// `slot` must be a box returned by `test_make_map` that has not been freed.
#[cfg(test)]
pub(crate) unsafe fn test_free_map(slot: Box<crate::database::map_db::MapData>) {
    use crate::database::map_db::{GlobalReg, WarpList};

    let cells = slot.bxs as usize * slot.bys as usize;
    std::alloc::dealloc(
        slot.warp as *mut u8,
        std::alloc::Layout::array::<*mut WarpList>(cells).unwrap(),
    );
    std::alloc::dealloc(
        slot.registry as *mut u8,
        std::alloc::Layout::array::<GlobalReg>(1).unwrap(),
    );
    let _ = Box::into_raw(slot); // prevent double-free via Box drop
}

/// Build a minimal live `BlockList` node.  Only available in test builds.
///
/// # Safety
/// The returned node has next/prev set to null.
#[cfg(test)]
pub(crate) unsafe fn test_make_bl_node(bl_type: u8, x: u16, y: u16) -> crate::database::map_db::BlockList {
    crate::database::map_db::BlockList {
        next:          std::ptr::null_mut(),
        prev:          std::ptr::null_mut(),
        id:            0,
        bx:            0,
        by:            0,
        graphic_id:    0,
        graphic_color: 0,
        m:             0,
        x,
        y,
        bl_type,
        subtype:       0,
    }
}

/// Insert `node` into the block_grid for the cell containing (x, y).
/// Only available in test builds.
///
/// # Safety
/// A block_grid must have been created for map slot 0 before calling this.
#[cfg(test)]
pub(crate) unsafe fn test_insert_in_block(
    _slot: &mut crate::database::map_db::MapData,
    node: *mut crate::database::map_db::BlockList,
    x: u16,
    y: u16,
) {
    // Insert into the safe block_grid instead of the linked list.
    // The node's bl_type is used for user_count tracking.
    let bl = &*node;
    if let Some(g) = crate::game::block_grid::get_grid_mut(0) {
        g.add(bl.id, x, y, bl.bl_type);
    }
}
