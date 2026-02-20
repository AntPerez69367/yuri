use std::future::Future;
use std::sync::OnceLock;

use sqlx::MySqlPool;
use tokio::runtime::Runtime;

pub mod board_db;
pub mod clan_db;
pub mod class_db;
pub mod item_db;
pub mod magic_db;
pub mod recipe_db;

static DB_POOL: OnceLock<MySqlPool> = OnceLock::new();
// Single persistent runtime — pool connections are bound to a reactor; reusing
// the same runtime keeps pool I/O registered with the correct reactor.
static DB_RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn get_runtime() -> &'static Runtime {
    DB_RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    })
}

pub(crate) fn get_pool() -> &'static MySqlPool {
    DB_POOL.get().expect("[db] pool not initialized — rust_db_connect() must be called first")
}

pub(crate) fn blocking_run<F: Future>(f: F) -> F::Output {
    get_runtime().block_on(f)
}

/// Connect to the database. Called from ffi::database::rust_db_connect.
pub fn connect(url: &str) -> Result<(), sqlx::Error> {
    let pool = blocking_run(MySqlPool::connect(url))?;
    let _ = DB_POOL.set(pool);
    tracing::info!("[db] Connected to MariaDB");
    Ok(())
}
