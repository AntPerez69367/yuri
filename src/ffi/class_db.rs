//! FFI bridge for class database.

use std::os::raw::{c_char, c_int, c_uint};
use std::ptr::null_mut;

use crate::database::class_db::{self as db, ClassData};

/// Exposed for C code that declares `extern struct class_data* cdata[20]`.
/// Unused in practice but required by the C headers.
#[no_mangle]
pub static mut cdata: [*mut ClassData; 20] = [null_mut(); 20];

#[no_mangle]
pub extern "C" fn rust_classdb_init(data_dir: *const c_char) -> c_int {
    db::init(data_dir)
}

#[no_mangle]
pub extern "C" fn rust_classdb_term() {
    db::term()
}

#[no_mangle]
pub extern "C" fn rust_classdb_search(id: c_int) -> *mut ClassData {
    db::search(id)
}

#[no_mangle]
pub extern "C" fn rust_classdb_searchexist(id: c_int) -> *mut ClassData {
    db::searchexist(id)
}

#[no_mangle]
pub extern "C" fn rust_classdb_level(path: c_int, lvl: c_int) -> c_uint {
    db::level(path, lvl)
}

#[no_mangle]
pub extern "C" fn rust_classdb_name(id: c_int, rank: c_int) -> *mut c_char {
    db::name(id, rank)
}

#[no_mangle]
pub extern "C" fn rust_classdb_path(id: c_int) -> c_int {
    db::path(id)
}

#[no_mangle]
pub extern "C" fn rust_classdb_chat(id: c_int) -> c_int {
    db::chat(id)
}

#[no_mangle]
pub extern "C" fn rust_classdb_icon(id: c_int) -> c_int {
    db::icon(id)
}
