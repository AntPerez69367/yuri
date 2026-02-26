//! FFI bridge for mob.rs — exposes #[no_mangle] symbols replacing mob.c logic.

use std::ffi::{c_char, c_int, c_uint};
use crate::game::mob::{self as g, MobSpawnData};

#[no_mangle]
pub unsafe extern "C" fn rust_mobspawn_read() -> c_int {
    g::mobspawn_read()
}

#[no_mangle]
pub unsafe extern "C" fn rust_mob_timer_spawns(id: c_int, n: c_int) -> c_int {
    g::mob_timer_spawns(id, n)
}

#[no_mangle]
pub unsafe extern "C" fn rust_mob_respawn_getstats(mob: *mut MobSpawnData) -> c_int {
    g::mob_respawn_getstats(mob)
}

#[no_mangle]
pub unsafe extern "C" fn rust_mob_warp(mob: *mut MobSpawnData, m: c_int, x: c_int, y: c_int) -> c_int {
    g::mob_warp(mob, m, x, y)
}

#[no_mangle]
pub unsafe extern "C" fn rust_mobspawn_onetime(
    id: c_uint, m: c_int, x: c_int, y: c_int,
    times: c_int, start: c_int, end: c_int,
    replace: c_uint, owner: c_uint,
) -> *mut c_uint {
    g::mobspawn_onetime(id, m, x, y, times, start, end, replace, owner)
}

#[no_mangle]
pub unsafe extern "C" fn rust_mob_readglobalreg(mob: *mut MobSpawnData, reg: *const c_char) -> c_int {
    g::mob_readglobalreg(mob, reg)
}

#[no_mangle]
pub unsafe extern "C" fn rust_mob_setglobalreg(mob: *mut MobSpawnData, reg: *const c_char, val: c_int) -> c_int {
    g::mob_setglobalreg(mob, reg, val)
}

#[no_mangle]
pub unsafe extern "C" fn rust_mob_drops(mob: *mut MobSpawnData, sd: *mut std::ffi::c_void) -> c_int {
    g::mobdb_drops(mob, sd)
}

#[no_mangle]
pub unsafe extern "C" fn rust_mob_handle_sub(mob: *mut MobSpawnData) -> c_int {
    g::mob_handle_sub(mob);
    0
}

#[no_mangle]
pub unsafe extern "C" fn rust_kill_mob(mob: *mut MobSpawnData) -> c_int {
    g::kill_mob(mob)
}

#[no_mangle]
pub unsafe extern "C" fn rust_mob_calcstat(mob: *mut MobSpawnData) -> c_int {
    g::mob_calcstat(mob)
}

#[no_mangle]
pub unsafe extern "C" fn rust_mob_respawn(mob: *mut MobSpawnData) -> c_int {
    g::mob_respawn(mob)
}

#[no_mangle]
pub unsafe extern "C" fn rust_mob_respawn_nousers(mob: *mut MobSpawnData) -> c_int {
    g::mob_respawn_nousers(mob)
}

#[no_mangle]
pub unsafe extern "C" fn rust_mob_flushmagic(mob: *mut MobSpawnData) -> c_int {
    g::mob_flushmagic(mob)
}

#[no_mangle]
pub unsafe extern "C" fn rust_move_mob(mob: *mut MobSpawnData) -> c_int {
    g::move_mob(mob)
}

#[no_mangle]
pub unsafe extern "C" fn rust_move_mob_ignore_object(mob: *mut MobSpawnData) -> c_int {
    g::move_mob_ignore_object(mob)
}

#[no_mangle]
pub unsafe extern "C" fn rust_moveghost_mob(mob: *mut MobSpawnData) -> c_int {
    g::moveghost_mob(mob)
}

#[no_mangle]
pub unsafe extern "C" fn rust_move_mob_intent(
    mob: *mut MobSpawnData,
    bl: *mut crate::database::map_db::BlockList,
) -> c_int {
    g::move_mob_intent(mob, bl)
}
