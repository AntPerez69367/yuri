//! NPC game logic.

#![allow(non_snake_case, dead_code)]

use std::sync::atomic::{AtomicU32, Ordering};
use crate::database::map_db::{BlockList, GlobalReg};
use crate::common::types::Item;
use crate::common::player::inventory::MAX_EQUIP;
use crate::game::types::GfxViewer;

use crate::database::map_db::{WarpList, BLOCK_SIZE};
use crate::database::{blocking_run_async, get_pool};
use crate::database::map_db::{get_map_ptr, map_is_loaded};

// MAX_EQUIP is defined in charstatus::MAX_EQUIP (imported above) — same slot count.
pub const MAX_GLOBALNPCREG: usize = 100;
pub const NPC_START_NUM: u32      = 3221225472;
pub const NPCT_START_NUM: u32     = 3321225472;
pub const F1_NPC: u32             = 4294967295;

pub const BL_PC:  i32 = 0x01;
pub const BL_MOB: i32 = 0x02;  // used by npc_move_sub (Task 10)
pub const BL_NPC: i32 = 0x04;

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
///  20372     state..retdist        10   (10 × i8)
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
/// `i8` fields leave the current offset at 20382, which is not 4-byte
/// aligned (20382 % 4 == 2).
#[repr(C)]
pub struct NpcData {
    pub bl:            BlockList,
    pub equip:         [Item; MAX_EQUIP],
    pub registry:      [GlobalReg; MAX_GLOBALNPCREG],
    pub gfx:           GfxViewer,
    pub id:            u32,
    pub actiontime:    u32,
    pub owner:         u32,
    pub duration:      u32,
    pub lastaction:    u32,
    pub time:          u32,
    pub duratime:      u32,
    pub item_look:     u32,
    pub item_owner:    u32,
    pub item_color:    u32,
    pub item_id:       u32,
    pub item_slot:     u32,
    pub item_pos:      u32,
    pub item_amount:   u32,
    pub item_dura:     u32,
    pub name:          [i8; 64],
    pub npc_name:      [i8; 64],
    pub itemreal_name: [i8; 64],
    pub state:         i8,
    pub side:          i8,
    pub canmove:       i8,
    pub npctype:       i8,
    pub clone:         i8,
    pub shop_npc:      i8,
    pub repair_npc:    i8,
    pub bank_npc:      i8,
    pub receive_item:  i8,
    pub retdist:       i8,
    pub _pad:          [u8; 2],
    pub movetimer:     u32,
    pub movetime:      u32,
    pub sex:           u16,
    pub face:          u16,
    pub face_color:    u16,
    pub hair:          u16,
    pub hair_color:    u16,
    pub armor_color:   u16,
    pub skin_color:    u16,
    pub startm:        u16,
    pub startx:        u16,
    pub starty:        u16,
    pub returning:     u8,
    // 3 bytes trailing padding added automatically by repr(C) to align struct to 8 bytes
}

// NPC ID counters — match C globals npc_id and npctemp_id.
// #[export_name] exports as "npc_id" so map_server.c can read it directly
// while keeping the Rust-idiomatic SCREAMING_SNAKE_CASE name internally.
// AtomicU32 has the same ABI as u32 on x86-64.
#[export_name = "npc_id"]
pub static NPC_ID: AtomicU32 = AtomicU32::new(NPC_START_NUM);
pub static NPCTEMP_ID: AtomicU32 = AtomicU32::new(NPCT_START_NUM);

use crate::game::map_server::map_canmove;
use crate::game::block::{map_addblock, map_delblock, map_moveblock};
use crate::game::map_parse::visual::clif_lookgone;
use crate::game::map_parse::movement::{clif_object_canmove, clif_object_canmove_from};

fn map_id2bl(id: u32) -> *mut BlockList {
    crate::game::map_server::map_id2bl_ref(id)
}

use crate::game::block::AreaType;
use crate::game::block_grid;
use crate::game::map_parse::visual::{
    clif_cnpclook_inner, clif_mob_look_start_func_inner, clif_mob_look_close_func_inner,
    clif_object_look_sub_inner, clif_object_look_sub2_inner,
};
use crate::game::map_parse::movement::clif_npc_move_inner;

/// Dispatch a Lua event with a single block_list argument.
unsafe fn sl_doscript_simple(root: *const i8, method: *const i8, bl: *mut BlockList) -> i32 {
    crate::game::scripting::doscript_blargs(root, method, &[bl as *mut _])
}

/// Dispatch a Lua event with two block_list arguments.
unsafe fn sl_doscript_2(root: *const i8, method: *const i8, bl1: *mut BlockList, bl2: *mut BlockList) -> i32 {
    crate::game::scripting::doscript_blargs(root, method, &[bl1 as *mut _, bl2 as *mut _])
}

/// Enum value for `AREA`:
/// `enum { ALL_CLIENT=0, SAMESRV=1, SAMEMAP=2, SAMEMAP_WOS=3, AREA=4, ... }`
const AREA: i32 = 4;

/// Enum value for `LOOK_SEND`:
/// `enum { LOOK_GET=0, LOOK_SEND=1 }`
const LOOK_SEND: i32 = 1;

/// Returns an available NPC ID, allocating a new one if needed.
///
/// Scans from `NPC_START_NUM` upward for a slot not present in the ID
/// database.  When the scan reaches `NPC_ID` it bumps the high-water mark
/// and returns it.
///
/// # Safety
///
/// Caller must hold the server-wide lock; mutates the `NPC_ID` global and
/// calls `map_id2bl` which reads the C-managed entity table.
pub unsafe fn npc_get_new_npcid() -> u32 {
    let mut x = NPC_START_NUM;
    loop {
        let cur = NPC_ID.load(Ordering::Relaxed);
        if x > cur { break; }
        if x == cur {
            NPC_ID.store(cur + 1, Ordering::Relaxed);
        }
        if map_id2bl(x).is_null() {
            return x;
        }
        x += 1;
    }
    NPC_ID.fetch_add(1, Ordering::Relaxed);
    NPC_ID.load(Ordering::Relaxed)
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
pub unsafe fn npc_get_new_npctempid() -> u32 {
    let mut x = NPCT_START_NUM;
    loop {
        let cur = NPCTEMP_ID.load(Ordering::Relaxed);
        if x > cur { break; }
        if x == cur {
            NPCTEMP_ID.store(cur + 1, Ordering::Relaxed);
        }
        if map_id2bl(x).is_null() {
            return x;
        }
        x += 1;
    }
    NPCTEMP_ID.fetch_add(1, Ordering::Relaxed);
    NPCTEMP_ID.load(Ordering::Relaxed)
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
pub unsafe fn npc_idlower(id: i32) -> i32 {
    let id_u = id as u32;
    if id_u >= NPCT_START_NUM && id_u != F1_NPC {
        let cur = NPCTEMP_ID.load(Ordering::Relaxed);
        NPCTEMP_ID.store(cur.saturating_sub(1), Ordering::Relaxed);
    }
    0
}

/// Reads an NPC's global registry value for the key `reg`.
///
/// Walks `nd.registry` looking for a case-insensitive match on the key
/// string.  Returns the stored `val` if found, or `0` if the key is absent.
///
/// # Safety
///
/// `nd` must be a valid, aligned, non-null pointer to an `NpcData`.
/// `reg` must be a valid null-terminated C string.
pub unsafe fn npc_readglobalreg(nd: *mut NpcData, reg: *const i8) -> i32 {
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
///
/// # Safety
///
/// `nd` must be a valid, aligned, non-null pointer to an `NpcData`.
/// `reg` must be a valid null-terminated C string whose length (including
/// the NUL) fits within 64 bytes.
pub unsafe fn npc_setglobalreg(nd: *mut NpcData, reg: *const i8, val: i32) -> i32 {
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
                *dst = src as i8;
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
///
/// # Safety
///
/// `nd` must be a valid, aligned, non-null pointer to an `NpcData` that is
/// currently registered in the map block-grid.  Caller must hold the
/// server-wide lock.
pub unsafe fn npc_warp(nd: *mut NpcData, m: i32, x: i32, y: i32) -> i32 {
    if nd.is_null() { return 0; }
    let nd = &mut *nd;
    if nd.bl.id < NPC_START_NUM { return 0; }

    map_delblock(&raw mut nd.bl);
    clif_lookgone(&raw const nd.bl);
    nd.bl.m = m as u16;
    nd.bl.x = x as u16;
    nd.bl.y = y as u16;
    nd.bl.bl_type = BL_NPC as u8;

    if map_addblock(&raw mut nd.bl) != 0 {
        tracing::error!("Error warping npcchar.");
    }

    if let Some(grid) = block_grid::get_grid(m as usize) {
        let slot = &*crate::database::map_db::raw_map_ptr().add(m as usize);
        let ids = block_grid::ids_in_area(grid, x, y, AreaType::Area, slot.xs as i32, slot.ys as i32);
        let nd_bl = nd as *const NpcData as *const BlockList;
        if nd.npctype == 1 {
            for id in ids {
                if let Some(arc) = crate::game::map_server::map_id2sd_pc(id) {
                    let pc = &*arc.read();
                    clif_cnpclook_inner(&raw const pc.bl, LOOK_SEND, nd_bl);
                }
            }
        } else {
            for id in ids {
                if let Some(arc) = crate::game::map_server::map_id2sd_pc(id) {
                    let pc = &*arc.read();
                    clif_object_look_sub2_inner(&raw const pc.bl, LOOK_SEND, nd_bl);
                }
            }
        }
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
///
/// # Safety
///
/// `nd` must be a valid, aligned, non-null pointer to a live `NpcData`.
/// Caller must hold the server-wide lock.
pub unsafe fn npc_action(nd: *mut NpcData) -> i32 {
    if nd.is_null() { return 0; }
    let nd = &mut *nd;

    nd.time = nd.time.wrapping_add(100);

    let tsd_arc = if nd.owner != 0 { crate::game::map_server::map_id2sd_pc(nd.owner) } else { None };
    let tsd_bl: Option<*mut BlockList> = tsd_arc.as_ref().map(|arc| &mut arc.write().bl as *mut BlockList);

    if nd.time >= nd.actiontime {
        nd.time = 0;
        if let Some(tsd_bl) = tsd_bl {
            sl_doscript_2(nd.name.as_ptr(), b"action\0".as_ptr() as *const i8, &raw mut nd.bl, tsd_bl);
        } else {
            sl_doscript_simple(nd.name.as_ptr(), b"action\0".as_ptr() as *const i8, &raw mut nd.bl);
        }
    }
    0
}

/// Advances the move timer for `nd` by 100 ms and fires the `"move"` Lua event
/// when `nd.movetimer` reaches `nd.movetime`.
///
///
/// # Safety
///
/// Same requirements as [`npc_action`].
pub unsafe fn npc_movetime(nd: *mut NpcData) -> i32 {
    if nd.is_null() { return 0; }
    let nd = &mut *nd;

    nd.movetimer = nd.movetimer.wrapping_add(100);

    let tsd_arc = if nd.owner != 0 { crate::game::map_server::map_id2sd_pc(nd.owner) } else { None };
    let tsd_bl: Option<*mut BlockList> = tsd_arc.as_ref().map(|arc| &mut arc.write().bl as *mut BlockList);

    if nd.movetimer >= nd.movetime {
        nd.movetimer = 0;
        if let Some(tsd_bl) = tsd_bl {
            sl_doscript_2(nd.name.as_ptr(), b"move\0".as_ptr() as *const i8, &raw mut nd.bl, tsd_bl);
        } else {
            sl_doscript_simple(nd.name.as_ptr(), b"move\0".as_ptr() as *const i8, &raw mut nd.bl);
        }
    }
    0
}

/// Advances the duration timer for `nd` by 100 ms and fires the `"endAction"`
/// Lua event when `nd.duratime` reaches `nd.duration`.
///
///
/// # Safety
///
/// Same requirements as [`npc_action`].
pub unsafe fn npc_duration(nd: *mut NpcData) -> i32 {
    if nd.is_null() { return 0; }
    let nd = &mut *nd;

    nd.duratime = nd.duratime.wrapping_add(100);

    let tsd_arc = if nd.owner != 0 { crate::game::map_server::map_id2sd_pc(nd.owner) } else { None };
    let tsd_bl: Option<*mut BlockList> = tsd_arc.as_ref().map(|arc| &mut arc.write().bl as *mut BlockList);

    if nd.duratime >= nd.duration {
        nd.duratime = 0;
        if let Some(tsd_bl) = tsd_bl {
            sl_doscript_2(nd.name.as_ptr(), b"endAction\0".as_ptr() as *const i8, &raw mut nd.bl, tsd_bl);
        } else {
            sl_doscript_simple(nd.name.as_ptr(), b"endAction\0".as_ptr() as *const i8, &raw mut nd.bl);
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
///
/// # Safety
///
/// Caller must hold the server-wide lock.  `map_id2npc` must be safe to call
/// for any ID in the scanned ranges.
pub unsafe fn npc_runtimers() {
    // regular NPCs
    let mut x = NPC_START_NUM;
    let npc_hi = NPC_ID.load(Ordering::Relaxed);
    while x <= npc_hi {
        if let Some(arc) = crate::game::map_server::map_id2npc_ref(x) {
            let nd = &mut *arc.write();
            let nd_ptr = nd as *mut NpcData;
            if nd.actiontime > 0 {
                npc_action(nd_ptr);
            }
            if nd.movetime > 0 {
                npc_movetime(nd_ptr);
            }
            if nd.duration > 0 {
                npc_duration(nd_ptr);
            }
        }
        x += 1;
    }

    // temp NPCs
    let mut x = NPCT_START_NUM;
    let npct_hi = NPCTEMP_ID.load(Ordering::Relaxed);
    while x <= npct_hi {
        if let Some(arc) = crate::game::map_server::map_id2npc_ref(x) {
            let nd = &mut *arc.write();
            let nd_ptr = nd as *mut NpcData;
            if nd.actiontime > 0 {
                npc_action(nd_ptr);
            }
            if nd.movetime > 0 {
                npc_movetime(nd_ptr);
            }
            if nd.duration > 0 {
                npc_duration(nd_ptr);
            }
        }
        x += 1;
    }
}

// ---------------------------------------------------------------------------
// npc_src_* — no-ops: NPC loading is done via SQL.
// compatibility so that any remaining C call sites link without error.
// ---------------------------------------------------------------------------

/// No-op stub — SQL-backed NPC loader has no source list to clear.
pub fn npc_src_clear() -> i32 { 0 }

/// No-op stub — file-based NPC source registration is unused.
pub fn npc_src_add(_file: *const i8) -> i32 { 0 }

/// No-op stub — warp source registration is unused.
pub fn npc_warp_add(_file: *const i8) -> i32 { 0 }

// ---------------------------------------------------------------------------
// warp_init — load Warps table into the map block grid
// ---------------------------------------------------------------------------

/// Loads warp data from the `Warps` table into the map grid.
pub async unsafe fn warp_init_async() -> i32 {
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

        if row.src_x < 0 || row.src_y < 0
            || row.src_x > md.xs as i32 - 1 || row.src_y > md.ys as i32 - 1
        {
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
pub unsafe fn warp_init() -> i32 {
    blocking_run_async(crate::database::assert_send(async { unsafe { warp_init_async().await } }))
}

// ---------------------------------------------------------------------------
// npc_init — load NPCs from DB into the map block grid
// ---------------------------------------------------------------------------

fn server_id() -> u32 {
    crate::config::config().server_id as u32
}

fn copy_str_to_array<const N: usize>(s: &str, dst: &mut [i8; N]) {
    let copy_len = s.len().min(N - 1);
    for (d, b) in dst.iter_mut().zip(s.bytes().take(copy_len)) {
        *d = b as i8;
    }
    dst[copy_len] = 0;
}

/// Async implementation of npc_init. Loads all NPCs from DB, allocates NpcData
/// structs, registers them in the block grid, then loads equipment for npctype==1.
///
pub async unsafe fn npc_init_async() -> i32 {
    let p = get_pool();
    let sid = server_id();

    #[derive(sqlx::FromRow)]
    struct NpcRow {
        row_npc_id:         u32,
        npc_identifier: String,
        npc_description: String,
        npc_type:       u32,   // SQLDT_UCHAR in C — use u32, downcast
        npc_map_id:     u32,
        npc_x:          u32,
        npc_y:          u32,
        npc_look:       u32,
        npc_look_color: u32,
        npc_timer:      u32,
        npc_sex:        u32,
        npc_side:       u32,   // SQLDT_UCHAR — use u32, downcast
        npc_state:      u32,   // SQLDT_UCHAR — use u32, downcast
        npc_face:       u32,
        npc_face_color: u32,
        npc_hair:       u32,
        npc_hair_color: u32,
        npc_skin_color: u32,
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
        let mut nd: *mut NpcData = crate::game::map_server::map_id2npc_ref(row.row_npc_id)
            .map(|arc| &mut *arc.write() as *mut NpcData)
            .unwrap_or(std::ptr::null_mut());

        let mut is_new_alloc = false;
        if row.npc_is_f1npc == 1 {
            // This is the F1 (special) NPC — use F1_NPC id
            nd = crate::game::map_server::map_id2npc_ref(F1_NPC)
                .map(|arc| &mut *arc.write() as *mut NpcData)
                .unwrap_or(std::ptr::null_mut());
            if nd.is_null() {
                nd = Box::into_raw(Box::new(std::mem::zeroed::<NpcData>()));
                is_new_alloc = true;
            } else {
                // Reload: unlink from block grid for re-add below; stay in NPC_MAP.
                map_delblock(&raw mut (*nd).bl);
            }
        } else if nd.is_null() {
            // New NPC — allocate
            nd = Box::into_raw(Box::new(std::mem::zeroed::<NpcData>()));
            is_new_alloc = true;
        } else {
            // Reload: unlink from block grid for re-add below; stay in NPC_MAP.
            map_delblock(&raw mut (*nd).bl);
        }

        // Copy name strings (C uses memcpy with sizeof(name) = 45 into nd->name[64])
        copy_str_to_array(&row.npc_identifier, &mut (*nd).name);
        copy_str_to_array(&row.npc_description, &mut (*nd).npc_name);

        // Set block_list fields
        (*nd).bl.bl_type = BL_NPC as u8;
        (*nd).bl.subtype = row.npc_type as u8;
        (*nd).bl.graphic_id = row.npc_look;
        (*nd).bl.graphic_color = row.npc_look_color;

        // Call npc_warp only if position changed (or if newly allocated — bl fields are 0)
        let m   = row.npc_map_id;
        let xc  = row.npc_x;
        let yc  = row.npc_y;
        if m as u16 != (*nd).startm || xc as u16 != (*nd).startx || yc as u16 != (*nd).starty {
            npc_warp(nd, m as i32, xc as i32, yc as i32);
        }

        (*nd).startm  = m as u16;
        (*nd).startx  = xc as u16;
        (*nd).starty  = yc as u16;
        (*nd).id      = row.row_npc_id;
        (*nd).actiontime = row.npc_timer;
        (*nd).sex     = row.npc_sex as u16;
        (*nd).side    = row.npc_side as i8;
        (*nd).state   = row.npc_state as i8;
        (*nd).face    = row.npc_face as u16;
        (*nd).face_color  = row.npc_face_color as u16;
        (*nd).hair    = row.npc_hair as u16;
        (*nd).hair_color  = row.npc_hair_color as u16;
        (*nd).armor_color = 0;
        (*nd).skin_color  = row.npc_skin_color as u16;
        (*nd).npctype = row.npc_is_char as i8;
        (*nd).shop_npc    = row.npc_is_shop as i8;
        (*nd).repair_npc  = row.npc_is_repair as i8;
        (*nd).bank_npc    = row.npc_is_bank as i8;
        (*nd).retdist     = row.npc_return_dist as i8;
        (*nd).movetime    = row.npc_move_time;
        (*nd).receive_item = row.npc_can_receive as i8;

        // ID assignment: if bl.id < NPC_START_NUM, this is a new/fresh NPC
        if (*nd).bl.id < NPC_START_NUM {
            (*nd).bl.m = m as u16;
            (*nd).bl.x = xc as u16;
            (*nd).bl.y = yc as u16;

            if row.npc_is_f1npc == 1 {
                (*nd).bl.id = F1_NPC;
            } else if row.row_npc_id >= 2 {
                (*nd).bl.id = NPC_START_NUM + row.row_npc_id - 2;
                NPC_ID.store(NPC_START_NUM + row.row_npc_id - 2, Ordering::Relaxed);
            } else {
                tracing::error!("[npc] row_npc_id={} < 2, cannot compute NPC ID", row.row_npc_id);
            }
        }

        // New NPCs: transfer Box ownership to NPC_MAP first — this moves data
        // into Arc<RwLock>, freeing the original allocation.
        if is_new_alloc {
            let id = (*nd).bl.id;
            crate::game::map_server::map_addiddb_npc(id, Box::from_raw(nd));
            // nd is dangling after this; get the live pointer from the Arc.
            nd = crate::game::map_server::map_id2npc_ref(id)
                .expect("npc just inserted").data_ptr();
        }

        // Add to block grid only if subtype < 3 (using live pointer)
        if (*nd).bl.subtype < 3 {
            map_addblock(&raw mut (*nd).bl);
        }
    }

    // Equipment loading: loop from NPC_START_NUM to NPC_ID
    // For each NPC with npctype == 1, load equipment from NPCEquipment table
    let mut x = NPC_START_NUM;
    let npc_hi = NPC_ID.load(Ordering::Relaxed);
    while x <= npc_hi {
        let nd: *mut NpcData = crate::game::map_server::map_id2npc_ref(x)
            .map(|arc| &mut *arc.write() as *mut NpcData)
            .unwrap_or(std::ptr::null_mut());
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
pub unsafe fn npc_init() -> i32 {
    blocking_run_async(crate::database::assert_send(async { unsafe { npc_init_async().await } }))
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------

/// Returns 1 if the MOB pointed to by `bl` is in the MOB_DEAD state, 0 otherwise.
/// Called from `npc_move_sub` during NPC movement collision checks.
///
/// # Safety
/// `bl` must point to a valid `MobSpawnData` (i.e. `bl.bl_type == BL_MOB`).
pub unsafe fn npc_helper_mob_is_dead(bl: *mut BlockList) -> i32 {
    if bl.is_null() { return 0; }
    let mob = bl as *const crate::game::mob::MobSpawnData;
    if (*mob).state == crate::game::mob::MOB_DEAD { 1 } else { 0 }
}

/// Returns 1 if the PC pointed to by `bl` should be skipped during NPC movement
/// collision (dead, invisible state, or GM level >= 50), 0 otherwise.
/// `npc_bl` is the NPC's block_list (used to read the map's show_ghosts flag).
///
/// # Safety
/// `bl` must point to a valid `MapSessionData` and `npc_bl` to a valid `BlockList`
/// whose `.m` field is a loaded map ID.
pub unsafe fn npc_helper_pc_is_skip(bl: *mut BlockList, npc_bl: *mut BlockList) -> i32 {
    use crate::game::pc::{MapSessionData, PC_DIE};
    if bl.is_null() || npc_bl.is_null() { return 0; }
    let sd = bl as *const MapSessionData;
    let npc_m = (*npc_bl).m;
    let show_ghosts: u8 = if map_is_loaded(npc_m) {
        (*get_map_ptr(npc_m)).show_ghosts
    } else {
        0
    };
    let state = (*sd).player.combat.state;
    if (show_ghosts != 0 && state == PC_DIE as i8)
        || state == -1
        || (*sd).player.identity.gm_level >= 50
    {
        1
    } else {
        0
    }
}


// ---------------------------------------------------------------------------
// npc_move_sub — callback for map_foreachincell during NPC movement
// ---------------------------------------------------------------------------

/// Callback for `map_foreachincell` during NPC movement — checks for blocking entities.
///
/// Sets `nd.canmove = 1` if the cell is occupied by a non-passable entity.
///
/// - `BL_NPC`: skip if `bl.subtype != 0` (non-zero subtype NPCs don't block).
/// - `BL_MOB`: skip if the mob is in `MOB_DEAD` state.
/// - `BL_PC`:  skip if dead/invisible/GM (delegated to `npc_helper_pc_is_skip`).
/// - Any other type: skip (return 0 without setting `canmove`).
///
/// # Safety
///
/// Must only be called by `map_foreachincell`. `ap` must contain exactly one
/// argument: a `*mut NpcData`.
pub unsafe fn npc_move_sub_inner(bl: *mut BlockList, nd: *mut NpcData) -> i32 {
    if nd.is_null() || bl.is_null() { return 0; }
    if (*nd).canmove == 1 { return 0; }

    let bl_ref = &*bl;
    match bl_ref.bl_type {
        x if x == BL_NPC as u8 => {
            // Non-zero subtype NPCs do not block movement.
            if bl_ref.subtype != 0 { return 0; }
        }
        x if x == BL_MOB as u8 => {
            // Dead mobs do not block movement.
            if npc_helper_mob_is_dead(bl) != 0 { return 0; }
        }
        x if x == BL_PC as u8 => {
            // Dead / invisible / GM players do not block movement.
            if npc_helper_pc_is_skip(bl, &raw mut (*nd).bl as *mut BlockList) != 0 { return 0; }
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
///
/// # Safety
///
/// `nd` must be a valid, aligned, non-null pointer to a live `NpcData`.
/// Caller must hold the server-wide lock.
pub unsafe fn npc_move(nd: *mut NpcData) -> i32 {
    if nd.is_null() { return 0; }
    let nd = &mut *nd;

    let m = nd.bl.m as i32;
    let backx = nd.bl.x as i32;
    let backy = nd.bl.y as i32;
    let mut dx = backx;
    let mut dy = backy;
    let direction = nd.side as i32;
    let mut x0 = backx;
    let mut y0 = backy;
    let mut x1: i32 = 0;
    let mut y1: i32 = 0;
    let mut nothingnew: i32 = 0;

    let md = crate::database::map_db::get_map_ptr(nd.bl.m);
    if md.is_null() { return 0; }
    let map_xs = (*md).xs as i32;
    let map_ys = (*md).ys as i32;

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
    let mut war = crate::database::map_db::map_get_warp(nd.bl.m, dx as u16, dy as u16);
    while !war.is_null() {
        if (*war).x == dx && (*war).y == dy { return 0; }
        war = (*war).next;
    }

    // Check for blockers in destination cell
    nd.canmove = 0;
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let cell_ids = grid.ids_at_tile(dx as u16, dy as u16);
        for id in cell_ids {
            let bl = crate::game::map_server::map_id2bl_ref(id);
            if !bl.is_null() {
                let ty = (*bl).bl_type as i32;
                if ty == BL_MOB || ty == BL_PC || ty == BL_NPC {
                    npc_move_sub_inner(bl, nd as *mut NpcData);
                }
            }
        }
    }

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
        nd.bl.bx = backx as u32;
        nd.bl.by = backy as u32;
        map_moveblock(&raw mut nd.bl, dx, dy);

        if nothingnew == 0 {
            let nm = nd.bl.m as i32;
            if let Some(grid) = block_grid::get_grid(nm as usize) {
                let rect_ids = grid.ids_in_rect(x0, y0, x0 + x1 - 1, y0 + y1 - 1);
                if nd.npctype == 1 {
                    let nd_bl = nd as *const NpcData as *const BlockList;
                    for &id in &rect_ids {
                        if let Some(pc_arc) = crate::game::map_server::map_id2sd_pc(id) {
                            clif_cnpclook_inner(&raw const pc_arc.read().bl, LOOK_SEND, nd_bl);
                        }
                    }
                } else {
                    let nd_bl = &raw mut nd.bl;
                    for &id in &rect_ids {
                        if let Some(arc) = crate::game::map_server::map_id2sd_pc(id) {
                            clif_mob_look_start_func_inner(&raw mut arc.write().bl);
                        }
                    }
                    for &id in &rect_ids {
                        if let Some(arc) = crate::game::map_server::map_id2sd_pc(id) {
                            clif_object_look_sub_inner(&raw mut arc.write().bl, LOOK_SEND, nd_bl);
                        }
                    }
                    for &id in &rect_ids {
                        if let Some(arc) = crate::game::map_server::map_id2sd_pc(id) {
                            clif_mob_look_close_func_inner(&raw mut arc.write().bl);
                        }
                    }
                }
            }
        }

        let nd_ptr = nd as *mut NpcData;
        if let Some(grid) = block_grid::get_grid(m as usize) {
            let slot = &*crate::database::map_db::raw_map_ptr().add(m as usize);
            let ids = block_grid::ids_in_area(grid, nd.bl.x as i32, nd.bl.y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
            for id in ids {
                if let Some(arc) = crate::game::map_server::map_id2sd_pc(id) {
                    clif_npc_move_inner(&raw const arc.read().bl, nd_ptr);
                }
            }
        }
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
        // canmove is the 3rd of 10 i8 fields starting at offset 20372
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
        let key = b"mykey\0".as_ptr() as *const i8;
        unsafe {
            assert_eq!(npc_setglobalreg(&raw mut *nd, key, 42), 0);
            assert_eq!(npc_readglobalreg(&raw mut *nd, key), 42);
        }
    }

    #[test]
    fn globalreg_set_zero_clears_key() {
        let mut nd = unsafe { Box::<NpcData>::new_zeroed().assume_init() };
        let key = b"mykey\0".as_ptr() as *const i8;
        unsafe {
            npc_setglobalreg(&raw mut *nd, key, 99);
            npc_setglobalreg(&raw mut *nd, key, 0);
            // After setting to 0, key should be cleared — re-reading returns 0
            assert_eq!(npc_readglobalreg(&raw mut *nd, key), 0);
        }
    }

    #[test]
    fn npc_idlower_decrements_temp() {
        use std::sync::atomic::Ordering;
        unsafe {
            let orig = NPCTEMP_ID.load(Ordering::Relaxed);
            NPCTEMP_ID.store(NPCT_START_NUM + 5, Ordering::Relaxed);
            npc_idlower((NPCT_START_NUM + 1) as i32);
            assert_eq!(NPCTEMP_ID.load(Ordering::Relaxed), NPCT_START_NUM + 4);
            NPCTEMP_ID.store(orig, Ordering::Relaxed);
        }
    }

    #[test]
    fn npc_idlower_ignores_regular_npc() {
        use std::sync::atomic::Ordering;
        unsafe {
            let orig = NPCTEMP_ID.load(Ordering::Relaxed);
            NPCTEMP_ID.store(NPCT_START_NUM + 5, Ordering::Relaxed);
            // NPC_START_NUM is not a temp NPC — counter should not change
            npc_idlower(NPC_START_NUM as i32);
            assert_eq!(NPCTEMP_ID.load(Ordering::Relaxed), NPCT_START_NUM + 5);
            NPCTEMP_ID.store(orig, Ordering::Relaxed);
        }
    }
}


// npc_init, warp_init, and npc_runtimers are on the original function definitions above.

pub unsafe fn npc_action_ffi(nd: *mut NpcData) -> i32 {
    npc_action(nd)
}

pub unsafe fn npc_movetime_ffi(nd: *mut NpcData) -> i32 {
    npc_movetime(nd)
}

pub unsafe fn npc_duration_ffi(nd: *mut NpcData) -> i32 {
    npc_duration(nd)
}

pub unsafe fn npc_warp_ffi(nd: *mut NpcData, m: i32, x: i32, y: i32) -> i32 {
    npc_warp(nd, m, x, y)
}

pub unsafe fn npc_move_ffi(nd: *mut NpcData) -> i32 {
    npc_move(nd)
}

pub unsafe fn npc_readglobalreg_ffi(nd: *mut NpcData, reg: *const i8) -> i32 {
    npc_readglobalreg(nd, reg)
}

pub unsafe fn npc_setglobalreg_ffi(nd: *mut NpcData, reg: *const i8, val: i32) -> i32 {
    npc_setglobalreg(nd, reg, val)
}

pub unsafe fn npc_idlower_ffi(id: i32) -> i32 {
    npc_idlower(id)
}

pub unsafe fn npc_src_clear_ffi() -> i32 {
    npc_src_clear()
}

pub unsafe fn npc_src_add_ffi(f: *const i8) -> i32 {
    npc_src_add(f)
}

pub unsafe fn npc_warp_add_ffi(f: *const i8) -> i32 {
    npc_warp_add(f)
}

pub unsafe fn npc_get_new_npctempid_ffi() -> u32 {
    npc_get_new_npctempid()
}
