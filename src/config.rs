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

/// A point in 3D space (map, x, y)
///
/// This matches the C struct exactly due to #[repr(C)]
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    pub m: u16,
    pub x: u16,
    pub y: u16,
}

impl Point {
    /// Create a new point
    pub fn new(m: u16, x: u16, y: u16) -> Self {
        Self { m, x, y }
    }
}

/// Main server configuration
///
/// This struct is automatically parsed from YAML by serde.
/// Just add a field here, and serde handles the rest!
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    // ============================================
    // MySQL Database Configuration
    // ============================================
    pub sql_ip: String,

    #[serde(default = "default_sql_port")]
    pub sql_port: u16,

    pub sql_id: String,
    pub sql_pw: String,
    pub sql_db: String,

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

fn default_sql_port() -> u16 {
    3306
}

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
    /// println!("SQL DB: {}", config.sql_db);
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
        anyhow::ensure!(!self.sql_ip.is_empty(), "sql_ip cannot be empty");
        anyhow::ensure!(!self.sql_id.is_empty(), "sql_id cannot be empty");
        anyhow::ensure!(!self.sql_db.is_empty(), "sql_db cannot be empty");
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
sql_ip: "127.0.0.1"
sql_id: "user"
sql_pw: "pass"
sql_db: "testdb"

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

        assert_eq!(config.sql_ip, "127.0.0.1");
        assert_eq!(config.sql_id, "user");
        assert_eq!(config.sql_pw, "pass");
        assert_eq!(config.sql_db, "testdb");
        assert_eq!(config.start_point, Point::new(0, 1, 1));
    }

    #[test]
    fn test_default_values() {
        let config = ServerConfig::from_str(minimal_config()).unwrap();

        // All these should have defaults
        assert_eq!(config.sql_port, 3306);
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
sql_ip: "127.0.0.1"
sql_port: 5432
sql_id: "user"
sql_pw: "pass"
sql_db: "testdb"

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
        assert_eq!(config.sql_port, 5432);
        assert_eq!(config.login_port, 3000);
        assert_eq!(config.char_port, 3005);
        assert_eq!(config.map_port, 3001);
    }

    #[test]
    fn test_meta_files_as_list() {
        let config_str = r#"
sql_ip: "127.0.0.1"
sql_id: "user"
sql_pw: "pass"
sql_db: "testdb"
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
sql_ip: "127.0.0.1"
sql_id: "user"
sql_pw: "pass"
sql_db: "testdb"
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
sql_ip: "127.0.0.1"
sql_id: "user"
# Missing sql_pw!
sql_db: "testdb"
"#;

        let result = ServerConfig::from_str(config_str);
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{:?}", err);
        assert!(err_msg.contains("sql_pw") || err_msg.contains("missing field"));
    }

    #[test]
    fn test_invalid_yaml() {
        let config_str = r#"
sql_ip: [this is not valid yaml
"#;

        let result = ServerConfig::from_str(config_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_type() {
        let config_str = r#"
sql_ip: "127.0.0.1"
sql_port: "not_a_number"
sql_id: "user"
sql_pw: "pass"
sql_db: "testdb"
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

        let result = ServerConfig::from_str(config_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_validation_empty_sql_ip() {
        let config_str = r#"
sql_ip: ""
sql_id: "user"
sql_pw: "pass"
sql_db: "testdb"
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

        let result = ServerConfig::from_str(config_str);
        assert!(result.is_err());

        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("sql_ip"));
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
sql_ip: "127.0.0.1"
sql_id: "user"
sql_pw: "pass"
sql_db: "testdb"
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
sql_ip: "192.168.1.2"
sql_port: 3306
sql_id: "gameuser"
sql_pw: "gamepass"
sql_db: "gamedb"

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
        assert_eq!(config.sql_ip, "192.168.1.2");
        assert_eq!(config.sql_id, "gameuser");
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

        assert_eq!(config.sql_ip, loaded.sql_ip);
        assert_eq!(config.sql_db, loaded.sql_db);
        assert_eq!(config.start_point, loaded.start_point);

        // Cleanup
        std::fs::remove_file(temp_file).ok();
    }
}
