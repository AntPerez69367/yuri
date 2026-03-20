use mlua::prelude::*;
use crate::game::lua::{error::unknown_field, log_missing};
use crate::game::lua::entity::types::EntityType;
#[derive(Clone)]
pub struct LuaPlayer {
    pub id: u32,
}

impl LuaPlayer {
    pub fn new(id: u32) -> Self {
        Self { id }
    }

    pub fn lua_get(&self, key: &str, _lua: &Lua) -> LuaResult<LuaValue> {
        Err(unknown_field(EntityType::Player, key))
    }

    pub fn lua_set(&self, key: &str, _value: LuaValue, _lua: &Lua) -> LuaResult<()> {
        Err(unknown_field(EntityType::Player, key))
    }
}

impl LuaUserData for LuaPlayer {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("get", |lua, this, key: String| {
            this.lua_get(&key, lua)
        });

        methods.add_method("set", |lua, this, (key, value): (String, LuaValue)| {
            this.lua_set(&key, value, lua)
        });

        methods.add_meta_method(LuaMetaMethod::Index, |lua, _this, key: String| {
            log_missing(EntityType::Player, &key, lua);
            Ok(LuaValue::Nil)
        });
    }
}