use std::sync::{Arc, OnceLock};
use std::time::Instant;
use dashmap::{DashMap, DashSet};
use sqlx::MySqlPool;
use tokio::sync::mpsc;
use crate::config::ServerConfig;
use crate::servers::login::LoginMessages;

/// Messages sent to the game thread (LocalSet) for operations that must
/// run where pe locks are uncontested.
pub enum GameThreadMsg {
    /// Kick a duplicate login (login listener → game thread).
    Kick { char_id: u32 },
    /// Run post-save cleanup for a voluntary logout (I/O task → game thread).
    /// Save + set-offline already happened on the I/O task.
    DisconnectCleanup { char_id: u32 },
}

/// Pending auth token — consumed on use (one-time, 30s expiry).
#[derive(Debug, Clone)]
pub struct AuthEntry {
    pub account_id: u32,
    pub char_id: u32,
    pub char_name: String,
    pub client_ip: u32,
    pub expires: Instant,
}

/// Shared state for the unified world server.
///
/// Owned by `main()`, passed as `Arc<WorldState>` to login listener,
/// map game loop, and background tasks.
pub struct WorldState {
    pub db: MySqlPool,
    pub config: ServerConfig,
    pub messages: LoginMessages,
    /// char_ids currently in-game.
    pub online: DashSet<u32>,
    /// Pending auth tokens keyed by normalized (lowercased) char_name.
    pub auth_db: DashMap<String, AuthEntry>,
    /// Login listener / I/O tasks → game thread channel.
    pub game_tx: mpsc::Sender<GameThreadMsg>,
}

static WORLD: OnceLock<Arc<WorldState>> = OnceLock::new();

pub fn set_world(w: Arc<WorldState>) {
    let _ = WORLD.set(w);
}

pub fn get_world() -> Option<&'static Arc<WorldState>> {
    WORLD.get()
}
