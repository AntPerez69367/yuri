pub mod item;
pub mod mob;
pub mod npc;
pub mod player;
pub mod shared;
pub mod types;
use mlua::prelude::*;
pub mod prelude {
    pub use crate::game::lua::entity::item::*;
    pub use crate::game::lua::entity::mob::*;
    pub use crate::game::lua::entity::npc::*;
    pub use crate::game::lua::entity::player::*;
    pub use crate::game::lua::entity::types::*;
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    let g = lua.globals();

    // Player — callable table namespace
    let player_ns = lua.create_table()?;
    let player_mt = lua.create_table()?;
    player_mt.set(
        "__call",
        lua.create_function(|_lua, (_self, id): (LuaValue, u32)| Ok(player::LuaPlayer::new(id)))?,
    )?;
    player_ns.set_metatable(Some(player_mt))?;
    g.set("Player", player_ns)?;

    // Mob — same pattern
    let mob_ns = lua.create_table()?;
    let mob_mt = lua.create_table()?;
    mob_mt.set(
        "__call",
        lua.create_function(|_lua, (_self, id): (LuaValue, u32)| Ok(mob::LuaMob::new(id)))?,
    )?;
    mob_ns.set_metatable(Some(mob_mt))?;
    g.set("Mob", mob_ns)?;

    // NPC
    let npc_ns = lua.create_table()?;
    let npc_mt = lua.create_table()?;
    npc_mt.set(
        "__call",
        lua.create_function(|_lua, (_self, id): (LuaValue, u32)| Ok(npc::LuaNpc::new(id)))?,
    )?;
    npc_ns.set_metatable(Some(npc_mt))?;
    g.set("NPC", npc_ns)?;

    // FloorItem — plain function is fine unless scripts namespace on it too
    g.set(
        "FloorItem",
        lua.create_function(|_lua, id: u32| Ok(item::LuaItem::new(id)))?,
    )?;

    Ok(())
}
