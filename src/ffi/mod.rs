//! FFI (Foreign Function Interface) layer
//!
//! C-compatible wrappers for Rust modules.
//! This entire module will be deleted once all C code is ported to Rust.

pub mod board_db;
pub mod class_db;
pub mod clan_db;
pub mod config;
pub mod core;
pub mod crypt;
pub mod database;
pub mod item_db;
pub mod magic_db;
pub mod recipe_db;
pub mod session;
pub mod timer;