use mlua::prelude::*;

use crate::database::map_db::{map_data, MapData};
use crate::game::block::map_user_count;

/// Check if a map slot is fully loaded (has registry data).
pub(super) fn is_loaded(md: &MapData) -> bool {
    !md.registry.is_null()
}

/// Get a loaded map reference, or None if invalid/unloaded.
pub(super) fn loaded_map(m: i32) -> Option<&'static MapData> {
    if m < 0 {
        return None;
    }
    let md = map_data(m as usize)?;
    if !is_loaded(md) {
        return None;
    }
    Some(md)
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    let g = lua.globals();

    g.set("getMapIsLoaded", lua.create_function(|_, m: i32| {
        Ok(loaded_map(m).is_some())
    })?)?;

    g.set("getMapUsers", lua.create_function(|_, m: i32| {
        Ok(loaded_map(m).map(|_| map_user_count(m as usize) as i64).unwrap_or(0))
    })?)?;

    g.set("getMapXMax", lua.create_function(|_, m: i32| {
        Ok(loaded_map(m).map(|md| md.xs as i64 - 1).unwrap_or(0))
    })?)?;

    g.set("getMapYMax", lua.create_function(|_, m: i32| {
        Ok(loaded_map(m).map(|md| md.ys as i64 - 1).unwrap_or(0))
    })?)?;

    Ok(())
}
