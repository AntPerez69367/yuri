//! FFI bridge for mob database.

use std::os::raw::{c_char, c_int, c_uint};
use std::ptr::null_mut;

use crate::database::mob_db::{self as db, MobDbData};

#[no_mangle]
pub extern "C" fn rust_mobdb_init() -> c_int { ffi_catch!(-1, db::init()) }

#[no_mangle]
pub extern "C" fn rust_mobdb_term() { ffi_catch!((), db::term()) }

#[no_mangle]
pub extern "C" fn rust_mobdb_search(id: c_uint) -> *mut MobDbData {
    ffi_catch!(null_mut(), db::search(id))
}

#[no_mangle]
pub extern "C" fn rust_mobdb_searchexist(id: c_uint) -> *mut MobDbData {
    ffi_catch!(null_mut(), db::searchexist(id))
}

#[no_mangle]
pub extern "C" fn rust_mobdb_searchname(s: *const c_char) -> *mut MobDbData {
    if s.is_null() { return null_mut(); }
    ffi_catch!(null_mut(), db::searchname(s))
}

#[no_mangle]
pub extern "C" fn rust_mobdb_level(id: c_uint) -> c_int {
    ffi_catch!(0, db::level(id))
}

#[no_mangle]
pub extern "C" fn rust_mobdb_experience(id: c_uint) -> c_uint {
    ffi_catch!(0, db::experience(id))
}

#[no_mangle]
pub extern "C" fn rust_mobdb_id(s: *const c_char) -> c_int {
    if s.is_null() { return 0; }
    ffi_catch!(0, db::find_id(s))
}
