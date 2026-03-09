//! Async coroutine system.
//!
//! Lua coroutines are stored in `USER->coref` as raw `luaL_ref` integers.
//! When a dialog/menu/input response arrives from the network layer, the
//! corresponding `resume_*` function pushes return values onto the main
//! `lua_State` and resumes the waiting coroutine.
//!
//! Raw `lua_State*` access uses the pointer cached in `super::sl_gstate`
//! (set in `sl_init` via `exec_raw`).  mlua-sys 0.6 exposes a Lua-5.4-style
//! compat wrapper for `lua_resume` even on LuaJIT: signature is
//! `(L, from, narg, nres) → i32`; pass `null` for `from` (ignored by JIT).

use mlua::ffi as lua_ffi;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::sync::Mutex;

// Option strings to return after a menuString yield.  Stored by user pointer
// before the yield so resume_menu can look them up and push the selected text.
static MENU_OPTS: Mutex<Option<HashMap<usize, Vec<String>>>> = Mutex::new(None);

pub fn store_menu_opts(user: *mut std::ffi::c_void, opts: Vec<String>) {
    MENU_OPTS.lock().unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(user as usize, opts);
}

// ─── PC accessor wrappers ─────────────────────────────────────────────────────
use crate::game::scripting::pc_accessors::{
    sl_user_coref, sl_user_set_coref, sl_user_coref_container, sl_user_map_id2sd,
};

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Return the raw `lua_State*` cached in `sl_gstate`.
/// Valid once `sl_init()` has run.
#[inline]
unsafe fn L() -> *mut lua_ffi::lua_State {
    super::sl_gstate as *mut lua_ffi::lua_State
}

/// Resolve the effective coref for `user`, following `coref_container`
/// indirection (used when a container NPC proxies the player's coroutine).
unsafe fn resolve_coref(user: *mut std::ffi::c_void) -> u32 {
    let own = sl_user_coref(user);
    if own != 0 {
        return own;
    }
    let nsd = sl_user_map_id2sd(sl_user_coref_container(user));
    if nsd.is_null() { 0 } else { sl_user_coref(nsd) }
}

/// Core resume.  Caller must have pushed exactly `nargs` values onto the
/// main state before calling this.  Transfers them to the coroutine and
/// resumes it; handles error and cleans the stack on all code paths.
unsafe fn do_resume(user: *mut std::ffi::c_void, nargs: i32) {
    let coref = resolve_coref(user);
    let state = L();

    if coref == 0 {
        // No active coroutine — discard the pushed args.
        lua_ffi::lua_settop(state, lua_ffi::lua_gettop(state) - nargs);
        return;
    }

    lua_ffi::lua_rawgeti(state, lua_ffi::LUA_REGISTRYINDEX, coref as lua_ffi::lua_Integer);
    if lua_ffi::lua_type(state, -1) != lua_ffi::LUA_TTHREAD {
        // Stale ref — discard thread slot + args.
        lua_ffi::lua_settop(state, lua_ffi::lua_gettop(state) - 1 - nargs);
        return;
    }

    let costate = lua_ffi::lua_tothread(state, -1);
    lua_ffi::lua_settop(state, lua_ffi::lua_gettop(state) - 1); // pop thread
    lua_ffi::lua_xmove(state, costate, nargs);                   // move args to coro

    // mlua-sys 0.6 compat wrapper: lua_resume(L, from, narg, nres)
    // `from` is ignored by LuaJIT; `nres` is an out-param we don't need.
    let mut nresults: i32 = 0;
    let status = lua_ffi::lua_resume(costate, std::ptr::null_mut(), nargs, &mut nresults);
    if status == lua_ffi::LUA_OK {
        // Coroutine returned normally (finished); free its registry slot.
        free_coref(user);
    } else if status != lua_ffi::LUA_YIELD {
        // Any error (LUA_ERRRUN, LUA_ERRMEM, LUA_ERRERR, or unknown).
        let msg_ptr = lua_ffi::lua_tolstring(costate, -1, std::ptr::null_mut());
        let msg = if msg_ptr.is_null() {
            "(unknown error)".to_owned()
        } else {
            CStr::from_ptr(msg_ptr).to_string_lossy().into_owned()
        };
        lua_ffi::lua_settop(costate, lua_ffi::lua_gettop(costate) - 1);
        free_coref(user);
        tracing::error!("[scripting] coroutine error (status={status}): {msg}");
        tracing::warn!("[scripting] coroutine error (status={status}): {msg}");
    }
    // LUA_YIELD: coroutine is suspended and waiting; keep the registry reference.
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Launch a new async coroutine for `user` from a Lua function.
///
/// `func` must be on top of `state`'s stack (pushed by exec_raw).
/// Pops `func` from the stack, frees any prior coroutine, and does the
/// initial resume. After this returns, the coroutine is either finished
/// (coref zeroed) or suspended at a yield waiting for a resume_* call.
pub unsafe fn start_async(user: *mut std::ffi::c_void, state: *mut lua_ffi::lua_State) {
    free_coref(user);

    let thread = lua_ffi::lua_newthread(state);
    lua_ffi::lua_pushvalue(state, -2);
    lua_ffi::lua_xmove(state, thread, 1);
    let coref = lua_ffi::luaL_ref(state, lua_ffi::LUA_REGISTRYINDEX);
    lua_ffi::lua_pop(state, 1);
    sl_user_set_coref(user, coref as u32);
    do_resume(user, 0);
}

/// Free the coroutine registry reference and zero `USER->coref`.
pub unsafe fn free_coref(user: *mut std::ffi::c_void) {
    let coref = sl_user_coref(user);
    if coref == 0 { return; }
    lua_ffi::luaL_unref(L(), lua_ffi::LUA_REGISTRYINDEX, coref as i32);
    sl_user_set_coref(user, 0);
    MENU_OPTS.lock().unwrap().as_mut().map(|m| m.remove(&(user as usize)));
}

/// Resume after a menu selection.
/// If opts were stored by `store_menu_opts` (menuString), pushes the selected
/// option string; otherwise pushes the raw selection number (menuSeq uses the
/// number directly, so its path never calls `store_menu_opts`).
pub unsafe fn resume_menu(selection: u32, user: *mut std::ffi::c_void) {
    let state = L();
    let opts = MENU_OPTS.lock().unwrap()
        .as_mut()
        .and_then(|m| m.remove(&(user as usize)));
    match opts {
        Some(opts) => {
            let idx = selection.saturating_sub(1) as usize;
            let s = opts.get(idx).map(|s| s.as_str()).unwrap_or("");
            let cs = CString::new(s).unwrap_or_default();
            lua_ffi::lua_pushstring(state, cs.as_ptr());
        }
        None => {
            lua_ffi::lua_pushnumber(state, selection as f64);
        }
    }
    do_resume(user, 1);
}

/// Resume after a sequential menu response.
/// `selection == 1` → quit; `selection == 2` → chosen.
/// If `store_menu_opts` was called (menuString path), pushes the selected
/// option string; otherwise pushes the 1-indexed choice number (menuSeq).
pub unsafe fn resume_menuseq(selection: u32, choice: i32, user: *mut std::ffi::c_void) {
    if selection == 1 { free_coref(user); return; }
    if selection == 2 {
        let state = L();
        let opts = MENU_OPTS.lock().unwrap()
            .as_mut()
            .and_then(|m| m.remove(&(user as usize)));
        match opts {
            Some(opts) => {
                let idx = (choice as usize).saturating_sub(1);
                let s = opts.get(idx).map(|s| s.as_str()).unwrap_or("");
                let cs = CString::new(s).unwrap_or_default();
                lua_ffi::lua_pushstring(state, cs.as_ptr());
            }
            None => {
                lua_ffi::lua_pushnumber(state, choice as f64);
            }
        }
        do_resume(user, 1);
    } else {
        tracing::warn!("[scripting] resume_menuseq: unexpected selection={selection}");
        free_coref(user);
    }
}

/// Resume after a dialog button click.
/// choice: 0 = previous, 1 = quit, 2 = next, other = "quit".
pub unsafe fn resume_dialog(choice: u32, user: *mut std::ffi::c_void) {
    if choice == 1 { free_coref(user); return; }
    let s: *const i8 = match choice {
        0 => b"previous\0".as_ptr() as *const i8,
        2 => b"next\0".as_ptr() as *const i8,
        _ => b"quit\0".as_ptr() as *const i8,
    };
    lua_ffi::lua_pushstring(L(), s);
    do_resume(user, 1);
}

/// Resume after a sequential input response.
pub unsafe fn resume_inputseq(choice: u32, input: *const i8, user: *mut std::ffi::c_void) {
    match choice {
        0 => {
            lua_ffi::lua_pushstring(L(), b"previous\0".as_ptr() as *const i8);
            do_resume(user, 1);
        }
        1 => { free_coref(user); }
        2 => {
            lua_ffi::lua_pushstring(L(), b"next\0".as_ptr() as *const i8);
            lua_ffi::lua_pushstring(L(), input);
            do_resume(user, 2);
        }
        _ => {
            lua_ffi::lua_pushstring(L(), b"quit\0".as_ptr() as *const i8);
            do_resume(user, 1);
        }
    }
}

/// Resume after a shop buy response.
pub unsafe fn resume_buy(items: *const i8, user: *mut std::ffi::c_void) {
    lua_ffi::lua_pushstring(L(), items);
    do_resume(user, 1);
}

/// Resume after a shop sell response.
pub unsafe fn resume_sell(choice: u32, user: *mut std::ffi::c_void) {
    lua_ffi::lua_pushnumber(L(), choice as f64);
    do_resume(user, 1);
}

/// Resume after a freeform input response.
/// Only the typed text (`input`) is returned to Lua; the tag is ignored.
pub unsafe fn resume_input(_tag: *const i8, input: *const i8, user: *mut std::ffi::c_void) {
    lua_ffi::lua_pushstring(L(), input);
    do_resume(user, 1);
}
