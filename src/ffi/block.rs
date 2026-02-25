//! FFI bridge for block grid mutation functions.
//!
//! Rust owns the grid mutation side: addblock, delblock, initblock, moveblock, termblock.
//! The foreachin* spatial query family stays in C (variadic va_list callbacks).
//!
//! `bl_head` is defined in map_server.c (non-static); imported here for sentinel comparison.

use std::os::raw::{c_int, c_ushort};
use std::ptr;

use crate::database::map_db::{BlockList, WarpList, MAP_SLOTS, BLOCK_SIZE};
use crate::ffi::map_db::map;

const BL_MOB: u8 = 0x02;
const BL_PC:  u8 = 0x01;

// Sentinel node — lives in map_server.c, exported via map_server.h.
// map_addblock sets bl->prev = &bl_head; map_delblock checks bl->prev == &bl_head
// to know whether the entity is at the head of its chain.
extern "C" {
    static mut bl_head: BlockList;
}

/// Allocate a zeroed array of `len` null pointers and return a raw pointer.
/// Caller owns the allocation; free via `Vec::from_raw_parts(ptr, len, len)`.
fn alloc_ptr_array<T>(len: usize) -> *mut *mut T {
    let mut v: Vec<*mut T> = vec![ptr::null_mut(); len];
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Allocate block/block_mob/warp arrays for every loaded map slot.
/// Replaces `map_initblock()` in `map_server.c`.
/// Called once at startup after `rust_map_init()` has populated the map array.
///
/// # Safety
/// - `map` global must be initialized via `rust_map_init` before calling.
/// - Must be called exactly once; calling again leaks the previous allocations.
#[no_mangle]
pub unsafe extern "C" fn map_initblock() {
    if map.is_null() {
        return;
    }
    let slots = std::slice::from_raw_parts_mut(map, MAP_SLOTS);
    for slot in slots.iter_mut() {
        if slot.bxs == 0 || slot.bys == 0 {
            continue; // sparse slot — not a loaded map
        }
        let cells = slot.bxs as usize * slot.bys as usize;
        slot.block     = alloc_ptr_array::<BlockList>(cells);
        slot.block_mob = alloc_ptr_array::<BlockList>(cells);
        slot.warp      = alloc_ptr_array::<WarpList>(cells);
    }
}

/// Free block grid arrays for all map slots.
/// Replaces `map_termblock()` in `map_server.c`. Currently a no-op (matches C).
///
/// # Safety
/// Always safe to call; no preconditions.
#[no_mangle]
pub unsafe extern "C" fn map_termblock() {
    // No-op: the C version had deferred-free logic that was commented out.
    // Proper teardown can be added here when needed.
}

/// Insert `bl` into the appropriate block grid chain for its map/coords.
/// Replaces `map_addblock()` in `map_server.c`. Returns 0 on success, 1 on error.
///
/// # Safety
/// - `bl` must be null or a valid, aligned `BlockList` that is not currently in any grid
///   (i.e. `bl->prev` is null). Passing an already-inserted node is an error (returns 1).
/// - `bl->m`, `bl->x`, `bl->y` must be valid for the loaded map grid.
/// - `map` global must be initialized via `rust_map_init` and `map_initblock` must have run.
#[no_mangle]
pub unsafe extern "C" fn map_addblock(bl: *mut BlockList) -> c_int {
    if bl.is_null() {
        return 1;
    }
    let bl = unsafe { &mut *bl };

    if !bl.prev.is_null() {
        tracing::error!("[map_addblock] bl->prev != NULL (already in grid) id={}", bl.id);
        return 1;
    }

    let m = bl.m as usize;
    if m >= MAP_SLOTS || map.is_null() {
        tracing::error!("[map_addblock] invalid map id id={} m={m}", bl.id);
        return 1;
    }
    let slot = unsafe { &mut *map.add(m) };

    // map_isloaded(m): registry pointer is non-null iff the map was loaded.
    if slot.registry.is_null() {
        tracing::error!("[map_addblock] map not loaded id={} m={m}", bl.id);
        return 1;
    }

    let x = bl.x as i32;
    let y = bl.y as i32;
    if x < 0 || x >= slot.xs as i32 || y < 0 || y >= slot.ys as i32 {
        tracing::error!(
            "[map_addblock] out-of-bounds m={m} x={x} y={y} xs={} ys={} id={}",
            slot.xs, slot.ys, bl.id
        );
        return 1;
    }

    let pos = (x as usize / BLOCK_SIZE) + (y as usize / BLOCK_SIZE) * slot.bxs as usize;

    if bl.bl_type == BL_MOB {
        let chain_head = slot.block_mob.add(pos);
        bl.next = *chain_head;
        bl.prev = ptr::addr_of_mut!(bl_head);
        if !bl.next.is_null() {
            (*bl.next).prev = bl as *mut BlockList;
        }
        *chain_head = bl as *mut BlockList;
    } else {
        let chain_head = slot.block.add(pos);
        bl.next = *chain_head;
        bl.prev = ptr::addr_of_mut!(bl_head);
        if !bl.next.is_null() {
            (*bl.next).prev = bl as *mut BlockList;
        }
        *chain_head = bl as *mut BlockList;
    }

    if bl.bl_type == BL_PC {
        slot.user += 1;
    }

    0
}

/// Remove `bl` from the block grid.
/// Replaces `map_delblock()` in `map_server.c`. Returns 0 always.
///
/// # Safety
/// - `bl` must be null or a valid, aligned `BlockList`. Null is handled as a no-op.
/// - If `bl->prev` is non-null, `bl` must currently be in the grid and the chain
///   it belongs to must be intact (no concurrent mutation).
/// - `map` global must be initialized.
#[no_mangle]
pub unsafe extern "C" fn map_delblock(bl: *mut BlockList) -> c_int {
    if bl.is_null() {
        return 0;
    }
    let bl = unsafe { &mut *bl };

    if bl.prev.is_null() {
        // Not in the grid — nothing to do.
        if !bl.next.is_null() {
            tracing::error!("[map_delblock] bl->next != NULL but bl->prev is NULL id={}", bl.id);
        }
        return 0;
    }

    let m = bl.m as usize;
    let pos = (bl.x as usize / BLOCK_SIZE) + (bl.y as usize / BLOCK_SIZE)
        * unsafe { (*map.add(m)).bxs as usize };

    // Stitch the next node's prev pointer.
    if !bl.next.is_null() {
        (*bl.next).prev = bl.prev;
    }

    // If bl is the chain head (prev == &bl_head), update the grid array.
    // Otherwise stitch prev->next.
    if bl.prev == ptr::addr_of_mut!(bl_head) {
        let slot = &mut *map.add(m);
        if bl.bl_type == BL_MOB {
            *slot.block_mob.add(pos) = bl.next;
        } else {
            *slot.block.add(pos) = bl.next;
        }
    } else {
        (*bl.prev).next = bl.next;
    }

    if bl.bl_type == BL_PC {
        let slot = &mut *map.add(m);
        slot.user -= 1;
    }

    bl.next = ptr::null_mut();
    bl.prev = ptr::null_mut();

    0
}

/// Remove `bl` from its current cell, update coords, re-insert.
/// Replaces `map_moveblock()` in `map_server.c`. Returns 0 always.
///
/// # Safety
/// Same as `map_delblock` + `map_addblock`: `bl` must be a valid pointer currently in the
/// grid, and `(x1, y1)` must be valid coords for the map `bl->m`.
#[no_mangle]
pub unsafe extern "C" fn map_moveblock(bl: *mut BlockList, x1: c_int, y1: c_int) -> c_int {
    map_delblock(bl);
    if !bl.is_null() {
        (*bl).x = x1 as c_ushort;
        (*bl).y = y1 as c_ushort;
    }
    map_addblock(bl);
    0
}
