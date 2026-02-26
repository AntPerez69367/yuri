//! NPC game logic — replaces `c_src/npc.c`.

#![allow(non_snake_case, dead_code)]

use std::ffi::{c_char, c_int, c_uint, c_uchar, c_ushort};
use crate::database::map_db::{BlockList, GlobalReg};
use crate::servers::char::charstatus::{Item, MAX_EQUIP};
use crate::game::types::GfxViewer;

#[cfg(not(test))]
use crate::database::map_db::{WarpList, BLOCK_SIZE};
#[cfg(not(test))]
use crate::database::{blocking_run, get_pool};
#[cfg(not(test))]
use crate::ffi::map_db::{get_map_ptr, map_is_loaded};

// MAX_EQUIP is defined in charstatus::MAX_EQUIP (imported above) — same slot count.
pub const MAX_GLOBALNPCREG: usize = 100;
pub const NPC_START_NUM: u32      = 3221225472;
pub const NPCT_START_NUM: u32     = 3321225472;
pub const F1_NPC: u32             = 4294967295;

pub const BL_PC:  c_int = 0x01;
pub const BL_MOB: c_int = 0x02;  // used by npc_move_sub (Task 10)
pub const BL_NPC: c_int = 0x04;

/// Mirrors `struct npc_data` from `map_server.h`. Must be 20416 bytes on 64-bit.
///
/// # Layout
///
/// ```text
/// offset     field               size
/// ------     -----               ----
///      0     bl                    48   (BlockList — two 8-byte pointers + 6×u32 + 3×u16 + 2×u8 + 2-byte implicit pad)
///     48     equip[15]          13200   (15 × Item@880)
///  13248     registry[100]       6800   (100 × GlobalReg@68)
///  20048     gfx                   72   (GfxViewer)
///  20120     id..item_dura         60   (15 × u32)
///  20180     name[64]              64
///  20244     npc_name[64]          64
///  20308     itemreal_name[64]     64
///  20372     state..retdist        10   (10 × c_char)
///  20382     _pad                   2   (align movetimer to 4-byte boundary)
///  20384     movetimer              4
///  20388     movetime               4
///  20392     sex..starty           20   (10 × u16)
///  20412     returning              1
///  20413     [repr(C) trailing pad] 3   (align struct to 8 bytes)
///  20416     END
/// ```
///
/// `_pad: [u8; 2]` before `movetimer` is required because the preceding 10
/// `c_char` fields leave the current offset at 20382, which is not 4-byte
/// aligned (20382 % 4 == 2).
#[repr(C)]
pub struct NpcData {
    pub bl:            BlockList,
    pub equip:         [Item; MAX_EQUIP],
    pub registry:      [GlobalReg; MAX_GLOBALNPCREG],
    pub gfx:           GfxViewer,
    pub id:            c_uint,
    pub actiontime:    c_uint,
    pub owner:         c_uint,
    pub duration:      c_uint,
    pub lastaction:    c_uint,
    pub time:          c_uint,
    pub duratime:      c_uint,
    pub item_look:     c_uint,
    pub item_owner:    c_uint,
    pub item_color:    c_uint,
    pub item_id:       c_uint,
    pub item_slot:     c_uint,
    pub item_pos:      c_uint,
    pub item_amount:   c_uint,
    pub item_dura:     c_uint,
    pub name:          [c_char; 64],
    pub npc_name:      [c_char; 64],
    pub itemreal_name: [c_char; 64],
    pub state:         c_char,
    pub side:          c_char,
    pub canmove:       c_char,
    pub npctype:       c_char,
    pub clone:         c_char,
    pub shop_npc:      c_char,
    pub repair_npc:    c_char,
    pub bank_npc:      c_char,
    pub receive_item:  c_char,
    pub retdist:       c_char,
    pub _pad:          [u8; 2],
    pub movetimer:     c_uint,
    pub movetime:      c_uint,
    pub sex:           c_ushort,
    pub face:          c_ushort,
    pub face_color:    c_ushort,
    pub hair:          c_ushort,
    pub hair_color:    c_ushort,
    pub armor_color:   c_ushort,
    pub skin_color:    c_ushort,
    pub startm:        c_ushort,
    pub startx:        c_ushort,
    pub starty:        c_ushort,
    pub returning:     c_uchar,
    // 3 bytes trailing padding added automatically by repr(C) to align struct to 8 bytes
}

// NPC ID counters — match C globals npc_id and npctemp_id.
// #[export_name] exports as "npc_id" so map_server.c can read it directly
// while keeping the Rust-idiomatic SCREAMING_SNAKE_CASE name internally.
#[export_name = "npc_id"]
pub static mut NPC_ID: c_uint = NPC_START_NUM as c_uint;
pub static mut NPCTEMP_ID: c_uint = NPCT_START_NUM as c_uint;

// C map functions needed by npc_get_new_npcid / npc_get_new_npctempid
extern "C" {
    pub fn map_id2bl(id: c_uint) -> *mut BlockList;
    pub fn map_id2npc(id: c_uint) -> *mut NpcData;
    pub fn map_addiddb(bl: *mut BlockList);
    pub fn map_deliddb(bl: *mut BlockList);
    pub fn map_addblock(bl: *mut BlockList) -> c_int;
    pub fn map_delblock(bl: *mut BlockList) -> c_int;
    pub fn map_foreachinarea(
        f: unsafe extern "C" fn(*mut BlockList, ...) -> c_int,
        m: c_int, x: c_int, y: c_int, range: c_int, t: c_int,
        ...
    ) -> c_int;
    pub fn clif_lookgone(bl: *mut BlockList);
    pub fn clif_cnpclook_sub(bl: *mut BlockList, ...) -> c_int;
    pub fn clif_object_look_sub2(bl: *mut BlockList, ...) -> c_int;
    pub fn map_foreachincell(
        f: unsafe extern "C" fn(*mut BlockList, ...) -> c_int,
        m: c_int, x: c_int, y: c_int, t: c_int,
        ...
    ) -> c_int;
    pub fn map_foreachinblock(
        f: unsafe extern "C" fn(*mut BlockList, ...) -> c_int,
        m: c_int, x0: c_int, y0: c_int, x1: c_int, y1: c_int, t: c_int,
        ...
    ) -> c_int;
    pub fn map_canmove(m: c_int, x: c_int, y: c_int) -> c_int;
    pub fn map_moveblock(bl: *mut BlockList, x: c_int, y: c_int) -> c_int;
    pub fn clif_object_canmove(m: c_int, x: c_int, y: c_int, dir: c_int) -> c_int;
    pub fn clif_object_canmove_from(m: c_int, x: c_int, y: c_int, dir: c_int) -> c_int;
    pub fn clif_mob_look_start_func(bl: *mut BlockList, ...) -> c_int;
    pub fn clif_mob_look_close_func(bl: *mut BlockList, ...) -> c_int;
    pub fn clif_object_look_sub(bl: *mut BlockList, ...) -> c_int;
    pub fn clif_npc_move(bl: *mut BlockList, ...) -> c_int;
    /// Returns USER* (opaque — the USER struct has BlockList as its first field).
    pub fn map_id2sd(id: c_uint) -> *mut std::ffi::c_void;
    /// Fires a Lua script event on an NPC by name.
    /// Variadic — caller passes `nargs` positional block_list* arguments after `nargs`.
    pub fn sl_doscript_blargs(
        name:  *const c_char,
        func:  *const c_char,
        nargs: c_int,
        ...
    ) -> c_int;
    /// Returns 1 if the MOB pointed to by `bl` is in the MOB_DEAD state, 0 otherwise.
    pub fn npc_helper_mob_is_dead(bl: *mut BlockList) -> c_int;
    /// Returns 1 if the PC pointed to by `bl` should be skipped during NPC movement
    /// collision (dead/invisible/GM) given the NPC's block_list `npc_bl`, 0 otherwise.
    pub fn npc_helper_pc_is_skip(bl: *mut BlockList, npc_bl: *mut BlockList) -> c_int;
}

/// Enum value for `AREA` as defined in `c_src/map_parse.h`:
/// `enum { ALL_CLIENT=0, SAMESRV=1, SAMEMAP=2, SAMEMAP_WOS=3, AREA=4, ... }`
const AREA: c_int = 4;

/// Enum value for `LOOK_SEND` as defined in `c_src/map_parse.h`:
/// `enum { LOOK_GET=0, LOOK_SEND=1 }`
const LOOK_SEND: c_int = 1;

/// Returns an available NPC ID, allocating a new one if needed.
///
/// Scans from `NPC_START_NUM` upward for a slot not present in the ID
/// database.  When the scan reaches `NPC_ID` it bumps the high-water mark
/// and returns it.  Mirrors `npc_get_new_npcid` in `npc.c`.
///
/// # Safety
///
/// Caller must hold the server-wide lock; mutates the `NPC_ID` global and
/// calls `map_id2bl` which reads the C-managed entity table.
pub unsafe fn npc_get_new_npcid() -> c_uint {
    let mut x = NPC_START_NUM;
    while x <= NPC_ID {
        if x == NPC_ID {
            NPC_ID += 1;
        }
        if map_id2bl(x).is_null() {
            return x;
        }
        x += 1;
    }
    NPC_ID += 1;
    NPC_ID
}

/// Returns an available temp NPC ID.
///
/// Scans from `NPCT_START_NUM` upward for a free slot, bumping
/// `NPCTEMP_ID` when the high-water mark is reached.  Mirrors
/// `npc_get_new_npctempid` in `npc.c`.
///
/// # Safety
///
/// Same requirements as [`npc_get_new_npcid`].
pub unsafe fn npc_get_new_npctempid() -> c_uint {
    let mut x = NPCT_START_NUM;
    while x <= NPCTEMP_ID {
        if x == NPCTEMP_ID {
            NPCTEMP_ID += 1;
        }
        if map_id2bl(x).is_null() {
            return x;
        }
        x += 1;
    }
    NPCTEMP_ID += 1;
    NPCTEMP_ID
}

/// Decrements the temp NPC ID counter when a temp NPC is removed.
///
/// Only acts when `id` falls in the temp-NPC range and is not the sentinel
/// `F1_NPC` value.  Returns `0` unconditionally.  Mirrors `npc_idlower` in
/// `npc.c`.
///
/// # Safety
///
/// Mutates the `NPCTEMP_ID` global; caller must hold the server-wide lock.
pub unsafe fn npc_idlower(id: c_int) -> c_int {
    let id_u = id as c_uint;
    if id_u >= NPCT_START_NUM && id_u != F1_NPC {
        NPCTEMP_ID = NPCTEMP_ID.saturating_sub(1);
    }
    0
}

/// Reads an NPC's global registry value for the key `reg`.
///
/// Walks `nd.registry` looking for a case-insensitive match on the key
/// string.  Returns the stored `val` if found, or `0` if the key is absent.
/// Mirrors `npc_readglobalreg` in `npc.c`.
///
/// # Safety
///
/// `nd` must be a valid, aligned, non-null pointer to an `NpcData`.
/// `reg` must be a valid null-terminated C string.
pub unsafe fn npc_readglobalreg(nd: *mut NpcData, reg: *const c_char) -> c_int {
    let nd = &*nd;
    let reg_key = std::ffi::CStr::from_ptr(reg).to_bytes();
    for entry in &nd.registry {
        if entry.str[0] != 0
            && std::ffi::CStr::from_ptr(entry.str.as_ptr())
                .to_bytes()
                .eq_ignore_ascii_case(reg_key)
        {
            return entry.val;
        }
    }
    0
}

/// Sets an NPC's global registry value for key `reg` to `val`.
///
/// If the key already exists it is updated in place; setting `val` to `0`
/// additionally clears the key slot so it can be reused.  If no existing
/// entry is found, the first empty slot is used.
///
/// Returns `0` on success, `1` if the registry is full.
/// Mirrors `npc_setglobalreg` in `npc.c`.
///
/// # Safety
///
/// `nd` must be a valid, aligned, non-null pointer to an `NpcData`.
/// `reg` must be a valid null-terminated C string whose length (including
/// the NUL) fits within 64 bytes.
pub unsafe fn npc_setglobalreg(nd: *mut NpcData, reg: *const c_char, val: c_int) -> c_int {
    let nd = &mut *nd;
    let reg_cstr = std::ffi::CStr::from_ptr(reg);
    let reg_key  = reg_cstr.to_bytes();

    // Update existing entry if present.
    for entry in nd.registry.iter_mut() {
        if entry.str[0] != 0
            && std::ffi::CStr::from_ptr(entry.str.as_ptr())
                .to_bytes()
                .eq_ignore_ascii_case(reg_key)
        {
            if val == 0 {
                entry.str[0] = 0; // clear key — slot is now free
            }
            entry.val = val;
            return 0;
        }
    }

    // Allocate a new slot.
    for entry in nd.registry.iter_mut() {
        if entry.str[0] == 0 {
            let bytes = reg_cstr.to_bytes();
            let copy_len = bytes.len().min(entry.str.len() - 1);
            for (dst, &src) in entry.str.iter_mut().zip(bytes[..copy_len].iter()) {
                *dst = src as c_char;
            }
            entry.str[copy_len] = 0;
            entry.val = val;
            return 0;
        }
    }

    tracing::error!("npc_setglobalreg: registry full, could not set {:?}", reg_cstr);
    1
}

// ---------------------------------------------------------------------------
// npc_warp — teleport an NPC to a new map position
// ---------------------------------------------------------------------------

/// Teleports an NPC to map `m` at coordinates `(x, y)`.
///
/// Removes the NPC from its current grid cell, signals surrounding players
/// that it has gone, updates the NPC's position fields, re-inserts it into
/// the new cell, then broadcasts its new appearance to nearby players.
///
/// Mirrors `npc_warp` in `c_src/npc.c`.
///
/// # Safety
///
/// `nd` must be a valid, aligned, non-null pointer to an `NpcData` that is
/// currently registered in the map block-grid.  Caller must hold the
/// server-wide lock.
pub unsafe fn npc_warp(nd: *mut NpcData, m: c_int, x: c_int, y: c_int) -> c_int {
    if nd.is_null() { return 0; }
    let nd = &mut *nd;
    if nd.bl.id < NPC_START_NUM { return 0; }

    map_delblock(&raw mut nd.bl);
    clif_lookgone(&raw mut nd.bl);
    nd.bl.m = m as c_ushort;
    nd.bl.x = x as c_ushort;
    nd.bl.y = y as c_ushort;
    nd.bl.bl_type = BL_NPC as c_uchar;

    if map_addblock(&raw mut nd.bl) != 0 {
        tracing::error!("Error warping npcchar.");
    }

    if nd.npctype == 1 {
        map_foreachinarea(
            clif_cnpclook_sub,
            m, x, y, AREA, BL_PC,
            LOOK_SEND, nd as *mut NpcData,
        );
    } else {
        map_foreachinarea(
            clif_object_look_sub2,
            m, x, y, AREA, BL_PC,
            LOOK_SEND, nd as *mut NpcData,
        );
    }
    0
}

// ---------------------------------------------------------------------------
// npc_action / npc_movetime / npc_duration / npc_runtimers — timer callbacks
// ---------------------------------------------------------------------------

/// Advances the action timer for `nd` by 100 ms and fires the `"action"` Lua
/// event when the timer reaches `nd.actiontime`.
///
/// If `nd.owner` is non-zero the owning player's `block_list` is passed as the
/// second argument to `sl_doscript_blargs`; otherwise only the NPC's own
/// `block_list` is passed.
///
/// Mirrors `npc_action` in `c_src/npc.c`.
///
/// # Safety
///
/// `nd` must be a valid, aligned, non-null pointer to a live `NpcData`.
/// Caller must hold the server-wide lock.
pub unsafe fn npc_action(nd: *mut NpcData) -> c_int {
    if nd.is_null() { return 0; }
    let nd = &mut *nd;

    nd.time = nd.time.wrapping_add(100);  // mirrors C unsigned int overflow semantics

    let tsd: *mut std::ffi::c_void = if nd.owner != 0 {
        map_id2sd(nd.owner)
    } else {
        std::ptr::null_mut()
    };

    if nd.time >= nd.actiontime {
        nd.time = 0;
        if !tsd.is_null() {
            // SAFETY: map_id2sd returns *mut map_sessiondata whose first field `bl`
            // (struct block_list) is at byte offset 0. Casting to *mut BlockList is
            // equivalent to &tsd->bl as used in C npc_action.
            sl_doscript_blargs(
                nd.name.as_ptr(),
                b"action\0".as_ptr() as *const c_char,
                2 as c_int,
                &raw mut nd.bl,
                tsd as *mut BlockList,
            );
        } else {
            sl_doscript_blargs(
                nd.name.as_ptr(),
                b"action\0".as_ptr() as *const c_char,
                1 as c_int,
                &raw mut nd.bl,
            );
        }
    }
    0
}

/// Advances the move timer for `nd` by 100 ms and fires the `"move"` Lua event
/// when `nd.movetimer` reaches `nd.movetime`.
///
/// Mirrors `npc_movetime` in `c_src/npc.c`.
///
/// # Safety
///
/// Same requirements as [`npc_action`].
pub unsafe fn npc_movetime(nd: *mut NpcData) -> c_int {
    if nd.is_null() { return 0; }
    let nd = &mut *nd;

    nd.movetimer = nd.movetimer.wrapping_add(100);  // mirrors C unsigned int overflow semantics

    let tsd: *mut std::ffi::c_void = if nd.owner != 0 {
        map_id2sd(nd.owner)
    } else {
        std::ptr::null_mut()
    };

    if nd.movetimer >= nd.movetime {
        nd.movetimer = 0;
        if !tsd.is_null() {
            // SAFETY: see npc_action — map_sessiondata.bl is at offset 0.
            sl_doscript_blargs(
                nd.name.as_ptr(),
                b"move\0".as_ptr() as *const c_char,
                2 as c_int,
                &raw mut nd.bl,
                tsd as *mut BlockList,
            );
        } else {
            sl_doscript_blargs(
                nd.name.as_ptr(),
                b"move\0".as_ptr() as *const c_char,
                1 as c_int,
                &raw mut nd.bl,
            );
        }
    }
    0
}

/// Advances the duration timer for `nd` by 100 ms and fires the `"endAction"`
/// Lua event when `nd.duratime` reaches `nd.duration`.
///
/// Mirrors `npc_duration` in `c_src/npc.c`.
///
/// # Safety
///
/// Same requirements as [`npc_action`].
pub unsafe fn npc_duration(nd: *mut NpcData) -> c_int {
    if nd.is_null() { return 0; }
    let nd = &mut *nd;

    nd.duratime = nd.duratime.wrapping_add(100);  // mirrors C unsigned int overflow semantics

    let tsd: *mut std::ffi::c_void = if nd.owner != 0 {
        map_id2sd(nd.owner)
    } else {
        std::ptr::null_mut()
    };

    if nd.duratime >= nd.duration {
        nd.duratime = 0;
        if !tsd.is_null() {
            // SAFETY: see npc_action — map_sessiondata.bl is at offset 0.
            sl_doscript_blargs(
                nd.name.as_ptr(),
                b"endAction\0".as_ptr() as *const c_char,
                2 as c_int,
                &raw mut nd.bl,
                tsd as *mut BlockList,
            );
        } else {
            sl_doscript_blargs(
                nd.name.as_ptr(),
                b"endAction\0".as_ptr() as *const c_char,
                1 as c_int,
                &raw mut nd.bl,
            );
        }
    }
    0
}

/// Timer callback fired every 100 ms by the map-server timer wheel.
///
/// Iterates all regular NPCs (IDs `NPC_START_NUM..=NPC_ID`) and all temp NPCs
/// (`NPCT_START_NUM..=NPCTEMP_ID`).  For each non-null NPC it dispatches
/// `npc_action`, `npc_movetime`, and `npc_duration` as appropriate based on
/// their configured intervals.
///
/// Mirrors `npc_runtimers` in `c_src/npc.c`.
///
/// # Safety
///
/// Caller must hold the server-wide lock.  `map_id2npc` must be safe to call
/// for any ID in the scanned ranges.
pub unsafe fn npc_runtimers(_id: c_int, _n: c_int) -> c_int {
    // regular NPCs
    let mut x = NPC_START_NUM;
    while x <= NPC_ID {
        let nd = map_id2npc(x);
        if !nd.is_null() {
            if (*nd).actiontime > 0 {
                npc_action(nd);
            }
            if (*nd).movetime > 0 {
                npc_movetime(nd);
            }
            if (*nd).duration > 0 {
                npc_duration(nd);
            }
        }
        x += 1;
    }

    // temp NPCs
    let mut x = NPCT_START_NUM;
    while x <= NPCTEMP_ID {
        let nd = map_id2npc(x);
        if !nd.is_null() {
            if (*nd).actiontime > 0 {
                npc_action(nd);
            }
            if (*nd).movetime > 0 {
                npc_movetime(nd);
            }
            if (*nd).duration > 0 {
                npc_duration(nd);
            }
        }
        x += 1;
    }
    0
}

// ---------------------------------------------------------------------------
// npc_src_* — no-ops: the file-based NPC loader was replaced by SQL and is
// fully commented out in the C source.  These stubs exist only for ABI
// compatibility so that any remaining C call sites link without error.
// ---------------------------------------------------------------------------

/// No-op stub — SQL-backed NPC loader has no source list to clear.
pub fn npc_src_clear() -> c_int { 0 }

/// No-op stub — file-based NPC source registration is unused.
pub fn npc_src_add(_file: *const c_char) -> c_int { 0 }

/// No-op stub — warp source registration is unused.
pub fn npc_warp_add(_file: *const c_char) -> c_int { 0 }

// ---------------------------------------------------------------------------
// warp_init — load Warps table into the map block grid
// ---------------------------------------------------------------------------

/// Loads warp data from the `Warps` table into the map grid.
/// Mirrors `warp_init` in `npc.c`.
#[cfg(not(test))]
pub async unsafe fn warp_init_async() -> c_int {
    let p = get_pool();

    #[derive(sqlx::FromRow)]
    struct WarpRow {
        warp_id: u32,  // int(10) unsigned
        src_map: i32,  // int(10) signed
        src_x:   i32,  // int(10) signed
        src_y:   i32,  // int(10) signed
        dst_map: i32,  // int(10) signed
        dst_x:   i32,  // int(10) signed
        dst_y:   i32,  // int(10) signed
    }

    let rows: Vec<WarpRow> = match sqlx::query_as(
        "SELECT `WarpId` AS warp_id, `SourceMapId` AS src_map, \
         `SourceX` AS src_x, `SourceY` AS src_y, \
         `DestinationMapId` AS dst_map, `DestinationX` AS dst_x, \
         `DestinationY` AS dst_y FROM `Warps`"
    ).fetch_all(p).await {
        Ok(r) => r,
        Err(e) => { tracing::error!("[warp] query error: {e}"); return -1; }
    };

    let mut count = 0u32;
    for row in &rows {
        let md_src = get_map_ptr(row.src_map as u16);

        if !map_is_loaded(row.src_map as u16) || !map_is_loaded(row.dst_map as u16) {
            tracing::error!("[warp] src or dst map not loaded warp_id={} src={} dst={}",
                row.warp_id, row.src_map, row.dst_map);
            continue;
        }

        let md = &mut *md_src;

        if row.src_x as i32 > md.xs as i32 - 1 || row.src_y as i32 > md.ys as i32 - 1 {
            tracing::error!("[warp] map id: {}, x: {}, y: {}, source out of bounds",
                row.src_map, row.src_x, row.src_y);
            continue;
        }

        // Check destination coords too (log only, don't skip — matches C behavior)
        let md_dst = &*get_map_ptr(row.dst_map as u16);
        if row.dst_x as i32 > md_dst.xs as i32 - 1 || row.dst_y as i32 > md_dst.ys as i32 - 1 {
            tracing::error!("[warp] map id: {}, x: {}, y: {}, destination out of bounds",
                row.dst_map, row.dst_x, row.dst_y);
        }

        let war = Box::new(WarpList {
            x:    row.src_x as i32,
            y:    row.src_y as i32,
            tm:   row.dst_map as i32,
            tx:   row.dst_x as i32,
            ty:   row.dst_y as i32,
            next: std::ptr::null_mut(),
            prev: std::ptr::null_mut(),
        });
        let war_ptr = Box::into_raw(war);

        let idx = (row.src_x as usize / BLOCK_SIZE)
            + (row.src_y as usize / BLOCK_SIZE) * md.bxs as usize;
        // SAFETY: idx is in bounds when src coords are valid (checked above).
        // If coords are out of bounds, idx can exceed bxs*bys — this is an inherited
        // C behavior (npc.c does not guard this either).

        let existing = md.warp.add(idx).read();
        (*war_ptr).next = existing;
        if !existing.is_null() {
            (*existing).prev = war_ptr;
        }
        md.warp.add(idx).write(war_ptr);

        count += 1;
    }

    tracing::info!("[npc] warps_loaded count={count}");
    0
}

/// Blocking wrapper. Must be called after the sqlx pool is initialized.
#[cfg(not(test))]
pub fn warp_init() -> c_int {
    blocking_run(async { unsafe { warp_init_async().await } })
}

// ---------------------------------------------------------------------------
// npc_init — load NPCs from DB into the map block grid
// ---------------------------------------------------------------------------

#[cfg(not(test))]
fn server_id() -> u32 {
    extern "C" { static serverid: c_int; }
    unsafe { serverid as u32 }
}

#[cfg(not(test))]
fn copy_str_to_array<const N: usize>(s: &str, dst: &mut [c_char; N]) {
    for (d, b) in dst.iter_mut().zip(s.bytes()) {
        *d = b as c_char;
    }
    // Ensure null termination if string fits
    if s.len() < N { dst[s.len()] = 0; }
}

/// Async implementation of npc_init. Loads all NPCs from DB, allocates NpcData
/// structs, registers them in the block grid, then loads equipment for npctype==1.
///
/// Mirrors `npc_init` in `c_src/npc.c`.
#[cfg(not(test))]
pub async unsafe fn npc_init_async() -> c_int {
    let p = get_pool();
    let sid = server_id();

    #[derive(sqlx::FromRow)]
    struct NpcRow {
        row_npc_id:         u32,
        npc_identifier: String,
        npc_description: String,
        npc_type:       u32,   // SQLDT_UCHAR in C — use u32, downcast
        npc_map_id:     u16,
        npc_x:          u16,
        npc_y:          u16,
        npc_look:       u32,
        npc_look_color: u32,
        npc_timer:      u32,
        npc_sex:        u16,
        npc_side:       u32,   // SQLDT_UCHAR — use u32, downcast
        npc_state:      u32,   // SQLDT_UCHAR — use u32, downcast
        npc_face:       u16,
        npc_face_color: u16,
        npc_hair:       u16,
        npc_hair_color: u16,
        npc_skin_color: u16,
        npc_is_char:    u32,   // SQLDT_UCHAR — use u32, downcast
        npc_is_f1npc:   u32,
        npc_is_repair:  u32,   // SQLDT_UCHAR — use u32, downcast
        npc_is_shop:    u32,   // SQLDT_UCHAR — use u32, downcast
        npc_is_bank:    u32,   // SQLDT_UCHAR — use u32, downcast
        npc_return_dist: u32,  // SQLDT_UCHAR — use u32, downcast
        npc_move_time:  u32,
        npc_can_receive: u32,  // SQLDT_UCHAR — use u32, downcast
    }

    let sql = format!(
        "SELECT `NpcId` AS row_npc_id, `NpcIdentifier` AS npc_identifier, \
         `NpcDescription` AS npc_description, `NpcType` AS npc_type, \
         `NpcMapId` AS npc_map_id, `NpcX` AS npc_x, `NpcY` AS npc_y, \
         `NpcLook` AS npc_look, `NpcLookColor` AS npc_look_color, \
         `NpcTimer` AS npc_timer, `NpcSex` AS npc_sex, `NpcSide` AS npc_side, \
         `NpcState` AS npc_state, `NpcFace` AS npc_face, `NpcFaceColor` AS npc_face_color, \
         `NpcHair` AS npc_hair, `NpcHairColor` AS npc_hair_color, \
         `NpcSkinColor` AS npc_skin_color, `NpcIsChar` AS npc_is_char, \
         `NpcIsF1Npc` AS npc_is_f1npc, `NpcIsRepairNpc` AS npc_is_repair, \
         `NpcIsShopNpc` AS npc_is_shop, `NpcIsBankNpc` AS npc_is_bank, \
         `NpcReturnDistance` AS npc_return_dist, `NpcMoveTime` AS npc_move_time, \
         `NpcCanReceiveItem` AS npc_can_receive \
         FROM `NPCs{sid}` ORDER BY `NpcId`"
    );

    let rows: Vec<NpcRow> = match sqlx::query_as(&sql).fetch_all(p).await {
        Ok(r) => r,
        Err(e) => { tracing::error!("[npc] query error: {e}"); return -1; }
    };

    let count = rows.len() as u32;

    for row in &rows {
        // Check if an NPC with this DB id already exists (reload case)
        let mut nd: *mut NpcData = map_id2npc(row.row_npc_id);

        if row.npc_is_f1npc == 1 {
            // This is the F1 (special) NPC — use F1_NPC id
            nd = map_id2npc(F1_NPC);
            if nd.is_null() {
                // Allocate new zeroed NpcData
                nd = Box::into_raw(Box::new(std::mem::zeroed::<NpcData>()));
            } else {
                map_deliddb(&raw mut (*nd).bl);
            }
        } else if nd.is_null() {
            // New NPC — allocate
            nd = Box::into_raw(Box::new(std::mem::zeroed::<NpcData>()));
        } else {
            // Reload — remove from grid
            map_delblock(&raw mut (*nd).bl);
            map_deliddb(&raw mut (*nd).bl);
        }

        // Copy name strings (C uses memcpy with sizeof(name) = 45 into nd->name[64])
        copy_str_to_array(&row.npc_identifier, &mut (*nd).name);
        copy_str_to_array(&row.npc_description, &mut (*nd).npc_name);

        // Set block_list fields
        (*nd).bl.bl_type = BL_NPC as c_uchar;
        (*nd).bl.subtype = row.npc_type as c_uchar;
        (*nd).bl.graphic_id = row.npc_look;
        (*nd).bl.graphic_color = row.npc_look_color;

        // Call npc_warp only if position changed (or if newly allocated — bl fields are 0)
        let m   = row.npc_map_id;
        let xc  = row.npc_x;
        let yc  = row.npc_y;
        if m as c_ushort != (*nd).startm || xc as c_ushort != (*nd).startx || yc as c_ushort != (*nd).starty {
            npc_warp(nd, m as c_int, xc as c_int, yc as c_int);
        }

        (*nd).startm  = m as c_ushort;
        (*nd).startx  = xc as c_ushort;
        (*nd).starty  = yc as c_ushort;
        (*nd).id      = row.row_npc_id;
        (*nd).actiontime = row.npc_timer;
        (*nd).sex     = row.npc_sex as c_ushort;
        (*nd).side    = row.npc_side as c_char;
        (*nd).state   = row.npc_state as c_char;
        (*nd).face    = row.npc_face as c_ushort;
        (*nd).face_color  = row.npc_face_color as c_ushort;
        (*nd).hair    = row.npc_hair as c_ushort;
        (*nd).hair_color  = row.npc_hair_color as c_ushort;
        (*nd).armor_color = 0;
        (*nd).skin_color  = row.npc_skin_color as c_ushort;
        (*nd).npctype = row.npc_is_char as c_char;
        (*nd).shop_npc    = row.npc_is_shop as c_char;
        (*nd).repair_npc  = row.npc_is_repair as c_char;
        (*nd).bank_npc    = row.npc_is_bank as c_char;
        (*nd).retdist     = row.npc_return_dist as c_char;
        (*nd).movetime    = row.npc_move_time;
        (*nd).receive_item = row.npc_can_receive as c_char;

        // ID assignment: if bl.id < NPC_START_NUM, this is a new/fresh NPC
        if (*nd).bl.id < NPC_START_NUM {
            (*nd).bl.m = m as c_ushort;
            (*nd).bl.x = xc as c_ushort;
            (*nd).bl.y = yc as c_ushort;

            if row.npc_is_f1npc == 1 {
                (*nd).bl.id = F1_NPC;
            } else {
                (*nd).bl.id = NPC_START_NUM + row.row_npc_id - 2;
                NPC_ID = NPC_START_NUM + row.row_npc_id - 2;
            }
        }

        // Add to block grid only if subtype < 3
        if (*nd).bl.subtype < 3 {
            map_addblock(&raw mut (*nd).bl);
        }

        // Always add to ID database
        map_addiddb(&raw mut (*nd).bl);
    }

    // Equipment loading: loop from NPC_START_NUM to NPC_ID
    // For each NPC with npctype == 1, load equipment from NPCEquipment table
    let mut x = NPC_START_NUM;
    while x <= NPC_ID {
        let nd: *mut NpcData = map_id2npc(x);
        if !nd.is_null() && (*nd).npctype == 1 {
            let nd_id = (*nd).id;

            #[derive(sqlx::FromRow)]
            struct EquipRow {
                neq_look:  u32,
                neq_color: u32,
                neq_slot:  u32,  // SQLDT_UCHAR in C — use u32, downcast
            }

            let equip_sql = format!(
                "SELECT `NeqLook` AS neq_look, `NeqColor` AS neq_color, \
                 `NeqSlot` AS neq_slot \
                 FROM `NPCEquipment{sid}` WHERE `NeqNpcId` = {nd_id} LIMIT 14"
            );

            let equip_rows: Vec<EquipRow> = match sqlx::query_as(&equip_sql).fetch_all(p).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!("[npc] equipment query error for NPC_ID={nd_id}: {e}");
                    x += 1;
                    continue;
                }
            };

            for erow in &equip_rows {
                let pos = erow.neq_slot as usize;
                if pos < MAX_EQUIP {
                    // C: memcpy(&nd->equip[(int)pos], &item, sizeof(item))
                    // item.id = NeqLook, item.custom = NeqColor; all others zeroed
                    // (the SELECT uses '' and literal '1','0','0' for other fields
                    //  but the binding assigns: real_name='', id=NeqLook, amount=1,
                    //  dura=0, owner=0, custom=NeqColor, pos=NeqSlot)
                    // We zero the slot first then set the relevant fields
                    (*nd).equip[pos] = std::mem::zeroed();
                    (*nd).equip[pos].id     = erow.neq_look;
                    (*nd).equip[pos].custom = erow.neq_color;
                    (*nd).equip[pos].amount = 1;
                    (*nd).equip[pos].pos    = pos as u8;  // C copies NeqSlot → item.pos via memcpy
                }
            }
        }
        x += 1;
    }

    tracing::info!("[npc] read done count={count}");
    0
}

/// Blocking wrapper. Must be called after the sqlx pool is initialized.
#[cfg(not(test))]
pub fn npc_init() -> c_int {
    blocking_run(async { unsafe { npc_init_async().await } })
}

// ---------------------------------------------------------------------------
// npc_move_sub — va_list callback for map_foreachincell during NPC movement
// ---------------------------------------------------------------------------

/// Callback for `map_foreachincell` during NPC movement — checks for blocking entities.
///
/// Receives the candidate `block_list*` and the NPC pointer via `va_list`.
/// Sets `nd.canmove = 1` if the cell is occupied by a non-passable entity.
///
/// Logic (mirrors `npc_move_sub` in `c_src/npc.c` exactly):
/// - `BL_NPC`: skip if `bl.subtype != 0` (non-zero subtype NPCs don't block).
/// - `BL_MOB`: skip if the mob is in `MOB_DEAD` state.
/// - `BL_PC`:  skip if dead/invisible/GM (delegated to `npc_helper_pc_is_skip`).
/// - Any other type: skip (return 0 without setting `canmove`).
///
/// # Safety
///
/// Must only be called by `map_foreachincell`. `ap` must contain exactly one
/// argument: a `*mut NpcData`.
#[no_mangle]
pub unsafe extern "C" fn npc_move_sub(bl: *mut BlockList, mut ap: ...) -> c_int {
    let nd = ap.arg::<*mut NpcData>();
    if nd.is_null() || bl.is_null() { return 0; }
    if (*nd).canmove == 1 { return 0; }

    let bl_ref = &*bl;
    match bl_ref.bl_type {
        x if x == BL_NPC as c_uchar => {
            // Non-zero subtype NPCs do not block movement.
            if bl_ref.subtype != 0 { return 0; }
        }
        x if x == BL_MOB as c_uchar => {
            // Dead mobs do not block movement.
            if npc_helper_mob_is_dead(bl) != 0 { return 0; }
        }
        x if x == BL_PC as c_uchar => {
            // Dead / invisible / GM players do not block movement.
            if npc_helper_pc_is_skip(bl, &raw mut (*nd).bl) != 0 { return 0; }
        }
        _ => {
            // Unknown type — do not block.
            return 0;
        }
    }

    (*nd).canmove = 1;
    0
}

// ---------------------------------------------------------------------------
// npc_move — move an NPC one step in its facing direction
// ---------------------------------------------------------------------------

/// Moves an NPC one step in its facing direction.
///
/// Computes the new candidate cell based on `nd.side` (direction 0-3 = up/right/down/left),
/// checks for warps and blocking entities, then calls `map_moveblock` if the move is valid.
/// Broadcasts visibility updates to nearby players.
///
/// Mirrors `npc_move` in `c_src/npc.c`.
///
/// # Safety
///
/// `nd` must be a valid, aligned, non-null pointer to a live `NpcData`.
/// Caller must hold the server-wide lock.
#[cfg(not(test))]
pub unsafe fn npc_move(nd: *mut NpcData) -> c_int {
    if nd.is_null() { return 0; }
    let nd = &mut *nd;

    let m = nd.bl.m as c_int;
    let backx = nd.bl.x as c_int;
    let backy = nd.bl.y as c_int;
    let mut dx = backx;
    let mut dy = backy;
    let direction = nd.side as c_int;
    let mut x0 = backx;
    let mut y0 = backy;
    let mut x1: c_int = 0;
    let mut y1: c_int = 0;
    let mut nothingnew: c_int = 0;

    let md = crate::ffi::map_db::get_map_ptr(nd.bl.m);
    if md.is_null() { return 0; }
    let map_xs = (*md).xs as c_int;
    let map_ys = (*md).ys as c_int;

    match direction {
        0 => {
            // UP
            if backy > 0 {
                dy = backy - 1;
                x0 -= 9;
                if x0 < 0 { x0 = 0; }
                y0 -= 9;
                y1 = 1;
                x1 = 19;
                if y0 < 7 { nothingnew = 1; }
                if y0 == 7 { y1 += 7; y0 = 0; }
                if x0 + 19 + 9 >= map_xs { x1 += 9 - (x0 + 19 + 9 - map_xs); }
                if x0 <= 8 { x1 += x0; x0 = 0; }
            }
        }
        1 => {
            // RIGHT
            if backx < map_xs {
                x0 += 10;
                y0 -= 8;
                if y0 < 0 { y0 = 0; }
                dx = backx + 1;
                y1 = 17;
                x1 = 1;
                if x0 > map_xs - 9 { nothingnew = 1; }
                if x0 == map_xs - 9 { x1 += 9; }
                if y0 + 17 + 8 >= map_ys { y1 += 8 - (y0 + 17 + 8 - map_ys); }
                if y0 <= 7 { y1 += y0; y0 = 0; }
            }
        }
        2 => {
            // DOWN
            if backy < map_ys {
                x0 -= 9;
                if x0 < 0 { x0 = 0; }
                y0 += 9;
                dy = backy + 1;
                y1 = 1;
                x1 = 19;
                if y0 + 8 > map_ys { nothingnew = 1; }
                if y0 + 8 == map_ys { y1 += 8; }
                if x0 + 19 + 9 >= map_xs { x1 += 9 - (x0 + 19 + 9 - map_xs); }
                if x0 <= 8 { x1 += x0; x0 = 0; }
            }
        }
        3 => {
            // LEFT
            if backx > 0 {
                x0 -= 10;
                y0 -= 8;
                if y0 < 0 { y0 = 0; }
                y1 = 17;
                x1 = 1;
                dx = backx - 1;
                if x0 < 8 { nothingnew = 1; }
                if x0 == 8 { x0 = 0; x1 += 8; }
                if y0 + 17 + 8 >= map_ys { y1 += 8 - (y0 + 17 + 8 - map_ys); }
                if y0 <= 7 { y1 += y0; y0 = 0; }
            }
        }
        _ => { return 0; }
    }

    if dx >= map_xs { dx = map_xs - 1; }
    if dy >= map_ys { dy = map_ys - 1; }

    // Check warp at destination block
    let mut war = crate::ffi::map_db::map_get_warp(nd.bl.m, dx as u16, dy as u16);
    while !war.is_null() {
        if (*war).x == dx && (*war).y == dy { return 0; }
        war = (*war).next;
    }

    // Check for blockers in destination cell
    nd.canmove = 0;
    map_foreachincell(npc_move_sub, m, dx, dy, BL_MOB, nd as *mut NpcData);
    map_foreachincell(npc_move_sub, m, dx, dy, BL_PC,  nd as *mut NpcData);
    map_foreachincell(npc_move_sub, m, dx, dy, BL_NPC, nd as *mut NpcData);

    if clif_object_canmove(m, dx, dy, direction) != 0 {
        nd.canmove = 0;
        return 0;
    }
    if clif_object_canmove_from(m, backx, backy, direction) != 0 {
        nd.canmove = 0;
        return 0;
    }
    if map_canmove(m, dx, dy) == 1 || nd.canmove == 1 {
        nd.canmove = 0;
        return 0;
    }

    if x0 > map_xs - 1 { x0 = map_xs - 1; }
    if y0 > map_ys - 1 { y0 = map_ys - 1; }
    if x0 < 0 { x0 = 0; }
    if y0 < 0 { y0 = 0; }
    if dx >= map_xs { dx = backx; }
    if dy >= map_ys { dy = backy; }
    if dx < 0 { dx = backx; }
    if dy < 0 { dy = backy; }

    if dx != backx || dy != backy {
        nd.bl.bx = backx as c_uint;
        nd.bl.by = backy as c_uint;
        map_moveblock(&raw mut nd.bl, dx, dy);

        if nothingnew == 0 {
            if nd.npctype == 1 {
                map_foreachinblock(
                    clif_cnpclook_sub,
                    nd.bl.m as c_int, x0, y0, x0 + x1 - 1, y0 + y1 - 1,
                    BL_PC,
                    LOOK_SEND as c_int, nd as *mut NpcData,
                );
            } else {
                map_foreachinblock(
                    clif_mob_look_start_func,
                    nd.bl.m as c_int, x0, y0, x0 + x1 - 1, y0 + y1 - 1,
                    BL_PC,
                    nd as *mut NpcData,
                );
                map_foreachinblock(
                    clif_object_look_sub,
                    nd.bl.m as c_int, x0, y0, x0 + x1 - 1, y0 + y1 - 1,
                    BL_PC,
                    LOOK_SEND as c_int, &raw mut nd.bl,
                );
                map_foreachinblock(
                    clif_mob_look_close_func,
                    nd.bl.m as c_int, x0, y0, x0 + x1 - 1, y0 + y1 - 1,
                    BL_PC,
                    nd as *mut NpcData,
                );
            }
        }

        map_foreachinarea(
            clif_npc_move,
            m, nd.bl.x as c_int, nd.bl.y as c_int, AREA, BL_PC,
            LOOK_SEND as c_int, nd as *mut NpcData,
        );
        return 1;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npc_data_size() {
        assert_eq!(
            std::mem::size_of::<NpcData>(),
            20416,
            "NpcData size mismatch — check struct npc_data in map_server.h"
        );
    }

    #[test]
    fn npc_data_offsets() {
        assert_eq!(std::mem::offset_of!(NpcData, equip),     48);
        assert_eq!(std::mem::offset_of!(NpcData, registry),  13248);
        assert_eq!(std::mem::offset_of!(NpcData, gfx),       20048);
        assert_eq!(std::mem::offset_of!(NpcData, id),        20120);
        assert_eq!(std::mem::offset_of!(NpcData, name),      20180);
        assert_eq!(std::mem::offset_of!(NpcData, movetimer), 20384);
        assert_eq!(std::mem::offset_of!(NpcData, sex),       20392);
    }

    #[test]
    fn npc_data_canmove_offset() {
        // canmove is the 3rd of 10 c_char fields starting at offset 20372
        // state=20372, side=20373, canmove=20374
        assert_eq!(std::mem::offset_of!(NpcData, canmove), 20374);
    }

    #[test]
    fn globalreg_read_missing_returns_zero() {
        let mut nd = unsafe { Box::<NpcData>::new_zeroed().assume_init() };
        assert_eq!(
            unsafe { npc_readglobalreg(&raw mut *nd, b"nosuchkey\0".as_ptr() as _) },
            0
        );
    }

    #[test]
    fn globalreg_set_then_read() {
        let mut nd = unsafe { Box::<NpcData>::new_zeroed().assume_init() };
        let key = b"mykey\0".as_ptr() as *const c_char;
        unsafe {
            assert_eq!(npc_setglobalreg(&raw mut *nd, key, 42), 0);
            assert_eq!(npc_readglobalreg(&raw mut *nd, key), 42);
        }
    }

    #[test]
    fn globalreg_set_zero_clears_key() {
        let mut nd = unsafe { Box::<NpcData>::new_zeroed().assume_init() };
        let key = b"mykey\0".as_ptr() as *const c_char;
        unsafe {
            npc_setglobalreg(&raw mut *nd, key, 99);
            npc_setglobalreg(&raw mut *nd, key, 0);
            // After setting to 0, key should be cleared — re-reading returns 0
            assert_eq!(npc_readglobalreg(&raw mut *nd, key), 0);
        }
    }

    #[test]
    fn npc_idlower_decrements_temp() {
        unsafe {
            let orig = NPCTEMP_ID;
            NPCTEMP_ID = NPCT_START_NUM + 5;
            npc_idlower((NPCT_START_NUM + 1) as c_int);
            assert_eq!(NPCTEMP_ID, NPCT_START_NUM + 4);
            NPCTEMP_ID = orig;
        }
    }

    #[test]
    fn npc_idlower_ignores_regular_npc() {
        unsafe {
            let orig = NPCTEMP_ID;
            NPCTEMP_ID = NPCT_START_NUM + 5;
            // NPC_START_NUM is not a temp NPC — counter should not change
            npc_idlower(NPC_START_NUM as c_int);
            assert_eq!(NPCTEMP_ID, NPCT_START_NUM + 5);
            NPCTEMP_ID = orig;
        }
    }
}
