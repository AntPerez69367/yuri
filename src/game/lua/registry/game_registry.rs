use crate::{
    database::blocking_run_async,
    game::map_server::{map_readglobalgamereg, map_setglobalgamereg_str},
};
use mlua::prelude::*;

#[derive(Clone)]
pub struct LuaGameRegistry;

impl LuaUserData for LuaGameRegistry {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(LuaMetaMethod::Index, |_, _this, key: String| {
            Ok(map_readglobalgamereg(&key))
        });

        methods.add_meta_method(
            LuaMetaMethod::NewIndex,
            |_, _this, (key, val): (String, LuaValue)| {
                let val_i: i32 = match val {
                    LuaValue::Integer(i) => i as i32,
                    LuaValue::Number(f) => f as i32,
                    _ => return Err(LuaError::runtime("gameRegistry expects integer values")),
                };
                blocking_run_async(async move {
                    map_setglobalgamereg_str(key, val_i).await;
                });
                Ok(())
            },
        );
    }
}
