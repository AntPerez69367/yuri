//! Server configuration globals populated at startup from the parsed config.


/// Town name entry.
#[derive(Copy, Clone)]
pub struct TownData {
    pub name: [i8; 32],
}

const fn zero_town() -> TownData { TownData { name: [0; 32] } }

// ─── Encryption / auth ───────────────────────────────────────────────────────

// SAFETY: Written once by rust_config_read at startup, then read-only.
// Single-threaded game loop — no concurrent access.
pub static mut xor_key:   [i8; 10] = [0; 10];

// ─── Start position (struct point) ───────────────────────────────────────────

// SAFETY: Written once at startup, read-only thereafter.
// Single-threaded game loop — no concurrent access.
pub static mut start_pos: crate::config::Point = crate::config::Point { m: 0, x: 0, y: 0 };

// ─── Login server ─────────────────────────────────────────────────────────────

// SAFETY: Written once by rust_config_read at startup, read-only thereafter.
// Single-threaded game loop — no concurrent access.
pub static mut login_id:   [i8; 33] = [0; 33];
// SAFETY: Written once by rust_config_read at startup, read-only thereafter.
// Single-threaded game loop — no concurrent access.
pub static mut login_pw:   [i8; 33] = [0; 33];
// SAFETY: Written once at startup, read-only thereafter.
pub static mut login_ip:   i32        = 0;
// SAFETY: Written once at startup, read-only thereafter.
pub static mut login_port: i32        = 2010;

// ─── Char server ──────────────────────────────────────────────────────────────

// SAFETY: Written once by rust_config_read at startup, read-only thereafter.
// Single-threaded game loop — no concurrent access.
pub static mut char_id:   [i8; 33] = [0; 33];
// SAFETY: Written once by rust_config_read at startup, read-only thereafter.
// Single-threaded game loop — no concurrent access.
pub static mut char_pw:   [i8; 33] = [0; 33];
// SAFETY: Written once at startup, read-only thereafter.
pub static mut char_ip:   i32        = 0;
// SAFETY: Written once at startup, read-only thereafter.
pub static mut char_port: i32        = 2005;

// ─── Map server ───────────────────────────────────────────────────────────────

// SAFETY: Written once at startup, read-only thereafter.
pub static mut map_ip:   u32 = 0;
// SAFETY: Written once at startup, read-only thereafter.
pub static mut map_port: u32 = 0;

// ─── SQL ──────────────────────────────────────────────────────────────────────

// SAFETY: Written once by rust_config_read at startup, read-only thereafter.
// Single-threaded game loop — no concurrent access.
pub static mut sql_id:   [i8; 32] = [0; 32];
// SAFETY: Written once by rust_config_read at startup, read-only thereafter.
// Single-threaded game loop — no concurrent access.
pub static mut sql_pw:   [i8; 32] = [0; 32];
// SAFETY: Written once by rust_config_read at startup, read-only thereafter.
// Single-threaded game loop — no concurrent access.
pub static mut sql_ip:   [i8; 32] = [0; 32];
// SAFETY: Written once by rust_config_read at startup, read-only thereafter.
// Single-threaded game loop — no concurrent access.
pub static mut sql_db:   [i8; 32] = [0; 32];
// SAFETY: Written once at startup, read-only thereafter.
pub static mut sql_port: i32         = 3306;

// ─── Server settings ─────────────────────────────────────────────────────────

// SAFETY: Written once at startup, read-only thereafter.
pub static mut serverid:    i32 = 0;
// SAFETY: Written once at startup, read-only thereafter.
pub static mut require_reg: i32 = 1;
// SAFETY: Written once at startup, read-only thereafter.
pub static mut nex_version: i32 = 0;
// SAFETY: Written once at startup, read-only thereafter.
pub static mut nex_deep:    i32 = 0;
// SAFETY: Written once at startup, read-only thereafter.
pub static mut save_time:   i32 = 60000;
// SAFETY: Written once at startup, read-only thereafter.
pub static mut xp_rate:     i32 = 0;
// SAFETY: Written once at startup, read-only thereafter.
pub static mut d_rate:      i32 = 0;

// ─── Meta files ──────────────────────────────────────────────────────────────

/// `char meta_file[META_MAX][256]` where META_MAX = 20.
// SAFETY: Written once at startup, read-only thereafter.
// Single-threaded game loop — no concurrent access.
pub static mut meta_file: [[i8; 256]; 20] = [[0; 256]; 20];
// SAFETY: Written once at startup, read-only thereafter.
pub static mut metamax:   i32               = 0;

// ─── Towns ───────────────────────────────────────────────────────────────────

// SAFETY: Written once at startup, read-only thereafter.
// Single-threaded game loop — no concurrent access.
pub static mut towns:  [TownData; 255] = [zero_town(); 255];
// SAFETY: Written once at startup, read-only thereafter.
pub static mut town_n: i32           = 0;

// ─── Directory pointers ───────────────────────────────────────────────────────
// Default paths; can be overridden by the config system at startup.

// SAFETY: Raw pointer to a static string literal, never mutated after init.
// Single-threaded game loop — no concurrent access.
pub static mut data_dir: *const i8 = b"./data/\0".as_ptr()           as *const i8;
// SAFETY: Raw pointer to a static string literal, never mutated after init.
// Single-threaded game loop — no concurrent access.
pub static mut lua_dir:  *const i8 = b"./data/lua/\0".as_ptr()       as *const i8;
// SAFETY: Raw pointer to a static string literal, never mutated after init.
// Single-threaded game loop — no concurrent access.
pub static mut maps_dir: *const i8 = b"./data/maps/\0".as_ptr()      as *const i8;
// SAFETY: Raw pointer to a static string literal, never mutated after init.
// Single-threaded game loop — no concurrent access.
pub static mut meta_dir: *const i8 = b"./data/meta/\0".as_ptr()      as *const i8;
