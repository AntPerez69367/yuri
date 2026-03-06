//! Rust port of `c_src/map_char.c`.
//!
//! Contains `intif_mmo_tosd` — the login landing function that installs a
//! freshly-received `MmoCharStatus` into a live session and fires the full
//! player-login sequence.

#![allow(non_snake_case, dead_code, unused_variables)]

use std::alloc::{alloc_zeroed, Layout};
use std::ffi::{c_char, c_int, c_ulong, c_void};
use std::ptr;

use crate::database::{blocking_run_async, get_pool};
use crate::database::map_db::BlockList;
use crate::game::pc::MapSessionData;
use crate::servers::char::charstatus::MmoCharStatus;

// ---------------------------------------------------------------------------
// Constants mirrored from map_server.h / map_parse.h
// ---------------------------------------------------------------------------

const SFLAG_FULLSTATS: c_int = 0x40;
const SFLAG_HPMP: c_int      = 0x20;
const SFLAG_XPMONEY: c_int   = 0x10;

const BL_ALL: c_int = 0x0F;
const BL_PC:  c_int = 0x01;

// enum { LOOK_GET, LOOK_SEND } from map_parse.h
const LOOK_GET: c_int = 0;

// enum { ..., SAMEAREA = 6, AREA = 4, ... } from map_parse.h
const SAMEAREA: c_int = 6;
const AREA: c_int     = 4;

// optFlag_walkthrough = 128 (from map_server.h)
const OPT_WALKTHROUGH: c_ulong = 128;

// ---------------------------------------------------------------------------
// C FFI declarations
// ---------------------------------------------------------------------------

extern "C" {
    fn rust_session_set_eof(fd: c_int, val: c_int);
    fn rust_session_set_data(fd: c_int, data: *mut c_void);

    // net_crypt.h: `static inline char *populate_table(const char*, char*, int)`
    // forwards to this symbol.
    fn rust_crypt_populate_table(name: *const c_char, table: *mut i8, len: c_int) -> *mut i8;

    // Inter-server file descriptor (declared extern in map_server.h).
    static map_fd: c_int;

    // map_isloaded is a C macro: `#define map_isloaded(m) (map[m].registry)`.
    // sl_compat.c exposes a thin function wrapper:
    //   int sl_map_isloaded(int m) { return map_isloaded(m); }
    fn sl_map_isloaded(m: c_int) -> c_int;

    // PC and clif game functions — all remain in C until map_parse.c is ported.
    fn rust_pc_setpos(sd: *mut MapSessionData, m: c_int, x: c_int, y: c_int) -> c_int;
    fn rust_pc_loadmagic(sd: *mut MapSessionData) -> c_int;
    fn rust_pc_starttimer(sd: *mut MapSessionData) -> c_int;
    fn rust_pc_requestmp(sd: *mut MapSessionData) -> c_int;

    fn clif_sendack(sd: *mut MapSessionData) -> c_int;
    fn clif_sendtime(sd: *mut MapSessionData) -> c_int;
    fn clif_sendid(sd: *mut MapSessionData) -> c_int;
    fn clif_sendmapinfo(sd: *mut MapSessionData) -> c_int;
    fn clif_sendstatus(sd: *mut MapSessionData, flags: c_int) -> c_int;
    fn clif_mystaytus(sd: *mut MapSessionData) -> c_int;
    fn clif_spawn(sd: *mut MapSessionData) -> c_int;
    fn clif_refresh(sd: *mut MapSessionData) -> c_int;
    fn clif_sendxy(sd: *mut MapSessionData) -> c_int;
    fn clif_getchararea(sd: *mut MapSessionData) -> c_int;

    fn clif_mob_look_start(sd: *mut MapSessionData) -> c_int;
    fn clif_mob_look_close(sd: *mut MapSessionData) -> c_int;

    // map_foreachinarea(fn*, m, x, y, range, bl_type, ...)
    fn map_foreachinarea(
        f: unsafe extern "C" fn(*mut BlockList, ...) -> c_int,
        m: c_int, x: c_int, y: c_int, range: c_int, bl_type: c_int,
        ...
    ) -> c_int;

    // Callbacks passed to map_foreachinarea — remain in C (map_parse.c).
    fn clif_object_look_sub(bl: *mut BlockList, ...) -> c_int;
    fn clif_updatestate(bl: *mut BlockList, ...) -> c_int;

    fn rust_pc_loaditem(sd: *mut MapSessionData) -> c_int;
    fn rust_pc_loadequip(sd: *mut MapSessionData) -> c_int;
    fn rust_pc_magic_startup(sd: *mut MapSessionData) -> c_int;

    fn rust_pc_calcstat(sd: *mut MapSessionData) -> c_int;
    fn rust_pc_checklevel(sd: *mut MapSessionData) -> c_int;

    fn clif_retrieveprofile(sd: *mut MapSessionData) -> c_int;
}

// ---------------------------------------------------------------------------
// intif_mmo_tosd
// ---------------------------------------------------------------------------

/// Installs an `MmoCharStatus` received from the char-server into a live
/// map-server session and fires the full player-login sequence.
///
/// Replaces `intif_mmo_tosd` in `c_src/map_char.c`.
#[no_mangle]
pub unsafe extern "C" fn intif_mmo_tosd(fd: c_int, p: *const MmoCharStatus) -> c_int {
    // Ignore packets arriving on the inter-server socket itself.
    if fd == map_fd {
        return 0;
    }

    // Null status pointer means the char-server signaled an error.
    if p.is_null() {
        rust_session_set_eof(fd, 7);
        return 0;
    }

    // Allocate a zero-initialised MapSessionData directly on the heap (mirrors C
    // CALLOC). Using alloc_zeroed instead of Box::new(zeroed()) avoids creating
    // the ~3 MB zeroed struct as a stack temporary, which would overflow the stack.
    let sd: *mut MapSessionData = alloc_zeroed(Layout::new::<MapSessionData>()) as *mut MapSessionData;
    if sd.is_null() {
        rust_session_set_eof(fd, 7);
        return 0;
    }

    // Copy MmoCharStatus into sd->status.
    ptr::copy_nonoverlapping(p, ptr::addr_of_mut!((*sd).status), 1);

    // Attach to session.
    (*sd).fd = fd;
    rust_session_set_data(fd, sd as *mut c_void);

    // Build the per-session encryption hash table from the character name.
    // C: populate_table(sd->status.name, sd->EncHash, sizeof(sd->EncHash))
    rust_crypt_populate_table(
        (*sd).status.name.as_ptr() as *const c_char,
        (*sd).EncHash.as_mut_ptr(),
        0x401, // sizeof(sd->EncHash)
    );

    // Set up the block-list header.
    (*sd).bl.id   = (*sd).status.id;
    (*sd).bl.prev = ptr::null_mut();
    (*sd).bl.next = ptr::null_mut();

    // Visual / display defaults.
    (*sd).disguise       = (*sd).status.disguise;
    (*sd).disguise_color = (*sd).status.disguise_color;
    (*sd).viewx = 8;
    (*sd).viewy = 7;

    // Copy IP address (null-terminated C string, max 255 bytes).
    let src_ptr = (*sd).status.ipaddress.as_ptr();
    let src_bytes = std::slice::from_raw_parts(src_ptr as *const u8, 255);
    let null_pos = src_bytes.iter().position(|&b| b == 0).unwrap_or(254);
    ptr::copy_nonoverlapping(
        src_ptr,
        (*sd).ipaddress.as_mut_ptr(),
        null_pos + 1, // copy including null terminator
    );

    // Query the Character table for the stored map position.
    // ChaMapId is `int(10) unsigned` → u32; ChaX/ChaY likewise.
    let char_id = (*sd).status.id;
    let pos_opt: Option<(u32, u32, u32)> = blocking_run_async(async move {
        let pool = get_pool();
        sqlx::query_as::<_, (u32, u32, u32)>(
            "SELECT `ChaMapId`, `ChaX`, `ChaY` FROM `Character` WHERE `ChaId` = ?",
        )
        .bind(char_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None)
    });

    // Apply the loaded position into last_pos.
    if let Some((map_id, cx, cy)) = pos_opt {
        (*sd).status.last_pos.m = map_id as u16;
        (*sd).status.last_pos.x = cx as u16;
        (*sd).status.last_pos.y = cy as u16;
    }

    // GM players walk through blocked tiles.
    if (*sd).status.gm_level != 0 {
        (*sd).optFlags |= OPT_WALKTHROUGH;
    }

    // Fall back to map 0 / spawn point if the target map is not loaded.
    if sl_map_isloaded((*sd).status.last_pos.m as c_int) == 0 {
        (*sd).status.last_pos.m = 0;
        (*sd).status.last_pos.x = 8;
        (*sd).status.last_pos.y = 7;
    }

    // Place the player on the map (adds to block grid).
    rust_pc_setpos(
        sd,
        (*sd).status.last_pos.m as c_int,
        (*sd).status.last_pos.x as c_int,
        (*sd).status.last_pos.y as c_int,
    );

    // Load magic timers and start session timers.
    rust_pc_loadmagic(sd);
    rust_pc_starttimer(sd);
    rust_pc_requestmp(sd);

    // Send initial login packets to the client.
    clif_sendack(sd);
    clif_sendtime(sd);
    clif_sendid(sd);
    clif_sendmapinfo(sd);
    clif_sendstatus(sd, SFLAG_FULLSTATS | SFLAG_HPMP | SFLAG_XPMONEY);
    clif_mystaytus(sd);
    clif_spawn(sd);
    clif_refresh(sd);
    clif_sendxy(sd);
    clif_getchararea(sd);

    // Broadcast visible entities to the new player.
    clif_mob_look_start(sd);
    map_foreachinarea(
        clif_object_look_sub,
        (*sd).bl.m as c_int,
        (*sd).bl.x as c_int,
        (*sd).bl.y as c_int,
        SAMEAREA,
        BL_ALL,
        LOOK_GET,
        sd,
    );
    clif_mob_look_close(sd);

    // Load inventory and equipment.
    rust_pc_loaditem(sd);
    rust_pc_loadequip(sd);

    // Initialise magic system state for this session.
    rust_pc_magic_startup(sd);

    // Register the player in the global ID database and mark online.
    crate::game::map_server::map_addiddb(ptr::addr_of_mut!((*sd).bl));
    crate::game::map_server::mmo_setonline((*sd).status.id, 1);

    // Final stat calculation and state broadcast.
    rust_pc_calcstat(sd);
    rust_pc_checklevel(sd);
    clif_mystaytus(sd);

    // Send our state to all PCs in the area.
    map_foreachinarea(
        clif_updatestate,
        (*sd).bl.m as c_int,
        (*sd).bl.x as c_int,
        (*sd).bl.y as c_int,
        AREA,
        BL_PC,
        sd,
    );

    clif_retrieveprofile(sd);
    0
}
