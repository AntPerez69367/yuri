use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use mlua::prelude::*;
use parking_lot::RwLock;
use tealr::mlu::TealDataMethods;

use crate::common::types::Point;
use crate::game::block::map_addblock_id;
use crate::game::lua::dispatch::dispatch;
use crate::game::lua::entity::npc::LuaNpc;
use crate::game::map_server::map_addiddb_npc;
use crate::game::npc::{npc_get_new_npctempid, NpcData, NpcEntity, BL_NPC};
use crate::game::util::str_to_carray;

pub fn register(methods: &mut impl TealDataMethods<LuaNpc>) {
    methods.document("Spawn a new NPC on the map.");
    methods.document("");
    methods.document("**Parameters:**");
    methods.document("- `name` (string) — Script/event name for the NPC");
    methods.document("- `pos` (table) — Position: `{ m = mapId, x = x, y = y }`");
    methods.document("- `options` (table, optional) — Spawn config:");
    methods.document("  - `subtype` (integer) — NPC subtype (default 0)");
    methods.document("  - `timer` (integer) — Action timer in ms (default 0)");
    methods.document("  - `duration` (integer) — Duration in ms (default 0)");
    methods.document("  - `owner` (integer) — Owner entity ID (default 0)");
    methods.document("  - `movetime` (integer) — Move interval in ms (default 0)");
    methods.document("  - `yname` (string) — Display name (default \"nothing\")");
    methods.document("");
    methods.document("**Example:**");
    methods.document("```lua");
    methods.document("npc:addNPC(\"GuardNpc\", { m = 1, x = 50, y = 50 }, { subtype = 1, timer = 5000 })");
    methods.document("```");
    methods.add_method("addNPC", |_, _this, (name, pos_tbl, opts): (String, LuaTable, Option<LuaTable>)| {
        let pos = Point {
            m: pos_tbl.get::<u16>("m")?,
            x: pos_tbl.get::<u16>("x")?,
            y: pos_tbl.get::<u16>("y")?,
        };

        let subtype = opt_i32(&opts, "subtype");
        let timer = opt_i32(&opts, "timer");
        let duration = opt_i32(&opts, "duration");
        let owner = opt_i32(&opts, "owner");
        let movetime = opt_i32(&opts, "movetime");
        let yname: Option<String> = opts
            .as_ref()
            .and_then(|t| t.get::<Option<String>>("yname").ok().flatten());

        spawn_npc(&name, pos, subtype, timer, duration, owner, movetime, yname.as_deref());
        Ok(())
    });
}

fn opt_i32(opts: &Option<LuaTable>, key: &str) -> i32 {
    opts.as_ref()
        .and_then(|t| t.get::<Option<i32>>(key).ok().flatten())
        .unwrap_or(0)
}

fn spawn_npc(
    name: &str,
    pos: Point,
    subtype: i32,
    timer: i32,
    duration: i32,
    owner: i32,
    movetime: i32,
    yname: Option<&str>,
) {
    // Build legacy NpcData (still needed for existing systems)
    let mut nd: NpcData = unsafe { std::mem::zeroed() };
    str_to_carray(name, &mut nd.name);
    str_to_carray(yname.unwrap_or("nothing"), &mut nd.npc_name);
    nd.bl_type = BL_NPC as u8;
    nd.subtype = subtype as u8;
    nd.m = pos.m;
    nd.x = pos.x;
    nd.y = pos.y;
    nd.id = npc_get_new_npctempid();
    nd.actiontime = timer as u32;
    nd.duration = duration as u32;
    nd.owner = owner as u32;
    nd.movetime = movetime as u32;

    let id = nd.id;
    let entity = Arc::new(NpcEntity {
        id,
        pos_atomic: AtomicU64::new(pos.to_u64()),
        name: name.to_owned(),
        npc_name: yname.unwrap_or("nothing").to_owned(),
        legacy: RwLock::new(nd),
    });
    map_addiddb_npc(id, entity);

    map_addblock_id(id, BL_NPC as u8, pos.m, pos.x, pos.y);
    dispatch(name, Some("on_spawn"), &[id]);
}
