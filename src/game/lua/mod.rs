use mlua::prelude::*;

pub mod error;
pub mod entity;

pub fn log_missing(entity_type: &str, key: &str, lua: Lua) {
    let location = lua
        .inspect_stack(2, |dbg| {
            let source = dbg.source();
            let src_name = source.short_src.as_deref().unwrap_or("unknown");
            format!("{}:{}", src_name, dbg.current_line().unwrap_or(0))
        })
        .unwrap_or_else(|| "unknown".to_string());
        tracing::warn!("[LUA-MISSING] {}:{} called from {}", entity_type, key, location);
    }