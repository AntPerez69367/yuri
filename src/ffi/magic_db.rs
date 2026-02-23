//! FFI bridge for magic/spell database.

use std::os::raw::{c_char, c_int};
use std::ptr::null_mut;

use crate::database::magic_db::{self as db, MagicData};

static EMPTY: &[u8] = b"\0";

#[no_mangle]
pub extern "C" fn rust_magicdb_init() -> c_int { ffi_catch!(-1, db::init()) }

#[no_mangle]
pub extern "C" fn rust_magicdb_term() { ffi_catch!((), db::term()) }

#[no_mangle]
pub extern "C" fn rust_magicdb_search(id: c_int) -> *mut MagicData { ffi_catch!(null_mut(), db::search(id)) }

#[no_mangle]
pub extern "C" fn rust_magicdb_searchexist(id: c_int) -> *mut MagicData { ffi_catch!(null_mut(), db::searchexist(id)) }

#[no_mangle]
pub extern "C" fn rust_magicdb_searchname(s: *const c_char) -> *mut MagicData { ffi_catch!(null_mut(), db::searchname(s)) }

#[no_mangle]
pub extern "C" fn rust_magicdb_id(s: *const c_char) -> c_int { ffi_catch!(0, db::id(s)) }

#[no_mangle]
pub extern "C" fn rust_magicdb_name(id: c_int) -> *mut c_char {
    ffi_catch!(null_mut(), {
        let p = db::search(id);
        if p.is_null() { null_mut() } else { unsafe { (*p).name.as_mut_ptr() } }
    })
}
#[no_mangle]
pub extern "C" fn rust_magicdb_yname(id: c_int) -> *mut c_char {
    ffi_catch!(null_mut(), {
        let p = db::search(id);
        if p.is_null() { null_mut() } else { unsafe { (*p).yname.as_mut_ptr() } }
    })
}
#[no_mangle]
pub extern "C" fn rust_magicdb_question(id: c_int) -> *mut c_char {
    ffi_catch!(null_mut(), {
        let p = db::search(id);
        if p.is_null() { null_mut() } else { unsafe { (*p).question.as_mut_ptr() } }
    })
}
#[no_mangle]
pub extern "C" fn rust_magicdb_type(id: c_int) -> c_int {
    ffi_catch!(0, {
        let p = db::search(id);
        if p.is_null() { 0 } else { unsafe { (*p).typ } }
    })
}
#[no_mangle]
pub extern "C" fn rust_magicdb_dispel(id: c_int) -> c_int {
    ffi_catch!(0, {
        let p = db::search(id);
        if p.is_null() { 0 } else { unsafe { (*p).dispell as c_int } }
    })
}
#[no_mangle]
pub extern "C" fn rust_magicdb_aether(id: c_int) -> c_int {
    ffi_catch!(0, {
        let p = db::search(id);
        if p.is_null() { 0 } else { unsafe { (*p).aether as c_int } }
    })
}
#[no_mangle]
pub extern "C" fn rust_magicdb_mute(id: c_int) -> c_int {
    ffi_catch!(0, {
        let p = db::search(id);
        if p.is_null() { 0 } else { unsafe { (*p).mute as c_int } }
    })
}
#[no_mangle]
pub extern "C" fn rust_magicdb_canfail(id: c_int) -> c_int {
    ffi_catch!(0, {
        let p = db::search(id);
        if p.is_null() { 0 } else { unsafe { (*p).canfail as c_int } }
    })
}
#[no_mangle]
pub extern "C" fn rust_magicdb_alignment(id: c_int) -> c_int {
    ffi_catch!(0, {
        let p = db::search(id);
        if p.is_null() { 0 } else { unsafe { (*p).alignment as c_int } }
    })
}
#[no_mangle]
pub extern "C" fn rust_magicdb_ticker(id: c_int) -> c_int {
    ffi_catch!(0, {
        let p = db::search(id);
        if p.is_null() { 0 } else { unsafe { (*p).ticker as c_int } }
    })
}
/// Takes spell name string, returns level.
#[no_mangle]
pub extern "C" fn rust_magicdb_level(s: *const c_char) -> c_int { ffi_catch!(0, db::level_by_name(s)) }

/// Script fields are always empty (never populated in original game).
#[no_mangle]
pub extern "C" fn rust_magicdb_script(_id: c_int) -> *const c_char {
    EMPTY.as_ptr() as *const c_char
}
#[no_mangle]
pub extern "C" fn rust_magicdb_script2(_id: c_int) -> *const c_char {
    EMPTY.as_ptr() as *const c_char
}
#[no_mangle]
pub extern "C" fn rust_magicdb_script3(_id: c_int) -> *const c_char {
    EMPTY.as_ptr() as *const c_char
}
