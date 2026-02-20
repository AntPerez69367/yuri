//! FFI (Foreign Function Interface) layer
//!
//! C-compatible wrappers for Rust modules.
//! This entire module will be deleted once all C code is ported to Rust.

pub mod config;
pub mod core;
pub mod crypt;
pub mod session;
pub mod timer;