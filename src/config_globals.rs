//! C-compatible global variables from `c_src/config.c`.
//!
//! These replace the definitions in `config.c`, which has been removed from the
//! build.  The same symbol names are exported via `#[no_mangle]` so all existing
//! C and Rust callers that reference them via `extern "C" { static … }` or
//! directly link without changes.
//!
//! Populated at startup by `rust_config_populate_c_globals()` in `ffi/config.rs`.

use std::os::raw::{c_char, c_int, c_uint};

/// Mirrors `struct town_data` from `c_src/config.h`.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct TownData {
    pub name: [c_char; 32],
}

const fn zero_town() -> TownData { TownData { name: [0; 32] } }

// ─── Encryption / auth ───────────────────────────────────────────────────────

#[no_mangle] pub static mut xor_key:   [c_char; 10] = [0; 10];

// ─── Start position (struct point) ───────────────────────────────────────────

#[no_mangle] pub static mut start_pos: crate::config::Point = crate::config::Point { m: 0, x: 0, y: 0 };

// ─── Login server ─────────────────────────────────────────────────────────────

#[no_mangle] pub static mut login_id:   [c_char; 33] = [0; 33];
#[no_mangle] pub static mut login_pw:   [c_char; 33] = [0; 33];
#[no_mangle] pub static mut login_ip:   c_int        = 0;
#[no_mangle] pub static mut login_port: c_int        = 2010;

// ─── Char server ──────────────────────────────────────────────────────────────

#[no_mangle] pub static mut char_id:   [c_char; 33] = [0; 33];
#[no_mangle] pub static mut char_pw:   [c_char; 33] = [0; 33];
#[no_mangle] pub static mut char_ip:   c_int        = 0;
#[no_mangle] pub static mut char_port: c_int        = 2005;

// ─── Map server ───────────────────────────────────────────────────────────────

#[no_mangle] pub static mut map_ip:   c_uint = 0;
#[no_mangle] pub static mut map_port: c_uint = 0;

// ─── SQL ──────────────────────────────────────────────────────────────────────

#[no_mangle] pub static mut sql_id:   [c_char; 32] = [0; 32];
#[no_mangle] pub static mut sql_pw:   [c_char; 32] = [0; 32];
#[no_mangle] pub static mut sql_ip:   [c_char; 32] = [0; 32];
#[no_mangle] pub static mut sql_db:   [c_char; 32] = [0; 32];
#[no_mangle] pub static mut sql_port: c_int         = 3306;

// ─── Server settings ─────────────────────────────────────────────────────────

#[no_mangle] pub static mut serverid:    c_int = 0;
#[no_mangle] pub static mut require_reg: c_int = 1;
#[no_mangle] pub static mut nex_version: c_int = 0;
#[no_mangle] pub static mut nex_deep:    c_int = 0;
#[no_mangle] pub static mut save_time:   c_int = 60000;
#[no_mangle] pub static mut xp_rate:     c_int = 0;
#[no_mangle] pub static mut d_rate:      c_int = 0;

// ─── Meta files ──────────────────────────────────────────────────────────────

/// `char meta_file[META_MAX][256]` where META_MAX = 20.
#[no_mangle] pub static mut meta_file: [[c_char; 256]; 20] = [[0; 256]; 20];
#[no_mangle] pub static mut metamax:   c_int               = 0;

// ─── Towns ───────────────────────────────────────────────────────────────────

#[no_mangle] pub static mut towns:  [TownData; 255] = [zero_town(); 255];
#[no_mangle] pub static mut town_n: c_int           = 0;

// ─── Directory pointers ───────────────────────────────────────────────────────
// Initialized to the same default string literals as config.c.
// After rust_config_populate_c_globals() runs these remain the defaults
// (the Rust config system doesn't overwrite the pointer values, only the
// arrays above).

#[no_mangle] pub static mut data_dir: *const c_char = b"./data/\0".as_ptr()           as *const c_char;
#[no_mangle] pub static mut lua_dir:  *const c_char = b"./data/lua/\0".as_ptr()       as *const c_char;
#[no_mangle] pub static mut maps_dir: *const c_char = b"./data/maps/\0".as_ptr()      as *const c_char;
#[no_mangle] pub static mut meta_dir: *const c_char = b"./data/meta/\0".as_ptr()      as *const c_char;
