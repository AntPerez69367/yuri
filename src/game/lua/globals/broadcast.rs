use std::ffi::CString;

use mlua::prelude::*;

use crate::game::map_parse::chat::{clif_broadcast, clif_gmbroadcast};
use crate::game::scripting::map_globals::sl_g_sendmeta;
use crate::game::scripting::sl_reload;

pub fn register(lua: &Lua) -> LuaResult<()> {
    let g = lua.globals();

    // TODO: refactor clif_broadcast/clif_gmbroadcast to use PacketWriter and take &str
    g.set("broadcast", lua.create_function(|_, (m, msg): (i32, String)| {
        let cmsg = CString::new(msg).map_err(LuaError::external)?;
        unsafe { clif_broadcast(cmsg.as_ptr(), m); }
        Ok(())
    })?)?;

    g.set("gmbroadcast", lua.create_function(|_, (m, msg): (i32, String)| {
        let cmsg = CString::new(msg).map_err(LuaError::external)?;
        unsafe { clif_gmbroadcast(cmsg.as_ptr(), m); }
        Ok(())
    })?)?;

    g.set("luaReload", lua.create_function(|_, ()| {
        sl_reload();
        Ok(())
    })?)?;

    g.set("sendMeta", lua.create_function(|_, ()| {
        unsafe { sl_g_sendmeta(); }
        Ok(())
    })?)?;

    Ok(())
}
