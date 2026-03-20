use mlua::prelude::*;
use tealr::mlu::TealDataMethods;
use tealr::ToTypename;

use crate::game::lua::dispatch::id_to_lua;
use crate::game::lua::registry::game_registry::LuaGameRegistry;
use crate::game::map_server::entity_position;
use crate::game::scripting::object_collect::{
    get_alive_objects_area, get_alive_objects_cell, get_alive_objects_same_map, get_objects_area,
    get_objects_cell, get_objects_cell_with_traps, get_objects_in_map, get_objects_same_map,
};

/// Trait for entity types that have an `id` field.
/// Allows shared method registration to work across all entity types.
pub trait HasEntityId {
    fn entity_id(&self) -> u32;
}

/// Convert entity IDs to a Lua table of typed entity objects.
fn ids_to_lua_table(lua: &Lua, ids: &[u32]) -> LuaResult<LuaTable> {
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
    // ── Cell queries ──
    methods.document("Get all entities in a specific map cell.");
    methods.document("");
    methods.document("**Parameters:** `(mapId, x, y, blType)` — blType filters by entity type (BL_PC, BL_MOB, etc.)");
    methods.document("");
    methods.document("**Returns:** table of entity objects");
    methods.add_method("getObjectsInCell", |lua, _this, (m, x, y, bl_type): (i32, i32, i32, i32)| {
        let ids = get_objects_cell(m, x, y, bl_type);
        ids_to_lua_table(lua, &ids).map(LuaValue::Table)
    });

    methods.document("Get all alive entities in a specific map cell.");
    methods.document("");
    methods.document("**Parameters:** `(mapId, x, y, blType)`");
    methods.document("");
    methods.document("**Returns:** table of entity objects");
    methods.add_method("getAliveObjectsInCell", |lua, _this, (m, x, y, bl_type): (i32, i32, i32, i32)| {
        let ids = get_alive_objects_cell(m, x, y, bl_type);
        ids_to_lua_table(lua, &ids).map(LuaValue::Table)
    });

    methods.document("Get all entities in a cell, including traps.");
    methods.document("");
    methods.document("**Parameters:** `(mapId, x, y, blType)`");
    methods.document("");
    methods.document("**Returns:** table of entity objects");
    methods.add_method("getObjectsInCellWithTraps", |lua, _this, (m, x, y, bl_type): (i32, i32, i32, i32)| {
        let ids = get_objects_cell_with_traps(m, x, y, bl_type);
        ids_to_lua_table(lua, &ids).map(LuaValue::Table)
    });

    // ── Area queries (use entity position) ──
    methods.document("Get all entities in the area around this entity.");
    methods.document("");
    methods.document("**Parameters:** `(blType)` — uses this entity's current position");
    methods.document("");
    methods.document("**Returns:** table of entity objects");
    methods.add_method("getObjectsInArea", |lua, this, bl_type: i32| {
        let Some((pos, _)) = entity_position(this.entity_id()) else {
            return lua.create_table().map(LuaValue::Table);
        };
        let ids = get_objects_area(pos.m, pos.x, pos.y, bl_type);
        ids_to_lua_table(lua, &ids).map(LuaValue::Table)
    });

    methods.document("Get all alive entities in the area around this entity.");
    methods.document("");
    methods.document("**Parameters:** `(blType)`");
    methods.document("");
    methods.document("**Returns:** table of entity objects");
    methods.add_method("getAliveObjectsInArea", |lua, this, bl_type: i32| {
        let Some((pos, _)) = entity_position(this.entity_id()) else {
            return lua.create_table().map(LuaValue::Table);
        };
        let ids = get_alive_objects_area(pos.m, pos.x, pos.y, bl_type);
        ids_to_lua_table(lua, &ids).map(LuaValue::Table)
    });

    methods.document("Get all entities on the same map as this entity.");
    methods.document("");
    methods.document("**Parameters:** `(blType)`");
    methods.document("");
    methods.document("**Returns:** table of entity objects");
    methods.add_method("getObjectsInSameMap", |lua, this, bl_type: i32| {
        let Some((pos, _)) = entity_position(this.entity_id()) else {
            return lua.create_table().map(LuaValue::Table);
        };
        let ids = get_objects_same_map(pos.m, bl_type);
        ids_to_lua_table(lua, &ids).map(LuaValue::Table)
    });

    methods.document("Get all alive entities on the same map as this entity.");
    methods.document("");
    methods.document("**Parameters:** `(blType)`");
    methods.document("");
    methods.document("**Returns:** table of entity objects");
    methods.add_method("getAliveObjectsInSameMap", |lua, this, bl_type: i32| {
        let Some((pos, _)) = entity_position(this.entity_id()) else {
            return lua.create_table().map(LuaValue::Table);
        };
        let ids = get_alive_objects_same_map(pos.m, bl_type);
        ids_to_lua_table(lua, &ids).map(LuaValue::Table)
    });

    // ── Map query ──
    methods.document("Get all entities on a specific map.");
    methods.document("");
    methods.document("**Parameters:** `(mapId, blType)`");
    methods.document("");
    methods.document("**Returns:** table of entity objects");
    methods.add_method("getObjectsInMap", |lua, _this, (m, bl_type): (i32, i32)| {
        let ids = get_objects_in_map(m, bl_type);
        ids_to_lua_table(lua, &ids).map(LuaValue::Table)
    });

    // ── Block lookup ──
    methods.document("Look up any entity by ID, returning its typed object.");
    methods.document("");
    methods.document("**Parameters:** `(entityId)`");
    methods.document("");
    methods.document("**Returns:** Player, Mob, NPC, or Item object");
    methods.add_method("getBlock", |lua, _this, id: u32| {
        id_to_lua(lua, id)
    });
}

/// Fallback for __index — handles keys not matched by registered fields/methods.
pub fn try_shared_index(lua: &Lua, key: &str, _entity_id: u32) -> Option<LuaResult<LuaValue>> {
    match key {
        "gameRegistry" => Some(LuaGameRegistry.into_lua(lua)),
        _ => None,
    }
}
