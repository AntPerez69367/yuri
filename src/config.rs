//! Server configuration module
//!
//! Parses and manages server configuration from YAML files.
//! This replaces the legacy C config.c implementation with a type-safe Rust version.
//!
//! Uses serde_yaml for automatic parsing - just define the struct and serde handles
//! all the parsing, validation, and type conversion!

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Maximum number of meta files that can be loaded
pub const META_MAX: usize = 20;

/// Maximum number of towns supported
pub const TOWN_MAX: usize = 255;

pub use crate::common::types::Point;

/// Main server configuration
///
/// This struct is automatically parsed from YAML by serde.
/// Just add a field here, and serde handles the rest!
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    // ============================================
    // Login Server Configuration
    // ============================================
    /// Authentication token for login server (32 char max)
    pub login_id: String,

    /// Authentication password for login server (32 char max)
    pub login_pw: String,

    /// Login server IP address
    pub login_ip: String,

    #[serde(default = "default_login_port")]
    pub login_port: u16,

    // ============================================
    // Character Server Configuration
    // ============================================
    /// Authentication token for character server (32 char max)
    pub char_id: String,

    /// Authentication password for character server (32 char max)
    pub char_pw: String,

    /// Character server IP address
    pub char_ip: String,

    #[serde(default = "default_char_port")]
    pub char_port: u16,

    // ============================================
    // Map Server Configuration
    // ============================================
    /// Public IP address for map server
    pub map_ip: String,

    #[serde(default = "default_map_port")]
    pub map_port: u16,

    #[serde(default)]
    pub server_id: i32,

    // ============================================
    // Encryption & Security
    // ============================================
    /// XOR encryption key (max 9 chars)
    #[serde(default)]
    pub xor_key: String,

    // ============================================
    // Game Settings
    // ============================================
    /// Starting position for new characters
    pub start_point: Point,

    /// Required client version
    #[serde(default = "default_version")]
    pub version: i32,

    /// Required client patch level
    #[serde(default)]
    pub deep: i32,

    /// Require account registration (0 = no, 1 = yes)
    #[serde(default = "default_require_reg")]
    pub require_reg: i32,

    /// Save interval in seconds
    #[serde(default = "default_save_time")]
    pub save_time: i32,

    /// XP rate multiplier
    #[serde(default = "default_xprate")]
    pub xprate: i32,

    /// Drop rate multiplier
    #[serde(default = "default_droprate")]
    pub droprate: i32,

    // ============================================
    // Meta Files & Towns
    // ============================================
    /// List of meta files to send to client on login
    #[serde(default)]
    pub meta: Vec<String>,

    /// List of town names (for hero list display)
    #[serde(default)]
    pub town: Vec<String>,

    // ============================================
    // Directory Paths
    // ============================================
    #[serde(default = "default_data_dir")]
    pub data_dir: String,

    #[serde(default = "default_lua_dir")]
    pub lua_dir: String,

    #[serde(default = "default_maps_dir")]
    pub maps_dir: String,

    #[serde(default = "default_meta_dir")]
    pub meta_dir: String,
}

// ============================================
// Default value functions
// These are called by serde when a field is missing
// ============================================

fn default_login_port() -> u16 {
    2000
}

fn default_char_port() -> u16 {
    2005
}

fn default_map_port() -> u16 {
    2001
}

fn default_version() -> i32 {
    750
}

fn default_require_reg() -> i32 {
    1
}

fn default_save_time() -> i32 {
    60
}

fn default_xprate() -> i32 {
    10
}

fn default_droprate() -> i32 {
    1
}

fn default_data_dir() -> String {
    "./data/".to_string()
}

fn default_lua_dir() -> String {
    "./data/lua/".to_string()
}

fn default_maps_dir() -> String {
    "./data/maps/".to_string()
}

fn default_meta_dir() -> String {
    "./data/meta/".to_string()
}

impl ServerConfig {
    /// Load configuration from a YAML file
    ///
    /// # Example
    /// ```no_run
    /// use yuri::config::ServerConfig;
    ///
    /// let config = ServerConfig::from_file("conf/server.yaml")
    ///     .expect("Failed to load config");
    /// println!("Map IP: {}", config.map_ip);
    /// ```
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        // Read file contents
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        // Parse YAML - serde does ALL the work!
        let config: ServerConfig = serde_yaml::from_str(&contents)
            .with_context(|| format!("Failed to parse YAML in {}", path.display()))?;

        // Validate the config
        config.validate()?;

        Ok(config)
    }

    /// Parse configuration from a YAML string
    ///
    /// Useful for testing
    pub fn from_str(contents: &str) -> Result<Self> {
        let config: ServerConfig = serde_yaml::from_str(contents)
            .context("Failed to parse YAML")?;

        config.validate()?;

        Ok(config)
    }

    /// Validate configuration values
    ///
    /// Checks that required fields are set and values are reasonable
    fn validate(&self) -> Result<()> {
        // Check required fields aren't empty
        anyhow::ensure!(!self.map_ip.is_empty(), "map_ip cannot be empty");
        anyhow::ensure!(!self.char_ip.is_empty(), "char_ip cannot be empty");
        anyhow::ensure!(!self.login_ip.is_empty(), "login_ip cannot be empty");

        // Check meta files count
        anyhow::ensure!(
            self.meta.len() <= META_MAX,
            "Too many meta files: {} (max {})",
            self.meta.len(),
            META_MAX
        );

        // Check towns count
        anyhow::ensure!(
            self.town.len() <= TOWN_MAX,
            "Too many towns: {} (max {})",
            self.town.len(),
            TOWN_MAX
        );

        // Check XOR key length (max 9 chars + null terminator in C)
        if !self.xor_key.is_empty() {
            anyhow::ensure!(
                self.xor_key.len() <= 9,
                "xor_key too long: {} chars (max 9)",
                self.xor_key.len()
            );
        }

        Ok(())
    }

    /// Save configuration to a YAML file
    ///
    /// Useful for generating config templates or saving modified configs
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let yaml = serde_yaml::to_string(&self)
            .context("Failed to serialize config to YAML")?;

        fs::write(path.as_ref(), yaml)
            .with_context(|| format!("Failed to write config to {}", path.as_ref().display()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a minimal valid config
    fn minimal_config() -> &'static str {
        r#"
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
"#
    }

    #[test]
    fn test_point_creation() {
        let point = Point::new(1, 100, 200);
        assert_eq!(point.m, 1);
        assert_eq!(point.x, 100);
        assert_eq!(point.y, 200);
    }

    #[test]
    fn test_minimal_config() {
        let config = ServerConfig::from_str(minimal_config()).unwrap();
        assert_eq!(config.start_point, Point::new(0, 1, 1));
    }

    #[test]
    fn test_default_values() {
        let config = ServerConfig::from_str(minimal_config()).unwrap();

        // All these should have defaults
        assert_eq!(config.login_port, 2000);
        assert_eq!(config.char_port, 2005);
        assert_eq!(config.map_port, 2001);
        assert_eq!(config.server_id, 0);
        assert_eq!(config.version, 750);
        assert_eq!(config.deep, 0);
        assert_eq!(config.require_reg, 1);
        assert_eq!(config.save_time, 60);
        assert_eq!(config.xprate, 10);
        assert_eq!(config.droprate, 1);
    }

    #[test]
    fn test_custom_ports() {
        let config_str = r#"
login_id: "loginid"
login_pw: "loginpw"
login_ip: "127.0.0.1"
login_port: 3000

char_id: "charid"
char_pw: "charpw"
char_ip: "127.0.0.1"
char_port: 3005

map_ip: "127.0.0.1"
map_port: 3001

start_point:
  m: 0
  x: 1
  y: 1
"#;

        let config = ServerConfig::from_str(config_str).unwrap();
        assert_eq!(config.login_port, 3000);
        assert_eq!(config.char_port, 3005);
        assert_eq!(config.map_port, 3001);
    }

    #[test]
    fn test_meta_files_as_list() {
        let config_str = r#"
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

meta:
  - RidableAnimals
  - CharicInfo0
  - ItemInfo0
"#;

        let config = ServerConfig::from_str(config_str).unwrap();
        assert_eq!(config.meta.len(), 3);
        assert_eq!(config.meta[0], "RidableAnimals");
        assert_eq!(config.meta[1], "CharicInfo0");
        assert_eq!(config.meta[2], "ItemInfo0");
    }

    #[test]
    fn test_towns_as_list() {
        let config_str = r#"
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

town:
  - Town1
  - Town2
  - Town3
"#;

        let config = ServerConfig::from_str(config_str).unwrap();
        assert_eq!(config.town.len(), 3);
        assert_eq!(config.town[0], "Town1");
        assert_eq!(config.town[2], "Town3");
    }

    #[test]
    fn test_missing_required_field() {
        let config_str = r#"
"#;

        let result = ServerConfig::from_str(config_str);
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{:?}", err);
        assert!(err_msg.contains("missing field") || err_msg.contains("map_ip") || err_msg.contains("login_ip"));
    }

    #[test]
    fn test_invalid_yaml() {
        let config_str = r#"
login_id: [this is not valid yaml
"#;

        let result = ServerConfig::from_str(config_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_type() {
        let config_str = r#"
login_id: "loginid"
login_pw: "loginpw"
login_ip: "127.0.0.1"
char_id: "charid"
char_pw: "charpw"
char_ip: "127.0.0.1"
map_ip: "127.0.0.1"
map_port: "not_a_number"
start_point:
  m: 0
  x: 1
  y: 1
"#;

        let result = ServerConfig::from_str(config_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_too_many_meta_files() {
        let mut config_str = String::from(minimal_config());
        config_str.push_str("\nmeta:\n");

        // Add 21 meta files (over the limit of 20)
        for i in 0..21 {
            config_str.push_str(&format!("  - MetaFile{}\n", i));
        }

        let result = ServerConfig::from_str(&config_str);
        assert!(result.is_err());

        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("Too many meta files"));
    }

    #[test]
    fn test_xor_key_too_long() {
        let config_str = r#"
login_id: "loginid"
login_pw: "loginpw"
login_ip: "127.0.0.1"
char_id: "charid"
char_pw: "charpw"
char_ip: "127.0.0.1"
map_ip: "127.0.0.1"
xor_key: "ThisIsWayTooLong123456789"
start_point:
  m: 0
  x: 1
  y: 1
"#;

        let result = ServerConfig::from_str(config_str);
        assert!(result.is_err());

        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("xor_key too long"));
    }

    #[test]
    fn test_full_config() {
        let config_str = r#"
# Full server configuration
login_id: "2d8ae0cc4ef940848d885e2493cd8d8a"
login_pw: "d6ed86ed53a749639b215436916c8c1e"
login_ip: "127.0.0.1"
login_port: 2000

char_id: "34d6adef1e3e4ba69f756247a58a8878"
char_pw: "d3adfd7f9e714bb7af2d4c8b613d2104"
char_ip: "127.0.0.1"
char_port: 2005

map_ip: "127.0.0.1"
map_port: 2001
server_id: 0

xor_key: "TestKey"

start_point:
  m: 0
  x: 1
  y: 1

version: 750
deep: 0
require_reg: 0
save_time: 60
xprate: 10
droprate: 1

meta:
  - RidableAnimals
  - CharicInfo0
  - CharicInfo1
  - ItemInfo0
  - ItemInfo1

town:
  - Town1
  - Town2
  - Town3
  - Town4
  - Town5
  - Town6
"#;

        let config = ServerConfig::from_str(config_str).unwrap();

        // Verify all fields
        assert_eq!(config.xor_key, "TestKey");
        assert_eq!(config.meta.len(), 5);
        assert_eq!(config.town.len(), 6);
        assert_eq!(config.start_point, Point::new(0, 1, 1));
    }

    #[test]
    fn test_save_and_load() {
        let config = ServerConfig::from_str(minimal_config()).unwrap();

        let temp_file = std::env::temp_dir().join("test_save_config.yaml");

        // Save config
        config.save(&temp_file).unwrap();

        // Load it back
        let loaded = ServerConfig::from_file(&temp_file).unwrap();
        assert_eq!(config.start_point, loaded.start_point);

        // Cleanup
        std::fs::remove_file(temp_file).ok();
    }
}

// ─── Public API exports ────────────────────────────────────────────────────

use std::ffi::{CStr, CString};
use std::net::Ipv4Addr;
use std::ptr;
use std::sync::OnceLock;

/// Global config instance
static CONFIG: OnceLock<ServerConfig> = OnceLock::new();

fn get_config() -> Option<&'static ServerConfig> {
    CONFIG.get()
}

/// Public accessor for the loaded config — used by game modules (e.g. scripting).
pub fn config() -> &'static ServerConfig {
    CONFIG.get().expect("config not loaded — rust_config_read must be called first")
}

pub unsafe fn rust_config_read(cfg_file: *const i8) -> i32 {
    if cfg_file.is_null() {
        tracing::error!("[rust_config_read] cfg_file is null");
        return -1;
    }

    let c_str = unsafe { CStr::from_ptr(cfg_file) };
    let file_path = match c_str.to_str() {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("[rust_config_read] invalid UTF-8 in path: {}", e);
            return -1;
        }
    };

    match ServerConfig::from_file(file_path) {
        Ok(config) => {
            tracing::info!("[rust_config_read] loaded config from: {}", file_path);

            if CONFIG.set(config).is_err() {
                tracing::error!("[rust_config_read] config already loaded");
                return -1;
            }

            unsafe { rust_config_populate_c_globals(); }
            0
        }
        Err(e) => {
            tracing::error!("[rust_config_read] failed to load config: {}", e);
            -1
        }
    }
}

pub fn rust_config_get_map_ip() -> u32 {
    match get_config() {
        Some(cfg) => {
            if let Ok(addr) = cfg.map_ip.parse::<std::net::Ipv4Addr>() {
                u32::from(addr)
            } else { 0 }
        }
        None => 0,
    }
}

pub fn rust_config_get_map_port() -> u16 {
    get_config().map(|c| c.map_port).unwrap_or(2001)
}

pub fn rust_config_get_char_ip() -> u32 {
    match get_config() {
        Some(cfg) => {
            if let Ok(addr) = cfg.char_ip.parse::<std::net::Ipv4Addr>() {
                u32::from(addr)
            } else { 0 }
        }
        None => 0,
    }
}

pub fn rust_config_get_char_port() -> u16 {
    get_config().map(|c| c.char_port).unwrap_or(2005)
}

pub fn rust_config_get_login_ip() -> u32 {
    match get_config() {
        Some(cfg) => {
            if let Ok(addr) = cfg.login_ip.parse::<std::net::Ipv4Addr>() {
                u32::from(addr)
            } else { 0 }
        }
        None => 0,
    }
}

pub fn rust_config_get_login_port() -> u16 {
    get_config().map(|c| c.login_port).unwrap_or(2000)
}

pub fn rust_config_get_xor_key() -> *const i8 {
    match get_config() {
        Some(cfg) => match CString::new(cfg.xor_key.clone()) {
            Ok(s) => s.into_raw(),
            Err(_) => ptr::null(),
        },
        None => ptr::null(),
    }
}

pub fn rust_config_get_start_point() -> Point {
    get_config().map(|c| c.start_point).unwrap_or(Point::new(0, 0, 0))
}

pub fn rust_config_get_server_id() -> i32 {
    get_config().map(|c| c.server_id).unwrap_or(0)
}

pub fn rust_config_get_meta_count() -> i32 {
    get_config().map(|c| c.meta.len() as i32).unwrap_or(0)
}

pub fn rust_config_get_meta_file(index: i32) -> *const i8 {
    match get_config() {
        Some(cfg) => {
            if index >= 0 && (index as usize) < cfg.meta.len() {
                match CString::new(cfg.meta[index as usize].clone()) {
                    Ok(s) => s.into_raw(),
                    Err(_) => ptr::null(),
                }
            } else { ptr::null() }
        }
        None => ptr::null(),
    }
}

pub fn rust_config_get_town_count() -> i32 {
    get_config().map(|c| c.town.len() as i32).unwrap_or(0)
}

pub fn rust_config_get_town_name(index: i32) -> *const i8 {
    match get_config() {
        Some(cfg) => {
            if index >= 0 && (index as usize) < cfg.town.len() {
                match CString::new(cfg.town[index as usize].clone()) {
                    Ok(s) => s.into_raw(),
                    Err(_) => ptr::null(),
                }
            } else { ptr::null() }
        }
        None => ptr::null(),
    }
}

pub unsafe fn rust_config_free_string(ptr: *mut i8) {
    if !ptr.is_null() {
        unsafe { let _ = CString::from_raw(ptr); }
    }
}

pub unsafe fn rust_config_populate_c_globals() {
    use crate::config_globals::{GlobalConfig, TownData, set_global_config, XP_RATE, D_RATE};
    use std::sync::atomic::Ordering;

    unsafe fn copy_cstr<const N: usize>(ptr: *const i8, buf: &mut [i8; N]) {
        if !ptr.is_null() {
            let cstr = CStr::from_ptr(ptr);
            let bytes = cstr.to_bytes();
            let len = bytes.len().min(N - 1);
            ptr::copy_nonoverlapping(bytes.as_ptr(), buf.as_mut_ptr() as *mut u8, len);
            buf[len] = 0;
            rust_config_free_string(ptr as *mut i8);
        }
    }

    let mut cfg = GlobalConfig {
        xor_key:     [0; 10],
        start_pos:   crate::config::Point { m: 0, x: 0, y: 0 },
        login_id:    [0; 33],
        login_pw:    [0; 33],
        login_ip:    0,
        login_port:  2000,
        char_id:     [0; 33],
        char_pw:     [0; 33],
        char_ip:     0,
        char_port:   2005,
        map_ip:      0,
        map_port:    0,
        serverid:    0,
        require_reg: 1,
        nex_version: 0,
        nex_deep:    0,
        save_time:   60000,
        meta_file:   [[0; 256]; 20],
        metamax:     0,
        towns:       [TownData { name: [0; 32] }; 255],
        town_n:      0,
        data_dir:    String::from("./data/"),
        lua_dir:     String::from("./data/lua/"),
        maps_dir:    String::from("./data/maps/"),
        meta_dir:    String::from("./data/meta/"),
    };

    unsafe {
        let config_opt = get_config();
        if let Some(config) = config_opt {
            if let Ok(s) = CString::new(config.login_id.clone()) {
                copy_cstr(s.into_raw(), &mut cfg.login_id);
            }
            if let Ok(s) = CString::new(config.login_pw.clone()) {
                copy_cstr(s.into_raw(), &mut cfg.login_pw);
            }
            cfg.login_port = config.login_port as i32;
            if let Ok(addr) = config.login_ip.parse::<Ipv4Addr>() {
                cfg.login_ip = u32::from_le_bytes(addr.octets()) as i32;
            }

            if let Ok(s) = CString::new(config.char_id.clone()) {
                copy_cstr(s.into_raw(), &mut cfg.char_id);
            }
            if let Ok(s) = CString::new(config.char_pw.clone()) {
                copy_cstr(s.into_raw(), &mut cfg.char_pw);
            }
            cfg.char_port = config.char_port as i32;
            if let Ok(addr) = config.char_ip.parse::<Ipv4Addr>() {
                cfg.char_ip = u32::from_le_bytes(addr.octets()) as i32;
            }

            cfg.map_port = config.map_port as u32;
            if let Ok(addr) = config.map_ip.parse::<Ipv4Addr>() {
                cfg.map_ip = u32::from_le_bytes(addr.octets());
            }

            if let Ok(s) = CString::new(config.xor_key.clone()) {
                copy_cstr(s.into_raw(), &mut cfg.xor_key);
            }

            cfg.start_pos   = config.start_point;
            cfg.serverid    = config.server_id;
            cfg.require_reg = config.require_reg;
            cfg.nex_version = config.version;
            cfg.nex_deep    = config.deep;
            cfg.save_time   = config.save_time * 1000;

            // XP_RATE and D_RATE are AtomicI32 (written at runtime by GM commands).
            XP_RATE.store(config.xprate, Ordering::Relaxed);
            D_RATE.store(config.droprate, Ordering::Relaxed);

            cfg.metamax = config.meta.len().min(20) as i32;
            for (i, meta) in config.meta.iter().take(20).enumerate() {
                if let Ok(s) = CString::new(meta.clone()) {
                    let bytes = s.as_bytes();
                    let len = bytes.len().min(255);
                    ptr::copy_nonoverlapping(bytes.as_ptr(), cfg.meta_file[i].as_mut_ptr() as *mut u8, len);
                    cfg.meta_file[i][len] = 0;
                }
            }

            cfg.town_n = config.town.len().min(255) as i32;
            for (i, town) in config.town.iter().take(255).enumerate() {
                if let Ok(s) = CString::new(town.clone()) {
                    let bytes = s.as_bytes();
                    let len = bytes.len().min(31);
                    ptr::copy_nonoverlapping(bytes.as_ptr(), cfg.towns[i].name.as_mut_ptr() as *mut u8, len);
                    cfg.towns[i].name[len] = 0;
                }
            }

            cfg.data_dir  = config.data_dir.clone();
            cfg.lua_dir   = config.lua_dir.clone();
            cfg.maps_dir  = config.maps_dir.clone();
            cfg.meta_dir  = config.meta_dir.clone();
        }
    }

    set_global_config(cfg);
}
