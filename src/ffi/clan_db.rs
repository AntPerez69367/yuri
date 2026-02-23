//! FFI bridge for clan database.

use std::os::raw::{c_char, c_int};
use std::ptr::null_mut;

use crate::database::clan_db::{self as db, ClanData};

#[no_mangle]
pub extern "C" fn rust_clandb_init() -> c_int { ffi_catch!(-1, db::init()) }

#[no_mangle]
pub extern "C" fn rust_clandb_term() { ffi_catch!((), db::term()) }

#[no_mangle]
pub extern "C" fn rust_clandb_search(id: c_int) -> *mut ClanData { ffi_catch!(null_mut(), db::search(id)) }

#[no_mangle]
pub extern "C" fn rust_clandb_searchexist(id: c_int) -> *mut ClanData { ffi_catch!(null_mut(), db::searchexist(id)) }

#[no_mangle]
pub extern "C" fn rust_clandb_searchname(s: *const c_char) -> *mut ClanData {
    if s.is_null() { return null_mut(); }
    ffi_catch!(null_mut(), db::searchname(s))
}

#[no_mangle]
pub extern "C" fn rust_clandb_name(id: c_int) -> *const c_char { ffi_catch!(b"??\0".as_ptr() as *const c_char, db::name(id)) }
