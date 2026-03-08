//! Closure-based spatial query API for the block grid.
//!
//! This module wraps the raw block grid traversal logic from `ffi::block` in an
//! idiomatic Rust closure API. Closures capture their environment, so captured
//! variables replace the `va_list` / `*mut c_void` pattern used by the C callbacks
//! in `sl_compat.c`. No nightly feature required; type-safe at compile time.
//!
//! # Safety
//! All public functions are `unsafe` because they dereference raw pointers into the
//! map grid and the entity linked-list chains. Callers must ensure:
//! - The `map` global is initialized (via `rust_map_init` + `map_initblock`).
//! - `m` is a valid, loaded map slot index.
//! - Entity pointers returned by callbacks are not stored across any mutation of the
//!   block grid (add/del/move).

use crate::database::map_db::{BlockList, MAP_SLOTS, BLOCK_SIZE};
#[cfg(not(test))]
use crate::database::map_db::map;
use crate::game::mob::{BL_MOB, MOB_DEAD, MobSpawnData};

// In test builds crate::ffi is absent.  We provide local substitutes:
//   - a module-level `map` static (same name as the production global)
//   - BL_LIST_MAX constant matching ffi::block::BL_LIST_MAX
//   - a sentinel BlockList node standing in for ffi::block::bl_head
#[cfg(test)]
static mut map: *mut crate::database::map_db::MapData = std::ptr::null_mut();

/// Entity list capacity cap — matches `ffi::block::BL_LIST_MAX`.
/// In non-test builds we read the authoritative constant from the ffi module.
/// In test builds crate::ffi is absent so we inline the same literal value.
#[cfg(not(test))]
#[inline(always)]
fn bl_list_max() -> usize { crate::game::block::BL_LIST_MAX }
#[cfg(test)]
#[inline(always)]
fn bl_list_max() -> usize { 32768 }

/// Sentinel node used in tests as a substitute for `ffi::block::bl_head`.
/// An entity whose `prev` points here is considered live by the traversal.
#[cfg(test)]
static mut TEST_BL_HEAD: crate::database::map_db::BlockList = crate::database::map_db::BlockList {
    next:          std::ptr::null_mut(),
    prev:          std::ptr::null_mut(),
    id:            0,
    bx:            0,
    by:            0,
    graphic_id:    0,
    graphic_color: 0,
    m:             0,
    x:             0,
    y:             0,
    bl_type:       0,
    subtype:       0,
};

// ─── Area type ───────────────────────────────────────────────────────────────

/// Spatial query shape, mirroring the `area` enum in `c_src/map_parse.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AreaType {
    /// Fixed 18×16 window centred on (x, y).  Covers ±19 columns and ±17 rows
    /// (NX+1 = 19, NY+1 = 17) so that the full client viewport is always included.
    Area,
    /// Only the cells outside the current viewport that have just scrolled into view
    /// (the "corner" strips).  Used for incremental send of entities to clients.
    Corner,
    /// The clamped 18×16 viewport window (same size as `Area` but shifted so it
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
    if m < 0 || map.is_null() {
        return false;
    }
    let m_idx = m as usize;
    if m_idx >= MAP_SLOTS {
        return false;
    }
    let slot = &*map.add(m_idx);
    !slot.registry.is_null()
}

// ─── foreach_in_rect (internal) ──────────────────────────────────────────────

/// Call `f(bl)` for each live entity of `bl_type` in the rectangle
/// [x0..x1]×[y0..y1] on map `m`.  Returns the sum of the return values of `f`.
///
/// This replicates `map_foreachinblockva` from `ffi::block` but uses a local
/// `Vec` instead of the global `bl_list` scratch buffer, and accepts a Rust
/// closure instead of a C function pointer + `va_list`.
///
/// # Safety
/// - `map` must be initialized and `m` must be a valid loaded map slot.
pub unsafe fn foreach_in_rect<F>(
    m: i32,
    mut x0: i32,
    mut y0: i32,
    mut x1: i32,
    mut y1: i32,
    bl_type: i32,
    mut f: F,
) -> i32
where
    F: FnMut(*mut BlockList) -> i32,
{
    let m_idx = m as usize;
    if m < 0 || map.is_null() || m_idx >= MAP_SLOTS {
        return 0;
    }
    let slot = &*map.add(m_idx);
    if slot.registry.is_null() {
        return 0;
    }

    // Clamp to map bounds.
    if x0 < 0 { x0 = 0; }
    if y0 < 0 { y0 = 0; }
    if x1 >= slot.xs as i32 { x1 = slot.xs as i32 - 1; }
    if y1 >= slot.ys as i32 { y1 = slot.ys as i32 - 1; }

    // Local scratch buffer — avoids the global bl_list.
    let mut collected: Vec<*mut BlockList> = Vec::new();

    // Collect non-mob entities (PCs, items, NPCs, …).
    if (bl_type & !BL_MOB) != 0 {
        let by0 = y0 as usize / BLOCK_SIZE;
        let by1 = y1 as usize / BLOCK_SIZE;
        let bx0 = x0 as usize / BLOCK_SIZE;
        let bx1 = x1 as usize / BLOCK_SIZE;
        'outer: for by in by0..=by1 {
            for bx in bx0..=bx1 {
                let pos = bx + by * slot.bxs as usize;
                let mut bl = *slot.block.add(pos);
                while !bl.is_null() {
                    let b = &*bl;
                    if (b.bl_type as i32 & bl_type) != 0
                        && b.x as i32 >= x0
                        && b.x as i32 <= x1
                        && b.y as i32 >= y0
                        && b.y as i32 <= y1
                    {
                        collected.push(bl);
                    }
                    bl = b.next;
                    if collected.len() >= bl_list_max() {
                        break 'outer;
                    }
                }
            }
        }
    }

    // Collect mob entities.
    if (bl_type & BL_MOB) != 0 {
        let by0 = y0 as usize / BLOCK_SIZE;
        let by1 = y1 as usize / BLOCK_SIZE;
        let bx0 = x0 as usize / BLOCK_SIZE;
        let bx1 = x1 as usize / BLOCK_SIZE;
        'outer_mob: for by in by0..=by1 {
            for bx in bx0..=bx1 {
                let pos = bx + by * slot.bxs as usize;
                let mut bl = *slot.block_mob.add(pos);
                while !bl.is_null() {
                    let b = &*bl;
                    let mob = bl as *mut MobSpawnData;
                    if (*mob).state != MOB_DEAD
                        && b.x as i32 >= x0
                        && b.x as i32 <= x1
                        && b.y as i32 >= y0
                        && b.y as i32 <= y1
                    {
                        collected.push(bl);
                    }
                    bl = b.next;
                    if collected.len() >= bl_list_max() {
                        break 'outer_mob;
                    }
                }
            }
        }
    }

    if collected.len() >= bl_list_max() {
        tracing::warn!("foreach_in_rect: entity list overflow (> {})", bl_list_max());
    }

    // Dispatch — skip entities that have been removed from the grid since collection.
    let mut total = 0i32;
    for bl in collected {
        if !(*bl).prev.is_null() {
            total += f(bl);
        }
    }
    total
}

// ─── foreach_in_area ─────────────────────────────────────────────────────────

/// Call `f(bl)` for each live entity of `bl_type` in the area defined by `area`
/// around (x, y) on map `m`.
///
/// # Safety
/// Same as `foreach_in_rect`.
pub unsafe fn foreach_in_area<F>(
    m: i32,
    x: i32,
    y: i32,
    area: AreaType,
    bl_type: i32,
    mut f: F,
) -> i32
where
    F: FnMut(*mut BlockList) -> i32,
{
    const NX: i32 = 18; // AREAX_SIZE
    const NY: i32 = 16; // AREAY_SIZE

    let m_idx = m as usize;
    if m < 0 || map.is_null() || m_idx >= MAP_SLOTS {
        return 0;
    }
    let slot = &*map.add(m_idx);
    if slot.registry.is_null() {
        return 0;
    }
    let xs = slot.xs as i32;
    let ys = slot.ys as i32;

    match area {
        AreaType::Area => {
            foreach_in_rect(m, x - (NX + 1), y - (NY + 1), x + (NX + 1), y + (NY + 1), bl_type, f)
        }

        AreaType::Corner => {
            // Mirrors the CORNER branch in ffi::block::map_foreachinarea.
            // Multiple non-overlapping rect calls accumulate into the total.
            if xs > (NX * 2 + 1) && ys > (NY * 2 + 1) {
                let mut total = 0i32;
                if x < (NX * 2 + 2) && x > NX {
                    total += foreach_in_rect(m, 0, y - (NY + 1), x - (NX + 2), y + (NY + 1), bl_type, &mut f);
                }
                if y < (NY * 2 + 2) && y > NY {
                    total += foreach_in_rect(m, x - (NX + 1), 0, x + (NX + 1), y - (NY + 2), bl_type, &mut f);
                    if x < (NX * 2 + 2) && x > NX {
                        total += foreach_in_rect(m, 0, 0, x - (NX + 2), y - (NY + 2), bl_type, &mut f);
                    } else if x > xs - (NX * 2 + 3) && x < xs - (NX + 1) {
                        total += foreach_in_rect(m, x + (NX + 2), 0, xs - 1, y + (NY + 2), bl_type, &mut f);
                    }
                }
                if x > xs - (NX * 2 + 3) && x < xs - (NX + 1) {
                    total += foreach_in_rect(m, x + (NX + 2), y - (NY + 1), xs - 1, y + (NY + 1), bl_type, &mut f);
                }
                if y > ys - (NY * 2 + 3) && y < ys - (NY + 1) {
                    total += foreach_in_rect(m, x - (NX + 1), y + (NY + 2), x + (NX + 1), ys - 1, bl_type, &mut f);
                    if x < (NX * 2 + 2) && x > NX {
                        total += foreach_in_rect(m, 0, y + (NY + 2), x - (NX + 2), ys - 1, bl_type, &mut f);
                    } else if x > xs - (NX * 2 + 3) && x < xs - (NX + 1) {
                        total += foreach_in_rect(m, x + (NX + 2), y + (NY + 2), xs - 1, ys - 1, bl_type, &mut f);
                    }
                }
                total
            } else {
                0
            }
        }

        AreaType::SameArea => {
            // Clamped 18×16 viewport — mirrors the SAMEAREA branch.
            let mut x0 = x - 9;
            let mut y0 = y - 8;
            let mut x1 = x + 9;
            let mut y1 = y + 8;
            if x0 < 0       { x1 += -x0; x0 = 0; if x1 >= xs { x1 = xs - 1; } }
            if y0 < 0       { y1 += -y0; y0 = 0; if y1 >= ys { y1 = ys - 1; } }
            if x1 >= xs     { x0 -= x1 - xs + 1; x1 = xs - 1; if x0 < 0 { x0 = 0; } }
            if y1 >= ys     { y0 -= y1 - ys + 1; y1 = ys - 1; if y0 < 0 { y0 = 0; } }
            foreach_in_rect(m, x0, y0, x1, y1, bl_type, f)
        }

        AreaType::SameMap => {
            foreach_in_rect(m, 0, 0, xs - 1, ys - 1, bl_type, f)
        }
    }
}

// ─── foreach_in_cell ─────────────────────────────────────────────────────────

/// Call `f(bl)` for each live entity of `bl_type` at the exact cell (x, y) on map `m`.
///
/// Unlike the C `map_foreachincell`, each callback does *not* get a fresh `va_list`
/// copy; closures capture their environment by value/reference as needed.
///
/// # Safety
/// Same as `foreach_in_rect`.
pub unsafe fn foreach_in_cell<F>(
    m: i32,
    x: i32,
    y: i32,
    bl_type: i32,
    mut f: F,
) -> i32
where
    F: FnMut(*mut BlockList) -> i32,
{
    let m_idx = m as usize;
    if m < 0 || map.is_null() || m_idx >= MAP_SLOTS {
        return 0;
    }
    let slot = &*map.add(m_idx);
    if slot.registry.is_null() {
        return 0;
    }
    if x < 0 || y < 0 || x >= slot.xs as i32 || y >= slot.ys as i32 {
        return 0;
    }

    let bx = x as usize / BLOCK_SIZE;
    let by = y as usize / BLOCK_SIZE;
    let pos = bx + by * slot.bxs as usize;

    let mut collected: Vec<*mut BlockList> = Vec::new();

    if (bl_type & !BL_MOB) != 0 {
        let mut bl = *slot.block.add(pos);
        while !bl.is_null() {
            let b = &*bl;
            if (b.bl_type as i32 & bl_type) != 0 && b.x as i32 == x && b.y as i32 == y {
                collected.push(bl);
            }
            bl = b.next;
        }
    }

    if (bl_type & BL_MOB) != 0 {
        let mut bl = *slot.block_mob.add(pos);
        while !bl.is_null() {
            let b = &*bl;
            let mob = bl as *mut MobSpawnData;
            if (*mob).state != MOB_DEAD && b.x as i32 == x && b.y as i32 == y {
                collected.push(bl);
            }
            bl = b.next;
        }
    }

    let mut total = 0i32;
    for bl in collected {
        if !(*bl).prev.is_null() {
            total += f(bl);
        }
    }
    total
}

// ─── collect_entities ────────────────────────────────────────────────────────

/// Collect up to `max` entity pointers of `bl_type` in the area around (x, y)
/// on map `m`.  Returns a `Vec` of raw pointers; all are live at the time of
/// collection but may be removed from the grid before the caller iterates them.
///
/// # Safety
/// Same as `foreach_in_area`.
pub unsafe fn collect_entities(
    m: i32,
    x: i32,
    y: i32,
    area: AreaType,
    bl_type: i32,
    max: usize,
) -> Vec<*mut BlockList> {
    let mut out: Vec<*mut BlockList> = Vec::with_capacity(max.min(256));
    foreach_in_area(m, x, y, area, bl_type, |bl| {
        if out.len() < max {
            out.push(bl);
        }
        0
    });
    out
}

// ─── is_alive ────────────────────────────────────────────────────────────────

/// Return `true` if the entity is alive and visible:
/// - Mobs: `state != MOB_DEAD`.
/// - PCs: not dead (`status.state != 1`) and not stealthed (`optFlags & OPT_FLAG_STEALTH == 0`).
/// - All other entity types: always `true`.
///
/// Requires the `map-game` feature because it imports `MapSessionData` and
/// `OPT_FLAG_STEALTH` from the PC module.
///
/// # Safety
/// `bl` must be a valid, aligned pointer to a live `BlockList` (or a struct
/// that begins with `BlockList` as its first field, such as `MapSessionData`
/// or `MobSpawnData`).
#[cfg(all(feature = "map-game", not(test)))]
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

// ─── collect_alive ───────────────────────────────────────────────────────────

/// Collect up to `max` *alive* entity pointers.  Combines `collect_entities` with
/// `is_alive` filtering.
///
/// Requires the `map-game` feature (same as `is_alive`).
///
/// # Safety
/// Same as `foreach_in_area`.
#[cfg(all(feature = "map-game", not(test)))]
pub unsafe fn collect_alive(
    m: i32,
    x: i32,
    y: i32,
    area: AreaType,
    bl_type: i32,
    max: usize,
) -> Vec<*mut BlockList> {
    let mut out: Vec<*mut BlockList> = Vec::with_capacity(max.min(256));
    foreach_in_area(m, x, y, area, bl_type, |bl| {
        if out.len() < max && is_alive(bl) {
            out.push(bl);
        }
        0
    });
    out
}

// ─── Test helpers (pub(crate) so sibling test modules can reuse them) ─────────

/// Set the module-level `map` static to `ptr`.  Only available in test builds.
/// Used by sibling modules (e.g. `game::scripting::object_collect`) that call
/// `foreach_in_cell` / `foreach_in_area` and need to inject a test map.
///
/// # Safety
/// The caller must ensure `ptr` stays valid for the lifetime of the test and
/// that no other thread concurrently reads the `map` global.
#[cfg(test)]
pub(crate) unsafe fn test_set_map(ptr: *mut crate::database::map_db::MapData) {
    map = ptr;
}

/// Allocate a zeroed `MapData` on the heap, set up minimum block-grid fields
/// for a `xs × ys` map, and return the `Box`.  The caller must eventually free
/// it via `test_free_map`.  Only available in test builds.
///
/// # Safety
/// Uses raw allocation.  The returned `Box` must not be dropped normally — pass
/// it to `test_free_map` to release the inner arrays before dropping the box.
#[cfg(test)]
pub(crate) unsafe fn test_make_map(xs: u16, ys: u16) -> Box<crate::database::map_db::MapData> {
    use crate::database::map_db::{GlobalReg, MapData, WarpList, BLOCK_SIZE};
    use crate::database::map_db::BlockList;

    let layout = std::alloc::Layout::new::<MapData>();
    let raw = std::alloc::alloc_zeroed(layout) as *mut MapData;
    assert!(!raw.is_null());
    let mut slot = Box::from_raw(raw);

    slot.xs = xs;
    slot.ys = ys;
    slot.bxs = ((xs as usize + BLOCK_SIZE - 1) / BLOCK_SIZE) as u16;
    slot.bys = ((ys as usize + BLOCK_SIZE - 1) / BLOCK_SIZE) as u16;

    let cells = slot.bxs as usize * slot.bys as usize;
    let block_layout = std::alloc::Layout::array::<*mut BlockList>(cells).unwrap();
    slot.block     = std::alloc::alloc_zeroed(block_layout) as *mut *mut BlockList;
    slot.block_mob = std::alloc::alloc_zeroed(block_layout) as *mut *mut BlockList;

    let warp_layout = std::alloc::Layout::array::<*mut WarpList>(cells).unwrap();
    slot.warp = std::alloc::alloc_zeroed(warp_layout) as *mut *mut WarpList;

    let reg_layout = std::alloc::Layout::array::<GlobalReg>(1).unwrap();
    slot.registry = std::alloc::alloc_zeroed(reg_layout) as *mut GlobalReg;

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
    use crate::database::map_db::BlockList;

    let cells = slot.bxs as usize * slot.bys as usize;
    std::alloc::dealloc(
        slot.block as *mut u8,
        std::alloc::Layout::array::<*mut BlockList>(cells).unwrap(),
    );
    std::alloc::dealloc(
        slot.block_mob as *mut u8,
        std::alloc::Layout::array::<*mut BlockList>(cells).unwrap(),
    );
    std::alloc::dealloc(
        slot.warp as *mut u8,
        std::alloc::Layout::array::<*mut WarpList>(cells).unwrap(),
    );
    std::alloc::dealloc(
        slot.registry as *mut u8,
        std::alloc::Layout::array::<GlobalReg>(1).unwrap(),
    );
    drop(Box::into_raw(slot)); // prevent double-free via Box drop
}

/// Build a minimal live `BlockList` node with `prev` pointing at the test
/// sentinel head (marks entity as live).  Only available in test builds.
///
/// # Safety
/// The returned node is only valid as long as `TEST_BL_HEAD` lives (static).
#[cfg(test)]
pub(crate) unsafe fn test_make_bl_node(bl_type: u8, x: u16, y: u16) -> crate::database::map_db::BlockList {
    crate::database::map_db::BlockList {
        next:          std::ptr::null_mut(),
        prev:          std::ptr::addr_of_mut!(TEST_BL_HEAD),
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

/// Insert `node` as the head of the non-mob chain for cell (x, y).
/// Only available in test builds.
///
/// # Safety
/// `slot.block` must be a valid allocated array; `node` must outlive the slot.
#[cfg(test)]
pub(crate) unsafe fn test_insert_in_block(
    slot: &mut crate::database::map_db::MapData,
    node: *mut crate::database::map_db::BlockList,
    x: u16,
    y: u16,
) {
    let bx = x as usize / BLOCK_SIZE;
    let by = y as usize / BLOCK_SIZE;
    let pos = bx + by * slot.bxs as usize;
    (*node).next = *slot.block.add(pos);
    *slot.block.add(pos) = node;
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::map_db::{BlockList, MapData, GlobalReg, WarpList, BLOCK_SIZE};
    use crate::game::mob::{BL_MOB, BL_PC, MOB_ALIVE, MOB_DEAD, MobSpawnData};
    use std::ptr;

    // SAFETY NOTE: These tests mutate the `map` global pointer and must run sequentially.
    // Run with: cargo test --features map-game -- block 2>&1 (tests are safe individually)
    // For parallel test runs use: -- --test-threads=1

    /// Allocate a zeroed MapData on the heap and set up minimum fields for a
    /// 100×100 map at slot `slot_id`.  Returns the box so the caller can
    /// insert entities before passing the raw pointer to the `map` global.
    unsafe fn make_test_map(xs: u16, ys: u16) -> Box<MapData> {
        // Allocate a zeroed MapData
        let layout = std::alloc::Layout::new::<MapData>();
        let raw = std::alloc::alloc_zeroed(layout) as *mut MapData;
        assert!(!raw.is_null());
        let mut slot = Box::from_raw(raw);

        slot.xs = xs;
        slot.ys = ys;
        slot.bxs = ((xs as usize + BLOCK_SIZE - 1) / BLOCK_SIZE) as u16;
        slot.bys = ((ys as usize + BLOCK_SIZE - 1) / BLOCK_SIZE) as u16;

        // Allocate block/block_mob arrays (zeroed → all null pointers)
        let cells = slot.bxs as usize * slot.bys as usize;
        let block_layout = std::alloc::Layout::array::<*mut BlockList>(cells).unwrap();
        slot.block     = std::alloc::alloc_zeroed(block_layout) as *mut *mut BlockList;
        slot.block_mob = std::alloc::alloc_zeroed(block_layout) as *mut *mut BlockList;

        // Warp array
        let warp_layout = std::alloc::Layout::array::<*mut WarpList>(cells).unwrap();
        slot.warp = std::alloc::alloc_zeroed(warp_layout) as *mut *mut WarpList;

        // Non-null registry marks the map as "loaded"
        let reg_layout = std::alloc::Layout::array::<GlobalReg>(1).unwrap();
        slot.registry = std::alloc::alloc_zeroed(reg_layout) as *mut GlobalReg;

        slot
    }

    /// Build a minimal live `BlockList` node.
    /// - `bl_type` is set to the given value.
    /// - `x`, `y` coordinates are set.
    /// - `prev` is set to `&raw mut bl_head` (marks the entity as live in the grid).
    /// - `next` is null (end of chain).
    unsafe fn make_bl_node(bl_type: u8, x: u16, y: u16) -> BlockList {
        BlockList {
            next:          ptr::null_mut(),
            prev:          ptr::addr_of_mut!(TEST_BL_HEAD),
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

    /// Insert a `BlockList` node as the head of the non-mob chain for cell (x, y).
    unsafe fn insert_in_block(slot: &mut MapData, node: *mut BlockList, x: u16, y: u16) {
        let bx = x as usize / BLOCK_SIZE;
        let by = y as usize / BLOCK_SIZE;
        let pos = bx + by * slot.bxs as usize;
        (*node).next = *slot.block.add(pos);
        *slot.block.add(pos) = node;
    }

    /// Insert a `BlockList` node as the head of the mob chain for cell (x, y).
    unsafe fn insert_in_block_mob(slot: &mut MapData, node: *mut BlockList, x: u16, y: u16) {
        let bx = x as usize / BLOCK_SIZE;
        let by = y as usize / BLOCK_SIZE;
        let pos = bx + by * slot.bxs as usize;
        (*node).next = *slot.block_mob.add(pos);
        *slot.block_mob.add(pos) = node;
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Test 1: foreach_in_cell — empty map returns 0
    // ─────────────────────────────────────────────────────────────────────────

    /// foreach_in_cell on an empty block grid must return 0.
    ///
    /// Verifies that traversal of null-terminated chains produces no callbacks
    /// when no entity has been inserted into the grid.
    #[test]
    fn test_foreach_in_cell_empty() {
        unsafe {
            let slot = make_test_map(100, 100);
            let slot_ptr = Box::into_raw(slot);

            // Temporarily point the global `map` at a single-slot array.
            // We use slot index 0 to avoid MAP_SLOTS bounds issues.
            let orig_map = map;
            map = slot_ptr;

            let result = foreach_in_cell(0, 50, 50, BL_PC | BL_MOB, |_| 1);

            // Restore global
            map = orig_map;

            // Free slot
            let slot = Box::from_raw(slot_ptr);
            let cells = slot.bxs as usize * slot.bys as usize;
            std::alloc::dealloc(
                slot.block as *mut u8,
                std::alloc::Layout::array::<*mut BlockList>(cells).unwrap(),
            );
            std::alloc::dealloc(
                slot.block_mob as *mut u8,
                std::alloc::Layout::array::<*mut BlockList>(cells).unwrap(),
            );
            std::alloc::dealloc(
                slot.warp as *mut u8,
                std::alloc::Layout::array::<*mut WarpList>(cells).unwrap(),
            );
            std::alloc::dealloc(
                slot.registry as *mut u8,
                std::alloc::Layout::array::<GlobalReg>(1).unwrap(),
            );
            drop(Box::into_raw(slot)); // prevent double-free via Box drop

            assert_eq!(result, 0, "empty grid should yield 0 callbacks");
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Test 2: foreach_in_cell — one PC entity found
    // ─────────────────────────────────────────────────────────────────────────

    /// foreach_in_cell with one PC entity at the queried cell must return 1.
    ///
    /// Verifies that a live (prev != null) entity whose bl_type matches the
    /// requested mask and whose coords match exactly is counted once.
    #[test]
    fn test_foreach_in_cell_one_pc() {
        unsafe {
            let mut slot = make_test_map(100, 100);
            let mut bl_node = make_bl_node(BL_PC as u8, 50, 50);

            insert_in_block(&mut slot, &raw mut bl_node, 50, 50);

            let slot_ptr = Box::into_raw(slot);
            let orig_map = map;
            map = slot_ptr;

            let result = foreach_in_cell(0, 50, 50, BL_PC, |_| 1);

            map = orig_map;

            let slot = Box::from_raw(slot_ptr);
            let cells = slot.bxs as usize * slot.bys as usize;
            std::alloc::dealloc(
                slot.block as *mut u8,
                std::alloc::Layout::array::<*mut BlockList>(cells).unwrap(),
            );
            std::alloc::dealloc(
                slot.block_mob as *mut u8,
                std::alloc::Layout::array::<*mut BlockList>(cells).unwrap(),
            );
            std::alloc::dealloc(
                slot.warp as *mut u8,
                std::alloc::Layout::array::<*mut WarpList>(cells).unwrap(),
            );
            std::alloc::dealloc(
                slot.registry as *mut u8,
                std::alloc::Layout::array::<GlobalReg>(1).unwrap(),
            );
            drop(Box::into_raw(slot));

            assert_eq!(result, 1, "one PC entity in cell should yield 1 callback");
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Test 3: foreach_in_cell — dead mob is skipped
    // ─────────────────────────────────────────────────────────────────────────

    /// foreach_in_cell must skip mobs whose state == MOB_DEAD.
    ///
    /// Constructs a full `MobSpawnData` with state=MOB_DEAD and inserts it into
    /// the mob grid chain. The traversal code casts bl → *mut MobSpawnData and
    /// reads `(*mob).state`; a dead mob must not fire the callback.
    #[test]
    fn test_foreach_in_cell_dead_mob_skipped() {
        unsafe {
            let mut slot = make_test_map(100, 100);

            // Box::new(zeroed()) gives a zeroed MobSpawnData.
            // state=0 = MOB_ALIVE by default; we set it to MOB_DEAD.
            let mut mob: Box<MobSpawnData> = Box::new(std::mem::zeroed());
            mob.bl.bl_type = BL_MOB as u8;
            mob.bl.x = 50;
            mob.bl.y = 50;
            mob.bl.prev = ptr::addr_of_mut!(TEST_BL_HEAD);
            mob.bl.next = ptr::null_mut();
            mob.state = MOB_DEAD;

            let mob_bl_ptr: *mut BlockList = &raw mut mob.bl;
            insert_in_block_mob(&mut slot, mob_bl_ptr, 50, 50);

            let slot_ptr = Box::into_raw(slot);
            let orig_map = map;
            map = slot_ptr;

            let result = foreach_in_cell(0, 50, 50, BL_MOB, |_| 1);

            map = orig_map;

            let slot = Box::from_raw(slot_ptr);
            let cells = slot.bxs as usize * slot.bys as usize;
            std::alloc::dealloc(
                slot.block as *mut u8,
                std::alloc::Layout::array::<*mut BlockList>(cells).unwrap(),
            );
            std::alloc::dealloc(
                slot.block_mob as *mut u8,
                std::alloc::Layout::array::<*mut BlockList>(cells).unwrap(),
            );
            std::alloc::dealloc(
                slot.warp as *mut u8,
                std::alloc::Layout::array::<*mut WarpList>(cells).unwrap(),
            );
            std::alloc::dealloc(
                slot.registry as *mut u8,
                std::alloc::Layout::array::<GlobalReg>(1).unwrap(),
            );
            drop(Box::into_raw(slot));

            assert_eq!(result, 0, "dead mob must be skipped by foreach_in_cell");
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Test 4: foreach_in_area(SameMap) — finds all PCs across the map
    // ─────────────────────────────────────────────────────────────────────────

    /// foreach_in_area with SameMap finds all 3 PCs placed at different cells.
    ///
    /// Verifies that the SameMap variant sweeps the entire map (rect 0..xs-1,
    /// 0..ys-1) and that entities in different block grid cells are all found.
    #[test]
    fn test_foreach_in_area_samemap_three_pcs() {
        unsafe {
            let mut slot = make_test_map(100, 100);

            // Place 3 PCs at different cells (in different block grid cells)
            let mut bl1 = make_bl_node(BL_PC as u8, 10, 10);
            let mut bl2 = make_bl_node(BL_PC as u8, 50, 50);
            let mut bl3 = make_bl_node(BL_PC as u8, 90, 90);

            insert_in_block(&mut slot, &raw mut bl1, 10, 10);
            insert_in_block(&mut slot, &raw mut bl2, 50, 50);
            insert_in_block(&mut slot, &raw mut bl3, 90, 90);

            let slot_ptr = Box::into_raw(slot);
            let orig_map = map;
            map = slot_ptr;

            let result = foreach_in_area(0, 0, 0, AreaType::SameMap, BL_PC, |_| 1);

            map = orig_map;

            let slot = Box::from_raw(slot_ptr);
            let cells = slot.bxs as usize * slot.bys as usize;
            std::alloc::dealloc(
                slot.block as *mut u8,
                std::alloc::Layout::array::<*mut BlockList>(cells).unwrap(),
            );
            std::alloc::dealloc(
                slot.block_mob as *mut u8,
                std::alloc::Layout::array::<*mut BlockList>(cells).unwrap(),
            );
            std::alloc::dealloc(
                slot.warp as *mut u8,
                std::alloc::Layout::array::<*mut WarpList>(cells).unwrap(),
            );
            std::alloc::dealloc(
                slot.registry as *mut u8,
                std::alloc::Layout::array::<GlobalReg>(1).unwrap(),
            );
            drop(Box::into_raw(slot));

            assert_eq!(result, 3, "SameMap should find all 3 PCs across the map");
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Test 5: collect_entities — collects and caps at max
    // ─────────────────────────────────────────────────────────────────────────

    /// collect_entities returns a Vec limited to `max` entries even if more exist.
    ///
    /// Places 5 PCs on the map, requests max=3, verifies vec length is 3.
    #[test]
    fn test_collect_entities_max_cap() {
        unsafe {
            let mut slot = make_test_map(100, 100);

            let mut bls = [
                make_bl_node(BL_PC as u8, 10, 10),
                make_bl_node(BL_PC as u8, 20, 20),
                make_bl_node(BL_PC as u8, 30, 30),
                make_bl_node(BL_PC as u8, 40, 40),
                make_bl_node(BL_PC as u8, 50, 50),
            ];

            for bl in bls.iter_mut() {
                insert_in_block(&mut slot, bl as *mut BlockList, bl.x, bl.y);
            }

            let slot_ptr = Box::into_raw(slot);
            let orig_map = map;
            map = slot_ptr;

            let entities = collect_entities(0, 0, 0, AreaType::SameMap, BL_PC, 3);

            map = orig_map;

            let slot = Box::from_raw(slot_ptr);
            let cells = slot.bxs as usize * slot.bys as usize;
            std::alloc::dealloc(
                slot.block as *mut u8,
                std::alloc::Layout::array::<*mut BlockList>(cells).unwrap(),
            );
            std::alloc::dealloc(
                slot.block_mob as *mut u8,
                std::alloc::Layout::array::<*mut BlockList>(cells).unwrap(),
            );
            std::alloc::dealloc(
                slot.warp as *mut u8,
                std::alloc::Layout::array::<*mut WarpList>(cells).unwrap(),
            );
            std::alloc::dealloc(
                slot.registry as *mut u8,
                std::alloc::Layout::array::<GlobalReg>(1).unwrap(),
            );
            drop(Box::into_raw(slot));

            assert_eq!(entities.len(), 3, "collect_entities should cap at max=3");
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Test 6: foreach_in_cell — alive mob is found
    // ─────────────────────────────────────────────────────────────────────────

    /// foreach_in_cell with one alive mob (state=MOB_ALIVE) must return 1.
    ///
    /// Counterpart to test 3: verifies that MOB_ALIVE mobs are not skipped.
    #[test]
    fn test_foreach_in_cell_alive_mob_found() {
        unsafe {
            let mut slot = make_test_map(100, 100);

            let mut mob: Box<MobSpawnData> = Box::new(std::mem::zeroed());
            mob.bl.bl_type = BL_MOB as u8;
            mob.bl.x = 32;
            mob.bl.y = 32;
            mob.bl.prev = ptr::addr_of_mut!(TEST_BL_HEAD);
            mob.bl.next = ptr::null_mut();
            mob.state = MOB_ALIVE; // 0 — alive

            let mob_bl_ptr: *mut BlockList = &raw mut mob.bl;
            insert_in_block_mob(&mut slot, mob_bl_ptr, 32, 32);

            let slot_ptr = Box::into_raw(slot);
            let orig_map = map;
            map = slot_ptr;

            let result = foreach_in_cell(0, 32, 32, BL_MOB, |_| 1);

            map = orig_map;

            let slot = Box::from_raw(slot_ptr);
            let cells = slot.bxs as usize * slot.bys as usize;
            std::alloc::dealloc(
                slot.block as *mut u8,
                std::alloc::Layout::array::<*mut BlockList>(cells).unwrap(),
            );
            std::alloc::dealloc(
                slot.block_mob as *mut u8,
                std::alloc::Layout::array::<*mut BlockList>(cells).unwrap(),
            );
            std::alloc::dealloc(
                slot.warp as *mut u8,
                std::alloc::Layout::array::<*mut WarpList>(cells).unwrap(),
            );
            std::alloc::dealloc(
                slot.registry as *mut u8,
                std::alloc::Layout::array::<GlobalReg>(1).unwrap(),
            );
            drop(Box::into_raw(slot));

            assert_eq!(result, 1, "alive mob must be found by foreach_in_cell");
        }
    }
}

// ─── FFI exports ─────────────────────────────────────────────────────────────
// Content moved from src/ffi/block.rs

use std::os::raw::{c_int, c_uchar, c_uint, c_ushort, c_void};
use crate::game::scripting::types::floor::FloorItemData;

/// `ITM_TRAPS` from `item_db.h` — floor items of this type are skipped by `map_firstincell`.
const ITM_TRAPS: c_int = 20;

extern "C" {
    #[link_name = "rust_itemdb_type"]
    fn itemdb_type(id: c_uint) -> c_int;
}

pub const BL_LIST_MAX: usize = 32768;

/// Sentinel node — previously defined in map_server.c; now owned by Rust.
#[no_mangle]
pub static mut bl_head: crate::database::map_db::BlockList = crate::database::map_db::BlockList {
    next:          std::ptr::null_mut(),
    prev:          std::ptr::null_mut(),
    id:            0 as c_uint,
    bx:            0 as c_uint,
    by:            0 as c_uint,
    graphic_id:    0 as c_uint,
    graphic_color: 0 as c_uint,
    m:             0 as c_ushort,
    x:             0 as c_ushort,
    y:             0 as c_ushort,
    bl_type:       0 as c_uchar,
    subtype:       0 as c_uchar,
};

/// Scratch buffer used by block query functions.
#[no_mangle]
pub static mut bl_list: [*mut crate::database::map_db::BlockList; BL_LIST_MAX] = [std::ptr::null_mut(); BL_LIST_MAX];

fn alloc_ptr_array<T>(len: usize) -> *mut *mut T {
    let mut v: Vec<*mut T> = vec![std::ptr::null_mut(); len];
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Allocate block/block_mob/warp arrays for every loaded map slot.
#[no_mangle]
pub unsafe extern "C" fn map_initblock() {
    if crate::database::map_db::map.is_null() { return; }
    let slots = std::slice::from_raw_parts_mut(crate::database::map_db::map, crate::database::map_db::MAP_SLOTS);
    for slot in slots.iter_mut() {
        if slot.bxs == 0 || slot.bys == 0 { continue; }
        let cells = slot.bxs as usize * slot.bys as usize;
        slot.block     = alloc_ptr_array::<crate::database::map_db::BlockList>(cells);
        slot.block_mob = alloc_ptr_array::<crate::database::map_db::BlockList>(cells);
        slot.warp      = alloc_ptr_array::<crate::database::map_db::WarpList>(cells);
    }
}

/// Free block grid arrays for all map slots (no-op, matches C).
#[no_mangle]
pub unsafe extern "C" fn map_termblock() {}

type BlockCallback = unsafe extern "C" fn(*mut crate::database::map_db::BlockList, *mut c_void) -> c_int;

/// Insert `bl` into the appropriate block grid chain.
#[no_mangle]
pub unsafe extern "C" fn map_addblock(bl: *mut crate::database::map_db::BlockList) -> c_int {
    if bl.is_null() { return 1; }
    let bl = unsafe { &mut *bl };

    if !bl.prev.is_null() {
        tracing::error!("[map_addblock] bl->prev != NULL (already in grid) id={}", bl.id);
        return 1;
    }

    let m = bl.m as usize;
    if m >= crate::database::map_db::MAP_SLOTS || crate::database::map_db::map.is_null() {
        tracing::error!("[map_addblock] invalid map id id={} m={m}", bl.id);
        return 1;
    }
    let slot = unsafe { &mut *crate::database::map_db::map.add(m) };

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

    let pos = (x as usize / crate::database::map_db::BLOCK_SIZE) + (y as usize / crate::database::map_db::BLOCK_SIZE) * slot.bxs as usize;

    if bl.bl_type as c_int == crate::game::mob::BL_MOB {
        let chain_head = slot.block_mob.add(pos);
        bl.next = *chain_head;
        bl.prev = std::ptr::addr_of_mut!(bl_head);
        if !bl.next.is_null() { (*bl.next).prev = bl as *mut crate::database::map_db::BlockList; }
        *chain_head = bl as *mut crate::database::map_db::BlockList;
    } else {
        let chain_head = slot.block.add(pos);
        bl.next = *chain_head;
        bl.prev = std::ptr::addr_of_mut!(bl_head);
        if !bl.next.is_null() { (*bl.next).prev = bl as *mut crate::database::map_db::BlockList; }
        *chain_head = bl as *mut crate::database::map_db::BlockList;
    }

    if bl.bl_type as c_int == crate::game::mob::BL_PC { slot.user += 1; }
    0
}

/// Remove `bl` from the block grid.
#[no_mangle]
pub unsafe extern "C" fn map_delblock(bl: *mut crate::database::map_db::BlockList) -> c_int {
    if bl.is_null() { return 0; }
    let bl = unsafe { &mut *bl };

    if bl.prev.is_null() {
        if !bl.next.is_null() {
            tracing::error!("[map_delblock] bl->next != NULL but bl->prev is NULL id={}", bl.id);
        }
        return 0;
    }

    let m = bl.m as usize;
    let pos = (bl.x as usize / crate::database::map_db::BLOCK_SIZE) + (bl.y as usize / crate::database::map_db::BLOCK_SIZE)
        * unsafe { (*crate::database::map_db::map.add(m)).bxs as usize };

    if !bl.next.is_null() { (*bl.next).prev = bl.prev; }

    if bl.prev == std::ptr::addr_of_mut!(bl_head) {
        let slot = &mut *crate::database::map_db::map.add(m);
        if bl.bl_type as c_int == crate::game::mob::BL_MOB {
            *slot.block_mob.add(pos) = bl.next;
        } else {
            *slot.block.add(pos) = bl.next;
        }
    } else {
        (*bl.prev).next = bl.next;
    }

    if bl.bl_type as c_int == crate::game::mob::BL_PC {
        let slot = &mut *crate::database::map_db::map.add(m);
        slot.user -= 1;
    }

    bl.next = std::ptr::null_mut();
    bl.prev = std::ptr::null_mut();
    0
}

/// Remove `bl` from current cell, update coords, re-insert.
#[no_mangle]
pub unsafe extern "C" fn map_moveblock(bl: *mut crate::database::map_db::BlockList, x1: c_int, y1: c_int) -> c_int {
    map_delblock(bl);
    if !bl.is_null() {
        (*bl).x = x1 as c_ushort;
        (*bl).y = y1 as c_ushort;
    }
    map_addblock(bl);
    0
}

/// Thin shim: call `func(bl, ap)` for each live entity of `bl_type` in rect.
#[no_mangle]
pub unsafe extern "C" fn map_foreachinblockva(
    func: Option<BlockCallback>,
    m: c_int,
    x0: c_int,
    y0: c_int,
    x1: c_int,
    y1: c_int,
    bl_type: c_int,
    ap: *mut c_void,
) -> c_int {
    let func = match func { Some(f) => f, None => return 0 };
    foreach_in_rect(
        m, x0 as i32, y0 as i32, x1 as i32, y1 as i32, bl_type as i32,
        |bl| func(bl, ap),
    ) as c_int
}

#[inline]
unsafe fn first_mob_in_cell(
    slot: &crate::database::map_db::MapData,
    pos: usize,
    x: c_int,
    y: c_int,
) -> *mut crate::database::map_db::BlockList {
    let mut bl = *slot.block_mob.add(pos);
    while !bl.is_null() {
        let b = &*bl;
        let mob = bl as *mut crate::game::mob::MobSpawnData;
        if (*mob).state != crate::game::mob::MOB_DEAD && b.x as c_int == x && b.y as c_int == y {
            return bl;
        }
        bl = b.next;
    }
    std::ptr::null_mut()
}

/// Return the first live entity at cell (x, y) on map `m`.
#[no_mangle]
pub unsafe extern "C" fn map_firstincell(
    m: c_int, x: c_int, y: c_int, bl_type: c_int,
) -> *mut crate::database::map_db::BlockList {
    if m < 0 || crate::database::map_db::map.is_null() { return std::ptr::null_mut(); }
    let m_idx = m as usize;
    if m_idx >= crate::database::map_db::MAP_SLOTS { return std::ptr::null_mut(); }
    let slot = &*crate::database::map_db::map.add(m_idx);
    if slot.registry.is_null() { return std::ptr::null_mut(); }

    let x = x.clamp(0, slot.xs as c_int - 1);
    let y = y.clamp(0, slot.ys as c_int - 1);
    let pos = (x as usize / crate::database::map_db::BLOCK_SIZE) + (y as usize / crate::database::map_db::BLOCK_SIZE) * slot.bxs as usize;

    if (bl_type & !(crate::game::mob::BL_MOB)) != 0 {
        let mut bl = *slot.block.add(pos);
        while !bl.is_null() {
            let b = &*bl;
            if (b.bl_type as c_int & bl_type) != 0 && b.x as c_int == x && b.y as c_int == y {
                if b.bl_type as c_int != crate::game::mob::BL_ITEM {
                    return bl;
                } else {
                    let fl = bl as *mut FloorItemData;
                    if itemdb_type((*fl).data.id) != ITM_TRAPS { return bl; }
                }
            }
            bl = b.next;
        }
    }

    if (bl_type & crate::game::mob::BL_MOB) != 0 {
        let result = first_mob_in_cell(slot, pos, x, y);
        if !result.is_null() { return result; }
    }
    std::ptr::null_mut()
}

/// Iterate mobs across entire map `m`.
#[no_mangle]
pub unsafe extern "C" fn map_respawnmobs(
    func: Option<BlockCallback>, m: c_int, bl_type: c_int, ap: *mut c_void,
) -> c_int {
    let func = match func { Some(f) => f, None => return 0 };
    if m < 0 || crate::database::map_db::map.is_null() { return 0; }
    let m_idx = m as usize;
    if m_idx >= crate::database::map_db::MAP_SLOTS { return 0; }
    let slot = &*crate::database::map_db::map.add(m_idx);
    if slot.registry.is_null() { return 0; }

    let x1 = slot.xs as usize;
    let y1 = slot.ys as usize;
    let mut blockcount: usize = 0;

    if (bl_type & crate::game::mob::BL_MOB) != 0 {
        let by1 = (y1.saturating_sub(1)) / crate::database::map_db::BLOCK_SIZE;
        let bx1 = (x1.saturating_sub(1)) / crate::database::map_db::BLOCK_SIZE;
        'outer: for by in 0..=by1 {
            for bx in 0..=bx1 {
                let pos = bx + by * slot.bxs as usize;
                let mut bl = *slot.block_mob.add(pos);
                while !bl.is_null() && blockcount < BL_LIST_MAX {
                    bl_list[blockcount] = bl;
                    blockcount += 1;
                    bl = (*bl).next;
                }
                if blockcount >= BL_LIST_MAX { break 'outer; }
            }
        }
    }

    if blockcount >= BL_LIST_MAX { tracing::warn!("map_respawnmobs: bl_list overflow"); }

    let mut return_count: c_int = 0;
    for i in 0..blockcount {
        let bl = bl_list[i];
        if !(*bl).prev.is_null() { return_count += func(bl, ap); }
    }
    return_count
}

const SAMEMAP:  c_int = 2;
const AREA:     c_int = 4;
const SAMEAREA: c_int = 6;
const CORNER:   c_int = 8;

/// Iterate all live entities in the area around (x, y) on map `m`.
#[no_mangle]
pub unsafe extern "C" fn map_foreachinarea(
    func: Option<BlockCallback>,
    m: c_int, x: c_int, y: c_int,
    area: c_int, bl_type: c_int,
    mut args: ...
) -> c_int {
    const NX: c_int = 18;
    const NY: c_int = 16;

    let m_idx = m as usize;
    if m < 0 || crate::database::map_db::map.is_null() || m_idx >= crate::database::map_db::MAP_SLOTS { return 0; }
    let slot = &*crate::database::map_db::map.add(m_idx);
    if slot.registry.is_null() { return 0; }
    let xs = slot.xs as c_int;
    let ys = slot.ys as c_int;

    let ap = std::ptr::addr_of_mut!(args) as *mut c_void;

    match area {
        AREA => { map_foreachinblockva(func, m, x - (NX + 1), y - (NY + 1), x + (NX + 1), y + (NY + 1), bl_type, ap); }
        CORNER => {
            if xs > (NX * 2 + 1) && ys > (NY * 2 + 1) {
                if x < (NX * 2 + 2) && x > NX {
                    map_foreachinblockva(func, m, 0, y - (NY + 1), x - (NX + 2), y + (NY + 1), bl_type, ap);
                }
                if y < (NY * 2 + 2) && y > NY {
                    map_foreachinblockva(func, m, x - (NX + 1), 0, x + (NX + 1), y - (NY + 2), bl_type, ap);
                    if x < (NX * 2 + 2) && x > NX {
                        map_foreachinblockva(func, m, 0, 0, x - (NX + 2), y - (NY + 2), bl_type, ap);
                    } else if x > xs - (NX * 2 + 3) && x < xs - (NX + 1) {
                        map_foreachinblockva(func, m, x + (NX + 2), 0, xs - 1, y + (NY + 2), bl_type, ap);
                    }
                }
                if x > xs - (NX * 2 + 3) && x < xs - (NX + 1) {
                    map_foreachinblockva(func, m, x + (NX + 2), y - (NY + 1), xs - 1, y + (NY + 1), bl_type, ap);
                }
                if y > ys - (NY * 2 + 3) && y < ys - (NY + 1) {
                    map_foreachinblockva(func, m, x - (NX + 1), y + (NY + 2), x + (NX + 1), ys - 1, bl_type, ap);
                    if x < (NX * 2 + 2) && x > NX {
                        map_foreachinblockva(func, m, 0, y + (NY + 2), x - (NX + 2), ys - 1, bl_type, ap);
                    } else if x > xs - (NX * 2 + 3) && x < xs - (NX + 1) {
                        map_foreachinblockva(func, m, x + (NX + 2), y + (NY + 2), xs - 1, ys - 1, bl_type, ap);
                    }
                }
            }
        }
        SAMEAREA => {
            let mut x0 = x - 9; let mut y0 = y - 8; let mut x1 = x + 9; let mut y1 = y + 8;
            if x0 < 0 { x1 += -x0; x0 = 0; if x1 >= xs { x1 = xs - 1; } }
            if y0 < 0 { y1 += -y0; y0 = 0; if y1 >= ys { y1 = ys - 1; } }
            if x1 >= xs { x0 -= x1 - xs + 1; x1 = xs - 1; if x0 < 0 { x0 = 0; } }
            if y1 >= ys { y0 -= y1 - ys + 1; y1 = ys - 1; if y0 < 0 { y0 = 0; } }
            map_foreachinblockva(func, m, x0, y0, x1, y1, bl_type, ap);
        }
        SAMEMAP => { map_foreachinblockva(func, m, 0, 0, xs - 1, ys - 1, bl_type, ap); }
        _ => {}
    }
    0
}

/// Iterate all live entities in rect [x0..x1]×[y0..y1] on map `m`.
#[no_mangle]
pub unsafe extern "C" fn map_foreachinblock(
    func: Option<BlockCallback>,
    m: c_int, mut x0: c_int, mut y0: c_int, mut x1: c_int, mut y1: c_int,
    bl_type: c_int, mut args: ...
) -> c_int {
    let m_idx = m as usize;
    if m < 0 || crate::database::map_db::map.is_null() || m_idx >= crate::database::map_db::MAP_SLOTS { return 0; }
    let slot = &*crate::database::map_db::map.add(m_idx);
    if slot.registry.is_null() { return 0; }
    if x0 < 0 { x0 = 0; }
    if y0 < 0 { y0 = 0; }
    if x1 >= slot.xs as c_int { x1 = slot.xs as c_int - 1; }
    if y1 >= slot.ys as c_int { y1 = slot.ys as c_int - 1; }

    let ap = std::ptr::addr_of_mut!(args) as *mut c_void;
    map_foreachinblockva(func, m, x0, y0, x1, y1, bl_type, ap);
    0
}

/// Iterate all live entities at exact cell (x, y) on map `m`.
#[no_mangle]
pub unsafe extern "C" fn map_foreachincell(
    func: Option<BlockCallback>,
    m: c_int, x: c_int, y: c_int, bl_type: c_int, mut args: ...
) -> c_int {
    let func = match func { Some(f) => f, None => return 0 };
    let m_idx = m as usize;
    if m < 0 || crate::database::map_db::map.is_null() || m_idx >= crate::database::map_db::MAP_SLOTS { return 0; }
    let slot = &*crate::database::map_db::map.add(m_idx);
    if slot.registry.is_null() { return 0; }
    if x < 0 || y < 0 || x >= slot.xs as c_int || y >= slot.ys as c_int { return 0; }

    let bx = x as usize / crate::database::map_db::BLOCK_SIZE;
    let by = y as usize / crate::database::map_db::BLOCK_SIZE;
    let pos = bx + by * slot.bxs as usize;
    let mut blockcount: usize = 0;

    if (bl_type & !(crate::game::mob::BL_MOB)) != 0 {
        let mut bl = *slot.block.add(pos);
        while !bl.is_null() && blockcount < BL_LIST_MAX {
            let b = &*bl;
            if (b.bl_type as c_int & bl_type) != 0 && b.x as c_int == x && b.y as c_int == y {
                if b.bl_type as c_int != crate::game::mob::BL_ITEM {
                    bl_list[blockcount] = bl; blockcount += 1;
                } else {
                    let fl = bl as *mut FloorItemData;
                    if itemdb_type((*fl).data.id) != ITM_TRAPS { bl_list[blockcount] = bl; blockcount += 1; }
                }
            }
            bl = b.next;
        }
    }

    if (bl_type & crate::game::mob::BL_MOB) != 0 {
        let mut bl = *slot.block_mob.add(pos);
        while !bl.is_null() && blockcount < BL_LIST_MAX {
            let b = &*bl;
            let mob = bl as *mut crate::game::mob::MobSpawnData;
            if (*mob).state != crate::game::mob::MOB_DEAD && b.x as c_int == x && b.y as c_int == y {
                bl_list[blockcount] = bl; blockcount += 1;
            }
            bl = b.next;
        }
    }

    if blockcount >= BL_LIST_MAX { tracing::warn!("map_foreachincell: bl_list overflow"); }

    let mut return_count: c_int = 0;
    for i in 0..blockcount {
        let bl = bl_list[i];
        if !(*bl).prev.is_null() {
            let ap = std::ptr::addr_of_mut!(args) as *mut c_void;
            return_count += func(bl, ap);
        }
    }
    return_count
}

/// Same as `map_foreachincell` but includes trap floor items.
#[no_mangle]
pub unsafe extern "C" fn map_foreachincellwithtraps(
    func: Option<BlockCallback>,
    m: c_int, x: c_int, y: c_int, bl_type: c_int, mut args: ...
) -> c_int {
    let func = match func { Some(f) => f, None => return 0 };
    let m_idx = m as usize;
    if m < 0 || crate::database::map_db::map.is_null() || m_idx >= crate::database::map_db::MAP_SLOTS { return 0; }
    let slot = &*crate::database::map_db::map.add(m_idx);
    if slot.registry.is_null() { return 0; }
    if x < 0 || y < 0 || x >= slot.xs as c_int || y >= slot.ys as c_int { return 0; }

    let bx = x as usize / crate::database::map_db::BLOCK_SIZE;
    let by = y as usize / crate::database::map_db::BLOCK_SIZE;
    let pos = bx + by * slot.bxs as usize;
    let mut blockcount: usize = 0;

    if (bl_type & !(crate::game::mob::BL_MOB)) != 0 {
        let mut bl = *slot.block.add(pos);
        while !bl.is_null() && blockcount < BL_LIST_MAX {
            let b = &*bl;
            if (b.bl_type as c_int & bl_type) != 0 && b.x as c_int == x && b.y as c_int == y {
                bl_list[blockcount] = bl; blockcount += 1;
            }
            bl = b.next;
        }
    }

    if (bl_type & crate::game::mob::BL_MOB) != 0 {
        let mut bl = *slot.block_mob.add(pos);
        while !bl.is_null() && blockcount < BL_LIST_MAX {
            let b = &*bl;
            let mob = bl as *mut crate::game::mob::MobSpawnData;
            if (*mob).state != crate::game::mob::MOB_DEAD && b.x as c_int == x && b.y as c_int == y {
                bl_list[blockcount] = bl; blockcount += 1;
            }
            bl = b.next;
        }
    }

    if blockcount >= BL_LIST_MAX { tracing::warn!("map_foreachincellwithtraps: bl_list overflow"); }

    let mut return_count: c_int = 0;
    for i in 0..blockcount {
        let bl = bl_list[i];
        if !(*bl).prev.is_null() {
            let ap = std::ptr::addr_of_mut!(args) as *mut c_void;
            return_count += func(bl, ap);
        }
    }
    return_count
}

/// Variadic shim: call `map_respawnmobs` after extracting the va_list.
#[no_mangle]
pub unsafe extern "C" fn map_respawn(
    func: Option<BlockCallback>, m: c_int, bl_type: c_int, mut args: ...
) -> c_int {
    let ap = std::ptr::addr_of_mut!(args) as *mut c_void;
    map_respawnmobs(func, m, bl_type, ap);
    0
}

/// Same as `map_firstincell` but includes trap floor items.
#[no_mangle]
pub unsafe extern "C" fn map_firstincellwithtraps(
    m: c_int, x: c_int, y: c_int, bl_type: c_int,
) -> *mut crate::database::map_db::BlockList {
    if m < 0 || crate::database::map_db::map.is_null() { return std::ptr::null_mut(); }
    let m_idx = m as usize;
    if m_idx >= crate::database::map_db::MAP_SLOTS { return std::ptr::null_mut(); }
    let slot = &*crate::database::map_db::map.add(m_idx);
    if slot.registry.is_null() { return std::ptr::null_mut(); }

    let x = x.clamp(0, slot.xs as c_int - 1);
    let y = y.clamp(0, slot.ys as c_int - 1);
    let pos = (x as usize / crate::database::map_db::BLOCK_SIZE) + (y as usize / crate::database::map_db::BLOCK_SIZE) * slot.bxs as usize;

    if (bl_type & !(crate::game::mob::BL_MOB)) != 0 {
        let mut bl = *slot.block.add(pos);
        while !bl.is_null() {
            let b = &*bl;
            if (b.bl_type as c_int & bl_type) != 0 && b.x as c_int == x && b.y as c_int == y { return bl; }
            bl = b.next;
        }
    }

    if (bl_type & crate::game::mob::BL_MOB) != 0 {
        let result = first_mob_in_cell(slot, pos, x, y);
        if !result.is_null() { return result; }
    }
    std::ptr::null_mut()
}
