use std::ffi::{CStr, CString};
use std::io::Write;

use mlua::prelude::*;

use crate::database::map_db::{map_data, map_data_mut, MAP_SLOTS};
use crate::game::map_server::{map_changepostcolor, map_setglobalreg_str};
use crate::game::scripting::map_globals::{
    sl_g_setmap, sl_g_setweather, sl_g_setweatherm, sl_g_throw, MapSettings, ThrowVisuals,
};

use super::map_dimensions::loaded_map;

// TODO(next pass): refactor all underlying functions to take &str/String instead of *const i8:
//   - sl_g_setmap → struct with String fields, not *const i8
//   - sl_g_throw → typed struct
//   - MapData.title / MapData.mapfile → String instead of [i8; 64]
//   - map registry CStr reads → String fields
// Once done, remove all CStr/CString usage from this file.

pub fn register(lua: &Lua) -> LuaResult<()> {
    let g = lua.globals();

    // ── Title ──

    g.set("getMapTitle", lua.create_function(|_, m: i32| {
        let Some(md) = loaded_map(m) else { return Ok(String::new()) };
        // TODO: MapData.title should be String
        let s = unsafe { CStr::from_ptr(md.title.as_ptr()).to_string_lossy().into_owned() };
        Ok(s)
    })?)?;

    g.set("setMapTitle", lua.create_function(|_, (m, title): (i32, String)| {
        let Some(md) = map_data_mut(m as usize).filter(|md| !md.registry.is_null()) else { return Ok(()) };
        // TODO: MapData.title should be String
        let bytes = title.as_bytes();
        let len = bytes.len().min(63);
        unsafe {
            let dst = md.title.as_mut_ptr() as *mut u8;
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, len);
            *dst.add(len) = 0;
        }
        Ok(())
    })?)?;

    // ── PvP ──

    g.set("getMapPvP", lua.create_function(|_, m: i32| {
        Ok(loaded_map(m).map(|md| md.pvp as i64).unwrap_or(0))
    })?)?;

    g.set("setMapPvP", lua.create_function(|_, (m, pvp): (i32, i32)| {
        if let Some(md) = map_data_mut(m as usize).filter(|md| !md.registry.is_null()) {
            md.pvp = pvp as u8;
        }
        Ok(())
    })?)?;

    // ── Weather / Light ──

    g.set("getWeatherM", lua.create_function(|_, m: i32| {
        Ok(loaded_map(m).map(|md| md.weather as i64).unwrap_or(0))
    })?)?;

    g.set("setWeatherM", lua.create_async_function(|_, (m, w): (i32, i32)| async move {
        unsafe { sl_g_setweatherm(m, w as u8).await; }
        Ok(())
    })?)?;

    g.set("getWeather", lua.create_function(|_, (region, indoor): (i32, i32)| {
        for id in 0..MAP_SLOTS as u16 {
            let Some(md) = map_data(id as usize) else { continue };
            if md.xs == 0 { continue; }
            if md.region as i32 == region && md.indoor as i32 == indoor {
                return Ok(md.weather as i64);
            }
        }
        Ok(0i64)
    })?)?;

    g.set("setWeather", lua.create_async_function(|_, (region, indoor, w): (i32, i32, i32)| async move {
        unsafe { sl_g_setweather(region as u8, indoor as u8, w as u8).await; }
        Ok(())
    })?)?;

    g.set("setLight", lua.create_function(|_, (region, indoor, light): (i32, i32, i32)| {
        for id in 0..MAP_SLOTS as u16 {
            let Some(md) = map_data_mut(id as usize) else { continue };
            if md.xs == 0 { continue; }
            if md.region as i32 == region && md.indoor as i32 == indoor && md.light == 0 {
                md.light = light as u8;
            }
        }
        Ok(())
    })?)?;

    // ── Map registry ──

    g.set("getMapRegistry", lua.create_function(|_, (m, key): (i32, String)| {
        let Some(md) = loaded_map(m) else { return Ok(0i64) };
        // TODO: registry entries should use String keys
        for i in 0..md.registry_num as usize {
            let reg = unsafe { &*md.registry.add(i) };
            let reg_str = unsafe { CStr::from_ptr(reg.str.as_ptr()) };
            if reg_str.to_string_lossy().eq_ignore_ascii_case(&key) {
                return Ok(reg.val as i64);
            }
        }
        Ok(0i64)
    })?)?;

    g.set("setMapRegistry", lua.create_async_function(|_, (m, key, val): (i32, String, i32)| async move {
        unsafe { map_setglobalreg_str(m, key, val).await; }
        Ok(())
    })?)?;

    // ── Map attribute get/set ──

    g.set("getMapAttribute", lua.create_function(|lua, (m, attr): (i32, String)| {
        let Some(md) = loaded_map(m) else { return Ok(LuaValue::Nil) };
        match attr.as_str() {
            "xmax" => Ok(LuaValue::Integer(md.xs as i64 - 1)),
            "ymax" => Ok(LuaValue::Integer(md.ys as i64 - 1)),
            // TODO: MapData.title/mapfile should be String
            "mapTitle" => {
                let s = unsafe { CStr::from_ptr(md.title.as_ptr()).to_string_lossy().into_owned() };
                Ok(LuaValue::String(lua.create_string(&s)?))
            }
            "mapFile" => {
                let s = unsafe { CStr::from_ptr(md.mapfile.as_ptr()).to_string_lossy().into_owned() };
                Ok(LuaValue::String(lua.create_string(&s)?))
            }
            "bgm" => Ok(LuaValue::Integer(md.bgm as i64)),
            "bgmType" => Ok(LuaValue::Integer(md.bgmtype as i64)),
            "pvp" => Ok(LuaValue::Integer(md.pvp as i64)),
            "spell" => Ok(LuaValue::Integer(md.spell as i64)),
            "light" => Ok(LuaValue::Integer(md.light as i64)),
            "weather" => Ok(LuaValue::Integer(md.weather as i64)),
            "sweepTime" => Ok(LuaValue::Integer(md.sweeptime as i64)),
            "canTalk" => Ok(LuaValue::Integer(md.cantalk as i64)),
            "showGhosts" => Ok(LuaValue::Integer(md.show_ghosts as i64)),
            "region" => Ok(LuaValue::Integer(md.region as i64)),
            "indoor" => Ok(LuaValue::Integer(md.indoor as i64)),
            "warpOut" => Ok(LuaValue::Integer(md.warpout as i64)),
            "bind" => Ok(LuaValue::Integer(md.bind as i64)),
            "reqLvl" => Ok(LuaValue::Integer(md.reqlvl as i64)),
            "reqVita" => Ok(LuaValue::Integer(md.reqvita as i64)),
            "reqMana" => Ok(LuaValue::Integer(md.reqmana as i64)),
            _ => Ok(LuaValue::Nil),
        }
    })?)?;

    g.set("setMapAttribute", lua.create_function(|_, (m, attr, val): (i32, String, LuaValue)| {
        let Some(md) = map_data_mut(m as usize).filter(|md| !md.registry.is_null()) else { return Ok(()) };
        let ival = match &val {
            LuaValue::Integer(i) => *i as i32,
            LuaValue::Number(f) => *f as i32,
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
            // TODO: MapData.title should be String
            "mapTitle" => {
                if let LuaValue::String(s) = &val {
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
    })?)?;

    // ── setMap / setPostColor / throw / saveMap ──
    // TODO: sl_g_setmap takes *const i8 — refactor to take &str/struct

    g.set("setMap", lua.create_function(|_, args: LuaMultiValue| {
        let args: Vec<LuaValue> = args.into_iter().collect();
        let mapfile = lua_str(&args, 1);
        let title = lua_str(&args, 2);
        let cmapfile = CString::new(mapfile).map_err(LuaError::external)?;
        let ctitle = CString::new(title).map_err(LuaError::external)?;
        unsafe {
            sl_g_setmap(
                lua_int(&args, 0),
                cmapfile.as_ptr(),
                MapSettings {
                    title: ctitle.as_ptr(),
                    bgm: lua_int(&args, 3),
                    bgmtype: lua_int(&args, 4),
                    pvp: lua_int(&args, 5),
                    spell: lua_int(&args, 6),
                    light: lua_int(&args, 7) as u8,
                    weather: lua_int(&args, 8),
                    sweeptime: lua_int(&args, 9),
                    cantalk: lua_int(&args, 10),
                    show_ghosts: lua_int(&args, 11),
                    region: lua_int(&args, 12),
                    indoor: lua_int(&args, 13),
                    warpout: lua_int(&args, 14),
                    bind: lua_int(&args, 15),
                    reqlvl: lua_int(&args, 16),
                    reqvita: lua_int(&args, 17),
                    reqmana: lua_int(&args, 18),
                },
            );
        }
        Ok(())
    })?)?;

    g.set("setPostColor", lua.create_async_function(|_, (board, post, color): (i32, i32, i32)| async move {
        unsafe { map_changepostcolor(board, post, color).await };
        Ok(())
    })?)?;

    // TODO: sl_g_throw — refactor to typed struct
    g.set("throw", lua.create_function(|_, args: LuaMultiValue| {
        let args: Vec<LuaValue> = args.into_iter().collect();
        unsafe {
            sl_g_throw(
                lua_int(&args, 0), lua_int(&args, 1), lua_int(&args, 2), lua_int(&args, 3),
                ThrowVisuals {
                    x2: lua_int(&args, 4),
                    y2: lua_int(&args, 5),
                    icon: lua_int(&args, 6),
                    color: lua_int(&args, 7),
                    action: lua_int(&args, 8),
                },
            );
        }
        Ok(())
    })?)?;

    g.set("saveMap", lua.create_function(|_, (m, path): (i32, String)| {
        let Some(md) = loaded_map(m) else { return Ok(false) };
        if md.xs == 0 || md.tile.is_null() || md.pass.is_null() || md.obj.is_null() {
            return Ok(false);
        }
        let mut fp = match std::fs::File::create(&path) {
            Ok(f) => f,
            Err(_) => return Ok(false),
        };
        if fp.write_all(&md.xs.to_be_bytes()).is_err() { return Ok(false); }
        if fp.write_all(&md.ys.to_be_bytes()).is_err() { return Ok(false); }
        for pos in 0..(md.xs as usize * md.ys as usize) {
            let tile = unsafe { *md.tile.add(pos) };
            let pass = unsafe { *md.pass.add(pos) };
            let obj = unsafe { *md.obj.add(pos) };
            let _ = fp.write_all(&tile.to_be_bytes());
            let _ = fp.write_all(&pass.to_be_bytes());
            let _ = fp.write_all(&obj.to_be_bytes());
        }
        Ok(true)
    })?)?;

    Ok(())
}

// TODO: remove once sl_g_setmap/sl_g_throw are refactored to take typed structs
fn lua_int(args: &[LuaValue], idx: usize) -> i32 {
    match args.get(idx) {
        Some(LuaValue::Integer(i)) => *i as i32,
        Some(LuaValue::Number(f)) => *f as i32,
        _ => 0,
    }
}

fn lua_str(args: &[LuaValue], idx: usize) -> String {
    match args.get(idx) {
        Some(LuaValue::String(s)) => s.to_str().map(|s| s.to_owned()).unwrap_or_default(),
        _ => String::new(),
    }
}
