//! Network and session globals for the map server.

use std::sync::atomic::AtomicI32;
use std::sync::{Mutex, OnceLock};

/// File descriptor for the char-server connection.
pub static CHAR_FD: AtomicI32 = AtomicI32::new(0);

/// File descriptor for the map network socket (map listen port).
pub static MAP_FD: AtomicI32 = AtomicI32::new(0);

/// Online user list (count + per-slot char-id array).
pub struct UserlistData {
    pub user_count: u32,
    pub user: [u32; 10000],
}

static USERLIST: OnceLock<Mutex<UserlistData>> = OnceLock::new();

#[inline]
pub fn userlist() -> std::sync::MutexGuard<'static, UserlistData> {
    USERLIST
        .get_or_init(|| {
            Mutex::new(UserlistData {
                user_count: 0,
                user: [0u32; 10000],
            })
        })
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Authentication-attempt counter.
pub static AUTH_COUNTER: AtomicI32 = AtomicI32::new(0);

/// File descriptor for the logging socket (unused in current build; kept for ABI).
pub static LOG_FD: AtomicI32 = AtomicI32::new(0);

/// Map server public IP string (dotted-decimal, e.g. "127.0.0.1").
pub static MAP_IP_S: OnceLock<[u8; 16]> = OnceLock::new();

/// Logging server IP string (dotted-decimal).
pub static LOG_IP_S: OnceLock<[u8; 16]> = OnceLock::new();
