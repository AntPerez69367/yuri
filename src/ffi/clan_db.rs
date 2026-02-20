//! FFI bridge for clan database.

use std::os::raw::{c_char, c_int};

use crate::database::clan_db::{self as db, ClanData};

#[no_mangle]
pub extern "C" fn rust_clandb_init() -> c_int { db::init() }

#[no_mangle]
pub extern "C" fn rust_clandb_term() { db::term() }

#[no_mangle]
pub extern "C" fn rust_clandb_search(id: c_int) -> *mut ClanData { db::search(id) }

#[no_mangle]
pub extern "C" fn rust_clandb_searchexist(id: c_int) -> *mut ClanData { db::searchexist(id) }

#[no_mangle]
pub extern "C" fn rust_clandb_searchname(s: *const c_char) -> *mut ClanData { db::searchname(s) }

#[no_mangle]
pub extern "C" fn rust_clandb_name(id: c_int) -> *mut c_char { db::name(id) }
