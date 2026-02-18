//! FFI imports for C timer system
//!
//! The C timer system (c_deps/timer.c) provides a simple heap-based timer.
//! We call it from the Rust event loop every 10ms to fire expired callbacks.

use std::os::raw::c_int;

extern "C" {
    /// Get current tick count in milliseconds (monotonic clock)
    pub fn gettick_nocache() -> u32;

    /// Get current tick count (may be cached)
    pub fn gettick() -> u32;

    /// Execute all expired timers. Returns ms until next timer fires.
    pub fn timer_do(tick: u32) -> c_int;

    /// Initialize timer subsystem (currently a no-op in C)
    pub fn timer_init();

    /// Free all timer memory
    pub fn timer_clear() -> c_int;
}
