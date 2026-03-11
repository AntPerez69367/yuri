//! Coroutine thread registry for NPC dialog/menu/input interactions.
//!
//! When a Lua NPC script yields (e.g. after sending a dialog packet to the
//! client), the suspended coroutine thread is stored here keyed by the
//! user's `MapSessionData` pointer.  When the client responds, the
//! corresponding `resume_*` handler retrieves the thread and resumes it
//! with the response value.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use mlua::{Lua, RegistryKey, Thread, ThreadStatus};

/// Wrapper to allow `HashMap<usize, RegistryKey>` in `OnceLock<Mutex<...>>`.
///
/// SAFETY: Only accessed from the single game thread (tokio LocalSet).
/// No concurrent access ever occurs.
struct ThreadMap(HashMap<usize, RegistryKey>);
unsafe impl Send for ThreadMap {}
unsafe impl Sync for ThreadMap {}

impl std::fmt::Debug for ThreadMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThreadMap").field("len", &self.0.len()).finish()
    }
}

static THREADS: OnceLock<Mutex<ThreadMap>> = OnceLock::new();

fn threads() -> std::sync::MutexGuard<'static, ThreadMap> {
    THREADS.get_or_init(|| Mutex::new(ThreadMap(HashMap::new())))
        .lock().unwrap_or_else(|e| e.into_inner())
}

pub fn init() {
    // Ensure the OnceLock is initialized.
    drop(threads());
}

/// Store a suspended Lua thread for `user`.
///
/// Any previously stored thread for this user is silently dropped
/// (equivalent to cancelling an in-progress NPC interaction).
pub fn store(lua: &Lua, user_key: usize, thread: &Thread) {
    let key = lua.create_registry_value(thread)
        .expect("failed to store thread in registry");
    threads().0.insert(user_key, key);
}

/// Resume a suspended thread with the given arguments.
///
/// Returns `true` if the thread was found and resumed (regardless of
/// whether it yielded again or completed).  Returns `false` if no thread
/// was stored for this user.
pub fn resume<A: mlua::IntoLuaMulti>(lua: &Lua, user_key: usize, args: A) -> bool {
    let reg_key = threads().0.remove(&user_key);
    let reg_key = match reg_key {
        Some(k) => k,
        None => return false,
    };
    let thread: Thread = match lua.registry_value(&reg_key) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("[scripting] thread_registry::resume: registry_value failed: {e}");
            lua.remove_registry_value(reg_key).ok();
            return false;
        }
    };
    lua.remove_registry_value(reg_key).ok();

    match thread.resume::<mlua::MultiValue>(args) {
        Ok(_) => {
            if thread.status() == ThreadStatus::Resumable {
                store(lua, user_key, &thread);
            }
        }
        Err(e) => {
            tracing::warn!("[scripting] thread_registry::resume: {e}");
        }
    }
    true
}

/// Cancel (drop) any stored thread for `user`.
pub fn cancel(user_key: usize) {
    threads().0.remove(&user_key);
}
