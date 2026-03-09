//! Async scripting resume dispatch.
//!
//! When a dialog/menu/input response arrives from the network layer, the
//! corresponding `resume_*` function decodes the response and calls
//! `pending::deliver` or `pending::cancel` to wake the waiting mlua async
//! future.

use std::collections::HashMap;
use std::ffi::CStr;
use std::sync::Mutex;

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
/// If opts were stored by `store_menu_opts` (menuString/menuString2), delivers
/// the selected option string as Text; otherwise delivers the raw selection
/// number as Number (for menu/menuSeq).
pub unsafe fn resume_menu(selection: u32, user: *mut std::ffi::c_void) {
    let opts = MENU_OPTS.lock().unwrap()
        .as_mut()
        .and_then(|m| m.remove(&(user as usize)));
    let response = match opts {
        Some(opts) => {
            let idx = selection.saturating_sub(1) as usize;
            let s = opts.get(idx).map(|s| s.clone()).unwrap_or_default();
            crate::game::scripting::pending::AsyncResponse::Text(s)
        }
        None => {
            crate::game::scripting::pending::AsyncResponse::Number(selection as f64)
        }
    };
    crate::game::scripting::pending::deliver(user, response);
}

/// Resume after a sequential menu response.
/// `selection == 1` → cancel; `selection == 2` → chosen.
/// If `store_menu_opts` was called (menuString path), delivers the selected
/// option string as Text; otherwise delivers the 1-indexed choice as Number.
pub unsafe fn resume_menuseq(selection: u32, choice: i32, user: *mut std::ffi::c_void) {
    if selection == 1 {
        crate::game::scripting::pending::cancel(user);
        return;
    }
    if selection == 2 {
        let opts = MENU_OPTS.lock().unwrap()
            .as_mut()
            .and_then(|m| m.remove(&(user as usize)));
        let response = match opts {
            Some(opts) => {
                let idx = (choice as usize).saturating_sub(1);
                let s = opts.get(idx).map(|s| s.clone()).unwrap_or_default();
                crate::game::scripting::pending::AsyncResponse::Text(s)
            }
            None => {
                crate::game::scripting::pending::AsyncResponse::Number(choice as f64)
            }
        };
        crate::game::scripting::pending::deliver(user, response);
    } else {
        tracing::warn!("[scripting] resume_menuseq: unexpected selection={selection}");
        crate::game::scripting::pending::cancel(user);
    }
}

/// Resume after a dialog button click.
/// choice: 0 = previous, 1 = quit, 2 = next, other = "quit".
pub unsafe fn resume_dialog(choice: u32, user: *mut std::ffi::c_void) {
    if choice == 1 {
        crate::game::scripting::pending::cancel(user);
        return;
    }
    let s = match choice {
        0 => "previous",
        2 => "next",
        _ => "quit",
    };
    crate::game::scripting::pending::deliver(
        user,
        crate::game::scripting::pending::AsyncResponse::Text(s.to_owned()),
    );
}

/// Resume after a sequential input response.
/// choice: 0 = previous, 1 = quit, 2 = next + typed text, other = quit.
pub unsafe fn resume_inputseq(choice: u32, input: *const i8, user: *mut std::ffi::c_void) {
    match choice {
        0 => {
            crate::game::scripting::pending::deliver(
                user,
                crate::game::scripting::pending::AsyncResponse::Text("previous".to_owned()),
            );
        }
        1 => {
            crate::game::scripting::pending::cancel(user);
        }
        2 => {
            let text = if input.is_null() {
                String::new()
            } else {
                CStr::from_ptr(input).to_string_lossy().into_owned()
            };
            crate::game::scripting::pending::deliver(
                user,
                crate::game::scripting::pending::AsyncResponse::Pair("next".to_owned(), text),
            );
        }
        _ => {
            crate::game::scripting::pending::deliver(
                user,
                crate::game::scripting::pending::AsyncResponse::Text("quit".to_owned()),
            );
        }
    }
}

/// Resume after a shop buy response.
pub unsafe fn resume_buy(items: *const i8, user: *mut std::ffi::c_void) {
    let text = if items.is_null() {
        String::new()
    } else {
        CStr::from_ptr(items).to_string_lossy().into_owned()
    };
    crate::game::scripting::pending::deliver(
        user,
        crate::game::scripting::pending::AsyncResponse::Text(text),
    );
}

/// Resume after a shop sell response.
pub unsafe fn resume_sell(choice: u32, user: *mut std::ffi::c_void) {
    crate::game::scripting::pending::deliver(
        user,
        crate::game::scripting::pending::AsyncResponse::Number(choice as f64),
    );
}

/// Resume after a freeform input response.
/// Only the typed text (`input`) is returned to Lua; the tag is ignored.
pub unsafe fn resume_input(_tag: *const i8, input: *const i8, user: *mut std::ffi::c_void) {
    let text = if input.is_null() {
        String::new()
    } else {
        CStr::from_ptr(input).to_string_lossy().into_owned()
    };
    crate::game::scripting::pending::deliver(
        user,
        crate::game::scripting::pending::AsyncResponse::Text(text),
    );
}
