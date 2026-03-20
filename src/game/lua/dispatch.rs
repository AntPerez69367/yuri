use crate::game::lua::coroutine::{self, CoroutineKey};
use crate::game::lua::entity::prelude::*;
use crate::game::map_server::{map_id2mob_ref, map_id2npc_ref, map_id2sd_pc};
use crate::game::mob::{FLOORITEM_START_NUM, MOB_START_NUM, NPC_START_NUM};
use crate::game::scripting::sl_state;
use mlua::prelude::*;

pub fn id_to_lua(lua: &Lua, id: u32) -> LuaResult<LuaValue> {
    if id == 0 {
        return Ok(LuaValue::Nil);
    }

    let exists = match id {
        _ if id < MOB_START_NUM => map_id2sd_pc(id).map(|_| ()),
        _ if id >= NPC_START_NUM => map_id2npc_ref(id).map(|_| ()),
        _ if id >= FLOORITEM_START_NUM => Some(()), // Items are ID-only
        _ => map_id2mob_ref(id).map(|_| ()),
    };

    match exists {
        Some(_) => match id {
            _ if id < MOB_START_NUM => LuaPlayer::new(id).into_lua(lua),
            _ if id >= NPC_START_NUM => LuaNpc::new(id).into_lua(lua),
            _ if id >= FLOORITEM_START_NUM => LuaItem::new(id).into_lua(lua),
            _ => LuaMob::new(id).into_lua(lua),
        },
        None => Ok(LuaValue::Nil),
    }
}

fn resolve_func(lua: &Lua, root: &str, method: Option<&str>) -> Option<LuaFunction> {
    match method {
        None => lua.globals().get::<LuaFunction>(root).ok(),
        Some(m) => {
            let tbl: LuaTable = lua.globals().get(root).ok()?;
            tbl.get::<LuaFunction>(m).ok()
        }
    }
}

pub fn dispatch(root: &str, method: Option<&str>, entity_ids: &[u32]) -> bool {
    let lua = sl_state();
    let Some(func) = resolve_func(lua, root, method) else {
        tracing::warn!(
            "[lua] {}{}: function not found",
            root,
            method.map(|m| format!(".{}", m)).unwrap_or_default()
        );
        return false;
    };
    let mut args = LuaMultiValue::new();
    for &id in entity_ids {
        args.push_back(id_to_lua(lua, id).unwrap_or(LuaValue::Nil));
    }
    match func.call::<LuaMultiValue>(args) {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!(
                "[lua] {}{}: {}",
                root,
                method.map(|m| format!(".{}", m)).unwrap_or_default(),
                e
            );
            false
        }
    }
}

pub fn dispatch_coro(root: &str, method: Option<&str>, entity_ids: &[u32]) -> bool {
    let lua = sl_state();
    let Some(func) = resolve_func(lua, root, method) else {
        return false;
    };

    let thread = match lua.create_thread(func) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("[lua] create_thread for {}: {}", root, e);
            return false;
        }
    };

    let mut args = LuaMultiValue::new();
    for &id in entity_ids {
        args.push_back(id_to_lua(lua, id).unwrap_or(LuaValue::Nil));
    }

    let player_id = entity_ids
        .iter()
        .copied()
        .find(|&id| id > 0 && id < MOB_START_NUM)
        .unwrap_or(0);
    let context_id = entity_ids
        .iter()
        .copied()
        .find(|&id| id >= MOB_START_NUM)
        .unwrap_or(0);

    match thread.resume::<LuaMultiValue>(args) {
        Ok(_) => {
            if thread.status() == LuaThreadStatus::Resumable && player_id > 0 {
                let key = CoroutineKey {
                    player_id,
                    context_id,
                };
                if let Err(e) = coroutine::store(lua, key, &thread) {
                    tracing::warn!("[lua] store coroutine for {}: {}", root, e);
                }
            }
        }
        Err(e) => {
            tracing::warn!("[lua] {}: {}", root, e);
        }
    }
    true
}

pub fn dispatch_strings(root: &str, method: Option<&str>, args: &[&str]) -> bool {
    let lua = sl_state();
    let Some(func) = resolve_func(lua, root, method) else {
        return false;
    };
    let mut mv = LuaMultiValue::new();
    for &s in args {
        mv.push_back(LuaValue::String(lua.create_string(s).unwrap()));
    }
    match func.call::<LuaMultiValue>(mv) {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!("[lua] {}: {}", root, e);
            false
        }
    }
}
