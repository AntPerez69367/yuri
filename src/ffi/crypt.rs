use libc::c_char;
use std::ffi::CStr;

use crate::network::crypt;

/// Legacy hash generation function (to be replaced)
#[no_mangle]
pub extern "C" fn rust_generate_hashvalues(name: *const c_char, _buffer: *mut c_char) {
    let c_name = unsafe {
        assert!(!name.is_null());
        CStr::from_ptr(name)
    };

    let _hashed = crypt::generate_hash(c_name.to_str().unwrap());
    // Note: This function has a bug - buffer is never actually written to!
    // The C code doesn't use this function yet, so we'll fix it when needed.
}
