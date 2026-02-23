//! FFI (Foreign Function Interface) bridge for config module
//!
//! This module provides C-compatible functions so the existing C code
//! can call our Rust config parser. Eventually, when all C code is ported,
//! we can delete this entire file.

use std::ffi::{CStr, CString};
use std::net::Ipv4Addr;
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

            // Automatically populate C global variables
            unsafe {
                rust_config_populate_c_globals();
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

/// C town_data struct (matches the C definition)
#[repr(C)]
struct TownData {
    name: [c_char; 32],
}

/// Populate C global variables from Rust config
/// Call this after rust_config_read() to populate the legacy C globals
///
/// # Safety
/// C global variables must be accessible and have sufficient buffer space
#[no_mangle]
pub unsafe extern "C" fn rust_config_populate_c_globals() {
    // Import the C global variables
    extern "C" {
        // SQL config
        static mut sql_id: [c_char; 32];
        static mut sql_pw: [c_char; 32];
        static mut sql_ip: [c_char; 32];
        static mut sql_db: [c_char; 32];
        static mut sql_port: c_int;

        // Server IDs and passwords (33 = 32 hex chars + null terminator)
        static mut login_id: [c_char; 33];
        static mut login_pw: [c_char; 33];
        static mut char_id: [c_char; 33];
        static mut char_pw: [c_char; 33];

        // Server IPs and ports
        static mut login_ip: c_int;
        static mut login_port: c_int;
        static mut char_ip: c_int;
        static mut char_port: c_int;
        static mut map_ip: u32;
        static mut map_port: u32;

        // XOR encryption key
        static mut xor_key: [c_char; 10];  // 9 chars + null terminator

        // Start position
        static mut start_pos: Point;

        // Server settings
        static mut serverid: c_int;
        static mut require_reg: c_int;
        static mut nex_version: c_int;
        static mut nex_deep: c_int;
        static mut save_time: c_int;
        static mut xp_rate: c_int;
        static mut d_rate: c_int;

        // Meta files
        static mut meta_file: [[c_char; 256]; 20];  // META_MAX = 20
        static mut metamax: c_int;

        // Towns
        static mut towns: [TownData; 255];  // TOWN_MAX = 255
        static mut town_n: c_int;
    }

    // Helper function to copy string to C buffer using raw pointers
    unsafe fn copy_string_to_buffer<const N: usize>(ptr: *const c_char, buffer_ptr: *mut [c_char; N]) {
        if !ptr.is_null() {
            let cstr = CStr::from_ptr(ptr);
            let bytes = cstr.to_bytes();
            let len = bytes.len().min(N - 1);
            ptr::copy_nonoverlapping(bytes.as_ptr(), buffer_ptr as *mut u8, len);
            (*(buffer_ptr as *mut [c_char; N]))[len] = 0; // Null terminate
            rust_config_free_string(ptr as *mut c_char);
        }
    }

    unsafe {
        // SQL configuration
        copy_string_to_buffer(rust_config_get_sql_id(), ptr::addr_of_mut!(sql_id));
        copy_string_to_buffer(rust_config_get_sql_pw(), ptr::addr_of_mut!(sql_pw));
        copy_string_to_buffer(rust_config_get_sql_ip(), ptr::addr_of_mut!(sql_ip));
        copy_string_to_buffer(rust_config_get_sql_db(), ptr::addr_of_mut!(sql_db));
        sql_port = rust_config_get_sql_port() as c_int;

        // Server authentication
        let cfg = get_config();
        if let Some(config) = cfg {
            // Login server
            if let Ok(s) = CString::new(config.login_id.clone()) {
                copy_string_to_buffer(s.into_raw(), ptr::addr_of_mut!(login_id));
            }
            if let Ok(s) = CString::new(config.login_pw.clone()) {
                copy_string_to_buffer(s.into_raw(), ptr::addr_of_mut!(login_pw));
            }
            login_port = config.login_port as c_int;
            if let Ok(addr) = config.login_ip.parse::<Ipv4Addr>() {
                // inet_addr returns IP in little-endian format on x86
                login_ip = u32::from_le_bytes(addr.octets()) as c_int;
            }

            // Char server
            if let Ok(s) = CString::new(config.char_id.clone()) {
                copy_string_to_buffer(s.into_raw(), ptr::addr_of_mut!(char_id));
            }
            if let Ok(s) = CString::new(config.char_pw.clone()) {
                copy_string_to_buffer(s.into_raw(), ptr::addr_of_mut!(char_pw));
            }
            char_port = config.char_port as c_int;
            if let Ok(addr) = config.char_ip.parse::<Ipv4Addr>() {
                // inet_addr returns IP in little-endian format on x86
                char_ip = u32::from_le_bytes(addr.octets()) as c_int;
            }

            // Map server
            map_port = config.map_port as u32;
            if let Ok(addr) = config.map_ip.parse::<Ipv4Addr>() {
                // inet_addr returns IP in little-endian format on x86
                map_ip = u32::from_le_bytes(addr.octets());
            }

            // XOR key
            if let Ok(s) = CString::new(config.xor_key.clone()) {
                copy_string_to_buffer(s.into_raw(), ptr::addr_of_mut!(xor_key));
            }

            // Start position
            start_pos = config.start_point;

            // Server settings
            serverid = config.server_id as c_int;
            require_reg = config.require_reg as c_int;
            nex_version = config.version as c_int;
            nex_deep = config.deep as c_int;
            save_time = (config.save_time * 1000) as c_int;  // Convert to milliseconds
            xp_rate = config.xprate as c_int;
            d_rate = config.droprate as c_int;

            // Meta files
            metamax = config.meta.len().min(20) as c_int;
            for (i, meta) in config.meta.iter().take(20).enumerate() {
                if let Ok(s) = CString::new(meta.clone()) {
                    let bytes = s.as_bytes_with_nul();
                    let len = bytes.len().min(256);
                    let dest = ptr::addr_of_mut!(meta_file[i]) as *mut u8;
                    ptr::copy_nonoverlapping(bytes.as_ptr(), dest, len);
                }
            }

            // Towns
            town_n = config.town.len().min(255) as c_int;
            for (i, town) in config.town.iter().take(255).enumerate() {
                if let Ok(s) = CString::new(town.clone()) {
                    let bytes = s.as_bytes();
                    let len = bytes.len().min(31);
                    let dest = ptr::addr_of_mut!(towns[i].name) as *mut u8;
                    ptr::copy_nonoverlapping(bytes.as_ptr(), dest, len);
                    let name_ptr = ptr::addr_of_mut!(towns[i].name) as *mut c_char;
                    *name_ptr.add(len) = 0;
                }
            }
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
        let _result = unsafe { rust_config_read(path.as_ptr()) };

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
