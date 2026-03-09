//! Server configuration globals populated once at startup from the parsed config.

use std::sync::OnceLock;
use std::sync::atomic::AtomicI32;

/// Town name entry.
#[derive(Copy, Clone)]
pub struct TownData {
    pub name: [i8; 32],
}

/// All server configuration globals, populated once at startup.
/// Fields use the same types as the original `static mut` variables so call sites
/// require no type changes.
pub struct GlobalConfig {
    pub xor_key:     [i8; 10],
    pub start_pos:   crate::config::Point,
    pub login_id:    [i8; 33],
    pub login_pw:    [i8; 33],
    pub login_ip:    i32,
    pub login_port:  i32,
    pub char_id:     [i8; 33],
    pub char_pw:     [i8; 33],
    pub char_ip:     i32,
    pub char_port:   i32,
    pub map_ip:      u32,
    pub map_port:    u32,
    pub sql_id:      [i8; 32],
    pub sql_pw:      [i8; 32],
    pub sql_ip:      [i8; 32],
    pub sql_db:      [i8; 32],
    pub sql_port:    i32,
    pub serverid:    i32,
    pub require_reg: i32,
    pub nex_version: i32,
    pub nex_deep:    i32,
    pub save_time:   i32,
    pub meta_file:   [[i8; 256]; 20],
    pub metamax:     i32,
    pub towns:       [TownData; 255],
    pub town_n:      i32,
    /// Default: "./data/"
    pub data_dir:    String,
    /// Default: "./data/lua/"
    pub lua_dir:     String,
    /// Default: "./data/maps/"
    pub maps_dir:    String,
    /// Default: "./data/meta/"
    pub meta_dir:    String,
}

// SAFETY: All fields are Send+Sync. [i8; N] arrays and i32/u32 are Send.
// String is Send. TownData contains only [i8; 32].
unsafe impl Send for GlobalConfig {}
unsafe impl Sync for GlobalConfig {}

static GLOBAL_CONFIG: OnceLock<GlobalConfig> = OnceLock::new();

/// Returns the global config. Panics if called before `rust_config_populate_c_globals()`.
pub fn global_config() -> &'static GlobalConfig {
    GLOBAL_CONFIG.get().expect("[config] global_config() called before rust_config_populate_c_globals()")
}

/// Set the global config. Called once from `rust_config_populate_c_globals()`.
/// Silently ignores duplicate calls (startup race guard).
pub fn set_global_config(cfg: GlobalConfig) {
    let _ = GLOBAL_CONFIG.set(cfg);
}

// ─── Runtime-mutable rates (written by GM commands at runtime) ────────────────
// These cannot live in GlobalConfig (which is write-once) because GM commands
// can change them at runtime. Use AtomicI32 so they are safe without unsafe.

/// Experience rate multiplier — written at startup and by GM /xprate command.
pub static XP_RATE: AtomicI32 = AtomicI32::new(0);

/// Drop rate multiplier — written at startup and by GM /drate command.
pub static D_RATE: AtomicI32 = AtomicI32::new(0);
