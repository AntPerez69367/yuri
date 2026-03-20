use crate::game::lua::entity::prelude::*;
use mlua::prelude::*;

#[macro_use]
pub mod macros;
pub mod coroutine;
pub mod dispatch;
pub mod entity;
pub mod error;
pub mod globals;
pub mod registry;

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
    entity::register(lua)?;
    globals::register(lua)?;
    Ok(())
}
