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
pub unsafe extern "C" fn rust_sl_resumemenu(selection: c_uint, sd: *mut c_void) {
    ffi_catch!((), sl::async_coro::resume_menu(selection, sd))
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_resumemenuseq(selection: c_uint, choice: c_int, sd: *mut c_void) {
    ffi_catch!((), sl::async_coro::resume_menuseq(selection, choice, sd))
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_resumeinputseq(
    choice: c_uint,
    input:  *mut c_char,
    sd:     *mut c_void,
) {
    ffi_catch!((), sl::async_coro::resume_inputseq(choice, input, sd))
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_resumedialog(choice: c_uint, sd: *mut c_void) {
    ffi_catch!((), sl::async_coro::resume_dialog(choice, sd))
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_resumebuy(items: *mut c_char, sd: *mut c_void) {
    ffi_catch!((), sl::async_coro::resume_buy(items, sd))
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_resumeinput(
    tag:   *mut c_char,
    input: *mut c_char,
    sd:    *mut c_void,
) {
    ffi_catch!((), sl::async_coro::resume_input(tag, input, sd))
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_resumesell(choice: c_uint, sd: *mut c_void) {
    ffi_catch!((), sl::async_coro::resume_sell(choice, sd))
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_exec(user: *mut c_void, code: *mut c_char) {
    ffi_catch!((), sl::sl_exec_str(user, code))
}

#[no_mangle]
pub unsafe extern "C" fn rust_sl_async_freeco(user: *mut c_void) {
    ffi_catch!((), sl::async_coro::free_coref(user))
}
