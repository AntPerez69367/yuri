//! FFI bridge for magic/spell database.

use std::os::raw::{c_char, c_int};

use crate::database::magic_db::{self as db, MagicData};

static EMPTY: &[u8] = b"\0";

#[no_mangle]
pub extern "C" fn rust_magicdb_init() -> c_int { db::init() }

#[no_mangle]
pub extern "C" fn rust_magicdb_term() { db::term() }

#[no_mangle]
pub extern "C" fn rust_magicdb_search(id: c_int) -> *mut MagicData { db::search(id) }

#[no_mangle]
pub extern "C" fn rust_magicdb_searchexist(id: c_int) -> *mut MagicData { db::searchexist(id) }

#[no_mangle]
pub extern "C" fn rust_magicdb_searchname(s: *const c_char) -> *mut MagicData { db::searchname(s) }

#[no_mangle]
pub extern "C" fn rust_magicdb_id(s: *const c_char) -> c_int { db::id(s) }

#[no_mangle]
pub extern "C" fn rust_magicdb_name(id: c_int) -> *mut c_char {
    unsafe { (*db::search(id)).name.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn rust_magicdb_yname(id: c_int) -> *mut c_char {
    unsafe { (*db::search(id)).yname.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn rust_magicdb_question(id: c_int) -> *mut c_char {
    unsafe { (*db::search(id)).question.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn rust_magicdb_type(id: c_int) -> c_int {
    unsafe { (*db::search(id)).typ }
}
#[no_mangle]
pub extern "C" fn rust_magicdb_dispel(id: c_int) -> c_int {
    unsafe { (*db::search(id)).dispell as c_int }
}
#[no_mangle]
pub extern "C" fn rust_magicdb_aether(id: c_int) -> c_int {
    unsafe { (*db::search(id)).aether as c_int }
}
#[no_mangle]
pub extern "C" fn rust_magicdb_mute(id: c_int) -> c_int {
    unsafe { (*db::search(id)).mute as c_int }
}
#[no_mangle]
pub extern "C" fn rust_magicdb_canfail(id: c_int) -> c_int {
    unsafe { (*db::search(id)).canfail as c_int }
}
#[no_mangle]
pub extern "C" fn rust_magicdb_alignment(id: c_int) -> c_int {
    unsafe { (*db::search(id)).alignment as c_int }
}
#[no_mangle]
pub extern "C" fn rust_magicdb_ticker(id: c_int) -> c_int {
    unsafe { (*db::search(id)).ticker as c_int }
}
/// Takes spell name string, returns level.
#[no_mangle]
pub extern "C" fn rust_magicdb_level(s: *const c_char) -> c_int { db::level_by_name(s) }

/// Script fields are always empty (never populated in original game).
#[no_mangle]
pub extern "C" fn rust_magicdb_script(_id: c_int) -> *mut c_char {
    EMPTY.as_ptr() as *mut c_char
}
#[no_mangle]
pub extern "C" fn rust_magicdb_script2(_id: c_int) -> *mut c_char {
    EMPTY.as_ptr() as *mut c_char
}
#[no_mangle]
pub extern "C" fn rust_magicdb_script3(_id: c_int) -> *mut c_char {
    EMPTY.as_ptr() as *mut c_char
}
