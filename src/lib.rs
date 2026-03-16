//! Yuri - MMORPG Server

// ============================================
// Shared types
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
/// Shared world state for single-binary mode
pub mod world;

// ============================================
// Game Logic Modules (Phase 3)
// ============================================

/// Game logic: NPC, mob, and player data types (replaces map_server C game layer).
pub mod game;
