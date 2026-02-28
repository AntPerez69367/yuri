//! Async coroutine system — replaces `sl_async*` in `c_src/scripting.c`.
//!
//! Lua coroutines are stored in `USER->coref` as raw `luaL_ref` integers.
//! When a dialog/menu/input response arrives from the network layer, the
//! corresponding `resume_*` function pushes return values onto the main
//! `lua_State` and resumes the waiting coroutine.
//!
//! Raw `lua_State*` access uses the pointer cached in `super::sl_gstate`
//! (set in `sl_init` via `exec_raw`).  mlua-sys 0.6 exposes a Lua-5.4-style
//! compat wrapper for `lua_resume` even on LuaJIT: signature is
//! `(L, from, narg, nres) → c_int`; pass `null` for `from` (ignored by JIT).

use mlua::ffi as lua_ffi;
use std::ffi::{CStr, c_char};
use std::os::raw::{c_int, c_uint, c_void};

// ─── C accessor stubs ────────────────────────────────────────────────────────
// sl_compat.c exposes thin wrappers for USER struct fields so Rust avoids
// hard-coded byte offsets into an unported C struct.

extern "C" {
    fn sl_user_coref(sd: *mut c_void) -> c_uint;
    fn sl_user_set_coref(sd: *mut c_void, v: c_uint);
    fn sl_user_coref_container(sd: *mut c_void) -> c_uint;
    fn sl_user_map_id2sd(id: c_uint) -> *mut c_void;
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Return the raw `lua_State*` cached in `sl_gstate`.
/// Valid once `sl_init()` has run.
#[inline]
unsafe fn L() -> *mut lua_ffi::lua_State {
    super::sl_gstate as *mut lua_ffi::lua_State
}

/// Resolve the effective coref for `user`, following `coref_container`
/// indirection (used when a container NPC proxies the player's coroutine).
unsafe fn resolve_coref(user: *mut c_void) -> c_uint {
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
unsafe fn do_resume(user: *mut c_void, nargs: c_int) {
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
    let mut nresults: c_int = 0;
    let status = lua_ffi::lua_resume(costate, std::ptr::null_mut(), nargs, &mut nresults);
    if status == lua_ffi::LUA_ERRRUN {
        let msg_ptr = lua_ffi::lua_tolstring(costate, -1, std::ptr::null_mut());
        let msg = if msg_ptr.is_null() {
            "(unknown error)".to_owned()
        } else {
            CStr::from_ptr(msg_ptr).to_string_lossy().into_owned()
        };
        lua_ffi::lua_settop(costate, lua_ffi::lua_gettop(costate) - 1);
        free_coref(user);
        tracing::warn!("[scripting] coroutine error: {msg}");
    }
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Free the coroutine registry reference and zero `USER->coref`.
/// Mirrors `sl_async_freeco` in scripting.c.
pub unsafe fn free_coref(user: *mut c_void) {
    let coref = sl_user_coref(user);
    if coref == 0 { return; }
    lua_ffi::luaL_unref(L(), lua_ffi::LUA_REGISTRYINDEX, coref as c_int);
    sl_user_set_coref(user, 0);
}

/// Resume after a menu selection. Mirrors `sl_resumemenu`.
pub unsafe fn resume_menu(selection: c_uint, user: *mut c_void) {
    lua_ffi::lua_pushnumber(L(), selection as f64);
    do_resume(user, 1);
}

/// Resume after a sequential menu response. Mirrors `sl_resumemenuseq`.
/// `selection == 1` → quit; `selection == 2` → chosen.
pub unsafe fn resume_menuseq(selection: c_uint, choice: c_int, user: *mut c_void) {
    if selection == 1 { free_coref(user); return; }
    if selection == 2 {
        lua_ffi::lua_pushnumber(L(), choice as f64);
        do_resume(user, 1);
    }
}

/// Resume after a dialog button click. Mirrors `sl_resumedialog`.
/// choice: 0 = previous, 1 = quit, 2 = next, other = "quit".
pub unsafe fn resume_dialog(choice: c_uint, user: *mut c_void) {
    if choice == 1 { free_coref(user); return; }
    let s: *const c_char = match choice {
        0 => b"previous\0".as_ptr() as *const c_char,
        2 => b"next\0".as_ptr() as *const c_char,
        _ => b"quit\0".as_ptr() as *const c_char,
    };
    lua_ffi::lua_pushstring(L(), s);
    do_resume(user, 1);
}

/// Resume after a sequential input response. Mirrors `sl_resumeinputseq`.
pub unsafe fn resume_inputseq(choice: c_uint, input: *const c_char, user: *mut c_void) {
    match choice {
        0 => {
            lua_ffi::lua_pushstring(L(), b"previous\0".as_ptr() as *const c_char);
            do_resume(user, 1);
        }
        1 => { free_coref(user); }
        2 => {
            lua_ffi::lua_pushstring(L(), b"next\0".as_ptr() as *const c_char);
            lua_ffi::lua_pushstring(L(), input);
            do_resume(user, 2);
        }
        _ => {
            lua_ffi::lua_pushstring(L(), b"quit\0".as_ptr() as *const c_char);
            do_resume(user, 1);
        }
    }
}

/// Resume after a shop buy response. Mirrors `sl_resumebuy`.
pub unsafe fn resume_buy(items: *const c_char, user: *mut c_void) {
    lua_ffi::lua_pushstring(L(), items);
    do_resume(user, 1);
}

/// Resume after a shop sell response. Mirrors `sl_resumesell`.
pub unsafe fn resume_sell(choice: c_uint, user: *mut c_void) {
    lua_ffi::lua_pushnumber(L(), choice as f64);
    do_resume(user, 1);
}

/// Resume after a freeform input response. Mirrors `sl_resumeinput`.
/// Only the typed text (`input`) is returned to Lua; the tag is ignored.
pub unsafe fn resume_input(_tag: *const c_char, input: *const c_char, user: *mut c_void) {
    lua_ffi::lua_pushstring(L(), input);
    do_resume(user, 1);
}
