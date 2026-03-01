//! Global Lua functions (91 total) — registered in sl_init.

use std::ffi::{CStr, CString, c_int, c_uint, c_uchar};
use mlua::{Lua, Value};

use crate::ffi::map_db::get_map_ptr;
use crate::game::scripting::ffi as sffi;

/// Register all 91 Lua globals on the given Lua state.
pub fn register(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();

    // -----------------------------------------------------------------------
    // BL type constants — used by getObjectsInCell, foreachincell etc.
    // -----------------------------------------------------------------------
    g.set("BL_PC",   sffi::BL_PC   as i64)?;
    g.set("BL_MOB",  sffi::BL_MOB  as i64)?;
    g.set("BL_NPC",  sffi::BL_NPC  as i64)?;
    g.set("BL_ITEM", sffi::BL_ITEM as i64)?;
    g.set("BL_ALL",  sffi::BL_ALL  as i64)?;

    // -----------------------------------------------------------------------
    // Async coroutines — Phase 5 stubs
    // -----------------------------------------------------------------------
    g.set("_async", lua.create_function(|_, _: mlua::MultiValue| {
        tracing::warn!("[scripting] _async: Phase 5 not yet implemented");
        Ok(())
    })?)?;
    g.set("_asyncDone", lua.create_function(|_, _: mlua::MultiValue| {
        Ok(())
    })?)?;

    // -----------------------------------------------------------------------
    // Tick / time
    // -----------------------------------------------------------------------
    g.set("getTick", lua.create_function(|_, ()| {
        Ok(unsafe { crate::ffi::timer::gettick() } as i64)
    })?)?;

    g.set("timeMS", lua.create_function(|_, ()| {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        Ok(ms)
    })?)?;

    g.set("msleep", lua.create_function(|_, _ms: i64| {
        // Intentional no-op — must not block the game thread.
        Ok(())
    })?)?;

    g.set("curServer", lua.create_function(|_, ()| {
        Ok(unsafe { sffi::serverid } as i64)
    })?)?;

    g.set("curYear", lua.create_function(|_, ()| {
        Ok(unsafe { sffi::cur_year } as i64)
    })?)?;

    g.set("curSeason", lua.create_function(|_, ()| {
        Ok(unsafe { sffi::cur_season } as i64)
    })?)?;

    g.set("curDay", lua.create_function(|_, ()| {
        Ok(unsafe { sffi::cur_day } as i64)
    })?)?;

    g.set("curTime", lua.create_function(|_, ()| {
        Ok(unsafe { sffi::cur_time } as i64)
    })?)?;

    g.set("realDay",    lua.create_function(|_, ()| Ok(realtime().0 as i64))?)?;
    g.set("realHour",   lua.create_function(|_, ()| Ok(realtime().1 as i64))?)?;
    g.set("realMinute", lua.create_function(|_, ()| Ok(realtime().2 as i64))?)?;
    g.set("realSecond", lua.create_function(|_, ()| Ok(realtime().3 as i64))?)?;

    // -----------------------------------------------------------------------
    // Broadcast / comms
    // -----------------------------------------------------------------------
    g.set("broadcast", lua.create_function(|_, (m, msg): (i32, String)| {
        let cmsg = CString::new(msg).map_err(mlua::Error::external)?;
        unsafe { sffi::clif_broadcast(cmsg.as_ptr(), m as c_int); }
        Ok(())
    })?)?;

    g.set("gmbroadcast", lua.create_function(|_, (m, msg): (i32, String)| {
        let cmsg = CString::new(msg).map_err(mlua::Error::external)?;
        unsafe { sffi::clif_gmbroadcast(cmsg.as_ptr(), m as c_int); }
        Ok(())
    })?)?;

    g.set("luaReload", lua.create_function(|_, ()| {
        unsafe { crate::game::scripting::sl_reload(); }
        Ok(())
    })?)?;

    g.set("sendMeta", lua.create_function(|_, ()| {
        unsafe { sffi::sl_g_sendmeta(); }
        Ok(())
    })?)?;

    // -----------------------------------------------------------------------
    // Map: dimensions, load status, user count
    // -----------------------------------------------------------------------
    g.set("getMapIsLoaded", lua.create_function(|_, m: i32| {
        let mp = unsafe { get_map_ptr(m as u16) };
        if mp.is_null() { return Ok(false); }
        Ok(unsafe { !(*mp).registry.is_null() })
    })?)?;

    g.set("getMapUsers", lua.create_function(|_, m: i32| {
        let mp = unsafe { get_map_ptr(m as u16) };
        if mp.is_null() || unsafe { (*mp).registry.is_null() } { return Ok(0i64); }
        Ok(unsafe { (*mp).user as i64 })
    })?)?;

    g.set("getMapXMax", lua.create_function(|_, m: i32| {
        let mp = unsafe { get_map_ptr(m as u16) };
        if mp.is_null() || unsafe { (*mp).registry.is_null() } { return Ok(0i64); }
        Ok(unsafe { (*mp).xs as i64 - 1 })
    })?)?;

    g.set("getMapYMax", lua.create_function(|_, m: i32| {
        let mp = unsafe { get_map_ptr(m as u16) };
        if mp.is_null() || unsafe { (*mp).registry.is_null() } { return Ok(0i64); }
        Ok(unsafe { (*mp).ys as i64 - 1 })
    })?)?;

    // -----------------------------------------------------------------------
    // Map: tile / object / pass arrays
    // -----------------------------------------------------------------------
    g.set("getObjectsMap", lua.create_function(|lua, _: mlua::MultiValue| {
        // Not implemented in the original C either (commented out).
        Ok(lua.create_table()?)
    })?)?;

    g.set("getObject", lua.create_function(|_, (m, x, y): (i32, i32, i32)| {
        let mp = unsafe { get_map_ptr(m as u16) };
        if mp.is_null() || unsafe { (*mp).registry.is_null() } { return Ok(0i64); }
        let md = unsafe { &*mp };
        let idx = (x + y * md.xs as i32) as usize;
        Ok(unsafe { *md.obj.add(idx) as i64 })
    })?)?;

    g.set("setObject", lua.create_function(|_, (m, x, y, val): (i32, i32, i32, i32)| {
        let mp = unsafe { get_map_ptr(m as u16) };
        if mp.is_null() || unsafe { (*mp).registry.is_null() } { return Ok(()); }
        let md = unsafe { &*mp };
        let idx = (x + y * md.xs as i32) as usize;
        unsafe { *md.obj.add(idx) = val as u16; }
        // map_foreachinarea(sl_updatepeople) omitted until foreachinarea is ported.
        Ok(())
    })?)?;

    g.set("getTile", lua.create_function(|_, (m, x, y): (i32, i32, i32)| {
        let mp = unsafe { get_map_ptr(m as u16) };
        if mp.is_null() || unsafe { (*mp).registry.is_null() } { return Ok(0i64); }
        let md = unsafe { &*mp };
        let idx = (x + y * md.xs as i32) as usize;
        Ok(unsafe { *md.tile.add(idx) as i64 })
    })?)?;

    g.set("setTile", lua.create_function(|_, (m, x, y, val): (i32, i32, i32, i32)| {
        let mp = unsafe { get_map_ptr(m as u16) };
        if mp.is_null() || unsafe { (*mp).registry.is_null() } { return Ok(()); }
        let md = unsafe { &*mp };
        let idx = (x + y * md.xs as i32) as usize;
        unsafe { *md.tile.add(idx) = val as u16; }
        Ok(())
    })?)?;

    g.set("setPass", lua.create_function(|_, (m, x, y, val): (i32, i32, i32, i32)| {
        let mp = unsafe { get_map_ptr(m as u16) };
        if mp.is_null() || unsafe { (*mp).registry.is_null() } { return Ok(()); }
        let md = unsafe { &*mp };
        let idx = (x + y * md.xs as i32) as usize;
        unsafe { *md.pass.add(idx) = val as u16; }
        Ok(())
    })?)?;

    g.set("getPass", lua.create_function(|_, (m, x, y): (i32, i32, i32)| {
        let mp = unsafe { get_map_ptr(m as u16) };
        if mp.is_null() || unsafe { (*mp).registry.is_null() } { return Ok(0i64); }
        let md = unsafe { &*mp };
        if x > md.xs as i32 - 1 || y > md.ys as i32 - 1 { return Ok(1i64); }
        let idx = (x + y * md.xs as i32) as usize;
        Ok(unsafe { *md.pass.add(idx) as i64 })
    })?)?;

    // -----------------------------------------------------------------------
    // Map: title, pvp, weather, registry
    // -----------------------------------------------------------------------
    g.set("getMapTitle", lua.create_function(|_, m: i32| {
        let mp = unsafe { get_map_ptr(m as u16) };
        if mp.is_null() || unsafe { (*mp).registry.is_null() } { return Ok(String::new()); }
        let s = unsafe { CStr::from_ptr((*mp).title.as_ptr()).to_string_lossy().into_owned() };
        Ok(s)
    })?)?;

    g.set("setMapTitle", lua.create_function(|_, (m, title): (i32, String)| {
        let mp = unsafe { get_map_ptr(m as u16) };
        if mp.is_null() || unsafe { (*mp).registry.is_null() } { return Ok(()); }
        let bytes = title.as_bytes();
        let len = bytes.len().min(63);
        unsafe {
            let dst = (*mp).title.as_mut_ptr() as *mut u8;
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, len);
            *dst.add(len) = 0;
        }
        Ok(())
    })?)?;

    g.set("getMapPvP", lua.create_function(|_, m: i32| {
        let mp = unsafe { get_map_ptr(m as u16) };
        if mp.is_null() || unsafe { (*mp).registry.is_null() } { return Ok(0i64); }
        Ok(unsafe { (*mp).pvp as i64 })
    })?)?;

    g.set("setMapPvP", lua.create_function(|_, (m, pvp): (i32, i32)| {
        let mp = unsafe { get_map_ptr(m as u16) };
        if mp.is_null() || unsafe { (*mp).registry.is_null() } { return Ok(()); }
        unsafe { (*mp).pvp = pvp as c_uchar; }
        Ok(())
    })?)?;

    g.set("getWeatherM", lua.create_function(|_, m: i32| {
        let mp = unsafe { get_map_ptr(m as u16) };
        if mp.is_null() { return Ok(0i64); }
        Ok(unsafe { (*mp).weather as i64 })
    })?)?;

    g.set("setWeatherM", lua.create_function(|_, (m, w): (i32, i32)| {
        unsafe { sffi::sl_g_setweatherm(m as c_int, w as c_uchar); }
        Ok(())
    })?)?;

    g.set("getWeather", lua.create_function(|_, (region, indoor): (i32, i32)| {
        Ok(unsafe { sffi::sl_g_getweather(region as c_uchar, indoor as c_uchar) as i64 })
    })?)?;

    g.set("setWeather", lua.create_function(|_, (region, indoor, w): (i32, i32, i32)| {
        unsafe { sffi::sl_g_setweather(region as c_uchar, indoor as c_uchar, w as c_uchar); }
        Ok(())
    })?)?;

    g.set("setLight", lua.create_function(|_, (region, indoor, light): (i32, i32, i32)| {
        unsafe { sffi::sl_g_setlight(region as c_uchar, indoor as c_uchar, light as c_uchar); }
        Ok(())
    })?)?;

    g.set("getMapRegistry", lua.create_function(|_, (m, key): (i32, String)| {
        let ckey = CString::new(key).map_err(mlua::Error::external)?;
        Ok(unsafe { sffi::map_readglobalreg(m as c_int, ckey.as_ptr()) as i64 })
    })?)?;

    g.set("setMapRegistry", lua.create_function(|_, (m, key, val): (i32, String, i32)| {
        let ckey = CString::new(key).map_err(mlua::Error::external)?;
        unsafe { sffi::map_setglobalreg(m as c_int, ckey.as_ptr(), val as c_int); }
        Ok(())
    })?)?;

    // -----------------------------------------------------------------------
    // Map: getMapAttribute / setMapAttribute
    // -----------------------------------------------------------------------
    g.set("getMapAttribute", lua.create_function(|lua, (m, attr): (i32, String)| {
        let mp = unsafe { get_map_ptr(m as u16) };
        if mp.is_null() || unsafe { (*mp).registry.is_null() } { return Ok(Value::Nil); }
        let md = unsafe { &*mp };
        match attr.as_str() {
            "xmax"       => Ok(Value::Integer(md.xs as i64 - 1)),
            "ymax"       => Ok(Value::Integer(md.ys as i64 - 1)),
            "mapTitle"   => {
                let s = unsafe { CStr::from_ptr(md.title.as_ptr()).to_string_lossy().into_owned() };
                Ok(Value::String(lua.create_string(s.as_bytes())?))
            }
            "mapFile"    => {
                let s = unsafe { CStr::from_ptr(md.mapfile.as_ptr()).to_string_lossy().into_owned() };
                Ok(Value::String(lua.create_string(s.as_bytes())?))
            }
            "bgm"        => Ok(Value::Integer(md.bgm as i64)),
            "bgmType"    => Ok(Value::Integer(md.bgmtype as i64)),
            "pvp"        => Ok(Value::Integer(md.pvp as i64)),
            "spell"      => Ok(Value::Integer(md.spell as i64)),
            "light"      => Ok(Value::Integer(md.light as i64)),
            "weather"    => Ok(Value::Integer(md.weather as i64)),
            "sweepTime"  => Ok(Value::Integer(md.sweeptime as i64)),
            "canTalk"    => Ok(Value::Integer(md.cantalk as i64)),
            "showGhosts" => Ok(Value::Integer(md.show_ghosts as i64)),
            "region"     => Ok(Value::Integer(md.region as i64)),
            "indoor"     => Ok(Value::Integer(md.indoor as i64)),
            "warpOut"    => Ok(Value::Integer(md.warpout as i64)),
            "bind"       => Ok(Value::Integer(md.bind as i64)),
            "reqLvl"     => Ok(Value::Integer(md.reqlvl as i64)),
            "reqVita"    => Ok(Value::Integer(md.reqvita as i64)),
            "reqMana"    => Ok(Value::Integer(md.reqmana as i64)),
            _            => Ok(Value::Nil),
        }
    })?)?;

    g.set("setMapAttribute", lua.create_function(|_, (m, attr, val): (i32, String, Value)| {
        let mp = unsafe { get_map_ptr(m as u16) };
        if mp.is_null() || unsafe { (*mp).registry.is_null() } { return Ok(()); }
        let md = unsafe { &mut *mp };
        let ival = match &val {
            Value::Integer(i) => *i as i32,
            Value::Number(f)  => *f as i32,
            _                 => 0,
        };
        match attr.as_str() {
            "bgm"        => md.bgm         = ival as u16,
            "bgmType"    => md.bgmtype     = ival as u16,
            "pvp"        => md.pvp         = ival as u8,
            "spell"      => md.spell       = ival as u8,
            "light"      => md.light       = ival as u8,
            "weather"    => md.weather     = ival as u8,
            "sweepTime"  => md.sweeptime   = ival as u32,
            "canTalk"    => md.cantalk     = ival as u8,
            "showGhosts" => md.show_ghosts = ival as u8,
            "region"     => md.region      = ival as u8,
            "indoor"     => md.indoor      = ival as u8,
            "warpOut"    => md.warpout     = ival as u8,
            "bind"       => md.bind        = ival as u8,
            "reqLvl"     => md.reqlvl      = ival as u32,
            "reqVita"    => md.reqvita     = ival as u32,
            "reqMana"    => md.reqmana     = ival as u32,
            "mapTitle"   => {
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
    })?)?;

    // -----------------------------------------------------------------------
    // setMap (full map file load)
    // -----------------------------------------------------------------------
    g.set("setMap", lua.create_function(|_, args: mlua::MultiValue| {
        let args: Vec<Value> = args.into_iter().collect();
        let cmapfile = CString::new(vs(&args, 1)).map_err(mlua::Error::external)?;
        let ctitle   = CString::new(vs(&args, 2)).map_err(mlua::Error::external)?;
        unsafe {
            sffi::sl_g_setmap(
                vi(&args, 0), cmapfile.as_ptr(), ctitle.as_ptr(),
                vi(&args, 3), vi(&args, 4), vi(&args, 5), vi(&args, 6),
                vi(&args, 7) as c_uchar, vi(&args, 8),
                vi(&args, 9), vi(&args, 10), vi(&args, 11),
                vi(&args, 12), vi(&args, 13), vi(&args, 14),
                vi(&args, 15), vi(&args, 16), vi(&args, 17), vi(&args, 18),
            );
        }
        Ok(())
    })?)?;

    // -----------------------------------------------------------------------
    // setPostColor / throw / saveMap
    // -----------------------------------------------------------------------
    g.set("setPostColor", lua.create_function(|_, (board, post, color): (i32, i32, i32)| {
        unsafe { sffi::map_changepostcolor(board as c_int, post as c_int, color as c_int); }
        Ok(())
    })?)?;

    g.set("throw", lua.create_function(|_, args: mlua::MultiValue| {
        let args: Vec<Value> = args.into_iter().collect();
        unsafe {
            sffi::sl_g_throw(
                vi(&args, 0), vi(&args, 1), vi(&args, 2), vi(&args, 3),
                vi(&args, 4), vi(&args, 5), vi(&args, 6), vi(&args, 7), vi(&args, 8),
            );
        }
        Ok(())
    })?)?;

    g.set("saveMap", lua.create_function(|_, (m, path): (i32, String)| {
        let cpath = CString::new(path).map_err(mlua::Error::external)?;
        Ok(unsafe { sffi::sl_g_savemap(m as c_int, cpath.as_ptr()) != 0 })
    })?)?;

    // -----------------------------------------------------------------------
    // Warps
    // -----------------------------------------------------------------------
    g.set("getWarp", lua.create_function(|_, (m, x, y): (i32, i32, i32)| {
        Ok(unsafe { sffi::sl_g_getwarp(m as c_int, x as c_int, y as c_int) != 0 })
    })?)?;

    g.set("setWarps", lua.create_function(|_, (mm, mx, my, tm, tx, ty): (i32,i32,i32,i32,i32,i32)| {
        Ok(unsafe { sffi::sl_g_setwarps(mm as c_int, mx as c_int, my as c_int, tm as c_int, tx as c_int, ty as c_int) != 0 })
    })?)?;

    g.set("getWarps", lua.create_function(|lua, _m: i32| {
        tracing::warn!("[scripting] getWarps: not yet implemented");
        Ok(lua.create_table()?)
    })?)?;

    // -----------------------------------------------------------------------
    // Spell / mob DB
    // -----------------------------------------------------------------------
    g.set("getSpellLevel", lua.create_function(|_, spell: String| {
        let cs = CString::new(spell).map_err(mlua::Error::external)?;
        Ok(unsafe { sffi::rust_magicdb_level(cs.as_ptr()) as i64 })
    })?)?;

    g.set("getMobAttributes", lua.create_function(|lua, id: u32| {
        let tbl = lua.create_table()?;
        let db = unsafe { sffi::rust_mobdb_search(id as c_uint) };
        if !db.is_null() {
            let d = unsafe { &*db };
            tbl.set(1,  d.vita as i64)?;
            tbl.set(2,  d.baseac as i64)?;
            tbl.set(3,  d.exp as i64)?;
            tbl.set(4,  d.might as i64)?;
            tbl.set(5,  d.will as i64)?;
            tbl.set(6,  d.grace as i64)?;
            tbl.set(7,  d.look as i64)?;
            tbl.set(8,  d.look_color as i64)?;
            tbl.set(9,  d.level as i64)?;
            let name  = unsafe { CStr::from_ptr(d.name.as_ptr()).to_string_lossy().into_owned() };
            let yname = unsafe { CStr::from_ptr(d.yname.as_ptr()).to_string_lossy().into_owned() };
            tbl.set(10, name)?;
            tbl.set(11, yname)?;
        }
        Ok(tbl)
    })?)?;

    // -----------------------------------------------------------------------
    // addMob / checkOnline / getOfflineID
    // -----------------------------------------------------------------------
    g.set("addMob", lua.create_function(|_, (m, x, y, mobid): (i32, i32, i32, i32)| {
        Ok(unsafe { sffi::sl_g_addmob(m as c_int, x as c_int, y as c_int, mobid as c_int) != 0 })
    })?)?;

    g.set("checkOnline", lua.create_function(|_, v: Value| {
        let result = match v {
            Value::Integer(id)   => unsafe { sffi::sl_g_checkonline_id(id as c_int) != 0 },
            Value::Number(f)     => unsafe { sffi::sl_g_checkonline_id(f as c_int) != 0 },
            Value::String(ref s) => {
                let text = s.to_str()?;
                let cs = CString::new(text.as_bytes()).map_err(mlua::Error::external)?;
                unsafe { sffi::sl_g_checkonline_name(cs.as_ptr()) != 0 }
            }
            _ => false,
        };
        Ok(result)
    })?)?;

    g.set("getOfflineID", lua.create_function(|_, name: String| {
        let cs = CString::new(name).map_err(mlua::Error::external)?;
        Ok(unsafe { sffi::sl_g_getofflineid(cs.as_ptr()) as i64 })
    })?)?;

    // -----------------------------------------------------------------------
    // Map modifiers
    // -----------------------------------------------------------------------
    g.set("getMapModifiers", lua.create_function(|lua, _: i32| {
        tracing::warn!("[scripting] getMapModifiers: not yet implemented");
        Ok(lua.create_table()?)
    })?)?;

    g.set("addMapModifier", lua.create_function(|_, (mapid, modifier, value): (i32, String, i32)| {
        let cm = CString::new(modifier).map_err(mlua::Error::external)?;
        Ok(unsafe { sffi::sl_g_addmapmodifier(mapid as c_uint, cm.as_ptr(), value as c_int) != 0 })
    })?)?;

    g.set("removeMapModifier", lua.create_function(|_, (mapid, modifier): (i32, String)| {
        let cm = CString::new(modifier).map_err(mlua::Error::external)?;
        Ok(unsafe { sffi::sl_g_removemapmodifier(mapid as c_uint, cm.as_ptr()) != 0 })
    })?)?;

    g.set("removeMapModifierId", lua.create_function(|_, mapid: i32| {
        Ok(unsafe { sffi::sl_g_removemapmodifierid(mapid as c_uint) != 0 })
    })?)?;

    g.set("getFreeMapModifierId", lua.create_function(|_, ()| {
        Ok(unsafe { sffi::sl_g_getfreemapmodifierid() as i64 })
    })?)?;

    // -----------------------------------------------------------------------
    // WisdomStar
    // -----------------------------------------------------------------------
    g.set("getWisdomStarMultiplier", lua.create_function(|_, ()| {
        Ok(unsafe { sffi::sl_g_getwisdomstarmultiplier() as f64 })
    })?)?;

    g.set("setWisdomStarMultiplier", lua.create_function(|_, (mult, val): (f64, i32)| {
        unsafe { sffi::sl_g_setwisdomstarmultiplier(mult as f32, val as c_int); }
        Ok(())
    })?)?;

    // -----------------------------------------------------------------------
    // KanDonationPoints
    // -----------------------------------------------------------------------
    g.set("getKanDonationPoints", lua.create_function(|_, ()| {
        Ok(unsafe { sffi::sl_g_getkandonationpoints() as i64 })
    })?)?;

    g.set("setKanDonationPoints", lua.create_function(|_, val: i32| {
        unsafe { sffi::sl_g_setkandonationpoints(val as c_int); }
        Ok(())
    })?)?;

    g.set("addKanDonationPoints", lua.create_function(|_, val: i32| {
        unsafe { sffi::sl_g_addkandonationpoints(val as c_int); }
        Ok(())
    })?)?;

    g.set("processKanDonations", lua.create_function(|_, ()| {
        tracing::warn!("[scripting] processKanDonations: not yet implemented");
        Ok(())
    })?)?;

    // -----------------------------------------------------------------------
    // Clan tribute
    // -----------------------------------------------------------------------
    g.set("getClanTribute", lua.create_function(|_, clan: i32| {
        Ok(unsafe { sffi::sl_g_getclantribute(clan as c_int) as i64 })
    })?)?;

    g.set("setClanTribute", lua.create_function(|_, (clan, val): (i32, i32)| {
        unsafe { sffi::sl_g_setclantribute(clan as c_int, val as c_uint); }
        Ok(())
    })?)?;

    g.set("addClanTribute", lua.create_function(|_, (clan, val): (i32, i32)| {
        unsafe { sffi::sl_g_addclantribute(clan as c_int, val as c_uint); }
        Ok(())
    })?)?;

    // -----------------------------------------------------------------------
    // Clan name
    // -----------------------------------------------------------------------
    g.set("getClanName", lua.create_function(|_, clan: i32| {
        let mut buf = vec![0i8; 65];
        let found = unsafe { sffi::sl_g_getclanname(clan as c_int, buf.as_mut_ptr(), 65) };
        if found == 0 { return Ok(String::new()); }
        let s = unsafe { CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned() };
        Ok(s)
    })?)?;

    g.set("setClanName", lua.create_function(|_, (clan, name): (i32, String)| {
        let cs = CString::new(name).map_err(mlua::Error::external)?;
        unsafe { sffi::sl_g_setclanname(clan as c_int, cs.as_ptr()); }
        Ok(())
    })?)?;

    // -----------------------------------------------------------------------
    // Clan bank slots
    // -----------------------------------------------------------------------
    g.set("getClanBankSlots", lua.create_function(|_, clan: i32| {
        Ok(unsafe { sffi::sl_g_getclanbankslots(clan as c_int) as i64 })
    })?)?;

    g.set("setClanBankSlots", lua.create_function(|_, (clan, val): (i32, i32)| {
        unsafe { sffi::sl_g_setclanbankslots(clan as c_int, val as c_int); }
        Ok(())
    })?)?;

    // -----------------------------------------------------------------------
    // Clan roster (table-returning — stub)
    // -----------------------------------------------------------------------
    g.set("getClanRoster", lua.create_function(|lua, _: i32| {
        tracing::warn!("[scripting] getClanRoster: not yet implemented");
        Ok(lua.create_table()?)
    })?)?;

    // -----------------------------------------------------------------------
    // Clan member management
    // -----------------------------------------------------------------------
    g.set("removeClanMember", lua.create_function(|_, id: i32| {
        Ok(unsafe { sffi::sl_g_removeclanmember(id as c_int) != 0 })
    })?)?;

    g.set("addClanMember", lua.create_function(|_, (id, clan): (i32, i32)| {
        Ok(unsafe { sffi::sl_g_addclanmember(id as c_int, clan as c_int) != 0 })
    })?)?;

    g.set("updateClanMemberRank", lua.create_function(|_, (id, rank): (i32, i32)| {
        Ok(unsafe { sffi::sl_g_updateclanmemberrank(id as c_int, rank as c_int) != 0 })
    })?)?;

    g.set("updateClanMemberTitle", lua.create_function(|_, (id, title): (i32, String)| {
        let cs = CString::new(title).map_err(mlua::Error::external)?;
        Ok(unsafe { sffi::sl_g_updateclanmembertitle(id as c_int, cs.as_ptr()) != 0 })
    })?)?;

    // -----------------------------------------------------------------------
    // Path member management
    // -----------------------------------------------------------------------
    g.set("removePathMember", lua.create_function(|_, id: i32| {
        Ok(unsafe { sffi::sl_g_removepathember(id as c_int) != 0 })
    })?)?;

    g.set("addPathMember", lua.create_function(|_, (id, cls): (i32, i32)| {
        Ok(unsafe { sffi::sl_g_addpathember(id as c_int, cls as c_int) != 0 })
    })?)?;

    // setOfflinePlayerRegistry — core logic was commented out in C, no-op.
    g.set("setOfflinePlayerRegistry", lua.create_function(|_, _: mlua::MultiValue| Ok(()))?)?;

    // -----------------------------------------------------------------------
    // XP for level
    // -----------------------------------------------------------------------
    g.set("getXPforLevel", lua.create_function(|_, (path, level): (i32, i32)| {
        Ok(unsafe { sffi::sl_g_getxpforlevel(path as c_int, level as c_int) as i64 })
    })?)?;

    // -----------------------------------------------------------------------
    // Board / poetry (stubs — complex DB)
    // -----------------------------------------------------------------------
    g.set("addToBoard",          lua.create_function(|_, _: mlua::MultiValue| { tracing::warn!("[scripting] addToBoard: not yet implemented"); Ok(()) })?)?;
    g.set("selectBulletinBoard", lua.create_function(|_, _: mlua::MultiValue| { tracing::warn!("[scripting] selectBulletinBoard: not yet implemented"); Ok(()) })?)?;
    g.set("getPoems",            lua.create_function(|lua, _: mlua::MultiValue| { tracing::warn!("[scripting] getPoems: not yet implemented"); Ok(lua.create_table()?) })?)?;
    g.set("clearPoems",          lua.create_function(|_, _: mlua::MultiValue| { tracing::warn!("[scripting] clearPoems: not yet implemented"); Ok(()) })?)?;
    g.set("copyPoemToPoetry",    lua.create_function(|_, _: mlua::MultiValue| { tracing::warn!("[scripting] copyPoemToPoetry: not yet implemented"); Ok(()) })?)?;

    // -----------------------------------------------------------------------
    // Auction (stubs)
    // -----------------------------------------------------------------------
    g.set("getAuctions",   lua.create_function(|lua, _: mlua::MultiValue| { tracing::warn!("[scripting] getAuctions: not yet implemented"); Ok(lua.create_table()?) })?)?;
    g.set("listAuction",   lua.create_function(|_, _: mlua::MultiValue| { tracing::warn!("[scripting] listAuction: not yet implemented"); Ok(()) })?)?;
    g.set("removeAuction", lua.create_function(|_, _: mlua::MultiValue| { tracing::warn!("[scripting] removeAuction: not yet implemented"); Ok(()) })?)?;

    // -----------------------------------------------------------------------
    // getSetItems (stub) / guitext (no-op, commented out in C too)
    // -----------------------------------------------------------------------
    g.set("getSetItems", lua.create_function(|lua, _: mlua::MultiValue| { tracing::warn!("[scripting] getSetItems: not yet implemented"); Ok(lua.create_table()?) })?)?;
    g.set("guitext",     lua.create_function(|_, _: mlua::MultiValue| Ok(()))?)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn realtime() -> (i32, i32, i32, i32) {
    let (mut day, mut hour, mut min, mut sec) = (0i32, 0i32, 0i32, 0i32);
    unsafe { sffi::sl_g_realtime(&mut day, &mut hour, &mut min, &mut sec); }
    (day, hour, min, sec)
}

fn vi(args: &[Value], idx: usize) -> c_int {
    args.get(idx).map(|v| match v {
        Value::Integer(i) => *i as c_int,
        Value::Number(f)  => *f as c_int,
        _                 => 0,
    }).unwrap_or(0)
}

fn vs(args: &[Value], idx: usize) -> String {
    args.get(idx).map(|v| match v {
        Value::String(s) => s.to_str().map(|s| s.to_owned()).unwrap_or_default(),
        _                => String::new(),
    }).unwrap_or_default()
}
