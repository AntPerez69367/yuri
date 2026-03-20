use crate::game::lua::entity::prelude::*;
use mlua::prelude::*;

pub mod coroutine;
pub mod dispatch;
pub mod entity;
pub mod error;

pub fn log_missing(entity_type: EntityType, key: &str, lua: &Lua) {
    let location = lua
        .inspect_stack(2, |dbg| {
            let source = dbg.source();
            let src_name = source.short_src.as_deref().unwrap_or("unknown");
            format!("{}:{}", src_name, dbg.current_line().unwrap_or(0))
        })
        .unwrap_or_else(|| "unknown".to_string());
    tracing::warn!(
        "[LUA-MISSING] {}:{} called from {}",
        entity_type,
        key,
        location
    );
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();
    globals.set(
        EntityType::Player.to_string(),
        lua.create_function(|_lua, id: u32| Ok(LuaPlayer::new(id)))?,
    )?;
    globals.set(
        EntityType::Npc.to_string(),
        lua.create_function(|_lua, id: u32| Ok(LuaNpc::new(id)))?,
    )?;
    globals.set(
        EntityType::Mob.to_string(),
        lua.create_function(|_lua, id: u32| Ok(LuaMob::new(id)))?,
    )?;
    globals.set(
        EntityType::Item.to_string(),
        lua.create_function(|_lua, id: u32| Ok(LuaItem::new(id)))?,
    )?;
    globals.set(
        "FloorItem",
        lua.create_function(|_lua, id: u32| Ok(LuaItem::new(id)))?,
    )?;

    Ok(())
}
