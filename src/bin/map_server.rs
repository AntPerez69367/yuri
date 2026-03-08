use anyhow::{Context, Result};
use sqlx::mysql::MySqlPoolOptions;
use std::ffi::CString;
use std::sync::Arc;
use yuri::config::ServerConfig;
use yuri::game::client::rust_clif_parse;
use yuri::servers::map::MapState;

/// C game-logic functions from libmap_game.a (pure C, not static inline).
extern "C" {
    fn map_initblock();
    fn map_initiddb();
    fn npc_init();
    fn warp_init() -> i32;
    fn rust_sl_init();
    // (rust_sl_doscript_blargs_vec removed; use yuri::game::scripting::doscript_blargs)
    fn map_loadgameregistry() -> i32;
    fn clif_timeout(fd: i32) -> i32;
    fn map_do_term(); // impl in src/game/map_server.rs
    fn intif_mmo_tosd(fd: i32, status: *mut u8) -> i32;
    fn lang_read(file: *const i8);
    fn rust_mob_timer_spawns(id: i32, n: i32) -> i32;
    fn npc_runtimers(id: i32, n: i32) -> i32;
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
    fn rust_mobspawn_read() -> i32;
    // Session functions (from libyuri.a ffi/session.rs)
    fn rust_session_set_default_parse(f: unsafe extern "C" fn(i32) -> i32);
    fn rust_session_set_default_timeout(f: unsafe extern "C" fn(i32) -> i32);
    fn rust_make_listen_port(port: i32) -> i32;
    fn rust_set_termfunc(f: Option<unsafe extern "C" fn()>);
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
    fn timer_init();
}

/// Stub replacing `db_init()` from `c_deps/db.c`.
/// The original function only increments a statistics counter; it has no
/// side-effects on any game state, so removing it is safe.
#[no_mangle]
pub extern "C" fn db_init() {}

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
        // db_init() is now a no-op stub defined above
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
        if unsafe { yuri::config::rust_config_read(cpath.as_ptr()) } != 0 {
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

    // Register the pool with the Rust DB module layer (map_db, mob_db, etc.).
    // We use set_pool() here instead of rust_db_connect() to avoid
    // block_on-inside-runtime panic (we're already inside #[tokio::main]).
    yuri::database::set_pool(pool.clone())
        .context("Failed to register DB pool with Rust DB modules")?;

    // Reset online flags
    sqlx::query("UPDATE `Character` SET `ChaOnline` = 0 WHERE `ChaOnline` = 1")
        .execute(&pool)
        .await
        .ok();

    // Run all blocking init (rust_map_init, rust_*db_init, C game init) on a
    // dedicated thread. spawn_blocking is required because these functions call
    // blocking_run() internally, which panics if called from within the tokio runtime.
    {
        let maps_dir = config.maps_dir.clone();
        let data_dir = config.data_dir.clone();
        let serverid = config.server_id;
        let map_port = config.map_port;

        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let maps_dir_c = CString::new(maps_dir.as_str()).unwrap();
            if unsafe { yuri::database::map_db::rust_map_init(maps_dir_c.as_ptr(), serverid) } != 0 {
                anyhow::bail!("rust_map_init failed");
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
                rust_mobspawn_read();
                rust_magicdb_init();
                let data_dir_c = CString::new(data_dir.as_str()).unwrap();
                rust_classdb_init(data_dir_c.as_ptr());
                rust_clandb_init();
                rust_boarddb_init();
                yuri::game::map_server::object_flag_init();
                rust_sl_init();
                map_loadgameregistry();
                rust_session_set_default_parse(rust_clif_parse);
                rust_session_set_default_timeout(clif_timeout);
                rust_make_listen_port(map_port as i32);

                // Timers from the old do_init — restored here after do_init was removed.
                let startup = std::ffi::CString::new("startup").unwrap();
                yuri::game::scripting::doscript_blargs(startup.as_ptr(), std::ptr::null(), &[]);
                yuri::timer::timer_insert(50,   50,   Some(rust_mob_timer_spawns), 0, 0);
                yuri::timer::timer_insert(100,  100,  Some(npc_runtimers),    0, 0);
                yuri::timer::timer_insert(1000, 1000, Some(yuri::game::map_server::rust_map_cronjob), 0, 0);

                rust_set_termfunc(Some(map_do_term));
            }
            Ok(())
        }).await
          .context("Init thread panicked")??;
    }

    let state = Arc::new(MapState::new(pool, config));

    // Register state with FFI bridge so C game logic can send packets to char_server.
    yuri::game::map_char::set_map_state(Arc::clone(&state));
    // Register intif_mmo_tosd so packet.rs can call it without linking map_game into libyuri.
    yuri::game::map_char::set_mmo_tosd_fn(intif_mmo_tosd);

    // Spawn auth DB expiry timer (replaces auth_timer — every 30s).
    // Does not touch Lua, safe on any thread.
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

    // Run the C session event loop. LocalSet is required for spawn_local (accept_loop,
    // session_io_task). This drives client accept + I/O until shutdown is signalled.
    //
    // connect_to_char is spawned on the LocalSet (not tokio::spawn) because its
    // intif_mmo_tosd → Lua login-event path touches the Lua state, which is
    // single-threaded and must run on the same thread as the C event loop.
    let local = tokio::task::LocalSet::new();
    {
        let s = Arc::clone(&state);
        local.spawn_local(async move {
            yuri::servers::map::char::connect_to_char(s).await;
        });
    }
    local.run_until(yuri::session::run_async_server(state.config.map_port)).await
        .map_err(|e| anyhow::anyhow!("session loop error: {}", e))?;

    tracing::info!("[map] Shutting down...");
    // Deregister the term callback before calling map_do_term() explicitly so
    // a signal arriving after the session loop cannot fire it a second time.
    unsafe { rust_set_termfunc(None); }
    unsafe { map_do_term(); }
    Ok(())
}
