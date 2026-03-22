use std::sync::Arc;
use std::sync::atomic::AtomicU64;

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
    methods.document("addNPC(yname, map, x, y [, timer, duration, owner, displayname])");
    // Legacy positional: addNPC(yname, map, x, y [, timer, duration, owner, displayname])
    methods.add_method("addNPC", |_, _this,
        (name, map, x, y, timer, duration, owner, displayname):
        (String, u16, Option<u16>, Option<u16>, Option<i32>, Option<i32>, Option<i32>, Option<String>)|
    {
        let pos = Point { m: map, x: x.unwrap_or(0), y: y.unwrap_or(0) };
        spawn_npc(
            &name, pos, 0,
            timer.unwrap_or(0),
            duration.unwrap_or(0),
            owner.unwrap_or(0),
            0,
            displayname.as_deref(),
        );
        Ok(())
    });
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
