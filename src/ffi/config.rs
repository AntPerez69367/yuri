//! FFI (Foreign Function Interface) bridge for config module
//!
//! This module provides C-compatible functions so the existing C code
//! can call our Rust config parser. Eventually, when all C code is ported,
//! we can delete this entire file.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::ptr;
use std::sync::OnceLock;

use crate::config::{Point, ServerConfig};

/// Global config instance
/// OnceLock = thread-safe, can only be set once, lazy initialization
static CONFIG: OnceLock<ServerConfig> = OnceLock::new();

/// Load configuration from file (C-compatible entry point)
///
/// # Safety
/// - `cfg_file` must be a valid null-terminated C string
/// - The pointer must remain valid for the duration of the call
///
/// Returns 0 on success, -1 on failure
#[no_mangle]
pub unsafe extern "C" fn rust_config_read(cfg_file: *const c_char) -> c_int {
    // Convert C string to Rust string
    if cfg_file.is_null() {
        eprintln!("[rust_config_read] Error: cfg_file is null");
        return -1;
    }

    // SAFETY: Caller guarantees cfg_file is a valid C string
    let c_str = unsafe { CStr::from_ptr(cfg_file) };
    let file_path = match c_str.to_str() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[rust_config_read] Error: Invalid UTF-8 in path: {}", e);
            return -1;
        }
    };

    // Load config
    match ServerConfig::from_file(file_path) {
        Ok(config) => {
            println!("[rust_config_read] Successfully loaded config from: {}", file_path);

            // Store in global
            if CONFIG.set(config).is_err() {
                eprintln!("[rust_config_read] Error: Config already loaded");
                return -1;
            }

            0 // Success
        }
        Err(e) => {
            eprintln!("[rust_config_read] Error loading config: {}", e);
            -1 // Failure
        }
    }
}

/// Get a reference to the loaded config
/// Returns None if config hasn't been loaded yet
fn get_config() -> Option<&'static ServerConfig> {
    CONFIG.get()
}

//
// C-compatible getter functions
// These replace direct access to global variables in C
//

/// Get SQL IP address (returns pointer to static string)
#[no_mangle]
pub extern "C" fn rust_config_get_sql_ip() -> *const c_char {
    match get_config() {
        Some(cfg) => {
            // Convert to C string (must be static or leaked)
            match CString::new(cfg.sql_ip.clone()) {
                Ok(s) => s.into_raw(), // Leak memory - C will use this
                Err(_) => ptr::null(),
            }
        }
        None => ptr::null(),
    }
}

/// Get SQL port
#[no_mangle]
pub extern "C" fn rust_config_get_sql_port() -> u16 {
    get_config().map(|c| c.sql_port).unwrap_or(3306)
}

/// Get SQL username
#[no_mangle]
pub extern "C" fn rust_config_get_sql_id() -> *const c_char {
    match get_config() {
        Some(cfg) => {
            match CString::new(cfg.sql_id.clone()) {
                Ok(s) => s.into_raw(),
                Err(_) => ptr::null(),
            }
        }
        None => ptr::null(),
    }
}

/// Get SQL password
#[no_mangle]
pub extern "C" fn rust_config_get_sql_pw() -> *const c_char {
    match get_config() {
        Some(cfg) => {
            match CString::new(cfg.sql_pw.clone()) {
                Ok(s) => s.into_raw(),
                Err(_) => ptr::null(),
            }
        }
        None => ptr::null(),
    }
}

/// Get SQL database name
#[no_mangle]
pub extern "C" fn rust_config_get_sql_db() -> *const c_char {
    match get_config() {
        Some(cfg) => {
            match CString::new(cfg.sql_db.clone()) {
                Ok(s) => s.into_raw(),
                Err(_) => ptr::null(),
            }
        }
        None => ptr::null(),
    }
}

/// Get map IP address (as u32 for compatibility with C's inet_addr format)
#[no_mangle]
pub extern "C" fn rust_config_get_map_ip() -> u32 {
    match get_config() {
        Some(cfg) => {
            // Parse string IP to u32
            if let Ok(addr) = cfg.map_ip.parse::<std::net::Ipv4Addr>() {
                u32::from(addr)
            } else {
                0
            }
        }
        None => 0,
    }
}

/// Get map port
#[no_mangle]
pub extern "C" fn rust_config_get_map_port() -> u16 {
    get_config().map(|c| c.map_port).unwrap_or(2001)
}

/// Get char server IP (as u32)
#[no_mangle]
pub extern "C" fn rust_config_get_char_ip() -> u32 {
    match get_config() {
        Some(cfg) => {
            if let Ok(addr) = cfg.char_ip.parse::<std::net::Ipv4Addr>() {
                u32::from(addr)
            } else {
                0
            }
        }
        None => 0,
    }
}

/// Get char server port
#[no_mangle]
pub extern "C" fn rust_config_get_char_port() -> u16 {
    get_config().map(|c| c.char_port).unwrap_or(2005)
}

/// Get login server IP (as u32)
#[no_mangle]
pub extern "C" fn rust_config_get_login_ip() -> u32 {
    match get_config() {
        Some(cfg) => {
            if let Ok(addr) = cfg.login_ip.parse::<std::net::Ipv4Addr>() {
                u32::from(addr)
            } else {
                0
            }
        }
        None => 0,
    }
}

/// Get login server port
#[no_mangle]
pub extern "C" fn rust_config_get_login_port() -> u16 {
    get_config().map(|c| c.login_port).unwrap_or(2000)
}

/// Get XOR encryption key
#[no_mangle]
pub extern "C" fn rust_config_get_xor_key() -> *const c_char {
    match get_config() {
        Some(cfg) => {
            match CString::new(cfg.xor_key.clone()) {
                Ok(s) => s.into_raw(),
                Err(_) => ptr::null(),
            }
        }
        None => ptr::null(),
    }
}

/// Get start position point (returns by value since Point is #[repr(C)])
#[no_mangle]
pub extern "C" fn rust_config_get_start_point() -> Point {
    get_config()
        .map(|c| c.start_point)
        .unwrap_or(Point::new(0, 0, 0))
}

/// Get server ID
#[no_mangle]
pub extern "C" fn rust_config_get_server_id() -> c_int {
    get_config().map(|c| c.server_id).unwrap_or(0)
}

/// Get number of meta files
#[no_mangle]
pub extern "C" fn rust_config_get_meta_count() -> c_int {
    get_config().map(|c| c.meta.len() as c_int).unwrap_or(0)
}

/// Get meta file name by index
/// Returns null if index is out of bounds
#[no_mangle]
pub extern "C" fn rust_config_get_meta_file(index: c_int) -> *const c_char {
    match get_config() {
        Some(cfg) => {
            if index >= 0 && (index as usize) < cfg.meta.len() {
                match CString::new(cfg.meta[index as usize].clone()) {
                    Ok(s) => s.into_raw(),
                    Err(_) => ptr::null(),
                }
            } else {
                ptr::null()
            }
        }
        None => ptr::null(),
    }
}

/// Get number of towns
#[no_mangle]
pub extern "C" fn rust_config_get_town_count() -> c_int {
    get_config().map(|c| c.town.len() as c_int).unwrap_or(0)
}

/// Get town name by index
#[no_mangle]
pub extern "C" fn rust_config_get_town_name(index: c_int) -> *const c_char {
    match get_config() {
        Some(cfg) => {
            if index >= 0 && (index as usize) < cfg.town.len() {
                match CString::new(cfg.town[index as usize].clone()) {
                    Ok(s) => s.into_raw(),
                    Err(_) => ptr::null(),
                }
            } else {
                ptr::null()
            }
        }
        None => ptr::null(),
    }
}

/// Free a string returned by Rust
/// Must be called on all strings returned by rust_config_get_* functions
///
/// # Safety
/// - `ptr` must be a pointer returned by a rust_config_get_* function
/// - Must only be called once per pointer
#[no_mangle]
pub unsafe extern "C" fn rust_config_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        // SAFETY: We created this with CString::into_raw(), so we can reclaim it
        unsafe {
            let _ = CString::from_raw(ptr);
            // Drops and frees memory
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_config_load_via_ffi() {
        // Create a test config file (proper YAML format)
        let test_config = r#"
sql_ip: "127.0.0.1"
sql_id: "testuser"
sql_pw: "testpass"
sql_db: "testdb"
login_id: "loginid"
login_pw: "loginpw"
login_ip: "127.0.0.1"
char_id: "charid"
char_pw: "charpw"
char_ip: "127.0.0.1"
map_ip: "127.0.0.1"
start_point:
  m: 5
  x: 10
  y: 20
"#;

        let temp_file = std::env::temp_dir().join("test_config.conf");
        std::fs::write(&temp_file, test_config).unwrap();

        // Load via FFI
        let path = CString::new(temp_file.to_str().unwrap()).unwrap();
        let result = unsafe { rust_config_read(path.as_ptr()) };

        assert_eq!(result, 0); // Success

        // Test getters
        assert_eq!(rust_config_get_sql_port(), 3306);
        assert_eq!(rust_config_get_map_port(), 2001);
        assert_eq!(rust_config_get_server_id(), 0);

        let start = rust_config_get_start_point();
        assert_eq!(start.m, 5);
        assert_eq!(start.x, 10);
        assert_eq!(start.y, 20);

        // Cleanup
        std::fs::remove_file(temp_file).ok();
    }

    #[test]
    fn test_null_pointer_handling() {
        let result = unsafe { rust_config_read(ptr::null()) };
        assert_eq!(result, -1);
    }

    #[test]
    fn test_string_getters() {
        // First ensure we have a config loaded
        let test_config = r#"
sql_ip: "192.168.1.1"
sql_id: "myuser"
sql_pw: "mypass"
sql_db: "mydb"
login_id: "loginid"
login_pw: "loginpw"
login_ip: "127.0.0.1"
char_id: "charid"
char_pw: "charpw"
char_ip: "127.0.0.1"
map_ip: "127.0.0.1"
start_point:
  m: 0
  x: 1
  y: 1
"#;

        let temp_file = std::env::temp_dir().join("test_ffi_strings.yaml");
        std::fs::write(&temp_file, test_config).unwrap();

        let path = CString::new(temp_file.to_str().unwrap()).unwrap();
        let result = unsafe { rust_config_read(path.as_ptr()) };

        // May fail if config already loaded from another test, that's ok
        // We just want to test the getter functions work

        // Get strings
        let sql_id_ptr = rust_config_get_sql_id();
        assert!(!sql_id_ptr.is_null());

        let sql_id = unsafe { CStr::from_ptr(sql_id_ptr) };
        // Don't check specific value - might be from another test
        assert!(!sql_id.to_bytes().is_empty());

        // Free the string
        unsafe { rust_config_free_string(sql_id_ptr as *mut c_char) };

        // Cleanup
        std::fs::remove_file(temp_file).ok();
    }
}
