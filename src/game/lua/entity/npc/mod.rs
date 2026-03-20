mod methods;

use mlua::prelude::*;
use tealr::mlu::TealData;
use tealr::mlu::TealDataMethods;
use tealr::ToTypename;

use crate::game::lua::entity::shared::{self, HasEntityId};
use crate::game::lua::entity::types::EntityType;
use crate::game::lua::log_missing;

#[derive(Clone, tealr::mlu::UserData)]
pub struct LuaNpc {
    pub id: u32,
}

impl LuaNpc {
    pub fn new(id: u32) -> Self {
        Self { id }
    }
}

impl HasEntityId for LuaNpc {
    fn entity_id(&self) -> u32 { self.id }
}

impl ToTypename for LuaNpc {
    fn to_typename() -> tealr::Type {
        tealr::Type::Single(tealr::SingleType {
            name: tealr::Name(std::borrow::Cow::Borrowed("NPC")),
            kind: tealr::KindOfType::External,
            generics: vec![],
        })
    }
}

impl TealData for LuaNpc {
    fn add_methods<T: TealDataMethods<Self>>(methods: &mut T) {
        methods.document_type("A non-player character entity. Created via `NPC(id)`.");
        shared::register_shared_methods(methods);
        methods::register(methods);
        methods.add_meta_method(LuaMetaMethod::Index, |lua, this, key: String| {
            if let Some(result) = shared::try_shared_index(lua, &key, this.id) {
                return result;
            }
            log_missing(EntityType::Npc, &key, lua);
            Ok(LuaValue::Nil)
        });
    }
}
