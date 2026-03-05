//! Rust ports of `c_src/map_server.c` utility functions.
//!
//! Functions are migrated here one at a time as their C dependencies are removed.
//! Each `#[no_mangle]` export directly replaces its C counterpart in `libmap_game.a`.

use std::ffi::c_char;
use std::os::raw::c_int;
use std::time::{SystemTime, UNIX_EPOCH};

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
