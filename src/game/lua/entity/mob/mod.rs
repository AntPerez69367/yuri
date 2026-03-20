mod fields;

use mlua::prelude::*;
use tealr::mlu::TealData;
use tealr::mlu::TealDataMethods;
use tealr::ToTypename;

use crate::game::lua::entity::shared::{self, HasEntityId};
use crate::game::lua::entity::types::EntityType;
use crate::game::lua::log_missing;

#[derive(Clone, tealr::mlu::UserData)]
pub struct LuaMob {
    pub id: u32,
}

impl LuaMob {
    pub fn new(id: u32) -> Self {
        Self { id }
    }
}

impl HasEntityId for LuaMob {
    fn entity_id(&self) -> u32 { self.id }
}

impl ToTypename for LuaMob {
    fn to_typename() -> tealr::Type {
        tealr::Type::Single(tealr::SingleType {
            name: tealr::Name(std::borrow::Cow::Borrowed("Mob")),
            kind: tealr::KindOfType::External,
            generics: vec![],
        })
    }
}

impl TealData for LuaMob {
    fn add_fields<F: tealr::mlu::TealDataFields<Self>>(fields: &mut F) {
        Self::register_fields(fields);
    }

    fn add_methods<T: TealDataMethods<Self>>(methods: &mut T) {
        methods.document_type("A mob (monster) entity. Created via `Mob(id)`.");
        shared::register_shared_methods(methods);
        methods.add_meta_method(LuaMetaMethod::Index, |lua, this, key: String| {
            if let Some(result) = shared::try_shared_index(lua, &key, this.id) {
                return result;
            }
            log_missing(EntityType::Mob, &key, lua);
            Ok(LuaValue::Nil)
        });
    }
}
