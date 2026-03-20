use std::sync::atomic::Ordering;

use crate::common::traits::LegacyEntity;
use crate::game::lua::entity::player::LuaPlayer;
use crate::game::lua::entity::types::EntityType;
use crate::game::map_server::map_id2sd_pc;

define_fields!(LuaPlayer, EntityType::Player, map_id2sd_pc, {
    @direct "Character name" name: String => |arc| arc.name.clone();
    @direct "Entity ID" id: u32 => |arc| arc.id;
    @direct "Current health points" health: i32 => |arc| arc.hp_atomic.load(Ordering::Relaxed);
    @direct "Current mana points" mana: i32 => |arc| arc.mp_atomic.load(Ordering::Relaxed);
    @direct "Current experience points" exp: u32 => |arc| arc.exp_atomic.load(Ordering::Relaxed);
    @read "Base level" level: u16 => |g| g.player.progression.level as u16;
    @read "Character class" class: u8 => |g| g.player.progression.class;
    @read "Clan ID (0 if none)" clan: u32 => |g| g.player.social.clan;
    @read_write "GM permission level" gm_level: i8 => |g| g.player.identity.gm_level, |g, val| { g.player.identity.gm_level = val; }
});
