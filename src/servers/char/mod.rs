pub mod charstatus;
pub mod db;
pub mod login;
pub mod map;
pub mod packet;

use anyhow::Result;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::Mutex;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::AsyncReadExt;
use sqlx::MySqlPool;
use crate::config::ServerConfig;

/// One connected map server's state.
#[derive(Debug)]
pub struct MapFifo {
    pub tx: tokio::sync::mpsc::Sender<Vec<u8>>,
    pub ip: u32,
    pub port: u16,
    pub maps: Vec<u16>,
}

/// One online character session routed through a map server.
#[derive(Debug)]
pub struct LoginEntry {
    pub map_server_idx: usize,
    pub char_name: String,
}

pub struct CharState {
    pub db: MySqlPool,
    pub config: ServerConfig,
    /// char_id → LoginEntry
    pub online: Mutex<HashMap<u32, LoginEntry>>,
    /// index → MapFifo
    pub map_servers: Mutex<Vec<Option<MapFifo>>>,
    /// sender to login server connection task
    pub login_tx: Mutex<Option<tokio::sync::mpsc::Sender<Vec<u8>>>>,
}

impl CharState {
    pub fn new(db: MySqlPool, config: ServerConfig) -> Self {
        Self {
            db,
            config,
            online: Mutex::new(HashMap::new()),
            map_servers: Mutex::new(Vec::new()),
            login_tx: Mutex::new(None),
        }
    }

    pub async fn run(state: Arc<Self>, bind_addr: &str) -> Result<()> {
        let listener = TcpListener::bind(bind_addr).await?;
        tracing::info!("[char] [ready] addr={}", bind_addr);
        loop {
            let (stream, _peer) = listener.accept().await?;
            let s = Arc::clone(&state);
            tokio::spawn(async move {
                handle_new_connection(s, stream).await;
            });
        }
    }
}

async fn handle_new_connection(state: Arc<CharState>, mut stream: TcpStream) {
    let mut cmd_bytes = [0u8; 2];
    if stream.read_exact(&mut cmd_bytes).await.is_err() {
        return;
    }
    let cmd = u16::from_le_bytes(cmd_bytes);

    if cmd == 0x3000 {
        map::handle_map_server(state, stream, cmd_bytes).await;
    } else {
        tracing::warn!("[char] [unknown_cmd] cmd={:04X}", cmd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_fifo_default_empty() {
        let _ = std::mem::size_of::<MapFifo>();
        let _ = std::mem::size_of::<LoginEntry>();
    }
}
