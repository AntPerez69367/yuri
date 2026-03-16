use std::future::Future;
use std::pin::Pin;
use std::sync::{Mutex, OnceLock};
use std::task::{Context, Poll};

/// Wraps a `!Send` future and asserts it is `Send`.
///
/// # Safety
/// The caller must ensure that all data captured by the future is actually
/// safe to send across threads (e.g., raw pointers to types that implement `Send`,
/// accessed with the calling thread blocked via `blocking_run_async`'s join).
pub(crate) struct AssertSend<F>(F);
unsafe impl<F: Future> Send for AssertSend<F> {}
impl<F: Future> Future for AssertSend<F> {
    type Output = F::Output;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        unsafe { self.map_unchecked_mut(|s| &mut s.0).poll(cx) }
    }
}
/// # Safety
/// The caller must ensure that all data captured in `f` is actually safe to
/// send across threads. Concretely, raw pointers must point to types that
/// implement `Send`, and the future must be driven by `blocking_run_async`
/// (which joins before returning), not by `tokio::spawn` or `spawn_local`.
pub(crate) fn assert_send<F: Future>(f: F) -> AssertSend<F> { AssertSend(f) }

use sqlx::MySqlPool;
use tokio::runtime::Runtime;

pub mod board_db;
pub mod boards;
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
    DB_POOL.get().expect("[db] pool not initialized — db_connect() must be called first")
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

/// Sync-to-async bridge: drives a `Send + 'static` future to completion, safe from
/// any calling context including `LocalSet` tasks.
///
/// When called inside a Tokio runtime, spawns a plain OS thread (not a Tokio worker)
/// so it has no active runtime context; the DB_RUNTIME drives the future there, then
/// the spawned thread is joined before this function returns.
///
/// Use this as the bridge for sync code (Lua callbacks, `_sync` wrappers, C FFI stubs)
/// that needs to call async functions but cannot use `.await`. It does not panic from
/// `LocalSet`, unlike `blocking_run`.
///
/// The future must be `Send + 'static` because it crosses a thread boundary.
/// For `!Send` futures capturing raw pointers, wrap with `assert_send` when the
/// pointee type implements `Send` (e.g. `*mut MapSessionData`).
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


/// Connect to the database.
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



/// Initialize tracing and connect to the database.
pub fn db_connect(url: &str) -> i32 {
    let _ = tracing_subscriber::fmt()
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    match crate::database::connect(url) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!("[db] Connect failed: {}", e);
            -1
        }
    }
}
