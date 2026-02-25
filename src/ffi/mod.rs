//! FFI (Foreign Function Interface) layer
//!
//! C-compatible wrappers for Rust modules.
//! This entire module will be deleted once all C code is ported to Rust.

/// Catch any Rust panic at the FFI boundary and return `$default` instead.
/// Panics must not unwind across `extern "C"` — doing so is undefined behavior.
macro_rules! ffi_catch {
    ($default:expr, $body:expr) => {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body)) {
            Ok(v) => v,
            Err(_) => $default,
        }
    };
}
pub mod block;
pub mod board_db;
pub mod map_char;
pub mod class_db;
pub mod clan_db;
pub mod config;
pub mod core;
pub mod crypt;
pub mod database;
pub mod item_db;
pub mod magic_db;
pub mod map_db;
pub mod mob_db;
pub mod recipe_db;
pub mod session;
pub mod timer;