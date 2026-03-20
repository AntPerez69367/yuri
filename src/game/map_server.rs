//! Map server — re-export shim.
//!
//! Functions that lived here have been moved to focused modules.
//! This file re-exports them so existing `use crate::game::map_server::*` sites
//! continue to compile. TODO: update consumers to import directly, then remove.

// World / spatial.
pub use crate::game::block::{hasCoref, map_canmove};
pub use crate::game::entity_store::*;
pub use crate::game::floor_items::*;
pub use crate::game::object_flags::*;

// State / config.
pub use crate::game::cron::*;
pub use crate::game::game_registry::*;
pub use crate::game::game_time::*;
pub use crate::game::lang::*;
pub use crate::game::party::*;

// Network globals.
pub use crate::network::globals::*;

// Board / mail.
pub use crate::game::boards::*;

// Lifecycle.
pub use crate::game::lifecycle::*;

// Database helpers.
pub use crate::database::character::*;
pub use crate::database::mob_db::map_lastdeath_mob;
