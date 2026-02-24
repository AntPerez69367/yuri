//! Yuri - MMORPG Server
//!
//! A Rust reimplementation of a legacy C MMORPG server.
//! Migrating incrementally from C to Rust for memory safety and performance.

// ============================================
// Core Modules (Pure Rust)
// ============================================

/// Server configuration (replaces config.c)
pub mod config;
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
// FFI Layer (Temporary - for C interop)
// ============================================

/// C-compatible wrapper functions
/// This entire module will be deleted once C code is fully ported
#[cfg(not(test))]
pub mod ffi;
