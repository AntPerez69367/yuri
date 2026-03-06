//! Rust ports of `c_src/map_server.c` utility functions.
//!
//! Functions are migrated here one at a time as their C dependencies are removed.
//! Each `#[no_mangle]` export directly replaces its C counterpart in `libmap_game.a`.

use std::collections::HashMap;
use std::ffi::c_char;
use std::os::raw::{c_int, c_uint, c_void};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::database::{blocking_run, blocking_run_async, get_pool};
use crate::game::pc::MapSessionData;

use crate::database::map_db::BlockList;

extern "C" {
    fn rust_session_exists(fd: c_int) -> c_int;
    fn rust_session_get_eof(fd: c_int) -> c_int;
    fn rust_session_get_client_ip(fd: c_int) -> c_uint;
}

// ---------------------------------------------------------------------------
// ID database — replaces C uidb_* hash table in map_server.c
// ---------------------------------------------------------------------------

static mut ID_DB: Option<HashMap<u32, *mut c_void>> = None;

unsafe fn id_db() -> &'static mut HashMap<u32, *mut c_void> {
    ID_DB.get_or_insert_with(HashMap::new)
}

#[no_mangle]
pub unsafe extern "C" fn map_initiddb() {
    id_db(); // initialise lazily
}

#[no_mangle]
pub unsafe extern "C" fn map_termiddb() {
    id_db().clear();
}

/// Returns a raw pointer to any game object (USER*, MOB*, NPC*, FLOORITEM*) by ID.
/// Returns null if not found. Callers cast the result to the appropriate type.
#[no_mangle]
pub unsafe extern "C" fn map_id2bl(id: c_uint) -> *mut c_void {
    id_db().get(&id).copied().unwrap_or(std::ptr::null_mut())
}

/// Returns the USER* for a player by character ID. NULL if not found or not a player.
#[no_mangle]
pub unsafe extern "C" fn map_id2sd(id: c_uint) -> *mut c_void {
    map_id2bl(id) // C caller casts to USER*; same raw pointer
}

#[no_mangle]
pub unsafe extern "C" fn map_addiddb(bl: *mut BlockList) {
    if bl.is_null() { return; }
    id_db().insert((*bl).id, bl as *mut c_void);
}

#[no_mangle]
pub unsafe extern "C" fn map_deliddb(bl: *mut BlockList) {
    if bl.is_null() { return; }
    id_db().remove(&(*bl).id);
}

/// Timer callback — runs Lua cron hooks based on wall-clock seconds.
/// Replaces `map_cronjob` in `c_src/map_server.c`.
///
/// Registered every 1000 ms via `timer_insert` in `map_server.rs`.
/// Must be called on the Lua-owning thread (LocalSet).
#[no_mangle]
pub unsafe extern "C" fn rust_map_cronjob(_id: c_int, _n: c_int) -> c_int {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    if t % 60    == 0 { cron(b"cronJobMin\0");    }
    if t % 300   == 0 { cron(b"cronJob5Min\0");   }
    if t % 1800  == 0 { cron(b"cronJob30Min\0");  }
    if t % 3600  == 0 { cron(b"cronJobHour\0");   }
    if t % 86400 == 0 { cron(b"cronJobDay\0");    }
    cron(b"cronJobSec\0");
    0
}

#[inline]
unsafe fn cron(name: &[u8]) {
    crate::game::scripting::sl_doscript_blargs_vec(
        name.as_ptr() as *const c_char,
        std::ptr::null(),
        0,
        std::ptr::null(),
    );
}

// ---------------------------------------------------------------------------
// Session state helpers
// ---------------------------------------------------------------------------

/// Returns 1 if `sd` is non-null and has an active session.
/// Mirrors `isPlayerActive` in `c_src/map_server.c`.
#[no_mangle]
pub unsafe extern "C" fn isPlayerActive(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() { return 0; }
    let fd = (*sd).fd;
    if fd == 0 { return 0; }
    if rust_session_exists(fd) == 0 {
        let name = std::ffi::CStr::from_ptr((*sd).status.name.as_ptr());
        eprintln!("[map] isPlayerActive: player exists but session does not ({})", name.to_string_lossy());
        return 0;
    }
    1
}

/// Returns 1 if `sd` has a live session with no EOF flag set.
/// Mirrors `isActive` in `c_src/map_server.c`.
#[no_mangle]
pub unsafe extern "C" fn isActive(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() { return 0; }
    let fd = (*sd).fd;
    if rust_session_exists(fd) == 0 { return 0; }
    if rust_session_get_eof(fd) != 0 { return 0; }
    1
}

// ---------------------------------------------------------------------------
// Online status
// ---------------------------------------------------------------------------

/// Updates `Character.ChaOnline`/`ChaLastIP` and fires the "login" Lua hook on first login.
/// Mirrors `mmo_setonline` in `c_src/map_server.c`.
#[no_mangle]
pub unsafe extern "C" fn mmo_setonline(id: c_uint, val: c_int) {
    let sd = map_id2sd(id) as *mut MapSessionData;
    if sd.is_null() { return; }

    let fd = (*sd).fd;
    // rust_session_get_client_ip returns IP in network byte order (sin_addr.s_addr).
    // The C code decomposes it as: a = ip & 0xff, b = (ip>>8)&0xff, c = (ip>>16)&0xff, d = (ip>>24)&0xff.
    let raw_ip = rust_session_get_client_ip(fd);
    let addr = format!(
        "{}.{}.{}.{}",
        raw_ip & 0xff,
        (raw_ip >> 8) & 0xff,
        (raw_ip >> 16) & 0xff,
        (raw_ip >> 24) & 0xff,
    );

    // Check character exists, then fire login script.
    let char_id = id;
    let exists: bool = blocking_run_async(async move {
        let pool = get_pool();
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM `Character` WHERE `ChaId` = ?"
        )
        .bind(char_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0) > 0
    });

    if exists && val != 0 {
        // status.name is [i8; 16] — convert to CStr for display.
        let name_ptr = (*sd).status.name.as_ptr() as *const std::ffi::c_char;
        println!("[map] [login] name={} addr={}",
            std::ffi::CStr::from_ptr(name_ptr).to_string_lossy(), addr);

        // Fire "login" Lua hook: sl_doscript_blargs("login", NULL, 1, &sd->bl)
        let bl_ptr = std::ptr::addr_of_mut!((*sd).bl) as *mut c_void;
        crate::game::scripting::sl_doscript_blargs_vec(
            b"login\0".as_ptr() as *const std::ffi::c_char,
            std::ptr::null(),
            1,
            &bl_ptr as *const *mut c_void,
        );
    }

    // Update online status + last IP regardless of whether character was found in SELECT.
    blocking_run_async(async move {
        let pool = get_pool();
        let _ = sqlx::query(
            "UPDATE `Character` SET `ChaOnline` = ?, `ChaLastIP` = ? WHERE `ChaId` = ?"
        )
        .bind(val)
        .bind(&addr)
        .bind(char_id)
        .execute(pool)
        .await;
    });
}
