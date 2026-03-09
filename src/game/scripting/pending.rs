//! Pending async response channels.
//!
//! When a Lua async method (menu, dialog, input, buy, sell, etc.) sends a
//! packet to the client and needs to wait for the client's response, it calls
//! `register(user)` to get a `Receiver`. When the network layer receives the
//! response packet, it calls `deliver(user, response)` to wake the waiting
//! future. On session disconnect, `cancel(user)` drops the sender so the
//! receiver returns `Err`, which the method maps to a Lua RuntimeError.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Mutex;
use tokio::sync::oneshot;

/// The response value delivered to a waiting async Lua method.
#[derive(Debug)]
pub enum AsyncResponse {
    /// A single number (menu index, sell/buy quantity, etc.)
    Number(f64),
    /// A single string (dialog direction, input text, menu option text)
    Text(String),
    /// Two strings (inputSeq: direction + typed text)
    Pair(String, String),
}

static PENDING: Mutex<Option<HashMap<usize, oneshot::Sender<AsyncResponse>>>> = Mutex::new(None);

/// Register a pending response for session `user`.
///
/// Returns the receiver to `.await` on. Any previous pending response for
/// this user is silently cancelled (sender dropped).
pub fn register(user: *mut c_void) -> oneshot::Receiver<AsyncResponse> {
    let (tx, rx) = oneshot::channel();
    PENDING
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(user as usize, tx);
    rx
}

/// Deliver a response for session `user`, waking the waiting future.
///
/// No-op if no sender is registered for this user.
pub fn deliver(user: *mut c_void, response: AsyncResponse) {
    if let Some(tx) = PENDING
        .lock()
        .unwrap()
        .as_mut()
        .and_then(|m| m.remove(&(user as usize)))
    {
        let _ = tx.send(response);
    }
}

/// Cancel any pending response for `user` (e.g. on session disconnect).
///
/// The receiver will get `Err(RecvError)`, which the method maps to a
/// Lua `RuntimeError("session disconnected")`.
pub fn cancel(user: *mut c_void) {
    PENDING
        .lock()
        .unwrap()
        .as_mut()
        .map(|m| m.remove(&(user as usize)));
    // sender is dropped → receiver gets Err
}
