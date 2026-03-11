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

use crate::database::map_db::{BlockList, MAP_SLOTS};
use crate::game::mob::{BL_MOB, MOB_DEAD, MobSpawnData};

/// Returns the live map pointer (delegates to `map_db::raw_map_ptr()`).
#[inline(always)]
unsafe fn map_ptr() -> *mut crate::database::map_db::MapData {
    crate::database::map_db::raw_map_ptr()
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

