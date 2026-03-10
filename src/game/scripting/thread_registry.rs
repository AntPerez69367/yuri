//! Coroutine thread registry for NPC dialog/menu/input interactions.
//!
//! When a Lua NPC script yields (e.g. after sending a dialog packet to the
//! client), the suspended coroutine thread is stored here keyed by the
//! user's `MapSessionData` pointer.  When the client responds, the
//! corresponding `resume_*` handler retrieves the thread and resumes it
//! with the response value.

use std::collections::HashMap;

use mlua::{Lua, RegistryKey, Thread, ThreadStatus};

/// Suspended coroutine threads, keyed by user pointer (as usize).
///
/// SAFETY: Only accessed from the single game thread (tokio LocalSet).
static mut THREADS: Option<HashMap<usize, RegistryKey>> = None;

pub fn init() {
    unsafe { THREADS = Some(HashMap::new()); }
}

/// Store a suspended Lua thread for `user`.
///
/// Any previously stored thread for this user is silently dropped
/// (equivalent to cancelling an in-progress NPC interaction).
pub fn store(lua: &Lua, user_key: usize, thread: &Thread) {
    let key = lua.create_registry_value(thread)
        .expect("failed to store thread in registry");
    unsafe {
        THREADS.as_mut().unwrap().insert(user_key, key);
    }
}

/// Resume a suspended thread with the given arguments.
///
/// Returns `true` if the thread was found and resumed (regardless of
/// whether it yielded again or completed).  Returns `false` if no thread
/// was stored for this user.
pub fn resume<A: mlua::IntoLuaMulti>(lua: &Lua, user_key: usize, args: A) -> bool {
    let reg_key = unsafe {
        THREADS.as_mut().unwrap().remove(&user_key)
    };
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
    unsafe {
        THREADS.as_mut().unwrap().remove(&user_key);
    }
}
