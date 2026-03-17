//! Block grid management and spatial query types.
//!
//! Provides entity lifecycle functions (`map_addblock`, `map_delblock`,
//! `map_moveblock`) that delegate to the safe `BlockGrid` in `block_grid.rs`.
//!
//! # Safety
//! All public functions are `unsafe` because they dereference raw pointers into
//! the map grid. Callers must ensure:
//! - The `map` global is initialized (via `map_init` + `map_initblock`).
//! - `m` is a valid, loaded map slot index.
#![allow(non_upper_case_globals)]

use crate::database::map_db::MAP_SLOTS;
use crate::game::mob::MOB_DEAD;

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


/// Return `true` if the entity with the given ID is alive, using typed lookups.
pub fn is_alive_id(id: u32) -> bool {
    use crate::game::mob::{MOB_START_NUM, FLOORITEM_START_NUM, NPC_START_NUM};
    use crate::game::pc::{OPT_FLAG_STEALTH, PC_DIE};
    use crate::game::map_server::{map_id2sd_pc, map_id2mob_ref};

    if id < MOB_START_NUM {
        // PC
        if let Some(arc) = map_id2sd_pc(id) {
            let sd = arc.read();
            let dead = sd.player.combat.state == PC_DIE as i8;
            let stealth = (sd.optFlags & OPT_FLAG_STEALTH) != 0;
            return !dead && !stealth;
        }
    } else if id >= NPC_START_NUM {
        return true; // NPCs are always alive
    } else if id >= FLOORITEM_START_NUM {
        return true; // Floor items are always alive
    } else {
        // Mob
        if let Some(arc) = map_id2mob_ref(id) {
            let mob = arc.read();
            return mob.state != MOB_DEAD;
        }
    }
    false
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

// ─── Value-based grid API ────────────────────────────────────────────────────

/// Insert entity into the block grid by ID and coordinates.
pub fn map_addblock_id(id: u32, bl_type: u8, m: u16, x: u16, y: u16) -> i32 {
    let m = m as usize;
    if m >= crate::database::map_db::MAP_SLOTS { return 1; }
    if let Some(g) = crate::game::block_grid::get_grid_mut(m) {
        g.add(id, x, y, bl_type);
    }
    0
}

/// Remove entity from the block grid by ID and map.
pub fn map_delblock_id(id: u32, m: u16) -> i32 {
    let m = m as usize;
    if m >= crate::database::map_db::MAP_SLOTS { return 0; }
    if let Some(g) = crate::game::block_grid::get_grid_mut(m) {
        g.remove(id);
    }
    0
}

/// Move entity on the grid by ID and coordinates.
pub fn map_moveblock_id(id: u32, m: u16, old_x: u16, old_y: u16, new_x: u16, new_y: u16) -> i32 {
    let m = m as usize;
    if m >= crate::database::map_db::MAP_SLOTS { return 0; }
    if let Some(g) = crate::game::block_grid::get_grid_mut(m) {
        g.move_entity(id, old_x, old_y, new_x, new_y);
    }
    0
}

/// Return the number of players on map `m`, using the block grid's user_count.
pub fn map_user_count(m: usize) -> i32 {
    crate::game::block_grid::get_grid(m).map(|g| g.user_count).unwrap_or(0)
}

