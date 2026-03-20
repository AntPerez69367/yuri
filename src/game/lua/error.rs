use mlua::prelude::*;

pub fn entity_not_found(entity_type: &str, id: u32) -> LuaError {
    mlua::Error::runtime(format!("{} with id {} not found", entity_type, id))
}

pub fn unknown_field(entity_type: &str, field: &str) -> LuaError {
    mlua::Error::runtime(format!("Unknown field '{}' on {}", field, entity_type))
}

pub fn type_mismatch(entity_type: &str, field: &str, expected: &str) -> LuaError {
    mlua::Error::runtime(format!("Expected {} for field '{}' on {}", expected, field, entity_type))
}