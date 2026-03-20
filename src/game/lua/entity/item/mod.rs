use mlua::prelude::*;
use tealr::mlu::TealData;
use tealr::mlu::TealDataMethods;
use tealr::ToTypename;

use crate::game::lua::entity::shared::{self, HasEntityId};
use crate::game::lua::entity::types::EntityType;
use crate::game::lua::log_missing;

#[derive(Clone, tealr::mlu::UserData)]
pub struct LuaItem {
    pub id: u32,
}

impl LuaItem {
    pub fn new(id: u32) -> Self {
        Self { id }
    }
}

impl HasEntityId for LuaItem {
    fn entity_id(&self) -> u32 { self.id }
}

impl ToTypename for LuaItem {
    fn to_typename() -> tealr::Type {
        tealr::Type::Single(tealr::SingleType {
            name: tealr::Name(std::borrow::Cow::Borrowed("Item")),
            kind: tealr::KindOfType::External,
            generics: vec![],
        })
    }
}

impl TealData for LuaItem {
    fn add_methods<T: TealDataMethods<Self>>(methods: &mut T) {
        methods.document_type("A floor item entity. Created via `FloorItem(id)`.");
        shared::register_shared_methods(methods);
        methods.add_meta_method(LuaMetaMethod::Index, |lua, _this, key: String| {
            log_missing(EntityType::Item, &key, lua);
            Ok(LuaValue::Nil)
        });
    }
}
