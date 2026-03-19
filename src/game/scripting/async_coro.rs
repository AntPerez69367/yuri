//! Coroutine resume dispatch.
//!
//! When a dialog/menu/input response arrives from the network layer, the
//! corresponding `resume_*` function decodes the response and resumes the
//! suspended Lua coroutine thread via `thread_registry::resume`.

use std::collections::HashMap;
use std::ffi::CStr;
use std::sync::Mutex;

use crate::game::scripting::thread_registry;

// Option strings to return after a menuString yield.  Stored by user pointer
// before the yield so resume_menu can look them up and push the selected text.
static MENU_OPTS: Mutex<Option<HashMap<usize, Vec<String>>>> = Mutex::new(None);

pub fn store_menu_opts(user: *mut std::ffi::c_void, opts: Vec<String>) {
    MENU_OPTS.lock().unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(user as usize, opts);
}

/// Remove any stored menu options for `user` (called on session disconnect/cancel).
pub fn clear_menu_opts(user: *mut std::ffi::c_void) {
    MENU_OPTS.lock().unwrap()
        .as_mut()
        .map(|m| m.remove(&(user as usize)));
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Resume after a menu selection.
/// If opts were stored by `store_menu_opts` (menuString/menuString2), resumes
/// with the selected option string; otherwise resumes with the raw selection number.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn resume_menu(selection: u32, user: *mut std::ffi::c_void) {
    let lua = crate::game::scripting::sl_state();
    let key = user as usize;
    let opts = MENU_OPTS.lock().unwrap()
        .as_mut()
        .and_then(|m| m.remove(&key));
    match opts {
        Some(opts) => {
            let idx = selection.saturating_sub(1) as usize;
            let s = opts.get(idx).cloned().unwrap_or_default();
            thread_registry::resume(lua, key, s);
        }
        None => {
            thread_registry::resume(lua, key, selection as i32);
        }
    }
}

/// Resume after a sequential menu response.
/// `selection == 1` → cancel; `selection == 2` → chosen.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn resume_menuseq(selection: u32, choice: i32, user: *mut std::ffi::c_void) {
    let lua = crate::game::scripting::sl_state();
    let key = user as usize;
    if selection == 1 {
        thread_registry::cancel(key);
        return;
    }
    if selection == 2 {
        let opts = MENU_OPTS.lock().unwrap()
            .as_mut()
            .and_then(|m| m.remove(&key));
        match opts {
            Some(opts) => {
                let idx = (choice as usize).saturating_sub(1);
                let s = opts.get(idx).cloned().unwrap_or_default();
                thread_registry::resume(lua, key, s);
            }
            None => {
                thread_registry::resume(lua, key, choice);
            }
        }
    } else {
        tracing::warn!("[scripting] resume_menuseq: unexpected selection={selection}");
        thread_registry::cancel(key);
    }
}

/// Resume after a dialog button click.
/// choice: 0 = previous, 1 = quit, 2 = next, other = "quit".
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn resume_dialog(choice: u32, user: *mut std::ffi::c_void) {
    let lua = crate::game::scripting::sl_state();
    let key = user as usize;
    if choice == 1 {
        thread_registry::cancel(key);
        return;
    }
    let s = match choice {
        0 => "previous",
        2 => "next",
        _ => "quit",
    };
    thread_registry::resume(lua, key, s);
}

/// Resume after a sequential input response.
/// choice: 0 = previous, 1 = quit, 2 = next + typed text, other = quit.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn resume_inputseq(choice: u32, input: *const i8, user: *mut std::ffi::c_void) {
    let lua = crate::game::scripting::sl_state();
    let key = user as usize;
    match choice {
        0 => {
            thread_registry::resume(lua, key, "previous");
        }
        1 => {
            thread_registry::cancel(key);
        }
        2 => {
            let text = if input.is_null() {
                String::new()
            } else {
                CStr::from_ptr(input).to_string_lossy().into_owned()
            };
            // Resume with two values: direction and typed text
            let dir = lua.create_string("next").unwrap();
            let txt = lua.create_string(&text).unwrap();
            thread_registry::resume(lua, key,
                mlua::MultiValue::from_vec(vec![
                    mlua::Value::String(dir),
                    mlua::Value::String(txt),
                ])
            );
        }
        _ => {
            thread_registry::resume(lua, key, "quit");
        }
    }
}

/// Resume after a shop buy response.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn resume_buy(items: *const i8, user: *mut std::ffi::c_void) {
    let lua = crate::game::scripting::sl_state();
    let text = if items.is_null() {
        String::new()
    } else {
        CStr::from_ptr(items).to_string_lossy().into_owned()
    };
    thread_registry::resume(lua, user as usize, text);
}

/// Resume after a shop sell response.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn resume_sell(choice: u32, user: *mut std::ffi::c_void) {
    let lua = crate::game::scripting::sl_state();
    thread_registry::resume(lua, user as usize, choice as i32);
}

/// Resume after a freeform input response.
/// Only the typed text (`input`) is returned to Lua; the tag is ignored.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn resume_input(_tag: *const i8, input: *const i8, user: *mut std::ffi::c_void) {
    let lua = crate::game::scripting::sl_state();
    let text = if input.is_null() {
        String::new()
    } else {
        CStr::from_ptr(input).to_string_lossy().into_owned()
    };
    thread_registry::resume(lua, user as usize, text);
}
