//! In-game time system — hour/day/season/year cycle with DB persistence.

use std::sync::atomic::{AtomicI32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::database::get_pool;
use crate::session::{get_fd_max, session_exists, session_get_data, SessionId};

// ── In-game time globals ─────────────────────────────────────────────────

/// Current in-game hour (0–23). Incremented by `change_time_char` every game hour.
pub static CURRENT_TIME: AtomicI32 = AtomicI32::new(0);

/// Current in-game day within the current season (1–91).
pub static CURRENT_DAY: AtomicI32 = AtomicI32::new(0);

/// Current in-game season (1–4).
pub static CURRENT_SEASON: AtomicI32 = AtomicI32::new(0);

/// Current in-game year.
pub static CURRENT_YEAR: AtomicI32 = AtomicI32::new(0);

/// Previous in-game hour; used by `map_weather` to detect hour transitions.
pub static OLD_TIME: AtomicI32 = AtomicI32::new(0);

// ── Time advancement ─────────────────────────────────────────────────────

/// Advance the in-game clock by one hour and broadcast the new time to all
/// connected players.
///
/// # Safety
/// Must be called on the game thread.
pub async unsafe fn change_time_char(_id: i32, _n: i32) -> i32 {
    use crate::game::map_parse::player_state::clif_sendtime;

    let t = CURRENT_TIME.fetch_add(1, Ordering::Relaxed) + 1;

    if t == 24 {
        CURRENT_TIME.store(0, Ordering::Relaxed);
        let d = CURRENT_DAY.fetch_add(1, Ordering::Relaxed) + 1;
        if d == 92 {
            CURRENT_DAY.store(1, Ordering::Relaxed);
            let s = CURRENT_SEASON.fetch_add(1, Ordering::Relaxed) + 1;
            if s == 5 {
                CURRENT_SEASON.store(1, Ordering::Relaxed);
                CURRENT_YEAR.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    // Broadcast updated time to all active sessions.
    for i in 0..get_fd_max() {
        let fd = SessionId::from_raw(i);
        if session_exists(fd) {
            if let Some(sd) = session_get_data(fd) {
                clif_sendtime(&sd);
            }
        }
    }

    // Persist updated time to the database.
    let (t, d, s, y) = (
        CURRENT_TIME.load(Ordering::Relaxed),
        CURRENT_DAY.load(Ordering::Relaxed),
        CURRENT_SEASON.load(Ordering::Relaxed),
        CURRENT_YEAR.load(Ordering::Relaxed),
    );
    sqlx::query("UPDATE `Time` SET `TimHour` = ?, `TimDay` = ?, `TimSeason` = ?, `TimYear` = ?")
        .bind(t)
        .bind(d)
        .bind(s)
        .bind(y)
        .execute(get_pool())
        .await
        .ok();

    0
}

/// Load in-game time from the database and initialise globals.
///
/// # Safety
/// Must be called on the game thread.
pub async unsafe fn get_time_thing() -> i32 {
    #[derive(sqlx::FromRow)]
    struct TimeRow {
        #[sqlx(rename = "TimHour")]
        hour: u32,
        #[sqlx(rename = "TimDay")]
        day: u32,
        #[sqlx(rename = "TimSeason")]
        season: u32,
        #[sqlx(rename = "TimYear")]
        year: u32,
    }

    if let Some(row) = sqlx::query_as::<_, TimeRow>(
        "SELECT `TimHour`, `TimDay`, `TimSeason`, `TimYear` FROM `Time` LIMIT 1",
    )
    .fetch_optional(get_pool())
    .await
    .ok()
    .flatten()
    {
        OLD_TIME.store(row.hour as i32, Ordering::Relaxed);
        CURRENT_TIME.store(row.hour as i32, Ordering::Relaxed);
        CURRENT_DAY.store(row.day as i32, Ordering::Relaxed);
        CURRENT_SEASON.store(row.season as i32, Ordering::Relaxed);
        CURRENT_YEAR.store(row.year as i32, Ordering::Relaxed);
    }

    0
}

/// Trigger the mapWeather Lua hook when the in-game hour changes.
pub fn map_weather(_id: i32, _n: i32) -> bool {
    let ot = OLD_TIME.load(Ordering::Relaxed);
    let ct = CURRENT_TIME.load(Ordering::Relaxed);
    if ot != ct {
        OLD_TIME.store(ct, Ordering::Relaxed);
        crate::game::lua::dispatch::dispatch("mapWeather", None, &[]);
    }
    false
}

/// Record the current UNIX timestamp as the server start time.
///
/// # Safety
/// Must be called on the game thread.
pub async unsafe fn uptime() -> i32 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i32)
        .unwrap_or(0);

    let pool = get_pool();
    sqlx::query("DELETE FROM `UpTime` WHERE `UtmId` = '3'")
        .execute(pool)
        .await
        .ok();
    sqlx::query("INSERT INTO `UpTime`(`UtmId`, `UtmValue`) VALUES('3', ?)")
        .bind(now)
        .execute(pool)
        .await
        .ok();

    0
}
