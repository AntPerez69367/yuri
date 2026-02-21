use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_uchar};
use std::slice;

use crate::network::crypt;

/// Whether the opcode uses dynamic encryption (client-side check).
#[no_mangle]
pub extern "C" fn rust_crypt_is_key_client(opcode: c_int) -> bool {
    crypt::is_key_client(opcode as u8)
}

/// Whether the opcode uses dynamic encryption (server-side check).
#[no_mangle]
pub extern "C" fn rust_crypt_is_key_server(opcode: c_int) -> bool {
    crypt::is_key_server(opcode as u8)
}

/// Generates an MD5 hex digest of `name` into `buffer` (must be ≥33 bytes).
/// Returns `buffer` on success, NULL if buffer too short.
#[no_mangle]
pub unsafe extern "C" fn rust_crypt_generate_hashvalues(
    name: *const c_char,
    buffer: *mut c_char,
    buflen: c_int,
) -> *mut c_char {
    if name.is_null() || buffer.is_null() || buflen < 33 {
        return std::ptr::null_mut();
    }
    let name_bytes = CStr::from_ptr(name).to_bytes();
    let buf = slice::from_raw_parts_mut(buffer as *mut u8, buflen as usize);
    if crypt::generate_hashvalues(name_bytes, buf) {
        buffer
    } else {
        std::ptr::null_mut()
    }
}

/// Builds the 1025-byte encryption lookup table from `name`.
/// Returns `table` on success, NULL on failure.
#[no_mangle]
pub unsafe extern "C" fn rust_crypt_populate_table(
    name: *const c_char,
    table: *mut c_char,
    tablelen: c_int,
) -> *mut c_char {
    if name.is_null() || table.is_null() || tablelen < 0x401 {
        return std::ptr::null_mut();
    }
    let name_bytes = CStr::from_ptr(name).to_bytes();
    let buf = slice::from_raw_parts_mut(table as *mut u8, tablelen as usize);
    if crypt::populate_table(name_bytes, buf) {
        table
    } else {
        std::ptr::null_mut()
    }
}

/// Appends 3 index bytes to `packet` and updates its length field.
/// Returns the new total packet size.
#[no_mangle]
pub unsafe extern "C" fn rust_crypt_set_packet_indexes(packet: *mut c_uchar) -> c_int {
    if packet.is_null() {
        return 0;
    }
    let psize = ((*packet.add(1) as usize) << 8) | (*packet.add(2) as usize);
    let buf_size = psize + 3 + 3; // current content + 3 trailer bytes
    let buf = slice::from_raw_parts_mut(packet, buf_size);
    crypt::set_packet_indexes(buf) as c_int
}

/// Derives a 9-byte session key into `keyout[0..10]` (NUL at [9]).
/// Returns `keyout` on success.
#[no_mangle]
pub unsafe extern "C" fn rust_crypt_generate_key2(
    packet: *mut c_uchar,
    table: *const c_char,
    keyout: *mut c_char,
    fromclient: c_int,
) -> *mut c_char {
    if packet.is_null() || table.is_null() || keyout.is_null() {
        return std::ptr::null_mut();
    }
    let psize = ((*packet.add(1) as usize) << 8) | (*packet.add(2) as usize);
    let packet_buf = slice::from_raw_parts(packet, psize + 3);
    let table_buf = slice::from_raw_parts(table as *const u8, 0x401);
    let mut key = [0u8; 10];
    crypt::generate_key2(packet_buf, table_buf, &mut key, fromclient != 0);
    let out = slice::from_raw_parts_mut(keyout as *mut u8, 10);
    out.copy_from_slice(&key);
    keyout
}

/// XOR-encrypts/decrypts `buff` in-place using a 9-byte `key`.
#[no_mangle]
pub unsafe extern "C" fn rust_crypt_dynamic(buff: *mut c_uchar, key: *const c_char) {
    if buff.is_null() || key.is_null() {
        return;
    }
    let total = ((*buff.add(1) as usize) << 8) | (*buff.add(2) as usize);
    let buf = slice::from_raw_parts_mut(buff, total);
    let key_bytes = slice::from_raw_parts(key as *const u8, 9);
    crypt::tk_crypt_dynamic(buf, key_bytes);
}

/// XOR-encrypts/decrypts `buff` using the static xor_key (passed from C config global).
#[no_mangle]
pub unsafe extern "C" fn rust_crypt_static(buff: *mut c_uchar, xor_key: *const c_char) {
    rust_crypt_dynamic(buff, xor_key);
}
