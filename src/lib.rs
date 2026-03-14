//! Yuri - MMORPG Server
//!
//! A Rust reimplementation of a legacy C MMORPG server.
//! Migrating incrementally from C to Rust for memory safety and performance.

/// Catch any Rust panic at the FFI boundary and return `$default` instead.
/// Panics must not unwind across FFI boundaries — doing so is undefined behavior.
macro_rules! ffi_catch {
    ($default:expr, $body:expr) => {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body)) {
            Ok(v) => v,
            Err(_) => $default,
        }
    };
}

// ============================================
// Shared types (repr(C) structs used across modules)
// ============================================
pub mod common;

// ============================================
// Core Modules (Pure Rust)
// ============================================

/// Server configuration (replaces config.c)
pub mod config;
/// Runtime-mutable rate globals (XP_RATE, D_RATE)
pub mod config_globals;
/// Core utilities and server lifecycle (replaces core.c)
pub mod core;
/// Network utilities (encryption, session management)
pub mod network;
/// Database modules (item_db, class_db, etc.)
pub mod database;
/// Server implementations (login, char, map)
pub mod servers;
/// Session management (replaces session.c)
pub mod session;

// ============================================
// Game Logic Modules (Phase 3)
// ============================================

/// Game logic: NPC, mob, and player data types (replaces map_server C game layer).
pub mod game;
