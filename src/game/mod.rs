// Spatial / world management.
pub mod world;
// Re-export for backwards compatibility — TODO: update consumers to use game::world::{module}.
pub use world::block;
pub use world::block_grid;
pub use world::entity_store;
pub use world::floor_items;
pub use world::object_flags;

// Global state / config.
pub mod state;
// Re-export for backwards compatibility — TODO: update consumers to use game::state::{module}.
pub use state::cron;
pub use state::game_registry;
pub use state::game_time;
pub use state::lang;
pub use state::party;

// Server lifecycle (shutdown, reload, timer).
pub mod lifecycle;

// Board / mail system.
pub mod boards;

// Map server layer.
pub mod map_server;
pub mod map_char;
pub mod map_parse;

// Entities.
pub mod mob;
pub mod npc;
pub mod player;
pub use self::player as pc;

// Client / networking.
pub mod client;

// Scripting.
pub mod scripting;
pub mod lua;

// GM commands.
pub mod gm_command;

// Utilities.
pub mod time_util;
pub mod util;
pub mod types;
