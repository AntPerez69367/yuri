use chrono::{Datelike, Local, Timelike};
use mlua::prelude::*;
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::config;
use crate::game::map_server::{CURRENT_TIME, CURRENT_YEAR, CURRENT_DAY, CURRENT_SEASON};
use crate::game::time_util::gettick;

fn realtime() -> (i32, i32, i32, i32) {
    let now = Local::now();
    (
        now.weekday().num_days_from_sunday() as i32,
        now.hour() as i32,
        now.minute() as i32,
        now.second() as i32,
    )
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    let g = lua.globals();

    g.set(
        "getTick",
        lua.create_function(|_, ()| Ok(gettick() as i64))?,
    )?;

    g.set(
        "timeMS",
        lua.create_function(|_, ()| {
            let ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            Ok(ms)
        })?,
    )?;

    // No-op — must not block the game thread.
    g.set("msleep", lua.create_function(|_, _ms: i64| Ok(()))?)?;

    g.set(
        "curServer",
        lua.create_function(|_, ()| Ok(config().server_id as i64))?,
    )?;

    g.set(
        "curYear",
        lua.create_function(|_, ()| Ok(CURRENT_YEAR.load(Ordering::Relaxed) as i64))?,
    )?;

    g.set(
        "curSeason",
        lua.create_function(|_, ()| Ok(CURRENT_SEASON.load(Ordering::Relaxed) as i64))?,
    )?;

    g.set(
        "curDay",
        lua.create_function(|_, ()| Ok(CURRENT_DAY.load(Ordering::Relaxed) as i64))?,
    )?;

    g.set(
        "curTime",
        lua.create_function(|_, ()| Ok(CURRENT_TIME.load(Ordering::Relaxed) as i64))?,
    )?;

    g.set(
        "realDay",
        lua.create_function(|_, ()| Ok(realtime().0 as i64))?,
    )?;
    g.set(
        "realHour",
        lua.create_function(|_, ()| Ok(realtime().1 as i64))?,
    )?;
    g.set(
        "realMinute",
        lua.create_function(|_, ()| Ok(realtime().2 as i64))?,
    )?;
    g.set(
        "realSecond",
        lua.create_function(|_, ()| Ok(realtime().3 as i64))?,
    )?;

    Ok(())
}
