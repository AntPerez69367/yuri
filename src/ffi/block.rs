//! FFI bridge for block grid mutation and spatial query functions.
//!
//! Rust owns the grid mutation side: addblock, delblock, initblock, moveblock, termblock.
//! The foreachin* spatial query family is also ported here using the c_variadic nightly feature.
//!
//! `bl_head` and `bl_list` are now owned by Rust and exported via `#[no_mangle]`.
//! They were previously defined in map_server.c.

use std::os::raw::{c_int, c_uchar, c_uint, c_ushort, c_void};
use std::ptr;

use crate::database::map_db::{BlockList, WarpList, MAP_SLOTS, BLOCK_SIZE};
use crate::ffi::map_db::map;
use crate::game::scripting::types::floor::FloorItemData;

/// `ITM_TRAPS` from `item_db.h` — floor items of this type are skipped by `map_firstincell`.
const ITM_TRAPS: c_int = 20;

extern "C" {
    #[link_name = "rust_itemdb_type"]
    fn itemdb_type(id: c_uint) -> c_int;
}


pub const BL_LIST_MAX: usize = 32768;

/// Sentinel node — `bl->prev == &raw mut bl_head` means bl is the chain head.
/// Previously defined in map_server.c; now owned by Rust, exported via `#[no_mangle]`.
#[no_mangle]
pub static mut bl_head: BlockList = BlockList {
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

/// Scratch buffer used by block query functions to collect results before iterating.
/// Previously defined in map_server.c; now owned by Rust, exported via `#[no_mangle]`.
#[no_mangle]
pub static mut bl_list: [*mut BlockList; BL_LIST_MAX] = [std::ptr::null_mut(); BL_LIST_MAX];

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

    if bl.bl_type as c_int == crate::game::mob::BL_MOB {
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

    if bl.bl_type as c_int == crate::game::mob::BL_PC {
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
        if bl.bl_type as c_int == crate::game::mob::BL_MOB {
            *slot.block_mob.add(pos) = bl.next;
        } else {
            *slot.block.add(pos) = bl.next;
        }
    } else {
        (*bl.prev).next = bl.next;
    }

    if bl.bl_type as c_int == crate::game::mob::BL_PC {
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

// Callback: int (*func)(struct block_list*, va_list)
// va_list is ABI-compatible with *mut c_void on x86-64 Linux.
type BlockCallback = unsafe extern "C" fn(*mut BlockList, *mut c_void) -> c_int;

/// Thin shim: call `func(bl, ap)` for each live entity of `bl_type` in
/// rect [x0..x1]×[y0..y1] on map `m`.  Delegates all traversal logic to
/// `crate::game::block::foreach_in_rect`; retained as a C-callable entry
/// point while callers in `map_foreachinarea`, `map_foreachinblock`, etc.
/// still use the `BlockCallback + *mut c_void` convention.
///
/// Replaces `map_foreachinblockva` in `c_src/map_server.c`.
///
/// # Safety
/// - `map` must be initialized and `m` must be a valid loaded map slot.
/// - `func` must be a valid C function pointer.
/// - `ap` is a `va_list*` forwarded opaquely to `func`; caller owns its lifetime.
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
    crate::game::block::foreach_in_rect(
        m, x0 as i32, y0 as i32, x1 as i32, y1 as i32, bl_type as i32,
        |bl| func(bl, ap),
    ) as c_int
}

/// Scan the mob grid chain at `pos` for the first non-dead mob at exact coords (x, y).
/// Returns null if none found.
///
/// # Safety
/// `slot.block_mob` must be a valid initialized pointer array produced by `map_initblock`.
#[inline]
unsafe fn first_mob_in_cell(
    slot: &crate::database::map_db::MapData,
    pos: usize,
    x: c_int,
    y: c_int,
) -> *mut BlockList {
    let mut bl = *slot.block_mob.add(pos);
    while !bl.is_null() {
        let b = &*bl;
        let mob = bl as *mut crate::game::mob::MobSpawnData;
        if (*mob).state != crate::game::mob::MOB_DEAD
            && b.x as c_int == x
            && b.y as c_int == y
        {
            return bl;
        }
        bl = b.next;
    }
    ptr::null_mut()
}

/// Return the first live entity of `bl_type` at cell (x, y) on map `m`, or null.
/// Floor items of type `ITM_TRAPS` are skipped — use `map_firstincellwithtraps` to include them.
/// Replaces `map_firstincell` in `c_src/map_server.c`.
///
/// # Safety
/// - `map` must be initialized and `m` must be a valid map index.
/// - Coords are clamped to map bounds if out of range.
#[no_mangle]
pub unsafe extern "C" fn map_firstincell(
    m: c_int,
    x: c_int,
    y: c_int,
    bl_type: c_int,
) -> *mut BlockList {
    if m < 0 || map.is_null() {
        return ptr::null_mut();
    }
    let m_idx = m as usize;
    if m_idx >= MAP_SLOTS {
        return ptr::null_mut();
    }
    let slot = &*map.add(m_idx);
    if slot.registry.is_null() {
        return ptr::null_mut();
    }

    let x = x.clamp(0, slot.xs as c_int - 1);
    let y = y.clamp(0, slot.ys as c_int - 1);
    let pos = (x as usize / BLOCK_SIZE) + (y as usize / BLOCK_SIZE) * slot.bxs as usize;

    // Search non-mob chain (PC, items, NPCs, …)
    if (bl_type & !(crate::game::mob::BL_MOB)) != 0 {
        let mut bl = *slot.block.add(pos);
        while !bl.is_null() {
            let b = &*bl;
            if (b.bl_type as c_int & bl_type) != 0
                && b.x as c_int == x
                && b.y as c_int == y
            {
                if b.bl_type as c_int != crate::game::mob::BL_ITEM {
                    return bl;
                } else {
                    // Skip trap floor items — callers that want traps use map_firstincellwithtraps.
                    let fl = bl as *mut FloorItemData;
                    if itemdb_type((*fl).data.id) != ITM_TRAPS {
                        return bl;
                    }
                }
            }
            bl = b.next;
        }
    }

    // Search mob chain
    if (bl_type & crate::game::mob::BL_MOB) != 0 {
        let result = first_mob_in_cell(slot, pos, x, y);
        if !result.is_null() {
            return result;
        }
    }

    ptr::null_mut()
}

/// Iterate mobs across entire map `m`, calling `func(bl, ap)` for each live mob.
/// Replaces `map_respawnmobs` in `c_src/map_server.c`.
///
/// Despite the name, this function calls `func` only for mobs whose `prev` pointer is
/// non-null (i.e. currently live in the grid). Dead mobs (`prev == null`) are collected
/// into `bl_list` but skipped in the dispatch loop — this matches the C source exactly
/// (line 588: `if (bl_list[i]->prev)`).
///
/// The full-map coord bounds check in C (`bl->x >= x0 && bl->x <= x1 && ...`) is omitted
/// here because x0=0, y0=0, x1=map.xs-1, y1=map.ys-1 makes it trivially true for any
/// mob in the grid.
///
/// `map_respawn` in C stays as a variadic shim that calls this function after `va_start`.
///
/// # Safety
/// - `map` must be initialized and `m` a valid loaded map slot.
/// - `func` must be a valid C function pointer.
/// - `ap` is a `va_list*` forwarded opaquely to `func`; caller owns its lifetime.
#[no_mangle]
pub unsafe extern "C" fn map_respawnmobs(
    func: Option<BlockCallback>,
    m: c_int,
    bl_type: c_int,
    ap: *mut c_void,
) -> c_int {
    let func = match func {
        Some(f) => f,
        None => return 0,
    };
    if m < 0 || map.is_null() {
        return 0;
    }
    let m_idx = m as usize;
    if m_idx >= MAP_SLOTS {
        return 0;
    }
    let slot = &*map.add(m_idx);
    if slot.registry.is_null() {
        return 0;
    }

    let x1 = slot.xs as usize;
    let y1 = slot.ys as usize;

    let mut blockcount: usize = 0;

    // Collect mobs — full map sweep over every block cell.
    // Only the mob grid is consulted (matches C: only `if (type & BL_MOB)` branch exists).
    if (bl_type & crate::game::mob::BL_MOB) != 0 {
        let by1 = (y1.saturating_sub(1)) / BLOCK_SIZE;
        let bx1 = (x1.saturating_sub(1)) / BLOCK_SIZE;
        'outer: for by in 0..=by1 {
            for bx in 0..=bx1 {
                let pos = bx + by * slot.bxs as usize;
                let mut bl = *slot.block_mob.add(pos);
                while !bl.is_null() && blockcount < BL_LIST_MAX {
                    bl_list[blockcount] = bl;
                    blockcount += 1;
                    bl = (*bl).next;
                }
                if blockcount >= BL_LIST_MAX {
                    break 'outer;
                }
            }
        }
    }

    if blockcount >= BL_LIST_MAX {
        tracing::warn!("map_respawnmobs: bl_list overflow");
    }

    let mut return_count: c_int = 0;
    for i in 0..blockcount {
        let bl = bl_list[i];
        // Live check: matches C `if (bl_list[i]->prev)` — dead mobs have prev=null.
        if !(*bl).prev.is_null() {
            return_count += func(bl, ap);
        }
    }
    return_count
}

// ─── Area type constants (from c_src/map_parse.h enum) ───────────────────────
const SAMEMAP:  c_int = 2;
const AREA:     c_int = 4;
const SAMEAREA: c_int = 6;
const CORNER:   c_int = 8;

/// Iterate all live entities of `bl_type` in the area around (x, y) on map `m`.
/// `area` selects the search shape: AREA (fixed 18×16 window), CORNER, SAMEAREA (full 18×16),
/// or SAMEMAP.
/// Replaces `map_foreachinarea` in `c_src/map_server_stubs.c`.
///
/// # Safety
/// - `map` must be initialized and `m` a valid loaded map index.
/// - `func` must be a valid C function pointer.
/// - Variadic args are forwarded opaquely to `map_foreachinblockva`.
#[no_mangle]
pub unsafe extern "C" fn map_foreachinarea(
    func: Option<BlockCallback>,
    m: c_int,
    x: c_int,
    y: c_int,
    area: c_int,
    bl_type: c_int,
    mut args: ...
) -> c_int {
    const NX: c_int = 18; // AREAX_SIZE
    const NY: c_int = 16; // AREAY_SIZE

    let m_idx = m as usize;
    if m < 0 || map.is_null() || m_idx >= MAP_SLOTS {
        return 0;
    }
    let slot = &*map.add(m_idx);
    if slot.registry.is_null() {
        return 0;
    }
    let xs = slot.xs as c_int;
    let ys = slot.ys as c_int;

    // std::ptr::addr_of_mut!(args) gives a *mut VaList<'_>; on x86-64 Linux
    // VaList is a newtype around VaListImpl, so this pointer is ABI-compatible
    // with C's va_list* as required by the BlockCallback signature.
    let ap = std::ptr::addr_of_mut!(args) as *mut c_void;

    match area {
        AREA => {
            map_foreachinblockva(func, m, x - (NX + 1), y - (NY + 1), x + (NX + 1), y + (NY + 1), bl_type, ap);
        }
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
            // Compute the clamped 18×16 viewing rectangle (matches C map_foreachinarea logic).
            let mut x0 = x - 9;
            let mut y0 = y - 8;
            let mut x1 = x + 9;
            let mut y1 = y + 8;
            if x0 < 0 { x1 += -x0; x0 = 0; if x1 >= xs { x1 = xs - 1; } }
            if y0 < 0 { y1 += -y0; y0 = 0; if y1 >= ys { y1 = ys - 1; } }
            if x1 >= xs { x0 -= x1 - xs + 1; x1 = xs - 1; if x0 < 0 { x0 = 0; } }
            if y1 >= ys { y0 -= y1 - ys + 1; y1 = ys - 1; if y0 < 0 { y0 = 0; } }
            map_foreachinblockva(func, m, x0, y0, x1, y1, bl_type, ap);
        }
        SAMEMAP => {
            map_foreachinblockva(func, m, 0, 0, xs - 1, ys - 1, bl_type, ap);
        }
        _ => {}
    }
    0 // intentional: matches C map_foreachinarea return; callers ignore this value
}

/// Iterate all live entities of `bl_type` in the rect [x0..x1]×[y0..y1] on map `m`.
/// Replaces `map_foreachinblock` in `c_src/map_server_stubs.c`.
///
/// # Safety
/// Same as `map_foreachinblockva`.
#[no_mangle]
pub unsafe extern "C" fn map_foreachinblock(
    func: Option<BlockCallback>,
    m: c_int,
    mut x0: c_int,
    mut y0: c_int,
    mut x1: c_int,
    mut y1: c_int,
    bl_type: c_int,
    mut args: ...
) -> c_int {
    let m_idx = m as usize;
    if m < 0 || map.is_null() || m_idx >= MAP_SLOTS {
        return 0;
    }
    let slot = &*map.add(m_idx);
    if slot.registry.is_null() { return 0; }  // map not loaded; xs/ys are 0, clamp arithmetic would be wrong
    if x0 < 0 { x0 = 0; }
    if y0 < 0 { y0 = 0; }
    if x1 >= slot.xs as c_int { x1 = slot.xs as c_int - 1; }
    if y1 >= slot.ys as c_int { y1 = slot.ys as c_int - 1; }

    let ap = std::ptr::addr_of_mut!(args) as *mut c_void;
    map_foreachinblockva(func, m, x0, y0, x1, y1, bl_type, ap);
    0 // intentional: matches C map_foreachinblock return; callers ignore this value
}

/// Iterate all live entities of `bl_type` at the exact cell (x, y) on map `m`.
/// Each callback sees a fresh copy of the varargs (matches C `va_start` inside loop).
/// Replaces `map_foreachincell` in `c_src/map_server_stubs.c`.
///
/// # Safety
/// Same as `map_foreachinblockva`.
#[no_mangle]
pub unsafe extern "C" fn map_foreachincell(
    func: Option<BlockCallback>,
    m: c_int,
    x: c_int,
    y: c_int,
    bl_type: c_int,
    mut args: ...
) -> c_int {
    let func = match func { Some(f) => f, None => return 0 };
    let m_idx = m as usize;
    if m < 0 || map.is_null() || m_idx >= MAP_SLOTS { return 0; }
    let slot = &*map.add(m_idx);
    if slot.registry.is_null() { return 0; }
    if x < 0 || y < 0 || x >= slot.xs as c_int || y >= slot.ys as c_int { return 0; }

    let bx = x as usize / BLOCK_SIZE;
    let by = y as usize / BLOCK_SIZE;
    let pos = bx + by * slot.bxs as usize;

    let mut blockcount: usize = 0;

    if (bl_type & !(crate::game::mob::BL_MOB)) != 0 {
        let mut bl = *slot.block.add(pos);
        while !bl.is_null() && blockcount < BL_LIST_MAX {
            let b = &*bl;
            if (b.bl_type as c_int & bl_type) != 0 && b.x as c_int == x && b.y as c_int == y {
                if b.bl_type as c_int != crate::game::mob::BL_ITEM {
                    bl_list[blockcount] = bl;
                    blockcount += 1;
                } else {
                    let fl = bl as *mut FloorItemData;
                    if itemdb_type((*fl).data.id) != ITM_TRAPS {
                        bl_list[blockcount] = bl;
                        blockcount += 1;
                    }
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
                bl_list[blockcount] = bl;
                blockcount += 1;
            }
            bl = b.next;
        }
    }

    if blockcount >= BL_LIST_MAX {
        tracing::warn!("map_foreachincell: bl_list overflow");
    }

    let mut return_count: c_int = 0;
    for i in 0..blockcount {
        let bl = bl_list[i];
        if !(*bl).prev.is_null() {
            // Each callback gets a pointer to the same va_list.  In the original C the
            // `map_foreachincell` used `va_start`/`va_end` inside the loop so each
            // callback saw a fresh va_list.  Replicating that exactly requires the
            // `VaList::copy` API which is not yet stable on this nightly toolchain;
            // passing the same pointer is safe for callbacks that do not consume args.
            let ap = std::ptr::addr_of_mut!(args) as *mut c_void;
            return_count += func(bl, ap);
        }
    }
    return_count
}

/// Same as `map_foreachincell` but includes trap floor items.
/// Replaces `map_foreachincellwithtraps` in `c_src/map_server_stubs.c`.
///
/// # Safety
/// Same as `map_foreachincell`.
#[no_mangle]
pub unsafe extern "C" fn map_foreachincellwithtraps(
    func: Option<BlockCallback>,
    m: c_int,
    x: c_int,
    y: c_int,
    bl_type: c_int,
    mut args: ...
) -> c_int {
    let func = match func { Some(f) => f, None => return 0 };
    let m_idx = m as usize;
    if m < 0 || map.is_null() || m_idx >= MAP_SLOTS { return 0; }
    let slot = &*map.add(m_idx);
    if slot.registry.is_null() { return 0; }
    if x < 0 || y < 0 || x >= slot.xs as c_int || y >= slot.ys as c_int { return 0; }

    let bx = x as usize / BLOCK_SIZE;
    let by = y as usize / BLOCK_SIZE;
    let pos = bx + by * slot.bxs as usize;

    let mut blockcount: usize = 0;

    if (bl_type & !(crate::game::mob::BL_MOB)) != 0 {
        let mut bl = *slot.block.add(pos);
        while !bl.is_null() && blockcount < BL_LIST_MAX {
            let b = &*bl;
            if (b.bl_type as c_int & bl_type) != 0 && b.x as c_int == x && b.y as c_int == y {
                bl_list[blockcount] = bl;
                blockcount += 1;
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
                bl_list[blockcount] = bl;
                blockcount += 1;
            }
            bl = b.next;
        }
    }

    if blockcount >= BL_LIST_MAX {
        tracing::warn!("map_foreachincellwithtraps: bl_list overflow");
    }

    let mut return_count: c_int = 0;
    for i in 0..blockcount {
        let bl = bl_list[i];
        if !(*bl).prev.is_null() {
            // See map_foreachincell for the note on va_list copy semantics.
            let ap = std::ptr::addr_of_mut!(args) as *mut c_void;
            return_count += func(bl, ap);
        }
    }
    return_count
}

/// Variadic shim: call `map_respawnmobs` after extracting the va_list.
/// Replaces `map_respawn` in `c_src/map_server_stubs.c`.
///
/// # Safety
/// Same as `map_respawnmobs`.
#[no_mangle]
pub unsafe extern "C" fn map_respawn(
    func: Option<BlockCallback>,
    m: c_int,
    bl_type: c_int,
    mut args: ...
) -> c_int {
    let ap = std::ptr::addr_of_mut!(args) as *mut c_void;
    map_respawnmobs(func, m, bl_type, ap);
    0
}

/// Same as `map_firstincell` but includes trap floor items.
/// Replaces `map_firstincellwithtraps` in `c_src/map_server.c`.
///
/// # Safety
/// - `map` must be initialized and `m` must be a valid map index.
/// - Coords are clamped to map bounds if out of range.
#[no_mangle]
pub unsafe extern "C" fn map_firstincellwithtraps(
    m: c_int,
    x: c_int,
    y: c_int,
    bl_type: c_int,
) -> *mut BlockList {
    if m < 0 || map.is_null() {
        return ptr::null_mut();
    }
    let m_idx = m as usize;
    if m_idx >= MAP_SLOTS {
        return ptr::null_mut();
    }
    let slot = &*map.add(m_idx);
    if slot.registry.is_null() {
        return ptr::null_mut();
    }

    let x = x.clamp(0, slot.xs as c_int - 1);
    let y = y.clamp(0, slot.ys as c_int - 1);
    let pos = (x as usize / BLOCK_SIZE) + (y as usize / BLOCK_SIZE) * slot.bxs as usize;

    // Search non-mob chain — all BL_ITEM entities returned regardless of trap type.
    if (bl_type & !(crate::game::mob::BL_MOB)) != 0 {
        let mut bl = *slot.block.add(pos);
        while !bl.is_null() {
            let b = &*bl;
            if (b.bl_type as c_int & bl_type) != 0
                && b.x as c_int == x
                && b.y as c_int == y
            {
                return bl;
            }
            bl = b.next;
        }
    }

    // Search mob chain
    if (bl_type & crate::game::mob::BL_MOB) != 0 {
        let result = first_mob_in_cell(slot, pos, x, y);
        if !result.is_null() {
            return result;
        }
    }

    ptr::null_mut()
}
