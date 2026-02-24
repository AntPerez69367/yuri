pub mod client;
pub mod db;
pub mod interserver;
pub mod meta;
pub mod packet;

use anyhow::Result;
use std::os::unix::io::AsRawFd;
use std::sync::Arc;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tokio::net::{TcpListener, TcpStream};
use sqlx::MySqlPool;
use crate::config::ServerConfig;
use crate::servers::login::packet::read_client_packet;

/// The 11 localised error messages, indexed by LGN_* constants.
#[derive(Debug, Clone, Default)]
pub struct LoginMessages(pub [String; 11]);

// Message key indices — mirror C enum in login_server.h
pub const LGN_ERRSERVER: usize = 0;
pub const LGN_WRONGPASS: usize = 1;
pub const LGN_WRONGUSER: usize = 2;
pub const LGN_ERRDB:     usize = 3;
pub const LGN_USEREXIST: usize = 4;
pub const LGN_ERRPASS:   usize = 5;
pub const LGN_ERRUSER:   usize = 6;
pub const LGN_NEWCHAR:   usize = 7;
pub const LGN_CHGPASS:   usize = 8;
pub const LGN_DBLLOGIN:  usize = 9;
pub const LGN_BANNED:    usize = 10;

/// Parses a `key: value` lang file (same format as C `lang_read`).
/// Lines starting with `//` are comments. Unknown keys are silently ignored.
pub fn parse_lang_file(content: &str) -> Result<LoginMessages> {
    let mut msgs = LoginMessages::default();
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("//") || line.is_empty() {
            continue;
        }
        if let Some((key, val)) = line.split_once(':') {
            let val = val.trim().to_string();
            match key.trim().to_ascii_uppercase().as_str() {
                "LGN_ERRSERVER" => msgs.0[LGN_ERRSERVER] = val,
                "LGN_WRONGPASS" => msgs.0[LGN_WRONGPASS] = val,
                "LGN_WRONGUSER" => msgs.0[LGN_WRONGUSER] = val,
                "LGN_ERRDB"     => msgs.0[LGN_ERRDB]     = val,
                "LGN_USEREXIST" => msgs.0[LGN_USEREXIST] = val,
                "LGN_ERRPASS"   => msgs.0[LGN_ERRPASS]   = val,
                "LGN_ERRUSER"   => msgs.0[LGN_ERRUSER]   = val,
                "LGN_NEWCHAR"   => msgs.0[LGN_NEWCHAR]   = val,
                "LGN_CHGPASS"   => msgs.0[LGN_CHGPASS]   = val,
                "LGN_DBLLOGIN"  => msgs.0[LGN_DBLLOGIN]  = val,
                "LGN_BANNED"    => msgs.0[LGN_BANNED]     = val,
                _ => {}
            }
        }
    }
    Ok(msgs)
}

/// Char server response routed back to a waiting client task.
pub struct CharResponse {
    pub session_id: u16,
    pub data: Vec<u8>,
}

pub struct LoginState {
    pub db: Option<MySqlPool>,
    pub config: ServerConfig,
    pub messages: LoginMessages,
    pub lockout: Mutex<HashMap<u32, u32>>,  // ip → fail count
    pub pending: Mutex<HashMap<u16, tokio::sync::mpsc::Sender<CharResponse>>>,
    pub char_tx: Mutex<Option<tokio::sync::mpsc::Sender<Vec<u8>>>>,
}

impl LoginState {
    pub fn new(db: MySqlPool, config: ServerConfig, messages: LoginMessages) -> Self {
        Self {
            db: Some(db),
            config,
            messages,
            lockout: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            char_tx: Mutex::new(None),
        }
    }

    pub fn test_only() -> Self {
        let config: ServerConfig = serde_yaml::from_str(r#"
sql_ip: "127.0.0.1"
sql_id: "test"
sql_pw: "test"
sql_db: "testdb"
login_id: "loginid"
login_pw: "loginpw"
login_ip: "127.0.0.1"
char_id: "charid"
char_pw: "charpw"
char_ip: "127.0.0.1"
map_ip: "127.0.0.1"
xor_key: "test"
start_point:
  m: 0
  x: 1
  y: 1
"#).expect("test config parse failed");
        Self {
            db: None,
            config,
            messages: LoginMessages::default(),
            lockout: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            char_tx: Mutex::new(None),
        }
    }

    pub async fn handle_new_connection(
        state: Arc<Self>,
        mut stream: TcpStream,
        peer: SocketAddr,
    ) {
        let ip_u32 = match peer.ip() {
            std::net::IpAddr::V4(v4) => u32::from(v4),
            _ => return,
        };

        // Check IP ban
        if let Some(pool) = &state.db {
            let ip_str = format!("{}", peer.ip());
            if db::is_ip_banned(pool, &ip_str).await {
                tracing::info!("[login] [banned] ip={}", ip_str);
                return;
            }
        }

        // Check lockout
        {
            let lock = state.lockout.lock().await;
            if lock.get(&ip_u32).copied().unwrap_or(0) >= 10 {
                tracing::info!("[login] [lockout] ip={}", peer.ip());
                return;
            }
        }

        // Send connect banner (mirrors C clif_accept ok branch)
        let banner: &[u8] = b"\xAA\x00\x13\x7E\x1B\x43\x4F\x4E\x4E\x45\x43\x54\x45\x44\x20\x53\x45\x52\x56\x45\x52\x0A";
        if stream.write_all(banner).await.is_err() {
            return;
        }

        // Read first packet to determine role
        let first = match read_client_packet(&mut stream).await {
            Ok(p) => p,
            Err(_) => return,
        };

        if first.len() < 4 {
            return;
        }

        let cmd = first[3];
        if cmd == 0xFF {
            interserver::promote_to_charserver(state, stream, first).await;
        } else {
            // Use the OS socket fd as session_id, matching the C login server where
            // session_id == the client's file descriptor (typically 4, 5, 6, ...).
            let session_id = stream.as_raw_fd() as u16;
            client::handle_client(state, stream, peer, session_id, first).await;
        }
    }

    pub async fn run(state: Arc<Self>, bind_addr: &str) -> anyhow::Result<()> {
        let listener = TcpListener::bind(bind_addr).await?;
        tracing::info!("[login] [ready] addr={}", bind_addr);
        loop {
            let (stream, peer) = listener.accept().await?;
            let s = Arc::clone(&state);
            tokio::spawn(async move {
                LoginState::handle_new_connection(s, stream, peer).await;
            });
        }
    }
}

#[cfg(test)]
mod accept_tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn test_server_sends_connect_banner() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let state = Arc::new(LoginState::test_only());

        tokio::spawn(async move {
            let (stream, peer) = listener.accept().await.unwrap();
            LoginState::handle_new_connection(Arc::clone(&state), stream, peer).await;
        });

        let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
        let mut banner = vec![0u8; 22];
        client.read_exact(&mut banner).await.unwrap();
        assert_eq!(banner[0], 0xAA);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"
// Login server lang file
LGN_ERRSERVER: Server error
LGN_WRONGPASS: Wrong password
LGN_WRONGUSER: Wrong username
LGN_ERRDB: Database error
LGN_USEREXIST: User already exists
LGN_ERRPASS: Bad password format
LGN_ERRUSER: Bad username format
LGN_NEWCHAR: Character created
LGN_CHGPASS: Password changed
LGN_DBLLOGIN: Already logged in
LGN_BANNED: IP is banned
"#;

    #[test]
    fn test_parse_lang_file_all_keys() {
        let msgs = parse_lang_file(FIXTURE).unwrap();
        assert_eq!(msgs.0[LGN_ERRSERVER], "Server error");
        assert_eq!(msgs.0[LGN_WRONGPASS], "Wrong password");
        assert_eq!(msgs.0[LGN_BANNED],    "IP is banned");
    }

    #[test]
    fn test_parse_lang_file_ignores_comments() {
        let msgs = parse_lang_file("// comment\nLGN_BANNED: x").unwrap();
        assert_eq!(msgs.0[LGN_BANNED], "x");
        // non-banned messages stay empty
        assert_eq!(msgs.0[LGN_ERRDB], "");
    }
}
