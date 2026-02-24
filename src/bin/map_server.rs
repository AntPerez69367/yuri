use anyhow::{Context, Result};
use sqlx::mysql::MySqlPoolOptions;
use std::ffi::CString;
use std::sync::Arc;
use yuri::config::ServerConfig;
use yuri::servers::map::MapState;

/// C game-logic functions from libmap_game.a (pure C, not static inline).
extern "C" {
    fn map_initblock();
    fn map_initiddb();
    fn npc_init();
    fn warp_init() -> i32;
    fn intif_init() -> i32;
    fn object_flag_init() -> i32;
    fn sl_init() -> i32;
    fn map_loadgameregistry() -> i32;
    fn clif_parse(fd: i32) -> i32;
    fn clif_timeout(fd: i32) -> i32;
    fn map_do_term(); // renamed from do_term in Task 5
    fn lang_read(file: *const i8);
    fn authdb_init(); // from map_char.c — stays until Task 6

    // Legacy C SQL functions from libdeps.a
    fn Sql_Malloc() -> *mut std::ffi::c_void;
    fn Sql_Connect(
        handle: *mut std::ffi::c_void,
        user: *const i8, pw: *const i8,
        host: *const i8, port: u16,
        db: *const i8,
    ) -> i32;
}

// Rust FFI functions from libyuri.a (these replace the static-inline C shims).
// boarddb_init() → rust_boarddb_init(), etc.
extern "C" {
    fn rust_boarddb_init() -> i32;
    fn rust_clandb_init() -> i32;
    fn rust_classdb_init(data_dir: *const i8) -> i32;
    fn rust_itemdb_init() -> i32;
    fn rust_recipedb_init() -> i32;
    fn rust_magicdb_init() -> i32;
    fn rust_mobdb_init() -> i32;
    // Session functions (from libyuri.a ffi/session.rs)
    fn rust_session_set_default_parse(f: unsafe extern "C" fn(i32) -> i32);
    fn rust_session_set_default_timeout(f: unsafe extern "C" fn(i32) -> i32);
    fn rust_make_listen_port(port: i32) -> i32;
    fn rust_set_termfunc(f: unsafe extern "C" fn());
}

// sql_handle is defined in map_server.c; we write to it after Sql_Connect succeeds.
extern "C" {
    static mut sql_handle: *mut std::ffi::c_void;
}

// fd_max is normally defined in core.c (which we exclude to avoid duplicate main()).
// The Rust session layer updates this via the c_update_fd_max callback.
#[no_mangle]
pub static mut fd_max: std::ffi::c_int = 0;

// Called by Rust session layer to update C's fd_max global.
#[no_mangle]
pub unsafe extern "C" fn c_update_fd_max(new_max: std::ffi::c_int) {
    fd_max = new_max;
}

extern "C" {
    fn rust_core_init();
    fn rust_register_fd_max_updater(cb: unsafe extern "C" fn(std::ffi::c_int));
    fn db_init();
    fn timer_init();
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Initialize C core state (mirrors core.c main() preamble).
    unsafe {
        rust_core_init();
        rust_register_fd_max_updater(c_update_fd_max);
        db_init();
        timer_init();
    }

    let mut conf_file = "conf/server.yaml".to_string();
    let mut lang_file = "conf/lang.yaml".to_string();

    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "--h" | "--?" | "/?" => {
                println!("Usage: map_server [--conf FILE] [--lang FILE]");
                return Ok(());
            }
            "--conf" if i + 1 < args.len() => { i += 1; conf_file = args[i].clone(); }
            "--lang" if i + 1 < args.len() => { i += 1; lang_file = args[i].clone(); }
            _ => {}
        }
        i += 1;
    }

    // Load config (Rust side)
    let config: ServerConfig = {
        let content = std::fs::read_to_string(&conf_file)
            .with_context(|| format!("Cannot read config: {}", conf_file))?;
        ServerConfig::from_str(&content)
            .with_context(|| format!("Cannot parse config: {}", conf_file))?
    };

    // Call rust_config_read so C code can access config globals
    {
        let cpath = CString::new(conf_file.as_str()).unwrap();
        if unsafe { yuri::ffi::config::rust_config_read(cpath.as_ptr()) } != 0 {
            anyhow::bail!("rust_config_read failed for {}", conf_file);
        }
    }

    // Load lang strings (C)
    {
        let clang = CString::new(lang_file.as_str()).unwrap();
        unsafe { lang_read(clang.as_ptr()); }
    }

    tracing::info!("[map] Map Server Started.");

    // Rust async DB pool
    let pool = {
        let db_url = format!(
            "mysql://{}:{}@{}:{}/{}",
            config.sql_id, config.sql_pw, config.sql_ip, config.sql_port, config.sql_db
        );
        MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await
            .with_context(|| format!(
                "Cannot connect to MySQL (host={}:{} db={} user={})",
                config.sql_ip, config.sql_port, config.sql_db, config.sql_id
            ))?
    };

    // rust_db_connect (for Rust DB modules: map_db, mob_db, etc.)
    {
        let db_url = format!(
            "mysql://{}:{}@{}:{}/{}",
            config.sql_id, config.sql_pw, config.sql_ip, config.sql_port, config.sql_db
        );
        let curl = CString::new(db_url).unwrap();
        if yuri::ffi::database::rust_db_connect(curl.as_ptr()) != 0 {
            anyhow::bail!("rust_db_connect failed");
        }
    }

    // Legacy C SQL handle
    unsafe {
        let handle = Sql_Malloc();
        if handle.is_null() {
            anyhow::bail!("Sql_Malloc failed");
        }
        let user = CString::new(config.sql_id.as_str()).unwrap();
        let pw   = CString::new(config.sql_pw.as_str()).unwrap();
        let host = CString::new(config.sql_ip.as_str()).unwrap();
        let db   = CString::new(config.sql_db.as_str()).unwrap();
        let rc = Sql_Connect(handle, user.as_ptr(), pw.as_ptr(), host.as_ptr(),
                              config.sql_port, db.as_ptr());
        if rc != 0 { // SQL_SUCCESS == 0
            anyhow::bail!("Sql_Connect failed");
        }
        sql_handle = handle;
    }

    // Reset online flags
    sqlx::query("UPDATE `Character` SET `ChaOnline` = 0 WHERE `ChaOnline` = 1")
        .execute(&pool)
        .await
        .ok();

    // rust_map_init
    {
        let maps_dir = CString::new(config.maps_dir.as_str()).unwrap();
        let serverid = config.server_id;
        if unsafe { yuri::ffi::map_db::rust_map_init(maps_dir.as_ptr(), serverid) } != 0 {
            anyhow::bail!("rust_map_init failed");
        }
    }

    // C game-logic init — order matches do_init exactly.
    // Static-inline C shims (boarddb_init, etc.) can't be linked from Rust;
    // we call the rust_* functions they wrap directly.
    unsafe {
        map_initblock();
        map_initiddb();
        npc_init();
        warp_init();
        rust_itemdb_init();
        rust_recipedb_init();
        rust_mobdb_init();
        rust_magicdb_init();
        let data_dir = CString::new(config.data_dir.as_str()).unwrap();
        rust_classdb_init(data_dir.as_ptr());
        rust_clandb_init();
        rust_boarddb_init();
        intif_init();
        object_flag_init();
        sl_init();
        map_loadgameregistry();
        rust_session_set_default_parse(clif_parse);
        rust_session_set_default_timeout(clif_timeout);
        rust_make_listen_port(config.map_port as i32);
        authdb_init();
        rust_set_termfunc(map_do_term);
    }

    let state = Arc::new(MapState::new(pool, config));

    // Spawn char server reconnect loop (replaces check_connect_char timer)
    {
        let s = Arc::clone(&state);
        tokio::spawn(async move {
            yuri::servers::map::char::connect_to_char(s).await;
        });
    }

    // Spawn auth DB expiry timer (replaces auth_timer — every 30s)
    {
        let s = Arc::clone(&state);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(30));
            loop {
                ticker.tick().await;
                yuri::servers::map::packet::expire_auth(&s).await;
            }
        });
    }

    tracing::info!("[map] [ready] Listening on {}:{}", state.config.map_ip, state.config.map_port);

    // Park the async runtime — C session layer (make_listen_port) drives accept loop.
    tokio::signal::ctrl_c().await.ok();
    tracing::info!("[map] Shutting down...");
    unsafe { map_do_term(); }
    Ok(())
}
