use std::future::Future;
use std::sync::{Mutex, OnceLock};

use sqlx::MySqlPool;
use tokio::runtime::Runtime;

pub mod board_db;
pub mod clan_db;
pub mod class_db;
pub mod item_db;
pub mod magic_db;
pub mod map_db;
pub mod mob_db;
pub mod recipe_db;

static DB_POOL: OnceLock<MySqlPool> = OnceLock::new();
// Mutex-wrapped so concurrent calls from spawned OS threads (blocking_run_async)
// are serialised. Pool connections are bound to this runtime's reactor.
static DB_RUNTIME: OnceLock<Mutex<Runtime>> = OnceLock::new();

fn get_runtime() -> &'static Mutex<Runtime> {
    DB_RUNTIME.get_or_init(|| {
        Mutex::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
        )
    })
}

pub(crate) fn get_pool() -> &'static MySqlPool {
    DB_POOL.get().expect("[db] pool not initialized — rust_db_connect() must be called first")
}

/// Run a future to completion, blocking the current thread.
///
/// Safe for startup code running in `spawn_blocking` (multi-thread runtime, not a LocalSet):
/// uses `block_in_place` so the blocking thread is parked while the future runs.
///
/// **NOT safe from a `LocalSet` task** — `block_in_place` panics there.
/// Use `blocking_run_async` instead for any code called from the game session loop.
pub(crate) fn blocking_run<F: Future>(f: F) -> F::Output {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(f)),
        Err(_) => get_runtime().lock().unwrap().block_on(f),
    }
}

/// Like `blocking_run`, but safe to call from within a Tokio async task (e.g. LocalSet).
///
/// When called from inside a runtime (detected via `Handle::try_current`), the future
/// is driven on a spawned OS thread using the existing runtime handle — no reactor
/// re-registration needed, sqlx pool connections remain valid.
///
/// The future must be `Send + 'static` because it crosses a thread boundary.
pub(crate) fn blocking_run_async<F>(f: F) -> F::Output
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    match tokio::runtime::Handle::try_current() {
        Err(_) => get_runtime().lock().unwrap().block_on(f),
        // Spawn a plain OS thread (not a Tokio worker) so it has no active runtime
        // context. DB_RUNTIME can then call block_on without "nested runtime" panic,
        // and sqlx I/O is driven by the correct reactor (the one the pool was created on).
        Ok(_) => std::thread::spawn(move || get_runtime().lock().unwrap().block_on(f))
            .join()
            .expect("blocking_run_async thread panicked"),
    }
}

/// Connect to the database. Called from ffi::database::rust_db_connect.
///
/// Returns an error if the pool is already initialized or if the connection fails.
pub fn connect(url: &str) -> Result<(), sqlx::Error> {
    if DB_POOL.get().is_some() {
        return Err(sqlx::Error::Configuration(
            "database pool already initialized".into(),
        ));
    }
    let pool = blocking_run(MySqlPool::connect(url))?;
    // set() only fails if another thread raced us; drop the new pool and return an error.
    if DB_POOL.set(pool).is_err() {
        return Err(sqlx::Error::Configuration(
            "database pool already initialized".into(),
        ));
    }
    tracing::info!("[db] Connected to MariaDB");
    Ok(())
}

/// Register an already-connected pool (for use from async Rust binaries that
/// create their own pool before the server starts).
/// Avoids the `block_on`-inside-runtime panic that `connect()` would cause.
pub fn set_pool(pool: MySqlPool) -> Result<(), sqlx::Error> {
    if DB_POOL.set(pool).is_err() {
        return Err(sqlx::Error::Configuration(
            "database pool already initialized".into(),
        ));
    }
    tracing::info!("[db] Pool registered from async context");
    Ok(())
}



/// Called from C's do_init() before any *_init() calls.
pub unsafe fn rust_db_connect(url: *const i8) -> i32 {
    let _ = tracing_subscriber::fmt()
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if url.is_null() {
            tracing::error!("[db] Connect called with null URL");
            return -1;
        }
        let url_str = match unsafe { std::ffi::CStr::from_ptr(url) }.to_str() {
            Ok(s) => s.to_owned(),
            Err(e) => {
                tracing::error!("[db] Connect URL is not valid UTF-8: {}", e);
                return -1;
            }
        };
        match crate::database::connect(&url_str) {
            Ok(()) => 0,
            Err(e) => {
                tracing::error!("[db] Connect failed: {}", e);
                -1
            }
        }
    })) {
        Ok(v) => v,
        Err(_) => {
            tracing::error!("[db] Connect panicked");
            -1
        }
    }
}
