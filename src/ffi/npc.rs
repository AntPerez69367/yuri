//! FFI bridge for npc.rs — exposes #[no_mangle] symbols replacing npc.c.

use std::ffi::{c_char, c_int, c_uint};
use crate::game::npc::{
    self, NpcData,
    npc_readglobalreg, npc_setglobalreg,
    npc_idlower, npc_src_clear, npc_src_add, npc_warp_add,
    npc_warp, npc_action, npc_movetime, npc_duration,
    npc_move, npc_get_new_npctempid,
};

// ---------------------------------------------------------------------------
// Exact-name exports (called by name from map_server.rs extern "C" block or
// timer infrastructure — no rename needed after npc.c is removed)
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn npc_init() -> c_int {
    npc::npc_init()
}

#[no_mangle]
pub unsafe extern "C" fn warp_init() -> c_int {
    npc::warp_init()
}

#[no_mangle]
pub unsafe extern "C" fn npc_runtimers(id: c_int, n: c_int) -> c_int {
    npc::npc_runtimers(id, n)
}

// ---------------------------------------------------------------------------
// _ffi-suffix exports (avoid link collision with npc.c during transition;
// npc.h inline wrappers redirect callers here; drop suffix after npc.c removed)
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn npc_action_ffi(nd: *mut NpcData) -> c_int {
    npc_action(nd)
}

#[no_mangle]
pub unsafe extern "C" fn npc_movetime_ffi(nd: *mut NpcData) -> c_int {
    npc_movetime(nd)
}

#[no_mangle]
pub unsafe extern "C" fn npc_duration_ffi(nd: *mut NpcData) -> c_int {
    npc_duration(nd)
}

#[no_mangle]
pub unsafe extern "C" fn npc_warp_ffi(nd: *mut NpcData, m: c_int, x: c_int, y: c_int) -> c_int {
    npc_warp(nd, m, x, y)
}

#[no_mangle]
pub unsafe extern "C" fn npc_move_ffi(nd: *mut NpcData) -> c_int {
    npc_move(nd)
}

#[no_mangle]
pub unsafe extern "C" fn npc_readglobalreg_ffi(nd: *mut NpcData, reg: *const c_char) -> c_int {
    npc_readglobalreg(nd, reg)
}

#[no_mangle]
pub unsafe extern "C" fn npc_setglobalreg_ffi(nd: *mut NpcData, reg: *const c_char, val: c_int) -> c_int {
    npc_setglobalreg(nd, reg, val)
}

#[no_mangle]
pub unsafe extern "C" fn npc_idlower_ffi(id: c_int) -> c_int {
    npc_idlower(id)
}

#[no_mangle]
pub unsafe extern "C" fn npc_src_clear_ffi() -> c_int {
    npc_src_clear()
}

#[no_mangle]
pub unsafe extern "C" fn npc_src_add_ffi(f: *const c_char) -> c_int {
    npc_src_add(f)
}

#[no_mangle]
pub unsafe extern "C" fn npc_warp_add_ffi(f: *const c_char) -> c_int {
    npc_warp_add(f)
}

#[no_mangle]
pub unsafe extern "C" fn npc_get_new_npctempid_ffi() -> c_uint {
    npc_get_new_npctempid()
}
