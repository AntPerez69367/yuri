use mlua::prelude::*;

use crate::common::constants::entity::{BL_ALL, BL_ITEM, BL_MOB, BL_NPC, BL_PC};

pub fn register(lua: &Lua) -> LuaResult<()> {
    let g = lua.globals();

    // Entity type constants
    g.set("BL_PC", BL_PC as i64)?;
    g.set("BL_MOB", BL_MOB as i64)?;
    g.set("BL_NPC", BL_NPC as i64)?;
    g.set("BL_ITEM", BL_ITEM as i64)?;
    g.set("BL_ALL", BL_ALL as i64)?;

    // Mob state constants
    g.set("MOB_ALIVE", 0i64)?;
    g.set("MOB_DEAD", 1i64)?;
    g.set("MOB_HIT", 4i64)?;
    g.set("MOB_ESCAPE", 5i64)?;

    Ok(())
}
