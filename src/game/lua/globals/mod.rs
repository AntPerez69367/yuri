pub mod broadcast;
pub mod constants;
pub mod map_dimensions;
pub mod map_properties;
pub mod map_tiles;
pub mod map_warps;
pub mod stubs;
pub mod time;

use mlua::prelude::*;

pub fn register(lua: &Lua) -> LuaResult<()> {
    constants::register(lua)?;
    time::register(lua)?;
    broadcast::register(lua)?;
    map_dimensions::register(lua)?;
    map_tiles::register(lua)?;
    map_properties::register(lua)?;
    map_warps::register(lua)?;
    stubs::register(lua)?;
    Ok(())
}
