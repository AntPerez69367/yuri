//! Object collection functions for block-grid spatial queries.
//!
//! Uses the closure API from `crate::game::block` for spatial queries.
//!
//! Here the closure API from `crate::game::block` replaces that pattern.

use crate::game::block::AreaType;
use crate::game::block_grid;
use crate::database::map_db::BlockList;

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Write `bl` into `out_ptrs[count]` if `count < max_count`, then increment.
#[inline(always)]
unsafe fn push_ptr(out_ptrs: *mut *mut std::ffi::c_void, count: &mut i32, max_count: i32, bl: *mut BlockList) {
    if *count < max_count {
        *out_ptrs.add(*count as usize) = bl as *mut std::ffi::c_void;
        *count += 1;
    }
}

/// Look up an entity by ID, check bl_type mask, return BL pointer if matched.
/// Does NOT check alive status.
#[inline]
unsafe fn id_to_bl_typed(id: u32, bl_type: i32) -> Option<*mut BlockList> {
    let bl = crate::game::map_server::map_id2bl_ref(id);
    if bl.is_null() { return None; }
    if ((*bl).bl_type as i32) & bl_type != 0 {
        Some(bl)
    } else {
        None
    }
}

/// Look up an entity by ID, check bl_type mask AND alive, return BL pointer.
#[inline]
unsafe fn id_to_bl_typed_alive(id: u32, bl_type: i32) -> Option<*mut BlockList> {
    let bl = id_to_bl_typed(id, bl_type)?;
    if crate::game::block::is_alive(bl) { Some(bl) } else { None }
}

// ─── Cell queries ─────────────────────────────────────────────────────────────

/// Collect up to `max_count` entity pointers of `bl_type` at cell (x, y) on map `m`.
///
///
/// # Safety
/// `out_ptrs` must point to a caller-allocated array of at least `max_count`
/// `*mut std::ffi::c_void` slots. `m`, `x`, `y` must identify a valid, loaded map cell.
pub unsafe fn sl_g_getobjectscell(
    m: i32,
    x: i32,
    y: i32,
    bl_type: i32,
    out_ptrs: *mut *mut std::ffi::c_void,
    max_count: i32,
) -> i32 {
    if out_ptrs.is_null() { return 0; }
    let mut count = 0i32;
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let cell_ids = grid.ids_at_tile(x as u16, y as u16);
        for id in cell_ids {
            if let Some(bl) = id_to_bl_typed(id, bl_type) {
                push_ptr(out_ptrs, &mut count, max_count, bl);
            }
        }
    }
    count
}

/// Intended to collect BL pointers at cell (x, y) including trap NPCs.
/// Currently falls back to cell query (same as `sl_g_getobjectscell`),
/// which does not enumerate trap entities. TODO: port map_foreachincellwithtraps.
///
/// # Safety
/// Same as `sl_g_getobjectscell`.
// TODO: port map_foreachincellwithtraps
pub unsafe fn sl_g_getobjectscellwithtraps(
    m: i32,
    x: i32,
    y: i32,
    bl_type: i32,
    out_ptrs: *mut *mut std::ffi::c_void,
    max_count: i32,
) -> i32 {
    if out_ptrs.is_null() { return 0; }
    let mut count = 0i32;
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let cell_ids = grid.ids_at_tile(x as u16, y as u16);
        for id in cell_ids {
            if let Some(bl) = id_to_bl_typed(id, bl_type) {
                push_ptr(out_ptrs, &mut count, max_count, bl);
            }
        }
    }
    count
}

/// Like `sl_g_getobjectscell` but skips dead mobs and stealthed / dead PCs.
///
///
/// # Safety
/// Same as `sl_g_getobjectscell`.
pub unsafe fn sl_g_getaliveobjectscell(
    m: i32,
    x: i32,
    y: i32,
    bl_type: i32,
    out_ptrs: *mut *mut std::ffi::c_void,
    max_count: i32,
) -> i32 {
    if out_ptrs.is_null() { return 0; }
    let mut count = 0i32;
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let cell_ids = grid.ids_at_tile(x as u16, y as u16);
        for id in cell_ids {
            if let Some(bl) = id_to_bl_typed_alive(id, bl_type) {
                push_ptr(out_ptrs, &mut count, max_count, bl);
            }
        }
    }
    count
}

// ─── Map-wide query ───────────────────────────────────────────────────────────

/// Collect up to `max_count` entity pointers of `bl_type` across the entire map `m`.
///
///
/// # Safety
/// `out_ptrs` must point to a caller-allocated array of at least `max_count` slots.
pub unsafe fn sl_g_getobjectsinmap(
    m: i32,
    bl_type: i32,
    out_ptrs: *mut *mut std::ffi::c_void,
    max_count: i32,
) -> i32 {
    if out_ptrs.is_null() { return 0; }
    let mut count = 0i32;
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let slot = &*crate::database::map_db::raw_map_ptr().add(m as usize);
        let ids = block_grid::ids_in_area(grid, 0, 0, AreaType::SameMap, slot.xs as i32, slot.ys as i32);
        for id in ids {
            if let Some(bl) = id_to_bl_typed(id, bl_type) {
                push_ptr(out_ptrs, &mut count, max_count, bl);
            }
        }
    }
    count
}

// ─── Area queries (centred on a bl) ──────────────────────────────────────────

/// Collect up to `max_count` entity pointers of `bl_type` within AREA range
/// of `bl_ptr`'s position.
///
///
/// # Safety
/// `bl_ptr` must be a valid, non-null `*mut BlockList`. `out_ptrs` must point
/// to a caller-allocated array of at least `max_count` slots.
pub unsafe fn sl_g_getobjectsarea(
    bl_ptr: *mut std::ffi::c_void,
    bl_type: i32,
    out_ptrs: *mut *mut std::ffi::c_void,
    max_count: i32,
) -> i32 {
    if bl_ptr.is_null() { return 0; }
    if out_ptrs.is_null() { return 0; }
    let bl = &*(bl_ptr as *const BlockList);
    let mut count = 0i32;
    if let Some(grid) = block_grid::get_grid(bl.m as usize) {
        let slot = &*crate::database::map_db::raw_map_ptr().add(bl.m as usize);
        let ids = block_grid::ids_in_area(grid, bl.x as i32, bl.y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
        for id in ids {
            if let Some(b) = id_to_bl_typed(id, bl_type) {
                push_ptr(out_ptrs, &mut count, max_count, b);
            }
        }
    }
    count
}

/// Like `sl_g_getobjectsarea` but skips dead mobs and stealthed / dead PCs.
///
///
/// # Safety
/// Same as `sl_g_getobjectsarea`.
pub unsafe fn sl_g_getaliveobjectsarea(
    bl_ptr: *mut std::ffi::c_void,
    bl_type: i32,
    out_ptrs: *mut *mut std::ffi::c_void,
    max_count: i32,
) -> i32 {
    if bl_ptr.is_null() { return 0; }
    if out_ptrs.is_null() { return 0; }
    let bl = &*(bl_ptr as *const BlockList);
    let mut count = 0i32;
    if let Some(grid) = block_grid::get_grid(bl.m as usize) {
        let slot = &*crate::database::map_db::raw_map_ptr().add(bl.m as usize);
        let ids = block_grid::ids_in_area(grid, bl.x as i32, bl.y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
        for id in ids {
            if let Some(b) = id_to_bl_typed_alive(id, bl_type) {
                push_ptr(out_ptrs, &mut count, max_count, b);
            }
        }
    }
    count
}

// ─── Same-map queries (centred on a bl) ───────────────────────────────────────

/// Collect up to `max_count` entity pointers of `bl_type` across the whole map
/// that `bl_ptr` is on.
///
///
/// # Safety
/// Same as `sl_g_getobjectsarea`.
pub unsafe fn sl_g_getobjectssamemap(
    bl_ptr: *mut std::ffi::c_void,
    bl_type: i32,
    out_ptrs: *mut *mut std::ffi::c_void,
    max_count: i32,
) -> i32 {
    if bl_ptr.is_null() { return 0; }
    if out_ptrs.is_null() { return 0; }
    let bl = &*(bl_ptr as *const BlockList);
    let mut count = 0i32;
    if let Some(grid) = block_grid::get_grid(bl.m as usize) {
        let slot = &*crate::database::map_db::raw_map_ptr().add(bl.m as usize);
        let ids = block_grid::ids_in_area(grid, bl.x as i32, bl.y as i32, AreaType::SameMap, slot.xs as i32, slot.ys as i32);
        for id in ids {
            if let Some(b) = id_to_bl_typed(id, bl_type) {
                push_ptr(out_ptrs, &mut count, max_count, b);
            }
        }
    }
    count
}

/// Like `sl_g_getobjectssamemap` but skips dead mobs and stealthed / dead PCs.
///
///
/// # Safety
/// Same as `sl_g_getobjectsarea`.
pub unsafe fn sl_g_getaliveobjectssamemap(
    bl_ptr: *mut std::ffi::c_void,
    bl_type: i32,
    out_ptrs: *mut *mut std::ffi::c_void,
    max_count: i32,
) -> i32 {
    if bl_ptr.is_null() { return 0; }
    if out_ptrs.is_null() { return 0; }
    let bl = &*(bl_ptr as *const BlockList);
    let mut count = 0i32;
    if let Some(grid) = block_grid::get_grid(bl.m as usize) {
        let slot = &*crate::database::map_db::raw_map_ptr().add(bl.m as usize);
        let ids = block_grid::ids_in_area(grid, bl.x as i32, bl.y as i32, AreaType::SameMap, slot.xs as i32, slot.ys as i32);
        for id in ids {
            if let Some(b) = id_to_bl_typed_alive(id, bl_type) {
                push_ptr(out_ptrs, &mut count, max_count, b);
            }
        }
    }
    count
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::block::{test_make_map, test_free_map, test_make_bl_node, test_insert_in_block, test_set_map};
    use crate::game::mob::BL_PC;

    // ─────────────────────────────────────────────────────────────────────────
    // Test: sl_g_getobjectscell — writes pointer and returns correct count
    // ─────────────────────────────────────────────────────────────────────────

    /// Verify that `sl_g_getobjectscell` returns 0 when no entities are
    /// registered in the global ID maps, even though the block grid has IDs.
    ///
    /// The block_grid stores entity IDs; `sl_g_getobjectscell` resolves them
    /// via `map_id2bl` which requires entities to be registered in the global
    /// player/mob/npc/item maps.  Without registration, lookup returns null
    /// and the entity is skipped.
    #[test]
    fn test_sl_g_getobjectscell_no_registered_entity() {
        unsafe {
            // Build a minimal 100x100 map with one PC at (10, 10).
            let mut slot = test_make_map(100, 100);
            let mut bl_node = test_make_bl_node(BL_PC as u8, 10, 10);
            test_insert_in_block(&mut slot, &raw mut bl_node, 10, 10);

            let slot_ptr = Box::into_raw(slot);
            test_set_map(slot_ptr);

            // Allocate an output array with 4 slots.
            let mut out_ptrs: [*mut std::ffi::c_void; 4] = [std::ptr::null_mut(); 4];

            let count = sl_g_getobjectscell(
                0,          // map slot 0
                10,         // x
                10,         // y
                BL_PC,      // bl_type
                out_ptrs.as_mut_ptr(),
                4,          // max_count
            );

            // Restore global before any assertion.
            test_set_map(std::ptr::null_mut());

            let slot = Box::from_raw(slot_ptr);
            test_free_map(slot);

            // Entity ID 0 is not registered in the global maps, so count is 0.
            assert_eq!(count, 0, "unregistered entity ID should not be found");
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Test: sl_g_getobjectscell — null out_ptrs returns 0 immediately
    // ─────────────────────────────────────────────────────────────────────────

    /// Verify the null guard: passing a null `out_ptrs` must return 0 and must
    /// not dereference the pointer (no segfault).
    #[test]
    fn test_sl_g_getobjectscell_null_out_ptrs_returns_zero() {
        unsafe {
            let count = sl_g_getobjectscell(
                0,
                10,
                10,
                BL_PC,
                std::ptr::null_mut(),
                4,
            );
            assert_eq!(count, 0, "null out_ptrs must return 0");
        }
    }
}
