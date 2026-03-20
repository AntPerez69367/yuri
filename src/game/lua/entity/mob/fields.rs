use crate::game::lua::entity::mob::LuaMob;
use crate::game::lua::entity::types::EntityType;
use crate::game::map_server::map_id2mob_ref;

define_fields!(LuaMob, EntityType::Mob, map_id2mob_ref, {
    @read "Minimum damage" minDam: i64 => |g| g.mindam as i64;
    @read "Maximum damage" maxDam: i64 => |g| g.maxdam as i64;
    @read "Maximum health" maxHealth: i64 => |g| g.maxvita as i64
});
