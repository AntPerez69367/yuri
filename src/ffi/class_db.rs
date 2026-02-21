//! FFI bridge for class database.

use std::os::raw::{c_char, c_int, c_uint};
use std::ptr::null_mut;
use std::sync::Arc;

use crate::database::class_db::{self as db, ClassData};

/// Catches panics from an FFI-exposed function body and returns a safe default
/// instead of unwinding into C (which is undefined behavior).
macro_rules! ffi_catch {
    ($default:expr, $body:expr) => {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body)) {
            Ok(v) => v,
            Err(_) => $default,
        }
    };
}

/// Exposed for C code that declares `extern struct class_data* cdata[20]`.
/// Unused in practice but required by the C headers.
#[no_mangle]
pub static mut cdata: [*mut ClassData; 20] = [null_mut(); 20];

#[no_mangle]
pub extern "C" fn rust_classdb_init(data_dir: *const c_char) -> c_int {
    ffi_catch!(-1, db::init(data_dir))
}

#[no_mangle]
pub extern "C" fn rust_classdb_term() {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(db::term));
}

/// Returns a raw pointer derived from an Arc::into_raw so the ClassData
/// allocation outlives any HashMap clear (e.g. term()). The C caller must
/// not free this pointer directly; call rust_classdb_free when done.
#[no_mangle]
pub extern "C" fn rust_classdb_search(id: c_int) -> *mut ClassData {
    ffi_catch!(null_mut(), Arc::into_raw(db::search(id)) as *mut ClassData)
}

#[no_mangle]
pub extern "C" fn rust_classdb_searchexist(id: c_int) -> *mut ClassData {
    ffi_catch!(null_mut(), match db::searchexist(id) {
        Some(arc) => Arc::into_raw(arc) as *mut ClassData,
        None => null_mut(),
    })
}

/// Decrements the Arc reference count for a pointer returned by
/// rust_classdb_search or rust_classdb_searchexist.
#[no_mangle]
pub extern "C" fn rust_classdb_free(ptr: *mut ClassData) {
    if !ptr.is_null() {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // SAFETY: ptr was produced by Arc::into_raw with ClassData.
            unsafe { drop(Arc::from_raw(ptr as *const ClassData)); }
        }));
    }
}

#[no_mangle]
pub extern "C" fn rust_classdb_level(path: c_int, lvl: c_int) -> c_uint {
    ffi_catch!(0, db::level(path, lvl))
}

/// Returns a caller-owned C string. Must be freed with rust_classdb_free_name().
#[no_mangle]
pub extern "C" fn rust_classdb_name(id: c_int, rank: c_int) -> *mut c_char {
    ffi_catch!(null_mut(), db::name(id, rank))
}

/// Frees a string returned by rust_classdb_name.
#[no_mangle]
pub extern "C" fn rust_classdb_free_name(ptr: *mut c_char) {
    if !ptr.is_null() {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // SAFETY: ptr was produced by CString::into_raw.
            unsafe { drop(std::ffi::CString::from_raw(ptr)); }
        }));
    }
}

#[no_mangle]
pub extern "C" fn rust_classdb_path(id: c_int) -> c_int {
    ffi_catch!(0, db::path(id))
}

#[no_mangle]
pub extern "C" fn rust_classdb_chat(id: c_int) -> c_int {
    ffi_catch!(0, db::chat(id))
}

#[no_mangle]
pub extern "C" fn rust_classdb_icon(id: c_int) -> c_int {
    ffi_catch!(0, db::icon(id))
}
