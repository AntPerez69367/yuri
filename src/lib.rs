//! Yuri - MMORPG Server

// Shared types.
pub mod common;

// Server engine (config, lifecycle, sessions, game loop).
pub mod engine;
pub use engine::config;
pub use engine::session;
pub use engine::world;

// Network utilities (encryption, DDoS, throttle).
pub mod network;

// Database modules (item_db, class_db, etc.).
pub mod database;

// Server implementations (login, char, map).
pub mod servers;

// Game logic (NPC, mob, player, scripting).
pub mod game;
