mod add_npc;

use tealr::mlu::TealDataMethods;

use crate::game::lua::entity::npc::LuaNpc;

pub fn register(methods: &mut impl TealDataMethods<LuaNpc>) {
    add_npc::register(methods);
}
