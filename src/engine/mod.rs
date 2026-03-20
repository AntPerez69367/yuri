//! Server engine — config, lifecycle, session management, and game loop.

pub mod config;
pub mod game_loop;
pub mod lifecycle;
pub mod session;
pub mod world;

pub use lifecycle::*;
