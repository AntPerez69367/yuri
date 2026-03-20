use anyhow::{Context, Result};
use sqlx::mysql::MySqlPoolOptions;
use std::sync::Arc;
use tokio::sync::mpsc;
use yuri::engine::{core_init, set_termfunc};
use yuri::game::block::map_initblock;
use yuri::game::client::visual::clif_timeout;
use yuri::game::lua::dispatch::dispatch;
use yuri::game::map_server::{lang_read, map_do_term, map_initiddb, map_loadgameregistry};
use yuri::game::mob::mobspawn_read;
use yuri::game::scripting::sl_init;
use yuri::servers::login::{parse_lang_file, LoginState};
use yuri::session::{get_session_manager, make_listen_port, sync_callback};
use yuri::world::{KickRequest, WorldState};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    core_init();

    let mut conf_file = "conf/server.yaml".to_string();
    let mut lang_file = "conf/lang.yaml".to_string();

    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "--h" | "--?" | "/?" => {
                println!("Usage: world_server [--conf FILE] [--lang FILE]");
                return Ok(());
            }
            "--conf" if i + 1 < args.len() => {
                i += 1;
                conf_file = args[i].clone();
            }
            "--lang" if i + 1 < args.len() => {
                i += 1;
                lang_file = args[i].clone();
            }
            _ => {}
        }
        i += 1;
    }

    // Load config into global CONFIG static.
    if yuri::config::config_read(&conf_file) != 0 {
        anyhow::bail!("config_read failed for {}", conf_file);
    }
    let config = yuri::config::config();

    // Load lang strings for map server.
    {
        let clang = std::ffi::CString::new(lang_file.as_str()).unwrap();
        unsafe {
            lang_read(clang.as_ptr());
        }
    }

    // Load login messages.
    let lang_content = std::fs::read_to_string(&lang_file).unwrap_or_default();
    let messages = parse_lang_file(&lang_content)?;

    tracing::info!("[world] World Server Starting...");

    // ── Database ──────────────────────────────────────────────────────
    let pool = {
        let db_url =
            std::env::var("DATABASE_URL").context("DATABASE_URL environment variable not set")?;
        MySqlPoolOptions::new()
            .max_connections(15)
            .connect(&db_url)
            .await
            .with_context(|| format!("Cannot connect to MySQL: {}", db_url))?
    };

    yuri::database::set_pool(pool.clone()).context("Failed to register DB pool")?;

    // Reset online flags.
    sqlx::query("UPDATE `Character` SET `ChaOnline` = 0 WHERE `ChaOnline` = 1")
        .execute(&pool)
        .await
        .ok();

    // ── World state ──────────────────────────────────────────────────
    let (kick_tx, kick_rx) = mpsc::channel::<KickRequest>(64);

    let world = Arc::new(WorldState {
        db: pool.clone(),
        config: config.clone(),
        messages,
        online: dashmap::DashSet::new(),
        auth_db: dashmap::DashMap::new(),
        kick_tx,
    });

    yuri::world::set_world(Arc::clone(&world));

    // ── Map init (sync, requires game thread) ────────────────────────
    unsafe {
        if yuri::database::map_db::map_init(&config.maps_dir, config.server_id) != 0 {
            anyhow::bail!("map_init failed");
        }
        map_initblock();
        map_initiddb();
    }

    // ── Async DB loads (parallel) ────────────────────────────────────
    let (npc_res, warp_res) = tokio::join!(
        async { unsafe { yuri::game::npc::npc_init_async().await } },
        async { unsafe { yuri::game::npc::warp_init_async().await } },
    );
    if npc_res != 0 { anyhow::bail!("npc_init_async failed"); }
    if warp_res != 0 { anyhow::bail!("warp_init_async failed"); }

    yuri::database::initialize().await?;

    unsafe { mobspawn_read().await };

    // ── Game state init (sync, after DB) ─────────────────────────────
    unsafe {
        yuri::game::map_server::object_flag_init();
        sl_init();
    }
    unsafe { map_loadgameregistry().await };

    // ── Network ──────────────────────────────────────────────────────
    {
        let manager = get_session_manager();
        let mut cbs = manager.default_callbacks.lock().unwrap();
        cbs.parse = Some(std::sync::Arc::new(
            |fd: yuri::session::SessionId| -> yuri::session::CallbackFuture {
                Box::pin(yuri::game::client::clif_parse(fd))
            },
        ));
        cbs.timeout = Some(sync_callback(clif_timeout));
    }
    make_listen_port(config.map_port as i32);

    dispatch("startup", None, &[]);
    unsafe { set_termfunc(Some(map_do_term)) };

    // ── Login listener ───────────────────────────────────────────────
    {
        let w = Arc::clone(&world);
        let bind = format!("{}:{}", config.login_ip, config.login_port);
        tokio::spawn(async move {
            let mut login_state =
                LoginState::new(w.db.clone(), w.config.clone(), w.messages.clone());
            login_state.world = Some(Arc::clone(&w));
            let login_state = Arc::new(login_state);
            tracing::info!("[login] [ready] addr={}", bind);
            if let Err(e) = LoginState::run(login_state, &bind).await {
                tracing::error!("[login] listener error: {}", e);
            }
        });
    }

    // ── Auth expiry timer ────────────────────────────────────────────
    {
        let w = Arc::clone(&world);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(30));
            loop {
                ticker.tick().await;
                let now = std::time::Instant::now();
                w.auth_db.retain(|_, entry| entry.expires > now);
            }
        });
    }

    tracing::info!(
        "[world] [ready] Login={}:{} Map={}:{}",
        config.login_ip,
        config.login_port,
        config.map_ip,
        config.map_port
    );

    // ── Deadlock detector ────────────────────────────────────────────
    std::thread::spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        let deadlocks = parking_lot::deadlock::check_deadlock();
        if deadlocks.is_empty() {
            continue;
        }
        tracing::error!(
            "[world] [deadlock] {} deadlock(s) detected!",
            deadlocks.len()
        );
        for (i, threads) in deadlocks.iter().enumerate() {
            tracing::error!("[world] [deadlock] Deadlock #{}", i + 1);
            for t in threads {
                tracing::error!(
                    "[world] [deadlock]   Thread {:?}:\n{:?}",
                    t.thread_id(),
                    t.backtrace()
                );
            }
        }
    });

    // ── LocalSet: kick drain + game loop ─────────────────────────────
    let local = tokio::task::LocalSet::new();

    {
        let mut kick_rx = kick_rx;
        local.spawn_local(async move {
            while let Some(req) = kick_rx.recv().await {
                if let Some(arc) = yuri::game::map_server::map_id2sd_pc(req.char_id) {
                    let fd = arc.fd;
                    yuri::session::session_set_eof(fd, 12);
                    tracing::info!(
                        "[world] [kick] char_id={} fd={:?} kicked (duplicate login)",
                        req.char_id,
                        fd
                    );
                }
            }
        });
    }

    local
        .run_until(yuri::engine::game_loop::run_game_loop(config.map_port))
        .await
        .map_err(|e| anyhow::anyhow!("game loop error: {}", e))?;

    tracing::info!("[world] Shutting down...");
    unsafe {
        set_termfunc(None);
        map_do_term();
    }
    Ok(())
}
