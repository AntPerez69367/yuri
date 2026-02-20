//! FFI bridge for board/bulletin-board database.

use std::os::raw::{c_char, c_int, c_uint};

use crate::database::board_db::{self as db, BoardData, BnData};

#[no_mangle]
pub extern "C" fn rust_boarddb_init() -> c_int { db::init() }

#[no_mangle]
pub extern "C" fn rust_boarddb_term() { db::term() }

#[no_mangle]
pub extern "C" fn rust_boarddb_search(id: c_int) -> *mut BoardData { db::search(id) }

#[no_mangle]
pub extern "C" fn rust_boarddb_searchexist(id: c_int) -> *mut BoardData { db::searchexist(id) }

#[no_mangle]
pub extern "C" fn rust_boarddb_id(s: *const c_char) -> c_uint { db::board_id(s) }

#[no_mangle]
pub extern "C" fn rust_boarddb_name(id: c_int) -> *mut c_char {
    unsafe { (*db::search(id)).name.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn rust_boarddb_yname(id: c_int) -> *mut c_char {
    unsafe { (*db::search(id)).yname.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn rust_boarddb_level(id: c_int) -> c_int {
    unsafe { (*db::search(id)).level }
}
#[no_mangle]
pub extern "C" fn rust_boarddb_gmlevel(id: c_int) -> c_int {
    unsafe { (*db::search(id)).gmlevel }
}
#[no_mangle]
pub extern "C" fn rust_boarddb_path(id: c_int) -> c_int {
    unsafe { (*db::search(id)).path }
}
#[no_mangle]
pub extern "C" fn rust_boarddb_clan(id: c_int) -> c_int {
    unsafe { (*db::search(id)).clan }
}
#[no_mangle]
pub extern "C" fn rust_boarddb_sort(id: c_int) -> c_int {
    unsafe { (*db::search(id)).sort }
}
/// Returns single-byte boolean (char in C), not a string pointer.
#[no_mangle]
pub extern "C" fn rust_boarddb_script(id: c_int) -> c_int {
    unsafe { (*db::search(id)).script as c_int }
}

#[no_mangle]
pub extern "C" fn rust_bn_search(id: c_int) -> *mut BnData { db::bn_search(id) }

#[no_mangle]
pub extern "C" fn rust_bn_searchexist(id: c_int) -> *mut BnData { db::bn_searchexist(id) }

#[no_mangle]
pub extern "C" fn rust_bn_name(id: c_int) -> *mut c_char {
    unsafe { (*db::bn_search(id)).name.as_mut_ptr() }
}
