//! Shared entity traits, methods, and index fallback.

mod queries;
mod spawn;

use mlua::prelude::*;
use tealr::mlu::TealDataMethods;
use tealr::ToTypename;

use crate::game::lua::dispatch::id_to_lua;
use crate::game::lua::registry::game_registry::LuaGameRegistry;

/// Trait for entity types that have an `id` field.
/// Allows shared method registration to work across all entity types.
pub trait HasEntityId {
    fn entity_id(&self) -> u32;
}

/// Convert entity IDs to a Lua table of typed entity objects.
pub(crate) fn ids_to_lua_table(lua: &Lua, ids: &[u32]) -> LuaResult<LuaTable> {
    let tbl = lua.create_table()?;
    for (i, &id) in ids.iter().enumerate() {
        let val = id_to_lua(lua, id)?;
        tbl.raw_set(i + 1, val)?;
    }
    Ok(tbl)
}

/// Register shared methods available on all entity types.
/// Call from `TealData::add_methods` on each entity.
pub fn register_shared_methods<T>(methods: &mut impl TealDataMethods<T>)
where
    T: 'static + Clone + Send + Sync + HasEntityId + ToTypename,
{
    queries::register(methods);
    spawn::register(methods);
}

/// Fallback for __index — handles keys not matched by registered fields/methods.
pub fn try_shared_index(lua: &Lua, key: &str, _entity_id: u32) -> Option<LuaResult<LuaValue>> {
    match key {
        "gameRegistry" => Some(LuaGameRegistry.into_lua(lua)),
        _ => None,
    }
}
