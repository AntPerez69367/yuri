use std::sync::{Arc, OnceLock};
use std::time::Instant;
use dashmap::{DashMap, DashSet};
use sqlx::MySqlPool;
use tokio::sync::mpsc;
use crate::config::ServerConfig;
use crate::servers::login::LoginMessages;

/// Request to kick an existing session (login listener → LocalSet).
/// The LocalSet task calls `session_set_eof` for the matching char_id.
pub struct KickRequest {
    pub char_id: u32,
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
    /// Login listener → LocalSet kick channel.
    pub kick_tx: mpsc::Sender<KickRequest>,
}

static WORLD: OnceLock<Arc<WorldState>> = OnceLock::new();

pub fn set_world(w: Arc<WorldState>) {
    let _ = WORLD.set(w);
}

pub fn get_world() -> Option<&'static Arc<WorldState>> {
    WORLD.get()
}
