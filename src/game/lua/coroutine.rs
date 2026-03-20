use mlua::prelude::*;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CoroutineKey {
    pub player_id: u32,
    pub context_id: u32,
}

struct CoroutineMap(HashMap<CoroutineKey, LuaRegistryKey>);
unsafe impl Send for CoroutineMap {}
unsafe impl Sync for CoroutineMap {}

static REGISTRY: OnceLock<Mutex<CoroutineMap>> = OnceLock::new();

fn registry() -> MutexGuard<'static, CoroutineMap> {
    REGISTRY
        .get_or_init(|| Mutex::new(CoroutineMap(HashMap::new())))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

pub fn init() {
    drop(registry());
}

pub fn store(lua: &Lua, key: CoroutineKey, thread: &LuaThread) -> LuaResult<()> {
    let registry_key = lua.create_registry_value(thread)?;
    if let Some(old_key) = registry().0.insert(key, registry_key) {
        lua.remove_registry_value(old_key)?;
    }
    Ok(())
}

pub fn resume(lua: &Lua, key: CoroutineKey, response: LuaValue) -> LuaResult<bool> {
    let registry_key = registry().0.remove(&key);
    let Some(registry_key) = registry_key else {
        return Ok(false);
    };
    let thread: LuaThread = lua.registry_value(&registry_key)?;
    lua.remove_registry_value(registry_key)?;

    match thread.resume::<LuaMultiValue>(response) {
        Ok(_) => {
            if thread.status() == LuaThreadStatus::Resumable {
                store(lua, key, &thread)?;
            }
            Ok(true)
        }
        Err(e) => {
            tracing::warn!(
                "[LUA-COROUTINE] Error resuming coroutine for player {:?}: {}",
                key,
                e
            );
            Ok(true)
        }
    }
}

pub fn cancel(key: CoroutineKey) {
    registry().0.remove(&key);
}

pub fn purge_player(player_id: u32) {
    registry().0.retain(|key, _| key.player_id != player_id);
}
