//! Global Lua functions (91 total) — registered in sl_init.

use mlua::{Lua, Value};
use std::ffi::{CStr, CString};
use std::sync::Arc;

use crate::common::traits::LegacyEntity;

use crate::common::constants::entity::{BL_ALL, BL_ITEM, BL_MOB, BL_NPC, BL_PC};
use crate::database::get_pool;
use crate::database::map_db::get_map_ptr;
use crate::game::map_parse::chat::{clif_broadcast, clif_gmbroadcast};
use crate::game::map_server::{
    map_changepostcolor, CURRENT_DAY, CURRENT_SEASON, CURRENT_TIME, CURRENT_YEAR,
};
use crate::game::scripting::map_globals::{
    sl_g_sendmeta, sl_g_setmap, sl_g_throw, MapSettings, ThrowVisuals,
};

/// Register all 91 Lua globals on the given Lua state.
pub fn register(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();

    // -----------------------------------------------------------------------
    // BL type constants — used by getObjectsInCell, foreachincell etc.
    // -----------------------------------------------------------------------
    g.set("BL_PC", BL_PC as i64)?;
    g.set("BL_MOB", BL_MOB as i64)?;
    g.set("BL_NPC", BL_NPC as i64)?;
    g.set("BL_ITEM", BL_ITEM as i64)?;
    g.set("BL_ALL", BL_ALL as i64)?;

    // -----------------------------------------------------------------------
    // MOB state constants — used by AI scripts (mob.state comparisons)
    // -----------------------------------------------------------------------
    g.set("MOB_ALIVE", 0i64)?;
    g.set("MOB_DEAD", 1i64)?;
    g.set("MOB_HIT", 4i64)?;
    g.set("MOB_ESCAPE", 5i64)?;

    // -----------------------------------------------------------------------
    // Tick / time
    // -----------------------------------------------------------------------
    g.set(
        "getTick",
        lua.create_function(|_, ()| Ok(crate::game::time_util::gettick() as i64))?,
    )?;

    g.set(
        "timeMS",
        lua.create_function(|_, ()| {
            use std::time::{SystemTime, UNIX_EPOCH};
            let ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            Ok(ms)
        })?,
    )?;

    g.set(
        "msleep",
        lua.create_function(|_, _ms: i64| {
            // Intentional no-op — must not block the game thread.
            Ok(())
        })?,
    )?;

    g.set(
        "curServer",
        lua.create_function(|_, ()| Ok(crate::config::config().server_id as i64))?,
    )?;

    g.set(
        "curYear",
        lua.create_function(|_, ()| {
            Ok(CURRENT_YEAR.load(std::sync::atomic::Ordering::Relaxed) as i64)
        })?,
    )?;

    g.set(
        "curSeason",
        lua.create_function(|_, ()| {
            Ok(CURRENT_SEASON.load(std::sync::atomic::Ordering::Relaxed) as i64)
        })?,
    )?;

    g.set(
        "curDay",
        lua.create_function(|_, ()| {
            Ok(CURRENT_DAY.load(std::sync::atomic::Ordering::Relaxed) as i64)
        })?,
    )?;

    g.set(
        "curTime",
        lua.create_function(|_, ()| {
            Ok(CURRENT_TIME.load(std::sync::atomic::Ordering::Relaxed) as i64)
        })?,
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

    // -----------------------------------------------------------------------
    // Broadcast / comms
    // -----------------------------------------------------------------------
    g.set(
        "broadcast",
        lua.create_function(|_, (m, msg): (i32, String)| {
            let cmsg = CString::new(msg).map_err(mlua::Error::external)?;
            unsafe {
                clif_broadcast(cmsg.as_ptr(), m);
            }
            Ok(())
        })?,
    )?;

    g.set(
        "gmbroadcast",
        lua.create_function(|_, (m, msg): (i32, String)| {
            let cmsg = CString::new(msg).map_err(mlua::Error::external)?;
            unsafe {
                clif_gmbroadcast(cmsg.as_ptr(), m);
            }
            Ok(())
        })?,
    )?;

    g.set(
        "luaReload",
        lua.create_function(|_, ()| {
            crate::game::scripting::sl_reload();
            Ok(())
        })?,
    )?;

    g.set(
        "sendMeta",
        lua.create_function(|_, ()| {
            unsafe {
                sl_g_sendmeta();
            }
            Ok(())
        })?,
    )?;

    // -----------------------------------------------------------------------
    // Map: dimensions, load status, user count
    // -----------------------------------------------------------------------
    g.set(
        "getMapIsLoaded",
        lua.create_function(|_, m: i32| {
            if m < 0 {
                return Ok(false);
            }
            let mp = get_map_ptr(m as u16);
            if mp.is_null() {
                return Ok(false);
            }
            Ok(unsafe { !(*mp).registry.is_null() })
        })?,
    )?;

    g.set(
        "getMapUsers",
        lua.create_function(|_, m: i32| {
            if m < 0 {
                return Ok(0i64);
            }
            let mp = get_map_ptr(m as u16);
            if mp.is_null() || unsafe { (*mp).registry.is_null() } {
                return Ok(0i64);
            }
            Ok(crate::game::block::map_user_count(m as usize) as i64)
        })?,
    )?;

    g.set(
        "getMapXMax",
        lua.create_function(|_, m: i32| {
            if m < 0 {
                return Ok(0i64);
            }
            let mp = get_map_ptr(m as u16);
            if mp.is_null() || unsafe { (*mp).registry.is_null() } {
                return Ok(0i64);
            }
            Ok(unsafe { (*mp).xs as i64 - 1 })
        })?,
    )?;

    g.set(
        "getMapYMax",
        lua.create_function(|_, m: i32| {
            if m < 0 {
                return Ok(0i64);
            }
            let mp = get_map_ptr(m as u16);
            if mp.is_null() || unsafe { (*mp).registry.is_null() } {
                return Ok(0i64);
            }
            Ok(unsafe { (*mp).ys as i64 - 1 })
        })?,
    )?;

    // -----------------------------------------------------------------------
    // Map: tile / object / pass arrays
    // -----------------------------------------------------------------------
    g.set(
        "getObjectsMap",
        lua.create_function(|lua, _: mlua::MultiValue| {
            // Not implemented in the original C either (commented out).
            lua.create_table()
        })?,
    )?;

    g.set(
        "getObject",
        lua.create_function(|_, (m, x, y): (i32, i32, i32)| {
            if m < 0 {
                return Ok(Value::Nil);
            }
            let mp = get_map_ptr(m as u16);
            if mp.is_null() || unsafe { (*mp).registry.is_null() } {
                return Ok(Value::Nil);
            }
            let md = unsafe { &*mp };
            if x < 0 || y < 0 || x >= md.xs as i32 || y >= md.ys as i32 {
                return Ok(Value::Nil);
            }
            let idx = (x + y * md.xs as i32) as usize;
            Ok(Value::Integer(unsafe { *md.obj.add(idx) as i64 }))
        })?,
    )?;

    g.set(
        "setObject",
        lua.create_function(|_, (m, x, y, val): (i32, i32, i32, i32)| {
            if m < 0 {
                return Ok(());
            }
            let mp = get_map_ptr(m as u16);
            if mp.is_null() || unsafe { (*mp).registry.is_null() } {
                return Ok(());
            }
            let md = unsafe { &*mp };
            if x < 0 || y < 0 || x >= md.xs as i32 || y >= md.ys as i32 {
                return Ok(());
            }
            let idx = (x + y * md.xs as i32) as usize;
            unsafe {
                *md.obj.add(idx) = val as u16;
            }
            // map_foreachinarea(sl_updatepeople) omitted until foreachinarea is ported.
            Ok(())
        })?,
    )?;

    g.set(
        "getTile",
        lua.create_function(|_, (m, x, y): (i32, i32, i32)| {
            if m < 0 {
                return Ok(Value::Nil);
            }
            let mp = get_map_ptr(m as u16);
            if mp.is_null() || unsafe { (*mp).registry.is_null() } {
                return Ok(Value::Nil);
            }
            let md = unsafe { &*mp };
            if x < 0 || y < 0 || x >= md.xs as i32 || y >= md.ys as i32 {
                return Ok(Value::Nil);
            }
            let idx = (x + y * md.xs as i32) as usize;
            Ok(Value::Integer(unsafe { *md.tile.add(idx) as i64 }))
        })?,
    )?;

    g.set(
        "setTile",
        lua.create_function(|_, (m, x, y, val): (i32, i32, i32, i32)| {
            if m < 0 {
                return Ok(());
            }
            let mp = get_map_ptr(m as u16);
            if mp.is_null() || unsafe { (*mp).registry.is_null() } {
                return Ok(());
            }
            let md = unsafe { &*mp };
            if x < 0 || y < 0 || x >= md.xs as i32 || y >= md.ys as i32 {
                return Ok(());
            }
            let idx = (x + y * md.xs as i32) as usize;
            unsafe {
                *md.tile.add(idx) = val as u16;
            }
            Ok(())
        })?,
    )?;

    g.set(
        "setPass",
        lua.create_function(|_, (m, x, y, val): (i32, i32, i32, i32)| {
            if m < 0 {
                return Ok(());
            }
            let mp = get_map_ptr(m as u16);
            if mp.is_null() || unsafe { (*mp).registry.is_null() } {
                return Ok(());
            }
            let md = unsafe { &*mp };
            if x < 0 || y < 0 || x >= md.xs as i32 || y >= md.ys as i32 {
                return Ok(());
            }
            let idx = (x + y * md.xs as i32) as usize;
            unsafe {
                *md.pass.add(idx) = val as u16;
            }
            Ok(())
        })?,
    )?;

    g.set(
        "getPass",
        lua.create_function(|_, (m, x, y): (i32, i32, i32)| {
            if m < 0 {
                return Ok(Value::Nil);
            }
            let mp = get_map_ptr(m as u16);
            if mp.is_null() || unsafe { (*mp).registry.is_null() } {
                return Ok(Value::Nil);
            }
            let md = unsafe { &*mp };
            if x < 0 || y < 0 || x >= md.xs as i32 || y >= md.ys as i32 {
                return Ok(Value::Nil);
            }
            let idx = (x + y * md.xs as i32) as usize;
            Ok(Value::Integer(unsafe { *md.pass.add(idx) as i64 }))
        })?,
    )?;

    // -----------------------------------------------------------------------
    // Map: title, pvp, weather, registry
    // -----------------------------------------------------------------------
    g.set(
        "getMapTitle",
        lua.create_function(|_, m: i32| {
            if m < 0 {
                return Ok(String::new());
            }
            let mp = get_map_ptr(m as u16);
            if mp.is_null() || unsafe { (*mp).registry.is_null() } {
                return Ok(String::new());
            }
            let s = unsafe {
                CStr::from_ptr((*mp).title.as_ptr())
                    .to_string_lossy()
                    .into_owned()
            };
            Ok(s)
        })?,
    )?;

    g.set(
        "setMapTitle",
        lua.create_function(|_, (m, title): (i32, String)| {
            if m < 0 {
                return Ok(());
            }
            let mp = get_map_ptr(m as u16);
            if mp.is_null() || unsafe { (*mp).registry.is_null() } {
                return Ok(());
            }
            let bytes = title.as_bytes();
            let len = bytes.len().min(63);
            unsafe {
                let dst = (*mp).title.as_mut_ptr() as *mut u8;
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, len);
                *dst.add(len) = 0;
            }
            Ok(())
        })?,
    )?;

    g.set(
        "getMapPvP",
        lua.create_function(|_, m: i32| {
            if m < 0 {
                return Ok(0i64);
            }
            let mp = get_map_ptr(m as u16);
            if mp.is_null() || unsafe { (*mp).registry.is_null() } {
                return Ok(0i64);
            }
            Ok(unsafe { (*mp).pvp as i64 })
        })?,
    )?;

    g.set(
        "setMapPvP",
        lua.create_function(|_, (m, pvp): (i32, i32)| {
            if m < 0 {
                return Ok(());
            }
            let mp = get_map_ptr(m as u16);
            if mp.is_null() || unsafe { (*mp).registry.is_null() } {
                return Ok(());
            }
            unsafe {
                (*mp).pvp = pvp as u8;
            }
            Ok(())
        })?,
    )?;

    g.set(
        "getWeatherM",
        lua.create_function(|_, m: i32| {
            if m < 0 {
                return Ok(0i64);
            }
            let mp = get_map_ptr(m as u16);
            if mp.is_null() || unsafe { (*mp).registry.is_null() } {
                return Ok(0i64);
            }
            Ok(unsafe { (*mp).weather as i64 })
        })?,
    )?;

    g.set(
        "setWeatherM",
        lua.create_async_function(|_, (m, w): (i32, i32)| async move {
            unsafe {
                crate::game::scripting::map_globals::sl_g_setweatherm(m, w as u8).await;
            }
            Ok(())
        })?,
    )?;

    g.set(
        "getWeather",
        lua.create_function(|_, (region, indoor): (i32, i32)| {
            for id in 0..crate::database::map_db::MAP_SLOTS as u16 {
                let ptr = get_map_ptr(id);
                if ptr.is_null() {
                    continue;
                }
                let md = unsafe { &*ptr };
                if md.xs == 0 {
                    continue;
                }
                if md.region as i32 == region && md.indoor as i32 == indoor {
                    return Ok(md.weather as i64);
                }
            }
            Ok(0i64)
        })?,
    )?;

    g.set(
        "setWeather",
        lua.create_async_function(|_, (region, indoor, w): (i32, i32, i32)| async move {
            unsafe {
                crate::game::scripting::map_globals::sl_g_setweather(
                    region as u8,
                    indoor as u8,
                    w as u8,
                )
                .await;
            }
            Ok(())
        })?,
    )?;

    g.set(
        "setLight",
        lua.create_function(|_, (region, indoor, light): (i32, i32, i32)| {
            for id in 0..crate::database::map_db::MAP_SLOTS as u16 {
                let ptr = get_map_ptr(id);
                if ptr.is_null() {
                    continue;
                }
                let md = unsafe { &mut *ptr };
                if md.xs == 0 {
                    continue;
                }
                if md.region as i32 == region && md.indoor as i32 == indoor && md.light == 0 {
                    md.light = light as u8;
                }
            }
            Ok(())
        })?,
    )?;

    g.set(
        "getMapRegistry",
        lua.create_function(|_, (m, key): (i32, String)| {
            if m < 0 {
                return Ok(0i64);
            }
            let ptr = get_map_ptr(m as u16);
            if ptr.is_null() {
                return Ok(0i64);
            }
            let md = unsafe { &*ptr };
            if md.registry.is_null() {
                return Ok(0i64);
            }
            for i in 0..md.registry_num as usize {
                let reg = unsafe { &*md.registry.add(i) };
                let reg_str = unsafe { CStr::from_ptr(reg.str.as_ptr()) };
                if reg_str.to_string_lossy().eq_ignore_ascii_case(&key) {
                    return Ok(reg.val as i64);
                }
            }
            Ok(0i64)
        })?,
    )?;

    g.set(
        "setMapRegistry",
        lua.create_async_function(|_, (m, key, val): (i32, String, i32)| async move {
            unsafe {
                crate::game::map_server::map_setglobalreg_str(m, key, val).await;
            }
            Ok(())
        })?,
    )?;

    // -----------------------------------------------------------------------
    // Map: getMapAttribute / setMapAttribute
    // -----------------------------------------------------------------------
    g.set(
        "getMapAttribute",
        lua.create_function(|lua, (m, attr): (i32, String)| {
            if m < 0 {
                return Ok(Value::Nil);
            }
            let mp = get_map_ptr(m as u16);
            if mp.is_null() || unsafe { (*mp).registry.is_null() } {
                return Ok(Value::Nil);
            }
            let md = unsafe { &*mp };
            match attr.as_str() {
                "xmax" => Ok(Value::Integer(md.xs as i64 - 1)),
                "ymax" => Ok(Value::Integer(md.ys as i64 - 1)),
                "mapTitle" => {
                    let s = unsafe {
                        CStr::from_ptr(md.title.as_ptr())
                            .to_string_lossy()
                            .into_owned()
                    };
                    Ok(Value::String(lua.create_string(s.as_bytes())?))
                }
                "mapFile" => {
                    let s = unsafe {
                        CStr::from_ptr(md.mapfile.as_ptr())
                            .to_string_lossy()
                            .into_owned()
                    };
                    Ok(Value::String(lua.create_string(s.as_bytes())?))
                }
                "bgm" => Ok(Value::Integer(md.bgm as i64)),
                "bgmType" => Ok(Value::Integer(md.bgmtype as i64)),
                "pvp" => Ok(Value::Integer(md.pvp as i64)),
                "spell" => Ok(Value::Integer(md.spell as i64)),
                "light" => Ok(Value::Integer(md.light as i64)),
                "weather" => Ok(Value::Integer(md.weather as i64)),
                "sweepTime" => Ok(Value::Integer(md.sweeptime as i64)),
                "canTalk" => Ok(Value::Integer(md.cantalk as i64)),
                "showGhosts" => Ok(Value::Integer(md.show_ghosts as i64)),
                "region" => Ok(Value::Integer(md.region as i64)),
                "indoor" => Ok(Value::Integer(md.indoor as i64)),
                "warpOut" => Ok(Value::Integer(md.warpout as i64)),
                "bind" => Ok(Value::Integer(md.bind as i64)),
                "reqLvl" => Ok(Value::Integer(md.reqlvl as i64)),
                "reqVita" => Ok(Value::Integer(md.reqvita as i64)),
                "reqMana" => Ok(Value::Integer(md.reqmana as i64)),
                _ => Ok(Value::Nil),
            }
        })?,
    )?;

    g.set(
        "setMapAttribute",
        lua.create_function(|_, (m, attr, val): (i32, String, Value)| {
            if m < 0 {
                return Ok(());
            }
            let mp = get_map_ptr(m as u16);
            if mp.is_null() || unsafe { (*mp).registry.is_null() } {
                return Ok(());
            }
            let md = unsafe { &mut *mp };
            let ival = match &val {
                Value::Integer(i) => *i as i32,
                Value::Number(f) => *f as i32,
                _ => 0,
            };
            match attr.as_str() {
                "bgm" => md.bgm = ival as u16,
                "bgmType" => md.bgmtype = ival as u16,
                "pvp" => md.pvp = ival as u8,
                "spell" => md.spell = ival as u8,
                "light" => md.light = ival as u8,
                "weather" => md.weather = ival as u8,
                "sweepTime" => md.sweeptime = ival as u32,
                "canTalk" => md.cantalk = ival as u8,
                "showGhosts" => md.show_ghosts = ival as u8,
                "region" => md.region = ival as u8,
                "indoor" => md.indoor = ival as u8,
                "warpOut" => md.warpout = ival as u8,
                "bind" => md.bind = ival as u8,
                "reqLvl" => md.reqlvl = ival as u32,
                "reqVita" => md.reqvita = ival as u32,
                "reqMana" => md.reqmana = ival as u32,
                "mapTitle" => {
                    if let Value::String(s) = &val {
                        let bytes = s.as_bytes();
                        let len = bytes.len().min(63);
                        unsafe {
                            let dst = md.title.as_mut_ptr() as *mut u8;
                            std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, len);
                            *dst.add(len) = 0;
                        }
                    }
                }
                _ => {}
            }
            Ok(())
        })?,
    )?;

    // -----------------------------------------------------------------------
    // setMap (full map file load)
    // -----------------------------------------------------------------------
    g.set(
        "setMap",
        lua.create_function(|_, args: mlua::MultiValue| {
            let args: Vec<Value> = args.into_iter().collect();
            let cmapfile = CString::new(vs(&args, 1)).map_err(mlua::Error::external)?;
            let ctitle = CString::new(vs(&args, 2)).map_err(mlua::Error::external)?;
            unsafe {
                sl_g_setmap(
                    vi(&args, 0),
                    cmapfile.as_ptr(),
                    MapSettings {
                        title: ctitle.as_ptr(),
                        bgm: vi(&args, 3),
                        bgmtype: vi(&args, 4),
                        pvp: vi(&args, 5),
                        spell: vi(&args, 6),
                        light: vi(&args, 7) as u8,
                        weather: vi(&args, 8),
                        sweeptime: vi(&args, 9),
                        cantalk: vi(&args, 10),
                        show_ghosts: vi(&args, 11),
                        region: vi(&args, 12),
                        indoor: vi(&args, 13),
                        warpout: vi(&args, 14),
                        bind: vi(&args, 15),
                        reqlvl: vi(&args, 16),
                        reqvita: vi(&args, 17),
                        reqmana: vi(&args, 18),
                    },
                );
            }
            Ok(())
        })?,
    )?;

    // -----------------------------------------------------------------------
    // setPostColor / throw / saveMap
    // -----------------------------------------------------------------------
    g.set(
        "setPostColor",
        lua.create_async_function(|_, (board, post, color): (i32, i32, i32)| async move {
            unsafe { map_changepostcolor(board, post, color).await };
            Ok(())
        })?,
    )?;

    g.set(
        "throw",
        lua.create_function(|_, args: mlua::MultiValue| {
            let args: Vec<Value> = args.into_iter().collect();
            unsafe {
                sl_g_throw(
                    vi(&args, 0),
                    vi(&args, 1),
                    vi(&args, 2),
                    vi(&args, 3),
                    ThrowVisuals {
                        x2: vi(&args, 4),
                        y2: vi(&args, 5),
                        icon: vi(&args, 6),
                        color: vi(&args, 7),
                        action: vi(&args, 8),
                    },
                );
            }
            Ok(())
        })?,
    )?;

    g.set(
        "saveMap",
        lua.create_function(|_, (m, path): (i32, String)| {
            if m < 0 {
                return Ok(false);
            }
            let ptr = get_map_ptr(m as u16);
            if ptr.is_null() {
                return Ok(false);
            }
            let md = unsafe { &*ptr };
            if md.xs == 0 || md.tile.is_null() || md.pass.is_null() || md.obj.is_null() {
                return Ok(false);
            }
            use std::io::Write;
            let mut fp = match std::fs::File::create(&path) {
                Ok(f) => f,
                Err(_) => return Ok(false),
            };
            if fp.write_all(&md.xs.to_be_bytes()).is_err() {
                return Ok(false);
            }
            if fp.write_all(&md.ys.to_be_bytes()).is_err() {
                return Ok(false);
            }
            for pos in 0..(md.xs as usize * md.ys as usize) {
                let tile = unsafe { *md.tile.add(pos) };
                let pass = unsafe { *md.pass.add(pos) };
                let obj = unsafe { *md.obj.add(pos) };
                let _ = fp.write_all(&tile.to_be_bytes());
                let _ = fp.write_all(&pass.to_be_bytes());
                let _ = fp.write_all(&obj.to_be_bytes());
            }
            Ok(true)
        })?,
    )?;

    // -----------------------------------------------------------------------
    // Warps
    // -----------------------------------------------------------------------
    g.set(
        "getWarp",
        lua.create_function(|_, (m, x, y): (i32, i32, i32)| {
            use crate::database::map_db::{BLOCK_SIZE, MAP_SLOTS};
            if m < 0 || m as usize >= MAP_SLOTS {
                return Ok(false);
            }
            let ptr = get_map_ptr(m as u16);
            if ptr.is_null() {
                return Ok(false);
            }
            let md = unsafe { &*ptr };
            if md.xs == 0 || md.warp.is_null() {
                return Ok(false);
            }
            let x = x.clamp(0, md.xs as i32 - 1) as usize;
            let y = y.clamp(0, md.ys as i32 - 1) as usize;
            let idx = x / BLOCK_SIZE + (y / BLOCK_SIZE) * md.bxs as usize;
            let mut node = unsafe { *md.warp.add(idx) };
            while !node.is_null() {
                let n = unsafe { &*node };
                if n.x == x as i32 && n.y == y as i32 {
                    return Ok(true);
                }
                node = n.next;
            }
            Ok(false)
        })?,
    )?;

    g.set(
        "setWarps",
        lua.create_function(
            |_, (mm, mx, my, tm, tx, ty): (i32, i32, i32, i32, i32, i32)| {
                use crate::database::map_db::{WarpList, BLOCK_SIZE, MAP_SLOTS};
                if mm < 0 || mm as usize >= MAP_SLOTS {
                    return Ok(false);
                }
                if tm < 0 || tm as usize >= MAP_SLOTS {
                    return Ok(false);
                }
                let mm_ptr = get_map_ptr(mm as u16);
                let tm_ptr = get_map_ptr(tm as u16);
                if mm_ptr.is_null() || tm_ptr.is_null() {
                    return Ok(false);
                }
                let md = unsafe { &mut *mm_ptr };
                if md.xs == 0 || unsafe { (*tm_ptr).xs } == 0 {
                    return Ok(false);
                }
                if mx < 0 || my < 0 || mx >= md.xs as i32 || my >= md.ys as i32 {
                    return Ok(false);
                }
                if md.warp.is_null() {
                    return Ok(false);
                }
                let idx = mx as usize / BLOCK_SIZE + (my as usize / BLOCK_SIZE) * md.bxs as usize;
                let existing = unsafe { *md.warp.add(idx) };
                let war = Box::into_raw(Box::new(WarpList {
                    x: mx,
                    y: my,
                    tm,
                    tx,
                    ty,
                    next: existing,
                    prev: std::ptr::null_mut(),
                }));
                unsafe {
                    if !existing.is_null() {
                        (*existing).prev = war;
                    }
                    *md.warp.add(idx) = war;
                }
                Ok(true)
            },
        )?,
    )?;

    g.set(
        "getWarps",
        lua.create_function(|lua, _m: i32| {
            tracing::warn!("[scripting] getWarps: not yet implemented");
            lua.create_table()
        })?,
    )?;

    // -----------------------------------------------------------------------
    // Spell / mob DB
    // -----------------------------------------------------------------------
    g.set(
        "getSpellLevel",
        lua.create_function(|_, spell: String| {
            Ok(crate::database::magic_db::level_by_name(&spell) as i64)
        })?,
    )?;

    g.set(
        "getMobAttributes",
        lua.create_function(|lua, id: u32| {
            let tbl = lua.create_table()?;
            if let Some(d) = crate::database::mob_db::searchexist(id) {
                tbl.set(1, d.vita as i64)?;
                tbl.set(2, d.baseac as i64)?;
                tbl.set(3, d.exp as i64)?;
                tbl.set(4, d.might as i64)?;
                tbl.set(5, d.will as i64)?;
                tbl.set(6, d.grace as i64)?;
                tbl.set(7, d.look as i64)?;
                tbl.set(8, d.look_color as i64)?;
                tbl.set(9, d.level as i64)?;
                let name = unsafe {
                    CStr::from_ptr(d.name.as_ptr())
                        .to_string_lossy()
                        .into_owned()
                };
                let yname = unsafe {
                    CStr::from_ptr(d.yname.as_ptr())
                        .to_string_lossy()
                        .into_owned()
                };
                tbl.set(10, name)?;
                tbl.set(11, yname)?;
            }
            Ok(tbl)
        })?,
    )?;

    // -----------------------------------------------------------------------
    // addMob / checkOnline / getOfflineID
    // -----------------------------------------------------------------------
    g.set(
        "addMob",
        lua.create_async_function(|_, (m, x, y, mobid): (i32, i32, i32, i32)| async move {
            if !unsafe { crate::database::map_db::map_is_loaded(m as u16) } {
                return Ok(false);
            }
            let sid = crate::config::config().server_id;
            let ok = sqlx::query(&format!(
                "INSERT INTO `Spawns{}` (`SpnMapId`,`SpnX`,`SpnY`,`SpnMobId`,\
             `SpnLastDeath`,`SpnStartTime`,`SpnEndTime`,`SpnMobIdReplace`) \
             VALUES(?,?,?,?,0,25,25,0)",
                sid
            ))
            .bind(m)
            .bind(x)
            .bind(y)
            .bind(mobid)
            .execute(get_pool())
            .await
            .is_ok();
            Ok(ok)
        })?,
    )?;

    g.set(
        "checkOnline",
        lua.create_async_function(|_, v: Value| async move {
            let online: bool = match v {
                Value::Integer(id) => {
                    let id = id as u32;
                    sqlx::query_scalar!(
                        "SELECT COUNT(*) FROM `Character` WHERE `ChaOnline`=1 AND `ChaId`=?",
                        id
                    )
                    .fetch_one(get_pool())
                    .await
                    .map(|n: i64| n > 0)
                    .unwrap_or(false)
                }
                Value::Number(f) => {
                    let id = f as u32;
                    sqlx::query_scalar!(
                        "SELECT COUNT(*) FROM `Character` WHERE `ChaOnline`=1 AND `ChaId`=?",
                        id
                    )
                    .fetch_one(get_pool())
                    .await
                    .map(|n: i64| n > 0)
                    .unwrap_or(false)
                }
                Value::String(ref s) => {
                    let name = s.to_str()?.to_owned();
                    sqlx::query_scalar!(
                        "SELECT COUNT(*) FROM `Character` WHERE `ChaOnline`=1 AND `ChaName`=?",
                        name
                    )
                    .fetch_one(get_pool())
                    .await
                    .map(|n: i64| n > 0)
                    .unwrap_or(false)
                }
                _ => false,
            };
            Ok(online)
        })?,
    )?;

    g.set(
        "getOfflineID",
        lua.create_async_function(|_, name: String| async move {
            let id: u32 =
                sqlx::query_scalar!("SELECT `ChaId` FROM `Character` WHERE `ChaName`=?", name)
                    .fetch_optional(get_pool())
                    .await
                    .unwrap_or(None)
                    .unwrap_or(0);
            Ok(id as i64)
        })?,
    )?;

    // -----------------------------------------------------------------------
    // Map modifiers
    // -----------------------------------------------------------------------
    g.set(
        "getMapModifiers",
        lua.create_function(|lua, _: i32| {
            tracing::warn!("[scripting] getMapModifiers: not yet implemented");
            lua.create_table()
        })?,
    )?;

    g.set(
        "addMapModifier",
        lua.create_async_function(
            |_, (mapid, modifier, value): (u32, String, i32)| async move {
                let ok = sqlx::query!(
            "INSERT INTO `MapModifiers` (`ModMapId`,`ModModifier`,`ModValue`) VALUES(?,?,?)",
            mapid, modifier, value
        )
                .execute(get_pool())
                .await
                .is_ok();
                Ok(ok)
            },
        )?,
    )?;

    g.set(
        "removeMapModifier",
        lua.create_async_function(|_, (mapid, modifier): (u32, String)| async move {
            let ok = sqlx::query!(
                "DELETE FROM `MapModifiers` WHERE `ModMapId`=? AND `ModModifier`=?",
                mapid,
                modifier
            )
            .execute(get_pool())
            .await
            .is_ok();
            Ok(ok)
        })?,
    )?;

    g.set(
        "removeMapModifierId",
        lua.create_async_function(|_, mapid: u32| async move {
            let ok = sqlx::query!("DELETE FROM `MapModifiers` WHERE `ModMapId`=?", mapid)
                .execute(get_pool())
                .await
                .is_ok();
            Ok(ok)
        })?,
    )?;

    g.set(
        "getFreeMapModifierId",
        lua.create_async_function(|_, ()| async move {
            let max: Option<u32> =
                sqlx::query_scalar!("SELECT MAX(`ModMapId`) FROM `MapModifiers`")
                    .fetch_one(get_pool())
                    .await
                    .unwrap_or(None);
            Ok(max.unwrap_or(0) as i64 + 1)
        })?,
    )?;

    // -----------------------------------------------------------------------
    // WisdomStar
    // -----------------------------------------------------------------------
    g.set(
        "getWisdomStarMultiplier",
        lua.create_async_function(|_, ()| async move {
            let mult: f32 = sqlx::query_scalar!("SELECT `WSMultiplier` FROM `WisdomStar`")
                .fetch_optional(get_pool())
                .await
                .unwrap_or(None)
                .unwrap_or(0.0);
            Ok(mult as f64)
        })?,
    )?;

    g.set(
        "setWisdomStarMultiplier",
        lua.create_async_function(|_, (mult, _val): (f64, i32)| async move {
            let mult = mult as f32;
            let _ = sqlx::query!("UPDATE `WisdomStar` SET `WSMultiplier`=?", mult)
                .execute(get_pool())
                .await;
            Ok(())
        })?,
    )?;

    // -----------------------------------------------------------------------
    // KanDonationPoints
    // -----------------------------------------------------------------------
    // KanDonationPool table does not exist — KanDonations is per-transaction only.
    // These globals are no-ops; the C versions were silently failing against the same missing table.
    g.set(
        "getKanDonationPoints",
        lua.create_function(|_, ()| Ok(0i64))?,
    )?;
    g.set(
        "setKanDonationPoints",
        lua.create_function(|_, _: i32| Ok(()))?,
    )?;
    g.set(
        "addKanDonationPoints",
        lua.create_function(|_, _: i32| Ok(()))?,
    )?;

    g.set(
        "processKanDonations",
        lua.create_function(|_, ()| {
            tracing::warn!("[scripting] processKanDonations: not yet implemented");
            Ok(())
        })?,
    )?;

    // -----------------------------------------------------------------------
    // Clan tribute
    // -----------------------------------------------------------------------
    g.set(
        "getClanTribute",
        lua.create_async_function(|_, clan: i32| async move {
            let val: u32 =
                sqlx::query_scalar!("SELECT `ClnTribute` FROM `Clans` WHERE `ClnId`=?", clan)
                    .fetch_optional(get_pool())
                    .await
                    .unwrap_or(None)
                    .unwrap_or(0);
            Ok(val as i64)
        })?,
    )?;

    g.set(
        "setClanTribute",
        lua.create_async_function(|_, (clan, val): (i32, u32)| async move {
            let _ = sqlx::query!(
                "UPDATE `Clans` SET `ClnTribute`=? WHERE `ClnId`=?",
                val,
                clan
            )
            .execute(get_pool())
            .await;
            Ok(())
        })?,
    )?;

    g.set(
        "addClanTribute",
        lua.create_async_function(|_, (clan, val): (i32, u32)| async move {
            let _ = sqlx::query!(
                "UPDATE `Clans` SET `ClnTribute`=`ClnTribute`+? WHERE `ClnId`=?",
                val,
                clan
            )
            .execute(get_pool())
            .await;
            Ok(())
        })?,
    )?;

    // -----------------------------------------------------------------------
    // Clan name
    // -----------------------------------------------------------------------
    g.set(
        "getClanName",
        lua.create_async_function(|_, clan: i32| async move {
            let name: String =
                sqlx::query_scalar!("SELECT `ClnName` FROM `Clans` WHERE `ClnId`=?", clan)
                    .fetch_optional(get_pool())
                    .await
                    .unwrap_or(None)
                    .unwrap_or_default();
            Ok(name)
        })?,
    )?;

    g.set(
        "setClanName",
        lua.create_async_function(|_, (clan, name): (i32, String)| async move {
            let _ = sqlx::query!("UPDATE `Clans` SET `ClnName`=? WHERE `ClnId`=?", name, clan)
                .execute(get_pool())
                .await;
            // Update in-memory ClanData if the clan is currently loaded.
            // SAFETY: mutating through Arc requires raw pointer cast — matches pre-Arc behavior.
            if let Some(arc) = crate::database::clan_db::searchexist(clan) {
                let ptr = Arc::as_ptr(&arc) as *mut crate::database::clan_db::ClanData;
                let dst = unsafe { &mut (*ptr).name };
                let bytes = name.as_bytes();
                let copy_len = bytes.len().min(dst.len() - 1);
                for (i, &b) in bytes.iter().take(copy_len).enumerate() {
                    dst[i] = b as i8;
                }
                dst[copy_len] = 0;
            }
            Ok(())
        })?,
    )?;

    // -----------------------------------------------------------------------
    // Clan bank slots
    // -----------------------------------------------------------------------
    g.set(
        "getClanBankSlots",
        lua.create_async_function(|_, clan: i32| async move {
            let val: u32 =
                sqlx::query_scalar!("SELECT `ClnBankSlots` FROM `Clans` WHERE `ClnId`=?", clan)
                    .fetch_optional(get_pool())
                    .await
                    .unwrap_or(None)
                    .unwrap_or(0);
            Ok(val as i64)
        })?,
    )?;

    g.set(
        "setClanBankSlots",
        lua.create_async_function(|_, (clan, val): (i32, i32)| async move {
            let _ = sqlx::query!(
                "UPDATE `Clans` SET `ClnBankSlots`=? WHERE `ClnId`=?",
                val,
                clan
            )
            .execute(get_pool())
            .await;
            Ok(())
        })?,
    )?;

    // -----------------------------------------------------------------------
    // Clan roster (table-returning — stub)
    // -----------------------------------------------------------------------
    g.set(
        "getClanRoster",
        lua.create_function(|lua, _: i32| {
            tracing::warn!("[scripting] getClanRoster: not yet implemented");
            lua.create_table()
        })?,
    )?;

    // -----------------------------------------------------------------------
    // Clan member management
    // -----------------------------------------------------------------------
    g.set("removeClanMember", lua.create_async_function(|_, id: i32| async move {
        // Mutate in-memory session data synchronously before any await point.
        if let Some(arc) = crate::game::map_server::map_id2sd_pc(id as u32) {
            {
                let sd = &mut *arc.write();
                sd.player.social.clan = 0;
                sd.player.social.clan_title.clear();
                sd.player.progression.clan_rank = 0;
            }
            unsafe { crate::game::map_parse::player_state::clif_mystatus(&arc); }
        }
        let ok = sqlx::query!(
            "UPDATE `Character` SET `ChaClnId`='0',`ChaClanTitle`='',`ChaClnRank`='0' WHERE `ChaId`=?",
            id as u32
        ).execute(get_pool()).await.map(|r| r.rows_affected() > 0).unwrap_or(false);
        Ok(ok)
    })?)?;

    g.set("addClanMember", lua.create_async_function(|_, (id, clan): (i32, i32)| async move {
        if let Some(arc) = crate::game::map_server::map_id2sd_pc(id as u32) {
            {
                let sd = &mut *arc.write();
                sd.player.social.clan = clan as u32;
                sd.player.social.clan_title.clear();
                sd.player.progression.clan_rank = 1;
            }
            unsafe { crate::game::map_parse::player_state::clif_mystatus(&arc); }
        }
        let ok = sqlx::query!(
            "UPDATE `Character` SET `ChaClnId`=?,`ChaClanTitle`='',`ChaClnRank`='1' WHERE `ChaId`=?",
            clan as u32, id as u32
        ).execute(get_pool()).await.map(|r| r.rows_affected() > 0).unwrap_or(false);
        Ok(ok)
    })?)?;

    g.set(
        "updateClanMemberRank",
        lua.create_async_function(|_, (id, rank): (i32, i32)| async move {
            // Mutate in-memory session data synchronously before any await point.
            if let Some(arc) = crate::game::map_server::map_id2sd_pc(id as u32) {
                let sd = &mut *arc.write();
                sd.player.progression.clan_rank = rank;
            }
            let ok = sqlx::query!(
                "UPDATE `Character` SET `ChaClnRank`=? WHERE `ChaId`=?",
                rank,
                id as u32
            )
            .execute(get_pool())
            .await
            .map(|r| r.rows_affected() > 0)
            .unwrap_or(false);
            Ok(ok)
        })?,
    )?;

    g.set(
        "updateClanMemberTitle",
        lua.create_async_function(|_, (id, title): (i32, String)| async move {
            // Mutate in-memory session data synchronously before any await point.
            if let Some(arc) = crate::game::map_server::map_id2sd_pc(id as u32) {
                {
                    let sd = &mut *arc.write();
                    sd.player.social.clan_title = title.clone();
                }
                unsafe {
                    crate::game::map_parse::player_state::clif_mystatus(&arc);
                }
            }
            let ok = sqlx::query!(
                "UPDATE `Character` SET `ChaClanTitle`=? WHERE `ChaId`=?",
                title,
                id as u32
            )
            .execute(get_pool())
            .await
            .map(|r| r.rows_affected() > 0)
            .unwrap_or(false);
            Ok(ok)
        })?,
    )?;

    // -----------------------------------------------------------------------
    // Path member management
    // -----------------------------------------------------------------------
    g.set(
        "removePathMember",
        lua.create_async_function(|_, id: i32| async move {
            // Online path: mutate in-memory state, then drop the lock before .await.
            let online_class = if let Some(arc) = crate::game::map_server::map_id2sd_pc(id as u32) {
                let new_class = {
                    let sd = &mut *arc.write();
                    let new_class =
                        crate::database::class_db::path(sd.player.progression.class as i32) as u8;
                    sd.player.progression.class = new_class;
                    sd.player.progression.class_rank = 0;
                    arc.set_class_level(new_class, sd.player.progression.level);
                    new_class
                };
                unsafe {
                    crate::game::map_parse::player_state::clif_mystatus(&arc);
                }
                Some(new_class)
            } else {
                None
            };
            let new_class = if let Some(c) = online_class {
                c as u32
            } else {
                // Offline path: fetch current class, apply path().
                let pth: u32 = sqlx::query_scalar!(
                    "SELECT `ChaPthId` FROM `Character` WHERE `ChaId`=?",
                    id as u32
                )
                .fetch_optional(get_pool())
                .await
                .unwrap_or(None)
                .unwrap_or(0);
                crate::database::class_db::path(pth as i32) as u32
            };
            let ok = sqlx::query!(
                "UPDATE `Character` SET `ChaPthId`=?,`ChaPthRank`='0' WHERE `ChaId`=?",
                new_class,
                id as u32
            )
            .execute(get_pool())
            .await
            .map(|r| r.rows_affected() > 0)
            .unwrap_or(false);
            Ok(ok)
        })?,
    )?;

    g.set(
        "addPathMember",
        lua.create_async_function(|_, (id, cls): (i32, i32)| async move {
            // Mutate in-memory session data synchronously before any await point.
            if let Some(arc) = crate::game::map_server::map_id2sd_pc(id as u32) {
                {
                    let sd = &mut *arc.write();
                    sd.player.progression.class = cls as u8;
                    sd.player.progression.class_rank = 0;
                    arc.set_class_level(cls as u8, sd.player.progression.level);
                }
                unsafe {
                    crate::game::map_parse::player_state::clif_mystatus(&arc);
                }
            }
            let ok = sqlx::query!(
                "UPDATE `Character` SET `ChaPthId`=?,`ChaPthRank`='0' WHERE `ChaId`=?",
                cls as u32,
                id as u32
            )
            .execute(get_pool())
            .await
            .map(|r| r.rows_affected() > 0)
            .unwrap_or(false);
            Ok(ok)
        })?,
    )?;

    // setOfflinePlayerRegistry — core logic was commented out in C, no-op.
    g.set(
        "setOfflinePlayerRegistry",
        lua.create_function(|_, _: mlua::MultiValue| Ok(()))?,
    )?;

    // -----------------------------------------------------------------------
    // XP for level
    // -----------------------------------------------------------------------
    g.set(
        "getXPforLevel",
        lua.create_function(|_, (path, level): (i32, i32)| {
            let path = if path > 5 {
                crate::database::class_db::path(path)
            } else {
                path
            };
            Ok(crate::database::class_db::level(path, level) as i64)
        })?,
    )?;

    // -----------------------------------------------------------------------
    // Board / poetry (stubs — complex DB)
    // -----------------------------------------------------------------------
    g.set(
        "addToBoard",
        lua.create_function(|_, _: mlua::MultiValue| {
            tracing::warn!("[scripting] addToBoard: not yet implemented");
            Ok(())
        })?,
    )?;
    g.set(
        "selectBulletinBoard",
        lua.create_function(|_, _: mlua::MultiValue| {
            tracing::warn!("[scripting] selectBulletinBoard: not yet implemented");
            Ok(())
        })?,
    )?;
    g.set(
        "getPoems",
        lua.create_function(|lua, _: mlua::MultiValue| {
            tracing::warn!("[scripting] getPoems: not yet implemented");
            lua.create_table()
        })?,
    )?;
    g.set(
        "clearPoems",
        lua.create_function(|_, _: mlua::MultiValue| {
            tracing::warn!("[scripting] clearPoems: not yet implemented");
            Ok(())
        })?,
    )?;
    g.set(
        "copyPoemToPoetry",
        lua.create_function(|_, _: mlua::MultiValue| {
            tracing::warn!("[scripting] copyPoemToPoetry: not yet implemented");
            Ok(())
        })?,
    )?;

    // -----------------------------------------------------------------------
    // Auction (stubs)
    // -----------------------------------------------------------------------
    g.set(
        "getAuctions",
        lua.create_function(|lua, _: mlua::MultiValue| {
            tracing::warn!("[scripting] getAuctions: not yet implemented");
            lua.create_table()
        })?,
    )?;
    g.set(
        "listAuction",
        lua.create_function(|_, _: mlua::MultiValue| {
            tracing::warn!("[scripting] listAuction: not yet implemented");
            Ok(())
        })?,
    )?;
    g.set(
        "removeAuction",
        lua.create_function(|_, _: mlua::MultiValue| {
            tracing::warn!("[scripting] removeAuction: not yet implemented");
            Ok(())
        })?,
    )?;

    // -----------------------------------------------------------------------
    // getSetItems (stub) / guitext (no-op, commented out in C too)
    // -----------------------------------------------------------------------
    g.set(
        "getSetItems",
        lua.create_function(|lua, _: mlua::MultiValue| {
            tracing::warn!("[scripting] getSetItems: not yet implemented");
            lua.create_table()
        })?,
    )?;
    g.set(
        "guitext",
        lua.create_function(|_, _: mlua::MultiValue| Ok(()))?,
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn realtime() -> (i32, i32, i32, i32) {
    use chrono::{Datelike, Local, Timelike};
    let now = Local::now();
    (
        now.weekday().num_days_from_sunday() as i32,
        now.hour() as i32,
        now.minute() as i32,
        now.second() as i32,
    )
}

fn vi(args: &[Value], idx: usize) -> i32 {
    args.get(idx)
        .map(|v| match v {
            Value::Integer(i) => *i as i32,
            Value::Number(f) => *f as i32,
            _ => 0,
        })
        .unwrap_or(0)
}

fn vs(args: &[Value], idx: usize) -> String {
    args.get(idx)
        .map(|v| match v {
            Value::String(s) => s.to_str().map(|s| s.to_owned()).unwrap_or_default(),
            _ => String::new(),
        })
        .unwrap_or_default()
}
