//! Mob spawn method shared across all entity types.
//!
//! `entity:spawn(mobNameOrId, pos, amount [, owner])`
//!
//! If `pos.m` is omitted, uses the calling entity's current map.
//! Returns a table of spawned `Mob` objects.

use mlua::prelude::*;
use tealr::mlu::TealDataMethods;
use tealr::ToTypename;

use crate::game::lua::entity::mob::LuaMob;
use crate::game::map_server::entity_position;
use crate::common::types::Point;
use crate::game::mob::{mobspawn_onetime, SpawnConfig};

use super::HasEntityId;

pub fn register<T>(methods: &mut impl TealDataMethods<T>)
where
    T: 'static + Clone + Send + Sync + HasEntityId + ToTypename,
{
    methods.document("Spawn mobs at a position.");
    methods.document("");
    methods.document("**Parameters:**");
    methods.document("- `mobNameOrId` (string|integer) — Mob DB name or numeric ID");
    methods
        .document("- `pos` (table) — `{ m = mapId, x = x, y = y }` (m defaults to entity's map)");
    methods.document("- `amount` (integer) — Number of mobs to spawn");
    methods.document("- `owner` (integer, optional) — Owner entity ID (default: 0)");
    methods.document("");
    methods.document("**Returns:** table of `Mob` objects");
    methods.document("");
    methods.document("**Example:**");
    methods.document("```lua");
    methods.document("local mobs = npc:spawn(\"Slime\", { m = 1, x = 50, y = 50 }, 3)");
    methods
        .document("local mobs = npc:spawn(\"Slime\", { x = 50, y = 50 }, 3) -- uses entity's map");
    methods.document("```");
    methods.add_method(
        "spawn",
        |lua, this, (mob_ref, pos, amount, owner): (LuaValue, LuaTable, i32, Option<u32>)| {
            let mob_id: u32 = match &mob_ref {
                LuaValue::String(s) => {
                    let name = s.to_str().map_err(LuaError::external)?;
                    crate::database::mob_db::find_id(&name) as u32
                }
                LuaValue::Integer(n) => *n as u32,
                LuaValue::Number(f) => *f as u32,
                _ => return lua.create_table().map(LuaValue::Table),
            };

            let x: i32 = pos.get("x").unwrap_or(0);
            let y: i32 = pos.get("y").unwrap_or(0);
            let m: i32 = pos.get::<Option<i32>>("m")?.unwrap_or_else(|| {
                entity_position(this.entity_id())
                    .map(|(p, _)| p.m as i32)
                    .unwrap_or(0)
            });
            let owner = owner.unwrap_or(0);

            let tbl = lua.create_table()?;
            if amount <= 0 || mob_id == 0 {
                return Ok(LuaValue::Table(tbl));
            }

            let spawned = unsafe {
                mobspawn_onetime(
                    mob_id,
                    Point::new(m as u16, x as u16, y as u16),
                    SpawnConfig {
                        times: amount,
                        start: 0,
                        end: 0,
                        replace: 0,
                        owner,
                    },
                )
            };

            for (i, id) in spawned.into_iter().enumerate() {
                tbl.set(i + 1, LuaMob::new(id))?;
            }
            Ok(LuaValue::Table(tbl))
        },
    );
}
