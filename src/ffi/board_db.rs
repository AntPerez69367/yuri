//! FFI bridge for board/bulletin-board database.

use std::os::raw::{c_char, c_int, c_uint};
use std::ptr::null_mut;

use crate::database::board_db::{self as db, BoardData, BnData};
use super::ffi_catch;

#[no_mangle]
pub extern "C" fn rust_boarddb_init() -> c_int { ffi_catch!(-1, db::init()) }

#[no_mangle]
pub extern "C" fn rust_boarddb_term() { ffi_catch!((), db::term()) }

#[no_mangle]
pub extern "C" fn rust_boarddb_search(id: c_int) -> *mut BoardData { ffi_catch!(null_mut(), db::search(id)) }

#[no_mangle]
pub extern "C" fn rust_boarddb_searchexist(id: c_int) -> *mut BoardData { ffi_catch!(null_mut(), db::searchexist(id)) }

#[no_mangle]
pub extern "C" fn rust_boarddb_id(s: *const c_char) -> c_uint { ffi_catch!(0, db::board_id(s)) }

#[no_mangle]
pub extern "C" fn rust_boarddb_name(id: c_int) -> *mut c_char {
    ffi_catch!(null_mut(), {
        let p = db::search(id);
        if p.is_null() { null_mut() } else { unsafe { (*p).name.as_mut_ptr() } }
    })
}
#[no_mangle]
pub extern "C" fn rust_boarddb_yname(id: c_int) -> *mut c_char {
    ffi_catch!(null_mut(), {
        let p = db::search(id);
        if p.is_null() { null_mut() } else { unsafe { (*p).yname.as_mut_ptr() } }
    })
}
#[no_mangle]
pub extern "C" fn rust_boarddb_level(id: c_int) -> c_int {
    ffi_catch!(-1, {
        let p = db::search(id);
        if p.is_null() { -1 } else { unsafe { (*p).level } }
    })
}
#[no_mangle]
pub extern "C" fn rust_boarddb_gmlevel(id: c_int) -> c_int {
    ffi_catch!(-1, {
        let p = db::search(id);
        if p.is_null() { -1 } else { unsafe { (*p).gmlevel } }
    })
}
#[no_mangle]
pub extern "C" fn rust_boarddb_path(id: c_int) -> c_int {
    ffi_catch!(-1, {
        let p = db::search(id);
        if p.is_null() { -1 } else { unsafe { (*p).path } }
    })
}
#[no_mangle]
pub extern "C" fn rust_boarddb_clan(id: c_int) -> c_int {
    ffi_catch!(-1, {
        let p = db::search(id);
        if p.is_null() { -1 } else { unsafe { (*p).clan } }
    })
}
#[no_mangle]
pub extern "C" fn rust_boarddb_sort(id: c_int) -> c_int {
    ffi_catch!(-1, {
        let p = db::search(id);
        if p.is_null() { -1 } else { unsafe { (*p).sort } }
    })
}
/// Returns single-byte boolean (char in C), not a string pointer.
#[no_mangle]
pub extern "C" fn rust_boarddb_script(id: c_int) -> c_int {
    ffi_catch!(-1, {
        let p = db::search(id);
        if p.is_null() { -1 } else { unsafe { (*p).script as c_int } }
    })
}

#[no_mangle]
pub extern "C" fn rust_bn_search(id: c_int) -> *mut BnData { ffi_catch!(null_mut(), db::bn_search(id)) }

#[no_mangle]
pub extern "C" fn rust_bn_searchexist(id: c_int) -> *mut BnData { ffi_catch!(null_mut(), db::bn_searchexist(id)) }

#[no_mangle]
pub extern "C" fn rust_bn_name(id: c_int) -> *mut c_char {
    ffi_catch!(null_mut(), {
        let p = db::bn_search(id);
        if p.is_null() { null_mut() } else { unsafe { (*p).name.as_mut_ptr() } }
    })
}
