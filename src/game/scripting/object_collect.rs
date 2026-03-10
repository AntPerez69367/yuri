//! Object collection functions for block-grid spatial queries.
//!
//! Uses the closure API from `crate::game::block` for spatial queries.
//!
//! Here the closure API from `crate::game::block` replaces that pattern.

use crate::game::block::{self, AreaType};
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
    block::foreach_in_cell(m, x, y, bl_type, |bl| {
        push_ptr(out_ptrs, &mut count, max_count, bl);
        0
    });
    count
}

/// Intended to collect BL pointers at cell (x, y) including trap NPCs.
/// Currently falls back to `foreach_in_cell` (same as `sl_g_getobjectscell`),
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
    block::foreach_in_cell(m, x, y, bl_type, |bl| {
        push_ptr(out_ptrs, &mut count, max_count, bl);
        0
    });
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
    block::foreach_in_cell(m, x, y, bl_type, |bl| {
        if block::is_alive(bl) {
            push_ptr(out_ptrs, &mut count, max_count, bl);
        }
        0
    });
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
    block::foreach_in_area(m, 0, 0, AreaType::SameMap, bl_type, |bl| {
        push_ptr(out_ptrs, &mut count, max_count, bl);
        0
    });
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
    block::foreach_in_area(bl.m as i32, bl.x as i32, bl.y as i32, AreaType::Area, bl_type, |b| {
        push_ptr(out_ptrs, &mut count, max_count, b);
        0
    });
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
    block::foreach_in_area(bl.m as i32, bl.x as i32, bl.y as i32, AreaType::Area, bl_type, |b| {
        if block::is_alive(b) {
            push_ptr(out_ptrs, &mut count, max_count, b);
        }
        0
    });
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
    block::foreach_in_area(bl.m as i32, bl.x as i32, bl.y as i32, AreaType::SameMap, bl_type, |b| {
        push_ptr(out_ptrs, &mut count, max_count, b);
        0
    });
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
    block::foreach_in_area(bl.m as i32, bl.x as i32, bl.y as i32, AreaType::SameMap, bl_type, |b| {
        if block::is_alive(b) {
            push_ptr(out_ptrs, &mut count, max_count, b);
        }
        0
    });
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

    /// Verify that `sl_g_getobjectscell` writes the correct `*mut BlockList`
    /// pointer into `out_ptrs[0]` and returns 1 when one PC entity is at the
    /// queried cell.
    ///
    /// This exercises the `push_ptr` write-into-array path end-to-end through
    /// the Lua-callable entry point.
    #[test]
    fn test_sl_g_getobjectscell_writes_ptr_and_count() {
        unsafe {
            // Build a minimal 100×100 map with one PC at (10, 10).
            let mut slot = test_make_map(100, 100);
            let mut bl_node = test_make_bl_node(BL_PC as u8, 10, 10);
            test_insert_in_block(&mut slot, &raw mut bl_node, 10, 10);

            let slot_ptr = Box::into_raw(slot);
            let orig_map_ptr = {
                // Capture original map pointer so we can restore it.
                // We read it indirectly by calling test_set_map with null, then
                // swap back the original via a second call.  Instead, just
                // set ours and restore null at the end (tests run single-threaded).
                std::ptr::null_mut::<crate::database::map_db::MapData>()
            };
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

            // Restore global before any assertion (so map isn't left dangling).
            test_set_map(orig_map_ptr);

            let slot = Box::from_raw(slot_ptr);
            test_free_map(slot);

            assert_eq!(count, 1, "should find exactly 1 PC entity at (10, 10)");
            assert!(
                !out_ptrs[0].is_null(),
                "out_ptrs[0] must be non-null (the BlockList pointer)"
            );
            assert_eq!(
                out_ptrs[0] as *mut BlockList,
                &raw mut bl_node,
                "out_ptrs[0] must equal the address of the inserted BlockList node"
            );
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
