pub mod char;
pub mod packet;

use std::sync::Arc;
use tokio::sync::Mutex;
use sqlx::MySqlPool;
use crate::config::ServerConfig;

pub struct MapState {
    pub db: MySqlPool,
    pub config: ServerConfig,
    /// Raw TCP write channel to char_server. None = not connected.
    pub char_tx: Mutex<Option<tokio::sync::mpsc::Sender<Vec<u8>>>>,
    /// Pending auth tokens: char_name → session fd on map server
    pub auth_db: Mutex<std::collections::HashMap<String, AuthEntry>>,
}

#[derive(Debug, Clone)]
pub struct AuthEntry {
    pub char_name: String,
    pub account_id: u32,
    pub client_ip: u32,
    pub expires: std::time::Instant,
}

impl MapState {
    pub fn new(db: MySqlPool, config: ServerConfig) -> Self {
        Self {
            db,
            config,
            char_tx: Mutex::new(None),
            auth_db: Mutex::new(std::collections::HashMap::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_map_state_compiles() {
        let _ = 1 + 1;
    }
}
