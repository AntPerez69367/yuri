//! FFI bridge for scripting.rs — exposes #[no_mangle] symbols replacing scripting.c.

use std::ffi::{c_char, c_int, c_uint};
use std::os::raw::c_void;
use crate::game::scripting as sl;

#[no_mangle]
pub unsafe extern "C" fn rust_sl_init() {
    ffi_catch!((), sl::sl_init())
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_fixmem() {
    ffi_catch!((), sl::sl_fixmem())
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_reload() -> c_int {
    ffi_catch!(-1, sl::sl_reload())
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_luasize(_user: *mut c_void) -> c_int {
    ffi_catch!(0, sl::sl_luasize())
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_doscript_blargs_vec(
    root:   *const c_char,
    method: *const c_char,
    nargs:  c_int,
    args:   *const *mut c_void,
) -> c_int {
    ffi_catch!(0, sl::sl_doscript_blargs_vec(root, method, nargs, args))
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_doscript_strings_vec(
    root:   *const c_char,
    method: *const c_char,
    nargs:  c_int,
    args:   *const *const c_char,
) -> c_int {
    ffi_catch!(0, sl::sl_doscript_strings_vec(root, method, nargs, args))
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_doscript_stackargs(
    root:   *const c_char,
    method: *const c_char,
    nargs:  c_int,
) -> c_int {
    ffi_catch!(0, sl::sl_doscript_stackargs(root, method, nargs))
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_updatepeople(
    bl: *mut c_void,
    ap: *mut c_void,
) -> c_int {
    ffi_catch!(0, sl::sl_updatepeople_impl(bl, ap))
}

/// Direct symbol used as a function pointer callback in map_foreachinarea.
#[no_mangle]
pub unsafe extern "C" fn sl_updatepeople(
    bl: *mut c_void,
    ap: *mut c_void,
) -> c_int {
    ffi_catch!(0, sl::sl_updatepeople_impl(bl, ap))
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_resumemenu(_id: c_uint, _sd: *mut c_void) {
    // Phase 5
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_resumemenuseq(_id: c_uint, _choice: c_int, _sd: *mut c_void) {
    // Phase 5
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_resumeinputseq(
    _id:    c_uint,
    _input: *mut c_char,
    _sd:    *mut c_void,
) {
    // Phase 5
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_resumedialog(_id: c_uint, _sd: *mut c_void) {
    // Phase 5
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_resumebuy(_items: *mut c_char, _sd: *mut c_void) {
    // Phase 5
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_resumeinput(
    _tag:   *mut c_char,
    _input: *mut c_char,
    _sd:    *mut c_void,
) {
    // Phase 5
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_resumesell(_id: c_uint, _sd: *mut c_void) {
    // Phase 5
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_exec(user: *mut c_void, code: *mut c_char) {
    ffi_catch!((), sl::sl_exec_str(user, code))
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_async_freeco(_user: *mut c_void) {
    // Phase 5
}
