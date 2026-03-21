#![allow(non_snake_case, dead_code, unused_variables)]

use super::entity::{ScriptReg, ScriptRegStr};
use super::spatial::pc_diescript;
use super::types::MapSessionData;
use crate::common::constants::entity::player::{
    EQ_LEFT, EQ_RIGHT, EQ_SHIELD, EQ_SUBLEFT, EQ_SUBRIGHT, EQ_WEAP, FLAG_ADVICE, FLAG_MAIL,
    FLAG_PARCEL, ITM_BAG, ITM_EAT, ITM_ETC, ITM_FACE, ITM_FACEACCTWO, ITM_HAIR_DYE, ITM_HAND,
    ITM_MAP, ITM_MOUNT, ITM_QUIVER, ITM_SET, ITM_SHIELD, ITM_SKIN, ITM_SMOKE, ITM_USE, ITM_USESPC,
    ITM_WEAP, MAP_ERRGHOST, MAP_ERRITM2H, MAP_ERRITMFULL, MAP_ERRITMLEVEL, MAP_ERRITMMARK,
    MAP_ERRITMMIGHT, MAP_ERRITMPATH, MAP_ERRITMSEX, MAP_ERRMOUNT, PC_DIE, PC_MOUNTED,
    SFLAG_FULLSTATS, SFLAG_HPMP, SFLAG_XPMONEY, SP_HP, SP_MHP, SP_MMP, SP_MP,
};
use crate::common::constants::entity::BL_NPC;
use crate::common::constants::entity::SUBTYPE_FLOOR as FLOOR;
use crate::common::constants::world::MAX_GROUP_MEMBERS;
use crate::common::player::spells::MAX_SPELLS;
use crate::common::types::{Item, SkillInfo};
use crate::database::class_db::{level as classdb_level, path as classdb_path};
use crate::database::{self, item_db, magic_db, map_db};
use crate::game::block::AreaType;
use crate::game::block_grid;
use crate::game::client::visual::{
    broadcast_update_state, clif_sendupdatestatus, clif_sendupdatestatus_onequip,
};
use crate::game::lua::dispatch::dispatch;
use crate::game::map_parse::chat::clif_sendminitext;
use crate::game::map_parse::combat::{
    clif_send_aether, clif_send_duration, clif_send_groupbars, clif_send_mobbars_inner,
    clif_send_selfbar, clif_sendaction_pc, clif_sendanimation_inner, clif_sendmagic,
};
use crate::game::map_parse::groups::clif_grouphealth_update;
use crate::game::map_parse::items::{clif_sendadditem, clif_senddelitem, clif_sendequip};
use crate::game::map_parse::movement::clif_sendchararea;
use crate::game::map_parse::packet::{wfifohead, wfifop, wfifoset};
use crate::game::map_parse::player_state::{clif_getchararea, clif_sendstatus};
use crate::game::map_parse::visual::{clif_lookgone_by_id, clif_object_look2_item};
use crate::game::map_server::{
    self as map_server, map_additem, map_delitem, map_id2fl, map_readglobalreg,
};
use crate::game::map_server::{groups, map_msg};
use crate::game::mob::MAX_MAGIC_TIMERS;
use crate::game::npc::NpcData;
use crate::game::player::prelude::*;
use crate::game::scripting::pc_accessors::sl_pc_forcesave;
use crate::game::scripting::types::floor::FloorItemData;
use crate::game::scripting::{self, sl_async_freeco};
use crate::game::time_util::{gettick, timer_insert, timer_remove};
use crate::network::crypt::encrypt;
use crate::session::{session_exists, SessionId};
use std::mem;

unsafe fn encrypt_fd(fd: SessionId) -> i32 {
    encrypt(fd)
}
unsafe fn gettick_pc() -> u32 {
    gettick()
}

// ─── Lua dispatch helpers ─────────────────────────────────────────────────────

/// Dispatch a Lua event with a single entity-ID argument.
pub(super) fn sl_doscript_simple_pc(root: &str, method: Option<&str>, id: u32) -> bool {
    dispatch(root, method, &[id])
}

/// Dispatch a Lua event with two entity-ID arguments.
pub(super) fn sl_doscript_2_pc(root: &str, method: Option<&str>, id1: u32, id2: u32) -> bool {
    dispatch(root, method, &[id1, id2])
}

// ─── Timer functions ─────────────────────────────────────────────────────────

/// Removes a floor item when its timer expires.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_item_timer(id: i32, _none: i32) -> i32 {
    if map_server::entity_position(id as u32).is_none() {
        return 1;
    }
    clif_lookgone_by_id(id as u32);
    map_delitem(id as u32);
    1
}

/// Periodically saves a player's character data.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_savetimer(id: i32, _none: i32) -> i32 {
    if let Some(pe) = map_server::map_id2sd_pc(id as u32) {
        sl_pc_forcesave(&pe);
    }
    0
}

/// Resets `castusetimer` field to 0 each tick.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_castusetimer(id: i32, _none: i32) -> i32 {
    if let Some(pe) = map_server::map_id2sd_pc(id as u32) {
        pe.write().castusetimer = 0;
    }
    0
}

/// Tracks AFK time and plays idle animations.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_afktimer(id: i32, _none: i32) -> i32 {
    let pe = match map_server::map_id2sd_pc(id as u32) {
        Some(pe) => pe,
        None => return 0,
    };

    pe.write().afktime += 1;

    let afk = pe.read().afk;
    let state = pe.read().player.combat.state;

    if afk == 1 && state == 0 {
        pe.write().totalafktime += 10;
        clif_sendaction_pc(&mut pe.write(), 0x10, 0x4E, 0);
        return 0;
    }

    if afk == 1 && state == 3 {
        pe.write().totalafktime += 10;
        let (m, x, y, player_id) = {
            let sd = pe.read();
            (sd.m, sd.x, sd.y, sd.id)
        };
        if let Some(grid) = block_grid::get_grid(m as usize) {
            let slot = &*map_db::get_map_ptr(m);
            let ids = block_grid::ids_in_area(
                grid,
                x as i32,
                y as i32,
                AreaType::Area,
                slot.xs as i32,
                slot.ys as i32,
            );
            for id in ids {
                if let Some(tsd_arc) = map_server::map_id2sd_pc(id) {
                    let tsd_guard = tsd_arc.read();
                    clif_sendanimation_inner(
                        tsd_guard.fd,
                        tsd_guard.player.appearance.setting_flags,
                        324,
                        player_id,
                        0,
                    );
                }
            }
        }
        return 0;
    }

    if afk == 1 && state == PC_DIE as i8 {
        pe.write().totalafktime += 10;
        return 0;
    }

    if pe.read().afktime >= 30 {
        if state == 0 {
            pe.write().totalafktime += 300;
            clif_sendaction_pc(&mut pe.write(), 0x10, 0x4E, 0);
        } else if state == 3 {
            pe.write().totalafktime += 300;
            let (m, x, y, player_id) = {
                let sd = pe.read();
                (sd.m, sd.x, sd.y, sd.id)
            };
            if let Some(grid) = block_grid::get_grid(m as usize) {
                let slot = &*map_db::get_map_ptr(m);
                let ids = block_grid::ids_in_area(
                    grid,
                    x as i32,
                    y as i32,
                    AreaType::Area,
                    slot.xs as i32,
                    slot.ys as i32,
                );
                for id in ids {
                    if let Some(tsd_arc) = map_server::map_id2sd_pc(id) {
                        let tsd_guard = tsd_arc.read();
                        clif_sendanimation_inner(
                            tsd_guard.fd,
                            tsd_guard.player.appearance.setting_flags,
                            324,
                            player_id,
                            0,
                        );
                    }
                }
            }
        }
        pe.write().afk = 1;
    }

    0
}

/// Registers all periodic timers for a logged-in player.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_starttimer(sd: *mut MapSessionData) -> i32 {
    (*sd).timer = timer_insert(
        1000,
        1000,
        Some(pc_timer as unsafe fn(i32, i32) -> i32),
        (*sd).id as i32,
        0,
    );
    (*sd).pongtimer = timer_insert(
        30000,
        30000,
        Some(pc_sendpong as unsafe fn(i32, i32) -> i32),
        (*sd).id as i32,
        0,
    );
    (*sd).savetimer = timer_insert(
        60000,
        60000,
        Some(pc_savetimer as unsafe fn(i32, i32) -> i32),
        (*sd).id as i32,
        0,
    );
    if (*sd).player.identity.gm_level < 50 {
        (*sd).afktimer = timer_insert(
            10000,
            10000,
            Some(pc_afktimer as unsafe fn(i32, i32) -> i32),
            (*sd).id as i32,
            0,
        );
    }
    (*sd).duratimer = timer_insert(
        1000,
        1000,
        Some(bl_duratimer as unsafe fn(i32, i32) -> i32),
        (*sd).id as i32,
        0,
    );
    (*sd).secondduratimer = timer_insert(
        250,
        250,
        Some(bl_secondduratimer as unsafe fn(i32, i32) -> i32),
        (*sd).id as i32,
        0,
    );
    (*sd).thirdduratimer = timer_insert(
        500,
        500,
        Some(bl_thirdduratimer as unsafe fn(i32, i32) -> i32),
        (*sd).id as i32,
        0,
    );
    (*sd).fourthduratimer = timer_insert(
        1500,
        1500,
        Some(bl_fourthduratimer as unsafe fn(i32, i32) -> i32),
        (*sd).id as i32,
        0,
    );
    (*sd).fifthduratimer = timer_insert(
        3000,
        3000,
        Some(bl_fifthduratimer as unsafe fn(i32, i32) -> i32),
        (*sd).id as i32,
        0,
    );
    (*sd).scripttimer = timer_insert(
        500,
        500,
        Some(pc_scripttimer as unsafe fn(i32, i32) -> i32),
        (*sd).id as i32,
        0,
    );
    (*sd).castusetimer = timer_insert(
        250,
        250,
        Some(pc_castusetimer as unsafe fn(i32, i32) -> i32),
        (*sd).id as i32,
        0,
    );
    0
}

/// Removes all periodic timers for a player.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_stoptimer(sd: *mut MapSessionData) -> i32 {
    if (*sd).timer != 0 {
        timer_remove((*sd).timer);
        (*sd).timer = 0;
    }
    if (*sd).healingtimer != 0 {
        timer_remove((*sd).healingtimer);
        (*sd).healingtimer = 0;
    }
    if (*sd).pongtimer != 0 {
        timer_remove((*sd).pongtimer);
        (*sd).pongtimer = 0;
    }
    if (*sd).afktimer != 0 {
        timer_remove((*sd).afktimer);
        (*sd).afktimer = 0;
    }
    if (*sd).duratimer != 0 {
        timer_remove((*sd).duratimer);
        (*sd).duratimer = 0;
    }
    if (*sd).savetimer != 0 {
        timer_remove((*sd).savetimer);
        (*sd).savetimer = 0;
    }
    if (*sd).secondduratimer != 0 {
        timer_remove((*sd).secondduratimer);
        (*sd).secondduratimer = 0;
    }
    if (*sd).thirdduratimer != 0 {
        timer_remove((*sd).thirdduratimer);
        (*sd).thirdduratimer = 0;
    }
    if (*sd).fourthduratimer != 0 {
        timer_remove((*sd).fourthduratimer);
        (*sd).fourthduratimer = 0;
    }
    if (*sd).fifthduratimer != 0 {
        timer_remove((*sd).fifthduratimer);
        (*sd).fifthduratimer = 0;
    }
    if (*sd).scripttimer != 0 {
        timer_remove((*sd).scripttimer);
        (*sd).scripttimer = 0;
    }
    0
}

/// 1000ms tick: processes skill passive/equip
/// while-effects and decrements duration/aether for active magic on a player.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn bl_duratimer(id: i32, _none: i32) -> i32 {
    let pe = match map_server::map_id2sd_pc(id as u32) {
        Some(pe) => pe,
        None => return 0,
    };

    // while_passive: each learned spell fires once per second
    for x in 0..52usize {
        let skill_id = pe.read().player.spells.skills[x];
        if skill_id > 0 {
            sl_doscript_simple_pc(
                scripting::carray_to_str(&magic_db::search(skill_id as i32).yname),
                Some("while_passive"),
                pe.id,
            );
        }
    }

    // while_equipped: each worn item fires once per second
    for x in 0..14usize {
        let equip_id = pe.read().player.inventory.equip[x].id;
        if equip_id > 0 {
            sl_doscript_simple_pc(
                scripting::carray_to_str(&item_db::search(equip_id).yname),
                Some("while_equipped"),
                pe.id,
            );
        }
    }

    // duration / aether tick for each active magic timer slot
    for x in 0..MAX_MAGIC_TIMERS {
        let (slot_id, mid, caster_id) = {
            let sd = pe.read();
            let slot = &sd.player.spells.dura_aether[x];
            (slot.id, slot.id as i32, slot.caster_id)
        };
        if slot_id > 0 {
            let caster_alive = if caster_id > 0 {
                if let Some(arc) = map_server::map_id2mob_ref(caster_id) {
                    arc.read().current_vita > 0
                } else {
                    map_server::map_id2sd_pc(caster_id).is_some()
                }
            } else {
                false
            };

            let duration = pe.read().player.spells.dura_aether[x].duration;
            if duration > 0 {
                pe.write().player.spells.dura_aether[x].duration -= 1000;

                if caster_id > 0 {
                    if caster_alive {
                        sl_doscript_2_pc(
                            scripting::carray_to_str(&magic_db::search(mid).yname),
                            Some("while_cast"),
                            pe.id,
                            caster_id,
                        );
                    }
                } else {
                    sl_doscript_simple_pc(
                        scripting::carray_to_str(&magic_db::search(mid).yname),
                        Some("while_cast"),
                        pe.id,
                    );
                }

                if pe.read().player.spells.dura_aether[x].duration <= 0 {
                    pe.write().player.spells.dura_aether[x].duration = 0;

                    // Send duration expiry to client
                    {
                        let spell_id = pe.read().player.spells.dura_aether[x].id as i32;
                        let caster_pe = if caster_id > 0 && caster_id != pe.id {
                            map_server::map_id2sd_pc(caster_id)
                        } else {
                            None
                        };
                        let fd = pe.fd;
                        let caster_name: Option<String> = if caster_id == pe.id {
                            // Self-cast
                            Some(pe.name.clone())
                        } else {
                            caster_pe.as_ref().map(|cpe| cpe.name.clone())
                        };
                        clif_send_duration(fd, spell_id, 0, caster_name.as_deref());
                    }

                    pe.write().player.spells.dura_aether[x].caster_id = 0;

                    // Broadcast animation removal
                    {
                        let (anim, m, px, py) = {
                            let sd = pe.read();
                            (
                                sd.player.spells.dura_aether[x].animation as i32,
                                sd.m,
                                sd.x,
                                sd.y,
                            )
                        };
                        if let Some(grid) = block_grid::get_grid(m as usize) {
                            let slot = &*map_db::get_map_ptr(m);
                            let ids = block_grid::ids_in_area(
                                grid,
                                px as i32,
                                py as i32,
                                AreaType::Area,
                                slot.xs as i32,
                                slot.ys as i32,
                            );
                            for id in ids {
                                if let Some(tsd_arc) = map_server::map_id2sd_pc(id) {
                                    let tsd_guard = tsd_arc.read();
                                    clif_sendanimation_inner(
                                        tsd_guard.fd,
                                        tsd_guard.player.appearance.setting_flags,
                                        anim,
                                        pe.id,
                                        -1,
                                    );
                                }
                            }
                        }
                    }

                    pe.write().player.spells.dura_aether[x].animation = 0;

                    if pe.read().player.spells.dura_aether[x].aether == 0 {
                        pe.write().player.spells.dura_aether[x].id = 0;
                    }

                    if caster_id > 0 {
                        sl_doscript_2_pc(
                            scripting::carray_to_str(&magic_db::search(mid).yname),
                            Some("uncast"),
                            pe.id,
                            caster_id,
                        );
                    } else {
                        sl_doscript_simple_pc(
                            scripting::carray_to_str(&magic_db::search(mid).yname),
                            Some("uncast"),
                            pe.id,
                        );
                    }
                }
            }

            let aether = pe.read().player.spells.dura_aether[x].aether;
            if aether > 0 {
                pe.write().player.spells.dura_aether[x].aether -= 1000;

                if pe.read().player.spells.dura_aether[x].aether <= 0 {
                    let spell_id = pe.read().player.spells.dura_aether[x].id as i32;
                    clif_send_aether(&mut pe.write(), spell_id, 0);

                    if pe.read().player.spells.dura_aether[x].duration == 0 {
                        pe.write().player.spells.dura_aether[x].id = 0;
                    }

                    pe.write().player.spells.dura_aether[x].aether = 0;
                }
            }
        }
    }

    0
}

/// 250ms tick: fires `while_passive_250`
/// and `while_equipped_250` and `while_cast_250` events (no expire logic).
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn bl_secondduratimer(id: i32, _none: i32) -> i32 {
    let pe = match map_server::map_id2sd_pc(id as u32) {
        Some(pe) => pe,
        None => return 0,
    };

    for x in 0..52usize {
        let skill_id = pe.read().player.spells.skills[x];
        if skill_id > 0 {
            sl_doscript_simple_pc(
                scripting::carray_to_str(&magic_db::search(skill_id as i32).yname),
                Some("while_passive_250"),
                pe.id,
            );
        }
    }

    for x in 0..14usize {
        let equip_id = pe.read().player.inventory.equip[x].id;
        if equip_id > 0 {
            sl_doscript_simple_pc(
                scripting::carray_to_str(&item_db::search(equip_id).yname),
                Some("while_equipped_250"),
                pe.id,
            );
        }
    }

    for x in 0..MAX_MAGIC_TIMERS {
        let (slot_id, caster_id, spell_duration) = {
            let sd = pe.read();
            let slot = &sd.player.spells.dura_aether[x];
            (slot.id, slot.caster_id, slot.duration)
        };
        if slot_id > 0 {
            let caster_alive = if caster_id > 0 {
                if let Some(arc) = map_server::map_id2mob_ref(caster_id) {
                    arc.read().current_vita > 0
                } else {
                    map_server::map_id2sd_pc(caster_id).is_some()
                }
            } else {
                false
            };

            if spell_duration > 0 {
                if caster_id > 0 {
                    if caster_alive {
                        sl_doscript_2_pc(
                            scripting::carray_to_str(&magic_db::search(slot_id as i32).yname),
                            Some("while_cast_250"),
                            pe.id,
                            caster_id,
                        );
                    }
                } else {
                    sl_doscript_simple_pc(
                        scripting::carray_to_str(&magic_db::search(slot_id as i32).yname),
                        Some("while_cast_250"),
                        pe.id,
                    );
                }
            }
        }
    }

    0
}

/// 500ms tick: fires `while_passive_500`,
/// `while_equipped_500`, `while_cast_500` events (no expire logic).
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn bl_thirdduratimer(id: i32, _none: i32) -> i32 {
    let pe = match map_server::map_id2sd_pc(id as u32) {
        Some(pe) => pe,
        None => return 0,
    };

    for x in 0..52usize {
        let skill_id = pe.read().player.spells.skills[x];
        if skill_id > 0 {
            sl_doscript_simple_pc(
                scripting::carray_to_str(&magic_db::search(skill_id as i32).yname),
                Some("while_passive_500"),
                pe.id,
            );
        }
    }

    for x in 0..14usize {
        let equip_id = pe.read().player.inventory.equip[x].id;
        if equip_id > 0 {
            sl_doscript_simple_pc(
                scripting::carray_to_str(&item_db::search(equip_id).yname),
                Some("while_equipped_500"),
                pe.id,
            );
        }
    }

    for x in 0..MAX_MAGIC_TIMERS {
        let (slot_id, caster_id, spell_duration) = {
            let sd = pe.read();
            let slot = &sd.player.spells.dura_aether[x];
            (slot.id, slot.caster_id, slot.duration)
        };
        if slot_id > 0 {
            let caster_alive = if caster_id > 0 {
                if let Some(arc) = map_server::map_id2mob_ref(caster_id) {
                    arc.read().current_vita > 0
                } else {
                    map_server::map_id2sd_pc(caster_id).is_some()
                }
            } else {
                false
            };

            if spell_duration > 0 {
                if caster_id > 0 {
                    if caster_alive {
                        sl_doscript_2_pc(
                            scripting::carray_to_str(&magic_db::search(slot_id as i32).yname),
                            Some("while_cast_500"),
                            pe.id,
                            caster_id,
                        );
                    }
                } else {
                    sl_doscript_simple_pc(
                        scripting::carray_to_str(&magic_db::search(slot_id as i32).yname),
                        Some("while_cast_500"),
                        pe.id,
                    );
                }
            }
        }
    }

    0
}

/// 1500ms tick: fires `while_passive_1500`,
/// `while_equipped_1500`, `while_cast_1500` events (no expire logic).
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn bl_fourthduratimer(id: i32, _none: i32) -> i32 {
    let pe = match map_server::map_id2sd_pc(id as u32) {
        Some(pe) => pe,
        None => return 0,
    };

    for x in 0..52usize {
        let skill_id = pe.read().player.spells.skills[x];
        if skill_id > 0 {
            sl_doscript_simple_pc(
                scripting::carray_to_str(&magic_db::search(skill_id as i32).yname),
                Some("while_passive_1500"),
                pe.id,
            );
        }
    }

    for x in 0..14usize {
        let equip_id = pe.read().player.inventory.equip[x].id;
        if equip_id > 0 {
            sl_doscript_simple_pc(
                scripting::carray_to_str(&item_db::search(equip_id).yname),
                Some("while_equipped_1500"),
                pe.id,
            );
        }
    }

    for x in 0..MAX_MAGIC_TIMERS {
        let (slot_id, caster_id, spell_duration) = {
            let sd = pe.read();
            let slot = &sd.player.spells.dura_aether[x];
            (slot.id, slot.caster_id, slot.duration)
        };
        if slot_id > 0 {
            let caster_alive = if caster_id > 0 {
                if let Some(arc) = map_server::map_id2mob_ref(caster_id) {
                    arc.read().current_vita > 0
                } else {
                    map_server::map_id2sd_pc(caster_id).is_some()
                }
            } else {
                false
            };

            if spell_duration > 0 {
                if caster_id > 0 {
                    if caster_alive {
                        sl_doscript_2_pc(
                            scripting::carray_to_str(&magic_db::search(slot_id as i32).yname),
                            Some("while_cast_1500"),
                            pe.id,
                            caster_id,
                        );
                    }
                } else {
                    sl_doscript_simple_pc(
                        scripting::carray_to_str(&magic_db::search(slot_id as i32).yname),
                        Some("while_cast_1500"),
                        pe.id,
                    );
                }
            }
        }
    }

    0
}

/// 3000ms tick: fires `while_passive_3000`,
/// `while_equipped_3000`, `while_cast_3000` events (no expire logic).
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn bl_fifthduratimer(id: i32, _none: i32) -> i32 {
    let pe = match map_server::map_id2sd_pc(id as u32) {
        Some(pe) => pe,
        None => return 0,
    };

    for x in 0..52usize {
        let skill_id = pe.read().player.spells.skills[x];
        if skill_id > 0 {
            sl_doscript_simple_pc(
                scripting::carray_to_str(&magic_db::search(skill_id as i32).yname),
                Some("while_passive_3000"),
                pe.id,
            );
        }
    }

    for x in 0..14usize {
        let equip_id = pe.read().player.inventory.equip[x].id;
        if equip_id > 0 {
            sl_doscript_simple_pc(
                scripting::carray_to_str(&item_db::search(equip_id).yname),
                Some("while_equipped_3000"),
                pe.id,
            );
        }
    }

    for x in 0..MAX_MAGIC_TIMERS {
        let (slot_id, caster_id, spell_duration) = {
            let sd = pe.read();
            let slot = &sd.player.spells.dura_aether[x];
            (slot.id, slot.caster_id, slot.duration)
        };
        if slot_id > 0 {
            let caster_alive = if caster_id > 0 {
                if let Some(arc) = map_server::map_id2mob_ref(caster_id) {
                    arc.read().current_vita > 0
                } else {
                    map_server::map_id2sd_pc(caster_id).is_some()
                }
            } else {
                false
            };

            if spell_duration > 0 {
                if caster_id > 0 {
                    if caster_alive {
                        sl_doscript_2_pc(
                            scripting::carray_to_str(&magic_db::search(slot_id as i32).yname),
                            Some("while_cast_3000"),
                            pe.id,
                            caster_id,
                        );
                    }
                } else {
                    sl_doscript_simple_pc(
                        scripting::carray_to_str(&magic_db::search(slot_id as i32).yname),
                        Some("while_cast_3000"),
                        pe.id,
                    );
                }
            }
        }
    }

    0
}

/// Decrements aether timers and clears
/// expired aether slots; called from NPC/scripting code via a one-shot timer.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn bl_aethertimer(id: i32, _none: i32) -> i32 {
    let pe = match map_server::map_id2sd_pc(id as u32) {
        Some(pe) => pe,
        None => return 0,
    };

    for x in 0..MAX_MAGIC_TIMERS {
        let slot_id = pe.read().player.spells.dura_aether[x].id;
        if slot_id > 0 {
            {
                let mut sd = pe.write();
                if sd.player.spells.dura_aether[x].aether > 0 {
                    sd.player.spells.dura_aether[x].aether -= 1000;
                }
            }

            let aether = pe.read().player.spells.dura_aether[x].aether;
            if aether <= 0 {
                let spell_id = pe.read().player.spells.dura_aether[x].id as i32;
                {
                    let mut sd = pe.write();
                    clif_send_aether(&mut sd, spell_id, 0);

                    if sd.player.spells.dura_aether[x].duration == 0 {
                        sd.player.spells.dura_aether[x].id = 0;
                    }

                    sd.player.spells.dura_aether[x].aether = 0;
                }
                return 0;
            }
        }
    }

    0
}

/// 1000ms main player tick: resets cooldowns,
/// expires PvP flags, decrements PK duration, and updates group health bars.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_timer(id: i32, _none: i32) -> i32 {
    let pe = match map_server::map_id2sd_pc(id as u32) {
        Some(pe) => pe,
        None => return 1,
    };

    {
        let mut sd = pe.write();
        sd.time2 += 1000;
        sd.time = 0;
        sd.chat_timer = 0;

        if sd.time2 >= 60000 {
            pc_requestmp(&mut *sd as *mut MapSessionData);
            sd.time2 = 0;
        }
    }

    let mut reset: i32 = 0;
    {
        let mut sd = pe.write();
        for x in 0..20usize {
            if sd.pvp[x][1] != 0 && gettick_pc().wrapping_sub(sd.pvp[x][1]) >= 60000 {
                sd.pvp[x][0] = 0;
                sd.pvp[x][1] = 0;
                reset = 1;
            }
        }
    }

    {
        let pk = pe.read().player.social.pk;
        let pk_duration = pe.read().player.social.pk_duration;
        if pk == 1 && pk_duration > 0 {
            pe.write().player.social.pk_duration -= 1000;

            if pe.read().player.social.pk_duration == 0 {
                pe.write().player.social.pk = 0;
                clif_sendchararea(&pe);
            }
        }
    }

    if pe.read().group_count > 0 {
        clif_grouphealth_update(&pe);
    }

    if reset != 0 {
        clif_getchararea(&pe);
    }

    0
}

/// 500ms script tick: updates UI bars,
/// fires die script on death, fires Lua `pc_timer` tick/advice hooks.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_scripttimer(id: i32, _none: i32) -> i32 {
    let pe = match map_server::map_id2sd_pc(id as u32) {
        Some(pe) => pe,
        None => return 1,
    };

    if pe.read().selfbar != 0 {
        clif_send_selfbar(&mut pe.write());
    }

    let (groupbars, group_count, groupid, m, x_pos, y_pos) = {
        let sd = pe.read();
        (sd.groupbars, sd.group_count, sd.groupid, sd.m, sd.x, sd.y)
    };

    if groupbars != 0 && group_count > 1 {
        let base = groupid as usize * 256;
        let grp = groups();
        if base < grp.len() {
            for x in 0..group_count as usize {
                if base + x >= grp.len() {
                    break;
                }
                let member_id = grp[base + x];
                let tsd_pe = map_server::map_id2sd_pc(member_id);
                let tsd_pe = match tsd_pe {
                    Some(t) => t,
                    None => continue,
                };
                let tsd_m = tsd_pe.read().m;
                if tsd_m == m {
                    if member_id == pe.id {
                        // Self — use single guard for both args
                        let mut sd = pe.write();
                        let sd_ptr = &mut *sd as *mut MapSessionData;
                        clif_send_groupbars(&mut *sd_ptr, &mut *sd_ptr);
                    } else {
                        let mut sd = pe.write();
                        let mut tsd = tsd_pe.write();
                        clif_send_groupbars(&mut sd, &mut tsd);
                    }
                    clif_grouphealth_update(&pe);
                }
            }
        }
    }

    if pe.read().mobbars != 0 {
        if let Some(grid) = block_grid::get_grid(m as usize) {
            let slot = &*map_db::get_map_ptr(m);
            let ids = block_grid::ids_in_area(
                grid,
                x_pos as i32,
                y_pos as i32,
                AreaType::Area,
                slot.xs as i32,
                slot.ys as i32,
            );
            for id in ids {
                if let Some(mob_arc) = map_server::map_id2mob_ref(id) {
                    let mob = mob_arc.read();
                    let sd = pe.read();
                    clif_send_mobbars_inner(&mob, &sd);
                }
            }
        }
    }

    {
        let sd = pe.read();
        if sd.player.combat.hp == 0 && sd.deathflag != 0 {
            drop(sd);
            pc_diescript(&mut *pe.write() as *mut MapSessionData);
            return 0;
        }
    }

    {
        let dmgshield = pe.read().dmgshield;
        if dmgshield > 0.0 {
            clif_send_duration(pe.fd, 0, dmgshield as i32 + 1, None);
        }
    }

    {
        let mut sd = pe.write();
        sd.deathflag = 0;
        sd.scripttick += 1;
    }

    sl_doscript_simple_pc("pc_timer", Some("tick"), pe.id);

    if pe.read().player.appearance.setting_flags & FLAG_ADVICE != 0 {
        sl_doscript_simple_pc("pc_timer", Some("advice"), pe.id);
    }

    0
}

/// Resets `attacked` flag; called by a one-shot timer.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_atkspeed(id: i32, _none: i32) -> i32 {
    let pe = match map_server::map_id2sd_pc(id as u32) {
        Some(pe) => pe,
        None => {
            tracing::warn!("[attack] pc_atkspeed: id={} sd=null, removing timer", id);
            return 1;
        }
    };
    tracing::debug!(
        "[attack] pc_atkspeed: id={} resetting attacked from {} to 0",
        id,
        pe.read().attacked
    );
    pe.write().attacked = 0;
    1
}

/// Counts down the display timer and fires
/// the Lua `display_timer` event when it reaches zero.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_disptimertick(id: i32, _none: i32) -> i32 {
    let pe = match map_server::map_id2sd_pc(id as u32) {
        Some(pe) => pe,
        None => return 1,
    };

    {
        let mut sd = pe.write();
        if (sd.disptimertick as i64) - 1 < 0 {
            sd.disptimertick = 0;
        } else {
            sd.disptimertick -= 1;
        }
    }

    if pe.read().disptimertick == 0 {
        sl_doscript_simple_pc("pc_timer", Some("display_timer"), pe.id);
        let mut sd = pe.write();
        timer_remove(sd.disptimer as i32);
        sd.disptimertype = 0;
        sd.disptimer = 0;
        return 1;
    }

    0
}

/// Sends a keep-alive ping packet to the client
/// and sets EOF if the session has already closed.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_sendpong(id: i32, _none: i32) -> i32 {
    let pe = match map_server::map_id2sd_pc(id as u32) {
        Some(pe) => pe,
        None => return 1,
    };

    let fd = pe.fd;
    if !session_exists(fd) {
        return 0;
    }

    // WFIFOHEAD(fd, 10)
    wfifohead(fd, 10);

    // WFIFOB(fd, 0) = 0xAA
    let p = wfifop(fd, 0);
    if !p.is_null() {
        *p = 0xAAu8;
    }

    // WFIFOW(fd, 1) = SWAP16(0x09)  — big-endian 16-bit (byte-swap of 0x0009 → 0x0900)
    let p = wfifop(fd, 1) as *mut u16;
    if !p.is_null() {
        p.write_unaligned(0x09u16.swap_bytes());
    }

    // WFIFOB(fd, 3) = 0x68
    let p = wfifop(fd, 3);
    if !p.is_null() {
        *p = 0x68u8;
    }

    // WFIFOL(fd, 5) = SWAP32(gettick())  — big-endian 32-bit tick
    let tick = gettick_pc();
    let p = wfifop(fd, 5) as *mut u32;
    if !p.is_null() {
        p.write_unaligned(tick.swap_bytes());
    }

    // WFIFOB(fd, 9) = 0x00
    let p = wfifop(fd, 9);
    if !p.is_null() {
        *p = 0x00u8;
    }

    // WFIFOSET(fd, encrypt(fd))
    let enc_len = encrypt_fd(fd);
    wfifoset(fd, enc_len as usize);

    pe.write().LastPingTick = gettick_pc() as u64;
    0
}

// ─── Stat-calculation functions ───────────────────────────────────────────────

/// Checks mail and parcel tables via SQL and sets
/// FLAG_MAIL / FLAG_PARCEL bits on `sd->flags`.
///
async fn check_new_mail(char_name: String) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM `Mail` WHERE `MalNew` = 1 AND `MalChaNameDestination` = ?",
    )
    .bind(char_name)
    .fetch_one(database::get_pool())
    .await
    .unwrap_or(0)
        > 0
}

async fn check_pending_parcels(char_id: u32) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM `Parcels` WHERE `ParChaIdDestination` = ?")
        .bind(char_id)
        .fetch_one(database::get_pool())
        .await
        .unwrap_or(0)
        > 0
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_requestmp(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() {
        return 0;
    }

    (*sd).flags = 0;

    let char_name = (*sd).player.identity.name.clone();
    let char_id = (*sd).player.identity.id;

    // EXEMPT from async conversion: this function is called from sync contexts
    // (timer callback pc_timer, Lua sl_pc_sendstatus, and the login sequence
    // intif_mmo_tosd). The flags must be set before clif_sendstatus writes them
    // into the login packet, so fire-and-forget is not safe here. Converting to
    // native async would require cascading intif_mmo_tosd → async, which is a
    // large refactor deferred to a later task.
    if database::blocking_run_async(check_new_mail(char_name)) {
        (*sd).flags |= FLAG_MAIL;
    }
    if database::blocking_run_async(check_pending_parcels(char_id)) {
        (*sd).flags |= FLAG_PARCEL;
    }

    0
}

/// Iterates from current level to 99, checks if
/// the player's XP meets the threshold, and fires the "onLevel" script for each
/// level they qualify for.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_checklevel(sd: *mut MapSessionData) -> i32 {
    let path_raw = (*sd).player.progression.class as i32;
    let path = if path_raw > 5 {
        classdb_path(path_raw)
    } else {
        path_raw
    };

    for x in (*sd).player.progression.level as i32..99 {
        let lvlxp = classdb_level(path, x);
        if (*sd).player.progression.exp >= lvlxp {
            sl_doscript_simple_pc("onLevel", None, (*sd).id);
        }
    }

    0
}

/// Like `pc_checklevel` but takes a `PlayerEntity` reference, properly
/// scoping lock guards so they are never held across Lua calls.
pub unsafe fn pc_checklevel_pe(pe: &PlayerEntity) -> i32 {
    let id = pe.id;
    let (path, level) = {
        let sd = pe.read();
        let path_raw = sd.player.progression.class as i32;
        (
            if path_raw > 5 {
                classdb_path(path_raw)
            } else {
                path_raw
            },
            sd.player.progression.level as i32,
        )
    };

    for x in level..99 {
        let lvlxp = classdb_level(path, x);
        // Re-read exp each iteration — Lua onLevel may award more XP.
        if pe.read().player.progression.exp >= lvlxp {
            sl_doscript_simple_pc("onLevel", None, id);
        }
    }

    0
}

/// Awards XP to
/// the player, checking stack-on-player and AFK conditions first, then calls
/// `pc_checklevel` and sends status updates.
///
/// Note: the `if (exp < 0)` branch in C is dead code because `exp` is `unsigned int`
/// and can never be negative; it is preserved here for faithful translation.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_givexp(pe: &PlayerEntity, exp: u32, xprate: u32) -> i32 {
    let mut xpstring = [0i8; 256];
    let mut stack: i32 = 0;

    // stack check — count non-GM PCs at the exact same tile
    let (sx, sy, m) = {
        let r = pe.read();
        (r.x, r.y, r.m)
    };
    if let Some(grid) = block_grid::get_grid(m as usize) {
        for id in grid.ids_at_tile(sx, sy) {
            if stack >= 32768 {
                break;
            }
            if let Some(tsd_arc) = map_server::map_id2sd_pc(id) {
                let pos = tsd_arc.position();
                if pos.x == sx && pos.y == sy && tsd_arc.read().player.identity.gm_level == 0 {
                    stack += 1;
                }
            }
        }
    }

    if stack > 1 {
        let msg = b"You cannot gain experience while on top of other players.\0";
        libc::snprintf(
            xpstring.as_mut_ptr(),
            xpstring.len(),
            msg.as_ptr() as *const i8,
        );
        clif_sendminitext(pe, xpstring.as_ptr());
        return 0;
    }

    // AFK check
    if pe.read().afk == 1 {
        let msg = b"You cannot gain experience while AFK.\0";
        libc::snprintf(
            xpstring.as_mut_ptr(),
            xpstring.len(),
            msg.as_ptr() as *const i8,
        );
        clif_sendminitext(pe, xpstring.as_ptr());
        return 0;
    }

    if exp == 0 {
        return 0;
    }

    // cast to i64 makes this unreachable; preserved as dead code matching C original where exp is unsigned int
    if (exp as i64) < 0 {
        let cur_exp = pe.read().player.progression.exp;
        if (cur_exp as i64) < (exp as i64).abs() {
            pe.write().player.progression.exp = 0;
            pe.set_exp(0);
        } else {
            let new_exp = cur_exp.wrapping_add(exp);
            pe.write().player.progression.exp = new_exp;
            pe.set_exp(new_exp);
        }
        return 0;
    }

    let cur_exp = pe.read().player.progression.exp;
    let totalxp: i64 = (exp as i64).wrapping_mul(xprate as i64);
    let difxp: u32 = 4294967295u32.wrapping_sub(cur_exp);

    let (tempxp, defaultxp): (u32, u32) = if (difxp as i64) > totalxp {
        (cur_exp.wrapping_add(totalxp as u32), totalxp as u32)
    } else {
        (cur_exp.wrapping_add(difxp), difxp)
    };

    pe.write().player.progression.exp = tempxp;
    pe.set_exp(tempxp);

    libc::snprintf(
        xpstring.as_mut_ptr(),
        xpstring.len(),
        c"%u experience!".as_ptr(),
        defaultxp,
    );

    pc_checklevel_pe(pe);
    clif_sendminitext(pe, xpstring.as_ptr());
    clif_sendstatus(pe, SFLAG_XPMONEY);
    clif_sendupdatestatus_onequip(pe);

    0
}

/// Recalculates all derived stats from base stats and
/// equipped items, applies active magic aether/passive skills, computes TNL percentage,
/// clamps all stats, then sends a full status update to the client.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_calcstat(pe: &PlayerEntity) -> i32 {
    {
        let mut sd = pe.write();

        // Reset combat modifiers
        sd.dam = 0;
        sd.hit = 0;
        sd.miss = 0;
        sd.crit = 0;
        sd.critmult = 0.0f32;
        sd.deduction = 1.0f32;
        sd.snare = 0;
        sd.sleep = 1.0f32;
        sd.silence = 0;
        sd.paralyzed = 0;
        sd.blind = 0;
        sd.drunk = 0;

        if sd.rage == 0.0f32 {
            sd.rage = 1.0f32;
        }
        if sd.enchanted == 0.0f32 {
            sd.enchanted = 1.0f32;
        }

        // C: `if (sd->status.basehp <= 0)` — unsigned int, so equivalent to == 0.
        if sd.player.combat.max_hp == 0 {
            sd.player.combat.max_hp = 5;
        }
        if sd.player.combat.max_mp == 0 {
            sd.player.combat.max_mp = 5;
        }

        // Copy base stats
        sd.armor = sd.player.combat.base_armor;
        sd.max_hp = sd.player.combat.max_hp;
        sd.max_mp = sd.player.combat.max_mp;
        sd.might = sd.player.combat.base_might as i32;
        sd.grace = sd.player.combat.base_grace as i32;
        sd.will = sd.player.combat.base_will as i32;

        sd.maxSdam = 0;
        sd.minSdam = 0;
        sd.minLdam = 0;
        sd.maxLdam = 0;

        sd.attack_speed = 20;
        sd.protection = 0;
        sd.healing = 0;
        sd.player.progression.tnl = 0;
        sd.player.progression.real_tnl = 0;

        // Accumulate stats from equipped items
        for x in 0..14usize {
            let id = sd.player.inventory.equip[x].id;
            if id > 0 {
                let db = item_db::search(id);
                sd.max_hp = sd.max_hp.wrapping_add(db.vita as u32);
                sd.max_mp = sd.max_mp.wrapping_add(db.mana as u32);
                sd.might += db.might;
                sd.grace += db.grace;
                sd.will += db.will;
                sd.armor += db.ac;
                sd.healing += db.healing;
                sd.dam += db.dam;
                sd.hit += db.hit;
                sd.minSdam += db.min_sdam as i32; // u32 field, i32 accumulator
                sd.maxSdam += db.max_sdam as i32;
                sd.minLdam += db.min_ldam as i32;
                sd.maxLdam += db.max_ldam as i32;
                sd.protection = (sd.protection as i32 + db.protection) as i16;
            }
        }

        // Mount state
        if sd.player.combat.state == PC_MOUNTED as i8 {
            if sd.player.identity.gm_level == 0 && sd.speed < 40 {
                sd.speed = 40;
            }
        } else {
            sd.speed = 90;
        }
    } // drop write guard before Lua calls

    // Mount state Lua hook
    if pe.read().player.combat.state == PC_MOUNTED as i8 {
        sl_doscript_simple_pc("remount", None, pe.id);
    }

    // Fire recast and passive scripts (only when alive)
    let state = pe.read().player.combat.state;
    if state != PC_DIE as i8 {
        // Recast active magic aether slots
        let dura_ids: Vec<(u16, i32, u32)> = pe
            .read()
            .player
            .spells
            .dura_aether
            .iter()
            .filter(|p| p.id > 0 && p.duration > 0)
            .map(|p| (p.id, p.id as i32, p.caster_id))
            .collect();
        for (_, mid, caster_id) in &dura_ids {
            if let Some(caster_pe) = map_server::map_id2sd_pc(*caster_id) {
                sl_doscript_2_pc(
                    scripting::carray_to_str(&magic_db::search(*mid).yname),
                    Some("recast"),
                    pe.id,
                    caster_pe.id,
                );
            } else {
                sl_doscript_simple_pc(
                    scripting::carray_to_str(&magic_db::search(*mid).yname),
                    Some("recast"),
                    pe.id,
                );
            }
        }

        // Passive skills
        let skills: Vec<u16> = pe.read().player.spells.skills.to_vec();
        for skill in skills {
            if skill > 0 {
                sl_doscript_simple_pc(
                    scripting::carray_to_str(&magic_db::search(skill as i32).yname),
                    Some("passive"),
                    pe.id,
                );
            }
        }

        // Re-equip scripts
        let equip_ids: Vec<u32> = pe
            .read()
            .player
            .inventory
            .equip
            .iter()
            .map(|e| e.id)
            .collect();
        for id in equip_ids {
            if id > 0 {
                sl_doscript_simple_pc(
                    scripting::carray_to_str(&item_db::search(id).yname),
                    Some("re_equip"),
                    pe.id,
                );
            }
        }
    }

    {
        let mut sd = pe.write();

        // Compute TNL percentage for group status window (added 8-5-16)
        if sd.player.progression.tnl == 0 {
            let path_raw = sd.player.progression.class as i32;
            let path = if path_raw > 5 {
                classdb_path(path_raw)
            } else {
                path_raw
            };
            let level = sd.player.progression.level as i32;

            if level < 99 {
                let helper =
                    classdb_level(path, level).wrapping_sub(classdb_level(path, level - 1)) as i64;
                let tnl = classdb_level(path, level) as i64 - sd.player.progression.exp as i64;
                let mut percentage = (((helper - tnl) as f32) / (helper as f32)) * 100.0f32;
                // C bug preserved: tnl assigned before death-penalty correction; C never re-assigns it
                sd.player.progression.tnl = percentage as i32 as u32;
                if tnl > helper {
                    // XP went below previous level threshold (e.g. after a death penalty);
                    // recomputes percentage for internal use only — status.tnl is NOT updated here (matches C)
                    percentage =
                        (sd.player.progression.exp as f32 / helper as f32) * 100.0f32 + 0.5f32;
                }
                let _ = percentage; // suppress unused-variable warning; death-penalty path uses it in C for nothing further
            } else {
                sd.player.progression.tnl =
                    ((sd.player.progression.exp as f64 / 4294967295.0f64) * 100.0f64) as i32 as u32;
            }
        }

        // Compute real TNL for F1 menu (added 8-6-16)
        if sd.player.progression.real_tnl == 0 {
            let path_raw = sd.player.progression.class as i32;
            let path = if path_raw > 5 {
                classdb_path(path_raw)
            } else {
                path_raw
            };
            let level = sd.player.progression.level as i32;

            if level < 99 {
                let tnl = classdb_level(path, level) as i64 - sd.player.progression.exp as i64;
                sd.player.progression.real_tnl = tnl as i32 as u32;
            } else {
                sd.player.progression.real_tnl = 0;
            }
        }

        // Clamp stat values
        if sd.might > 255 {
            sd.might = 255;
        }
        if sd.grace > 255 {
            sd.grace = 255;
        }
        if sd.will > 255 {
            sd.will = 255;
        }
        if sd.might < 0 {
            sd.might = 0;
        }
        if sd.grace < 0 {
            sd.grace = 0;
        }
        if sd.will < 0 {
            sd.will = 0;
        }

        sd.dam = sd.dam.clamp(0, 255);
        sd.armor = sd.armor.clamp(-127, 127);
        if sd.dam < 0 {
            sd.dam = 0;
        } // duplicate clamp, preserved faithfully
        if sd.attack_speed < 3 {
            sd.attack_speed = 3;
        }

        // Global map health/magic overrides
        let max_health = map_readglobalreg(sd.m as i32, c"maxHealth".as_ptr());
        let max_magic = map_readglobalreg(sd.m as i32, c"maxMagic".as_ptr());
        if max_health > 0 {
            sd.max_hp = max_health as u32;
        }
        if max_magic > 0 {
            sd.max_mp = max_magic as u32;
        }

        // Clamp current HP/MP
        let (max_hp, max_mp) = (sd.max_hp, sd.max_mp);
        if sd.player.combat.hp > max_hp {
            sd.player.combat.hp = max_hp;
        }
        if sd.player.combat.mp > max_mp {
            sd.player.combat.mp = max_mp;
        }
    } // drop write guard

    clif_sendstatus(pe, SFLAG_FULLSTATS | SFLAG_HPMP | SFLAG_XPMONEY);

    0
}

/// Calculates the physical damage the player
/// can deal: base damage from might plus a random roll from equipped weapon range.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_calcdamage(sd: *mut MapSessionData) -> f32 {
    let mut damage: f32 = 6.0f32 + ((*sd).might as f32) / 8.0f32;

    if (*sd).minSdam > 0 && (*sd).maxSdam > 0 {
        let mut ran = (*sd).maxSdam - (*sd).minSdam;
        if ran <= 0 {
            ran = 1;
        }
        ran = ((rand::random::<u32>() & 0x00FF_FFFF) % (ran as u32)) as i32 + (*sd).minSdam;
        damage += (ran as f32) / 2.0f32;
    }

    damage
}

// ─── Registry functions ───────────────────────────────────────────────────────
//
// These functions manage player variable storage (local and global registries).
// Local registries (reg/regstr) are heap-allocated growable arrays on MapSessionData.
// Global registries (global_reg, global_regstring, acctreg, npcintreg, questreg)
// are fixed-size arrays in MmoCharStatus, found by scanning for matching key strings.
//
// All string comparisons use `libc::strcasecmp` (case-insensitive), matching C.
// String copies into fixed [i8; N] arrays use `libc::strcpy` (safe within bounds).

// ── Local integer registry (per-script, heap-allocated) ──────────────────────

/// Reads a local integer variable by index.
///
/// Scans `sd->reg[0..reg_num]` for a slot with `index == reg`.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_readreg(sd: *mut MapSessionData, reg: i32) -> i32 {
    if sd.is_null() {
        return 0;
    }
    let sd = &*sd;
    let reg_arr = std::slice::from_raw_parts(sd.reg, sd.reg_num as usize);
    for r in reg_arr {
        if r.index == reg {
            return r.data;
        }
    }
    0
}

/// Sets a local integer variable by index.
///
/// Scans for an existing slot; if found, updates `data`. If not found, grows the
/// `reg` array, zeroes the new slot, then sets index and data.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_setreg(sd: *mut MapSessionData, reg: i32, val: i32) -> i32 {
    if sd.is_null() {
        return 0;
    }
    // Search for existing slot
    for i in 0..(*sd).reg_num as usize {
        if (*(*sd).reg.add(i)).index == reg {
            (*(*sd).reg.add(i)).data = val;
            return 0;
        }
    }
    // Not found — grow array
    let new_num = (*sd).reg_num + 1;
    let new_ptr = libc::realloc(
        (*sd).reg as *mut libc::c_void,
        new_num as usize * mem::size_of::<ScriptReg>(),
    ) as *mut ScriptReg;
    if new_ptr.is_null() {
        return 0;
    }
    (*sd).reg = new_ptr;
    let slot = (*sd).reg_num as usize;
    (*sd).reg_num = new_num;
    std::ptr::write_bytes((*sd).reg.add(slot), 0, 1);
    (*(*sd).reg.add(slot)).index = reg;
    (*(*sd).reg.add(slot)).data = val;
    0
}

// ── Local string registry (per-script, heap-allocated) ───────────────────────

/// Reads a local string variable by index.
/// Returns pointer to the stored C string, or NULL if not found.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_readregstr(sd: *mut MapSessionData, reg: i32) -> *mut i8 {
    if sd.is_null() {
        return std::ptr::null_mut();
    }
    for i in 0..(*sd).regstr_num as usize {
        if (*(*sd).regstr.add(i)).index == reg {
            return (*(*sd).regstr.add(i)).data.as_mut_ptr();
        }
    }
    std::ptr::null_mut()
}

/// Sets a local string variable by index.
///
/// Checks length, updates existing slot or grows the `regstr` array.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_setregstr(sd: *mut MapSessionData, reg: i32, str_: *mut i8) -> i32 {
    if sd.is_null() {
        return 0;
    }
    // Check string length — must fit in data[256] (including null terminator)
    let len = libc::strlen(str_ as *const i8);
    if len + 1 >= mem::size_of::<[i8; 256]>() {
        libc::printf(c"pc_setregstr: string too long !\n".as_ptr());
        return 0;
    }
    // Search for existing slot
    for i in 0..(*sd).regstr_num as usize {
        if (*(*sd).regstr.add(i)).index == reg {
            libc::strcpy((*(*sd).regstr.add(i)).data.as_mut_ptr(), str_ as *const i8);
            return 0;
        }
    }
    // Not found — grow array
    let new_num = (*sd).regstr_num + 1;
    let new_ptr = libc::realloc(
        (*sd).regstr as *mut libc::c_void,
        new_num as usize * mem::size_of::<ScriptRegStr>(),
    ) as *mut ScriptRegStr;
    if new_ptr.is_null() {
        return 0;
    }
    (*sd).regstr = new_ptr;
    let slot = (*sd).regstr_num as usize;
    (*sd).regstr_num = new_num;
    std::ptr::write_bytes((*sd).regstr.add(slot), 0, 1);
    (*(*sd).regstr.add(slot)).index = reg;
    libc::strcpy(
        (*(*sd).regstr.add(slot)).data.as_mut_ptr(),
        str_ as *const i8,
    );
    0
}

// ── Global string registry (persisted in PlayerRegistries) ───────────────────

/// Reads a global string variable from the player's registry HashMap.
/// Returns pointer to stored value, or pointer to static empty string.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_readglobalregstring(sd: *mut MapSessionData, reg: *const i8) -> *mut i8 {
    if sd.is_null() || reg.is_null() {
        return c"".as_ptr() as *mut i8;
    }
    let sd = &mut *sd;
    let key_str = std::ffi::CStr::from_ptr(reg).to_str().unwrap_or("");
    match sd.player.registries.get_reg_str(key_str) {
        Some(v) => v.as_ptr() as *mut i8,
        None => c"".as_ptr() as *mut i8,
    }
}

/// Sets a global string variable.
///
/// Inserts or updates the key in the player's global string registry HashMap.
/// Setting to `""` removes the key.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_setglobalregstring(
    sd: *mut MapSessionData,
    reg: *const i8,
    val: *const i8,
) -> i32 {
    if sd.is_null() || reg.is_null() {
        return 0;
    }
    let sd = &mut *sd;
    let key_str = std::ffi::CStr::from_ptr(reg).to_str().unwrap_or("");
    let val_str = if val.is_null() {
        ""
    } else {
        std::ffi::CStr::from_ptr(val).to_str().unwrap_or("")
    };
    if val_str.is_empty() {
        sd.player.registries.global_regstring.remove(key_str);
    } else {
        sd.player.registries.set_reg_str(key_str, val_str);
    }
    0
}

// ── Global integer registry (persisted in PlayerRegistries) ──────────────────

/// Reads a global integer variable.
///
/// Looks up `reg` in the player's global integer registry HashMap.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_readglobalreg(sd: *mut MapSessionData, reg: *const i8) -> i32 {
    if sd.is_null() || reg.is_null() {
        return 0;
    }
    let sd = &*sd;
    let key_str = std::ffi::CStr::from_ptr(reg).to_str().unwrap_or("");
    sd.player.registries.get_reg(key_str).unwrap_or(0)
}

/// Sets a global integer variable.
///
/// Inserts or updates the key in the player's global integer registry HashMap.
/// Setting val to 0 removes the key.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_setglobalreg(sd: *mut MapSessionData, reg: *const i8, val: u64) -> i32 {
    if sd.is_null() || reg.is_null() {
        return 0;
    }
    let sd = &mut *sd;
    let key_str = std::ffi::CStr::from_ptr(reg).to_str().unwrap_or("");
    if val == 0 {
        sd.player.registries.global_reg.remove(key_str);
    } else {
        sd.player.registries.set_reg(key_str, val as i32);
    }
    0
}

// ── Parameter read/write (HP/MP/max) ─────────────────────────────────────────

/// Reads a player parameter by SP_* constant.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_readparam(sd: *mut MapSessionData, type_: i32) -> i32 {
    if sd.is_null() {
        return 0;
    }
    let sd = &*sd;
    match type_ {
        SP_HP => sd.player.combat.hp as i32,
        SP_MP => sd.player.combat.mp as i32,
        SP_MHP => sd.max_hp as i32,
        SP_MMP => sd.max_mp as i32,
        _ => 0,
    }
}

/// Sets a player parameter by SP_* constant.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_setparam(pe: &PlayerEntity, type_: i32, val: i32) -> i32 {
    {
        let mut sd = pe.write();
        match type_ {
            SP_HP => sd.player.combat.hp = val as u32,
            SP_MP => sd.player.combat.mp = val as u32,
            SP_MHP => sd.max_hp = val as u32,
            SP_MMP => sd.max_mp = val as u32,
            _ => {}
        }
    } // drop write guard
    clif_sendupdatestatus(pe);
    0
}

// ── Account registry (persisted in PlayerRegistries) ─────────────────────────

/// Reads an account-scoped integer variable.
///
/// Looks up `reg` in the player's account registry HashMap.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_readacctreg(sd: *mut MapSessionData, reg: *const i8) -> i32 {
    if sd.is_null() || reg.is_null() {
        return 0;
    }
    let sd = &*sd;
    let key_str = std::ffi::CStr::from_ptr(reg).to_str().unwrap_or("");
    sd.player.registries.get_acct_reg(key_str).unwrap_or(0)
}

/// Sets an account-scoped integer variable.
///
/// Inserts or updates the key in the player's account registry HashMap.
/// Setting val to 0 removes the key.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_setacctreg(sd: *mut MapSessionData, reg: *const i8, val: i32) -> i32 {
    if sd.is_null() || reg.is_null() {
        return 0;
    }
    let sd = &mut *sd;
    let key_str = std::ffi::CStr::from_ptr(reg).to_str().unwrap_or("");
    if val == 0 {
        sd.player.registries.acct_reg.remove(key_str);
    } else {
        sd.player.registries.set_acct_reg(key_str, val);
    }
    0
}

// ── NPC integer registry (persisted in PlayerRegistries) ─────────────────────

/// Reads an NPC-scoped integer variable.
///
/// Looks up `reg` in the player's NPC integer registry HashMap.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_readnpcintreg(sd: *mut MapSessionData, reg: *const i8) -> i32 {
    if sd.is_null() || reg.is_null() {
        return 0;
    }
    let sd = &*sd;
    let key_str = std::ffi::CStr::from_ptr(reg).to_str().unwrap_or("");
    sd.player.registries.get_npc_reg(key_str).unwrap_or(0)
}

/// Sets an NPC-scoped integer variable.
///
/// Inserts or updates the key in the player's NPC integer registry HashMap.
/// Setting val to 0 removes the key.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_setnpcintreg(sd: *mut MapSessionData, reg: *const i8, val: i32) -> i32 {
    if sd.is_null() || reg.is_null() {
        return 0;
    }
    let sd = &mut *sd;
    let key_str = std::ffi::CStr::from_ptr(reg).to_str().unwrap_or("");
    if val == 0 {
        sd.player.registries.npc_int_reg.remove(key_str);
    } else {
        sd.player.registries.set_npc_reg(key_str, val);
    }
    0
}

// ── Quest registry (persisted in PlayerRegistries) ───────────────────────────

/// Reads a quest integer variable.
///
/// Looks up `reg` in the player's quest registry HashMap.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_readquestreg(sd: *mut MapSessionData, reg: *const i8) -> i32 {
    if sd.is_null() || reg.is_null() {
        return 0;
    }
    let sd = &*sd;
    let key_str = std::ffi::CStr::from_ptr(reg).to_str().unwrap_or("");
    sd.player.registries.get_quest_reg(key_str).unwrap_or(0)
}

/// Sets a quest integer variable.
///
/// Inserts or updates the key in the player's quest registry HashMap.
/// Setting val to 0 removes the key.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_setquestreg(sd: *mut MapSessionData, reg: *const i8, val: i32) -> i32 {
    if sd.is_null() || reg.is_null() {
        return 0;
    }
    let sd = &mut *sd;
    let key_str = std::ffi::CStr::from_ptr(reg).to_str().unwrap_or("");
    if val == 0 {
        sd.player.registries.quest_reg.remove(key_str);
    } else {
        sd.player.registries.set_quest_reg(key_str, val);
    }
    0
}

// ─── Item management functions ────────────────────────────────────────────────

// ─── pc_isinvenspace ─────────────────────────────────────────────────────────

/// Cosmetic customization fields used for inventory-slot matching.
#[derive(Clone, Copy)]
pub struct ItemCustomization {
    pub engrave: *const i8,
    pub custom_look: u32,
    pub custom_look_color: u32,
    pub custom_icon: u32,
    pub custom_icon_color: u32,
}

/// Returns the first inventory slot that can accept an item with the given
/// attributes, or `max_inv` when no slot is available.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_isinvenspace(
    sd: *mut MapSessionData,
    id: i32,
    owner: i32,
    look: ItemCustomization,
) -> i32 {
    let ItemCustomization {
        engrave,
        custom_look,
        custom_look_color,
        custom_icon,
        custom_icon_color,
    } = look;
    if sd.is_null() {
        return 0;
    }
    let sd = &mut *sd;
    let maxinv = sd.player.inventory.max_inv as usize;
    let id_u = id as u32;
    let own_u = owner as u32;

    if item_db::search(id_u).max_amount > 0 {
        // Count how many of this item the player already owns (inventory + equip).
        let mut maxamount: i32 = 0;
        for i in 0..maxinv {
            let inv = &sd.player.inventory.inventory[i];
            if inv.id == id_u
                && item_db::search(id_u).max_amount > 0
                && inv.owner == own_u
                && libc::strcasecmp(inv.real_name.as_ptr(), engrave) == 0
                && inv.custom_look == custom_look
                && inv.custom_look_color == custom_look_color
                && inv.custom_icon == custom_icon
                && inv.custom_icon_color == custom_icon_color
            {
                maxamount += inv.amount;
            }
        }
        for i in 0..14usize {
            let eq = &sd.player.inventory.equip[i];
            if eq.id == id_u
                && item_db::search(id_u).max_amount > 0
                && sd.player.inventory.inventory[i].owner == own_u
                && libc::strcasecmp(sd.player.inventory.inventory[i].real_name.as_ptr(), engrave)
                    == 0
                && sd.player.inventory.inventory[i].custom_look == custom_look
                && sd.player.inventory.inventory[i].custom_look_color == custom_look_color
                && sd.player.inventory.inventory[i].custom_icon == custom_icon
                && sd.player.inventory.inventory[i].custom_icon_color == custom_icon_color
            {
                maxamount += 1;
            }
        }

        // Find a slot that already has the item but isn't full.
        for i in 0..maxinv {
            let inv = &sd.player.inventory.inventory[i];
            if inv.id == id_u
                && inv.amount < item_db::search(id_u).stack_amount
                && maxamount < item_db::search(id_u).max_amount
                && inv.owner == own_u
                && libc::strcasecmp(inv.real_name.as_ptr(), engrave) == 0
                && inv.custom_look == custom_look
                && inv.custom_look_color == custom_look_color
                && inv.custom_icon == custom_icon
                && inv.custom_icon_color == custom_icon_color
            {
                return i as i32;
            }
        }

        // Find an empty slot under the global cap.
        for i in 0..maxinv {
            if sd.player.inventory.inventory[i].id == 0
                && maxamount < item_db::search(id_u).max_amount
            {
                return i as i32;
            }
        }

        sd.player.inventory.max_inv as i32
    } else {
        // No per-player max — just stack or find empty.
        for i in 0..maxinv {
            let inv = &sd.player.inventory.inventory[i];
            if inv.id == id_u
                && inv.amount < item_db::search(id_u).stack_amount
                && inv.owner == own_u
                && libc::strcasecmp(inv.real_name.as_ptr(), engrave) == 0
                && inv.custom_look == custom_look
                && inv.custom_look_color == custom_look_color
                && inv.custom_icon == custom_icon
                && inv.custom_icon_color == custom_icon_color
            {
                return i as i32;
            }
        }
        for i in 0..maxinv {
            if sd.player.inventory.inventory[i].id == 0 {
                return i as i32;
            }
        }
        sd.player.inventory.max_inv as i32
    }
}

// ─── pc_isinvenitemspace ──────────────────────────────────────────────────────

/// Returns the number of additional units of `id` that can be placed in
/// inventory slot `num`. Returns 0 when the slot is incompatible.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_isinvenitemspace(
    sd: *mut MapSessionData,
    num: i32,
    id: i32,
    owner: i32,
    engrave: *mut i8,
) -> i32 {
    if sd.is_null() {
        return 0;
    }
    let sd = &mut *sd;
    let id_u = id as u32;
    let own_u = owner as u32;
    let num = num as usize;

    if item_db::search(id_u).max_amount > 0 {
        let mut maxamount: i32 = 0;
        let maxinv = sd.player.inventory.max_inv as usize;
        for i in 0..maxinv {
            if sd.player.inventory.inventory[i].id == id_u && item_db::search(id_u).max_amount > 0 {
                maxamount += sd.player.inventory.inventory[i].amount;
            }
        }
        for i in 0..14usize {
            if sd.player.inventory.equip[i].id == id_u && item_db::search(id_u).max_amount > 0 {
                // C checks takeoffid: skip the slot being unequipped
                if sd.takeoffid == -1 || sd.player.inventory.equip[sd.takeoffid as usize].id != id_u
                {
                    maxamount += 1;
                }
            }
        }

        if sd.player.inventory.inventory[num].id == 0
            && item_db::search(id_u).max_amount - maxamount >= item_db::search(id_u).stack_amount
        {
            item_db::search(id_u).stack_amount
        } else if sd.player.inventory.inventory[num].id != id_u
            || sd.player.inventory.inventory[num].owner != own_u
            || libc::strcasecmp(
                sd.player.inventory.inventory[num].real_name.as_ptr(),
                engrave,
            ) != 0
        {
            0
        } else {
            item_db::search(id_u).max_amount - maxamount
        }
    } else {
        if sd.player.inventory.inventory[num].id == 0 {
            item_db::search(id_u).stack_amount
        } else if sd.player.inventory.inventory[num].id != id_u
            || sd.player.inventory.inventory[num].owner != own_u
            || libc::strcasecmp(
                sd.player.inventory.inventory[num].real_name.as_ptr(),
                engrave,
            ) != 0
        {
            0
        } else {
            item_db::search(id_u).stack_amount - sd.player.inventory.inventory[num].amount
        }
    }
}

// ─── pc_dropitemfull (helper) ─────────────────────────────────────────────────

/// Allocate a `FloorItemData` from `fl2`, attempt to stack it on an existing
/// floor item at the player's cell, and if no match exists add it to the map.
unsafe fn pc_dropitemfull_inner(sd: *mut MapSessionData, fl2: *const Item) -> i32 {
    let mut fl = Box::new(mem::zeroed::<FloorItemData>());

    fl.m = (*sd).m;
    fl.x = (*sd).x;
    fl.y = (*sd).y;
    // Copy the item into fl->data (BoundItem and Item share the same layout)
    libc::memcpy(
        &mut fl.data as *mut _ as *mut libc::c_void,
        fl2 as *const libc::c_void,
        mem::size_of::<Item>(),
    );
    // looters is already zeroed by mem::zeroed()

    let mut def = [0i32; 2];

    // Only attempt stacking if item is at full durability.
    if fl.data.dura == item_db::search(fl.data.id as u32).dura {
        if let Some(grid) = block_grid::get_grid(fl.m as usize) {
            let cell_ids = grid.ids_at_tile(fl.x, fl.y);
            for id in cell_ids {
                if let Some(fl_arc) = map_server::map_id2fl_ref(id) {
                    let mut fl_existing = fl_arc.write();
                    pc_addtocurrent2_inner(
                        &mut *fl_existing as *mut FloorItemData,
                        def.as_mut_ptr(),
                        fl.as_mut() as *mut FloorItemData,
                    );
                }
            }
        }
    }

    if def[0] == 0 {
        let fl_raw = Box::into_raw(fl);
        map_additem(fl_raw);
        if let Some(grid) = block_grid::get_grid((*sd).m as usize) {
            let slot = &*map_db::get_map_ptr((*sd).m);
            let ids = block_grid::ids_in_area(
                grid,
                (*sd).x as i32,
                (*sd).y as i32,
                AreaType::Area,
                slot.xs as i32,
                slot.ys as i32,
            );
            for id in ids {
                if let Some(tsd_arc) = map_server::map_id2sd_pc(id) {
                    clif_object_look2_item(tsd_arc.fd, tsd_arc.read().player.identity.id, &*fl_raw);
                }
            }
        }
    } else {
        drop(fl);
    }
    0
}

/// Public C-callable export.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_dropitemfull(sd: *mut MapSessionData, fl2: *mut Item) -> i32 {
    if sd.is_null() || fl2.is_null() {
        return 0;
    }
    pc_dropitemfull_inner(sd, fl2)
}

/// Typed inner callback: attempt to stack `fl2` onto the existing floor item `bl`.
/// Sets `def[0] = 1` on a successful merge.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_addtocurrent2_inner(
    fl: *mut FloorItemData,
    def: *mut i32,
    fl2: *mut FloorItemData,
) -> i32 {
    if fl.is_null() {
        return 0;
    }

    if def.is_null() || fl2.is_null() {
        return 0;
    }
    if *def != 0 {
        return 0;
    }

    // Items stack when all identity fields match exactly.
    if (*fl).data.id == (*fl2).data.id
        && (*fl).data.owner == (*fl2).data.owner
        && libc::strcasecmp(
            (*fl).data.real_name.as_ptr(),
            (*fl2).data.real_name.as_ptr(),
        ) == 0
        && (*fl).data.custom_icon == (*fl2).data.custom_icon
        && (*fl).data.custom_icon_color == (*fl2).data.custom_icon_color
        && (*fl).data.custom_look == (*fl2).data.custom_look
        && (*fl).data.custom_look_color == (*fl2).data.custom_look_color
        && libc::strcmp((*fl).data.note.as_ptr(), (*fl2).data.note.as_ptr()) == 0
        && (*fl).data.custom == (*fl2).data.custom
        && (*fl).data.protected == (*fl2).data.protected
    {
        (*fl).data.amount += (*fl2).data.amount;
        *def = 1;
    }
    0
}

/// Typed inner callback: stack inventory slot `id` amount onto existing floor item `fl`.
/// Sets `def[0] = fl->bl.id` on successful merge.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_addtocurrent_inner(
    fl: *mut FloorItemData,
    def: *mut i32,
    id: i32,
    type_: i32,
    sd: *mut MapSessionData,
) -> i32 {
    if fl.is_null() {
        return 0;
    }
    let id = id as usize; // inventory slot index

    if def.is_null() || sd.is_null() {
        return 0;
    }
    if *def != 0 {
        return 0;
    }

    // Only stack items at full durability.
    if (*fl).data.dura < item_db::search((*fl).data.id).dura {
        return 0;
    }
    libc::memset(
        (*fl).looters.as_mut_ptr() as *mut libc::c_void,
        0,
        mem::size_of::<u32>() * MAX_GROUP_MEMBERS,
    );

    let inv = &(&(*sd).player.inventory.inventory)[id];
    if (*fl).data.id == inv.id
        && (*fl).data.owner == inv.owner
        && libc::strcasecmp((*fl).data.real_name.as_ptr(), inv.real_name.as_ptr()) == 0
        && (*fl).data.custom_icon == inv.custom_icon
        && (*fl).data.custom_icon_color == inv.custom_icon_color
        && (*fl).data.custom_look == inv.custom_look
        && (*fl).data.custom_look_color == inv.custom_look_color
        && libc::strcmp((*fl).data.note.as_ptr(), inv.note.as_ptr()) == 0
        && (*fl).data.custom == inv.custom
        && (*fl).data.protected == inv.protected
    {
        (*fl).lastamount = (*fl).data.amount as u32;
        if type_ != 0 {
            (*fl).data.amount += inv.amount;
        } else {
            (*fl).data.amount += 1;
        }
        sl_doscript_2_pc("characterLog", Some("dropWrite"), (*sd).id, (*fl).id);
        *def = (*fl).id as i32;
    }
    0
}

// ─── pc_additem ───────────────────────────────────────────────────────────────

/// Add item to inventory with logging.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_additem(pe: &PlayerEntity, fl: *mut Item) -> i32 {
    if fl.is_null() {
        return 0;
    }

    // Gold dupe guard: id==0 with amount is bogus.
    if (*fl).id == 0 && (*fl).amount != 0 {
        return 0;
    }

    let id_u = (*fl).id;
    let maxinv;
    let mut num;

    {
        let mut sd = pe.write();
        maxinv = sd.player.inventory.max_inv as i32;
        num = pc_isinvenspace(
            &mut *sd as *mut MapSessionData,
            id_u as i32,
            (*fl).owner as i32,
            ItemCustomization {
                engrave: (*fl).real_name.as_ptr(),
                custom_look: (*fl).custom_look,
                custom_look_color: (*fl).custom_look_color,
                custom_icon: (*fl).custom_icon,
                custom_icon_color: (*fl).custom_icon_color,
            },
        );
    }

    if num >= maxinv {
        if item_db::search(id_u).max_amount > 0 {
            let mut errbuf = [0i8; 64];
            libc::snprintf(
                errbuf.as_mut_ptr(),
                64,
                c"(%s). You can't have more than (%i).".as_ptr(),
                item_db::search(id_u).name.as_ptr(),
                item_db::search(id_u).max_amount,
            );
            {
                let mut sd = pe.write();
                pc_dropitemfull_inner(&mut *sd as *mut MapSessionData, fl);
            }
            clif_sendminitext(pe, errbuf.as_ptr());
        } else {
            clif_sendminitext(pe, map_msg()[MAP_ERRITMFULL].message.as_ptr());
            {
                let mut sd = pe.write();
                pc_dropitemfull_inner(&mut *sd as *mut MapSessionData, fl);
            }
        }
        return 0;
    }

    loop {
        {
            let mut sd = pe.write();
            let i = pc_isinvenitemspace(
                &mut *sd as *mut MapSessionData,
                num,
                id_u as i32,
                (*fl).owner as i32,
                (*fl).real_name.as_mut_ptr(),
            );

            if (*fl).amount > i {
                // Partial fill: put as much as fits.
                let inv = &mut sd.player.inventory.inventory[num as usize];
                inv.id = id_u;
                inv.dura = (*fl).dura;
                inv.protected = (*fl).protected;
                inv.owner = (*fl).owner;
                inv.time = (*fl).time;
                libc::strcpy(inv.real_name.as_mut_ptr(), (*fl).real_name.as_ptr());
                libc::strcpy(inv.note.as_mut_ptr(), (*fl).note.as_ptr());
                inv.custom_look = (*fl).custom_look;
                inv.custom_look_color = (*fl).custom_look_color;
                inv.custom_icon = (*fl).custom_icon;
                inv.custom_icon_color = (*fl).custom_icon_color;
                inv.custom = (*fl).custom;
                inv.amount += i;
                (*fl).amount -= i;
            } else {
                // Full fill: place the remaining amount.
                let inv = &mut sd.player.inventory.inventory[num as usize];
                inv.id = id_u;
                inv.dura = (*fl).dura;
                inv.protected = (*fl).protected;
                inv.owner = (*fl).owner;
                inv.time = (*fl).time;
                libc::strcpy(inv.real_name.as_mut_ptr(), (*fl).real_name.as_ptr());
                libc::strcpy(inv.note.as_mut_ptr(), (*fl).note.as_ptr());
                inv.custom_look = (*fl).custom_look;
                inv.custom_look_color = (*fl).custom_look_color;
                inv.custom_icon = (*fl).custom_icon;
                inv.custom_icon_color = (*fl).custom_icon_color;
                inv.custom = (*fl).custom;
                inv.amount += (*fl).amount;
                (*fl).amount = 0;
            }
        } // write guard dropped before clif call

        clif_sendadditem(pe, num);

        {
            let mut sd = pe.write();
            num = pc_isinvenspace(
                &mut *sd as *mut MapSessionData,
                id_u as i32,
                (*fl).owner as i32,
                ItemCustomization {
                    engrave: (*fl).real_name.as_ptr(),
                    custom_look: (*fl).custom_look,
                    custom_look_color: (*fl).custom_look_color,
                    custom_icon: (*fl).custom_icon,
                    custom_icon_color: (*fl).custom_icon_color,
                },
            );
        }

        if !((*fl).amount != 0 && num < maxinv) {
            break;
        }
    }

    if num >= maxinv && (*fl).amount != 0 {
        if item_db::search(id_u).max_amount > 0 {
            let mut errbuf = [0i8; 64];
            libc::snprintf(
                errbuf.as_mut_ptr(),
                64,
                c"(%s). You can't have more than (%i).".as_ptr(),
                item_db::search(id_u).name.as_ptr(),
                item_db::search(id_u).max_amount,
            );
            {
                let mut sd = pe.write();
                pc_dropitemfull_inner(&mut *sd as *mut MapSessionData, fl);
            }
            clif_sendminitext(pe, errbuf.as_ptr());
        } else {
            {
                let mut sd = pe.write();
                pc_dropitemfull_inner(&mut *sd as *mut MapSessionData, fl);
            }
            clif_sendminitext(pe, map_msg()[MAP_ERRITMFULL].message.as_ptr());
        }
    }
    0
}

// ─── pc_additemnolog ──────────────────────────────────────────────────────────

/// Add item without SQL logging.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_additemnolog(pe: &PlayerEntity, fl: *mut Item) -> i32 {
    if fl.is_null() {
        return 0;
    }

    if (*fl).id == 0 && (*fl).amount != 0 {
        return 0;
    }

    let id_u = (*fl).id;
    let maxinv;
    let mut num;

    {
        let mut sd = pe.write();
        maxinv = sd.player.inventory.max_inv as i32;
        num = pc_isinvenspace(
            &mut *sd as *mut MapSessionData,
            id_u as i32,
            (*fl).owner as i32,
            ItemCustomization {
                engrave: (*fl).real_name.as_ptr(),
                custom_look: (*fl).custom_look,
                custom_look_color: (*fl).custom_look_color,
                custom_icon: (*fl).custom_icon,
                custom_icon_color: (*fl).custom_icon_color,
            },
        );
    }

    if num >= maxinv {
        if item_db::search(id_u).max_amount > 0 {
            let mut errbuf = [0i8; 64];
            libc::snprintf(
                errbuf.as_mut_ptr(),
                64,
                c"(%s). You can't have more than (%i).".as_ptr(),
                item_db::search(id_u).name.as_ptr(),
                item_db::search(id_u).max_amount,
            );
            {
                let mut sd = pe.write();
                pc_dropitemfull_inner(&mut *sd as *mut MapSessionData, fl);
            }
            clif_sendminitext(pe, errbuf.as_ptr());
        } else {
            clif_sendminitext(pe, map_msg()[MAP_ERRITMFULL].message.as_ptr());
            {
                let mut sd = pe.write();
                pc_dropitemfull_inner(&mut *sd as *mut MapSessionData, fl);
            }
        }
        return 0;
    }

    loop {
        {
            let mut sd = pe.write();
            let i = pc_isinvenitemspace(
                &mut *sd as *mut MapSessionData,
                num,
                id_u as i32,
                (*fl).owner as i32,
                (*fl).real_name.as_mut_ptr(),
            );

            if (*fl).amount > i {
                let inv = &mut sd.player.inventory.inventory[num as usize];
                inv.id = id_u;
                inv.dura = (*fl).dura;
                inv.protected = (*fl).protected;
                inv.owner = (*fl).owner;
                inv.time = (*fl).time;
                libc::strcpy(inv.real_name.as_mut_ptr(), (*fl).real_name.as_ptr());
                inv.custom_look = (*fl).custom_look;
                inv.custom_look_color = (*fl).custom_look_color;
                inv.custom_icon = (*fl).custom_icon;
                inv.custom_icon_color = (*fl).custom_icon_color;
                inv.custom = (*fl).custom;
                inv.amount += i;
                (*fl).amount -= i;
            } else {
                let inv = &mut sd.player.inventory.inventory[num as usize];
                inv.id = id_u;
                inv.dura = (*fl).dura;
                inv.protected = (*fl).protected;
                inv.owner = (*fl).owner;
                inv.time = (*fl).time;
                libc::strcpy(inv.real_name.as_mut_ptr(), (*fl).real_name.as_ptr());
                inv.custom_look = (*fl).custom_look;
                inv.custom_look_color = (*fl).custom_look_color;
                inv.custom_icon = (*fl).custom_icon;
                inv.custom_icon_color = (*fl).custom_icon_color;
                inv.custom = (*fl).custom;
                inv.amount += (*fl).amount;
                (*fl).amount = 0;
            }
        } // write guard dropped before clif call

        clif_sendadditem(pe, num);

        {
            let mut sd = pe.write();
            num = pc_isinvenspace(
                &mut *sd as *mut MapSessionData,
                id_u as i32,
                (*fl).owner as i32,
                ItemCustomization {
                    engrave: (*fl).real_name.as_ptr(),
                    custom_look: (*fl).custom_look,
                    custom_look_color: (*fl).custom_look_color,
                    custom_icon: (*fl).custom_icon,
                    custom_icon_color: (*fl).custom_icon_color,
                },
            );
        }

        if !((*fl).amount != 0 && num < maxinv) {
            break;
        }
    }

    if num >= maxinv && (*fl).amount != 0 {
        if item_db::search(id_u).max_amount > 0 {
            let mut errbuf = [0i8; 64];
            libc::snprintf(
                errbuf.as_mut_ptr(),
                64,
                c"(%s). You can't have more than (%i).".as_ptr(),
                item_db::search(id_u).name.as_ptr(),
                item_db::search(id_u).max_amount,
            );
            {
                let mut sd = pe.write();
                pc_dropitemfull_inner(&mut *sd as *mut MapSessionData, fl);
            }
            clif_sendminitext(pe, errbuf.as_ptr());
        } else {
            {
                let mut sd = pe.write();
                pc_dropitemfull_inner(&mut *sd as *mut MapSessionData, fl);
            }
            clif_sendminitext(pe, map_msg()[MAP_ERRITMFULL].message.as_ptr());
        }
    }
    0
}

// ─── pc_delitem ───────────────────────────────────────────────────────────────

/// Remove `amount`
/// units from inventory slot `id`.  If the slot becomes empty it is zeroed and
/// the client is notified with a delete-item packet; otherwise the client
/// receives an updated add-item count and a mini-text with the item name.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_delitem(pe: &PlayerEntity, id: i32, amount: i32, type_: i32) -> i32 {
    // 0 = full delete, 1 = partial with text update
    let action: i32;
    let mut buf = [0i8; 255];

    {
        let mut sd = pe.write();
        let maxinv = sd.player.inventory.max_inv as i32;
        if id < 0 || id >= maxinv {
            return 0;
        }
        let inv = &mut sd.player.inventory.inventory[id as usize];
        if inv.id == 0 {
            return 0;
        }
        if amount <= 0 {
            return 0;
        }

        if amount >= inv.amount {
            inv.amount = 0;
            libc::memset(
                inv as *mut Item as *mut libc::c_void,
                0,
                mem::size_of::<Item>(),
            );
            action = 0;
        } else {
            inv.amount -= amount;
            if inv.amount <= 0 {
                libc::memset(
                    inv as *mut Item as *mut libc::c_void,
                    0,
                    mem::size_of::<Item>(),
                );
                action = 0;
            } else {
                let item_id = sd.player.inventory.inventory[id as usize].id;
                libc::snprintf(
                    buf.as_mut_ptr(),
                    255,
                    c"%s (%d)".as_ptr(),
                    item_db::search(item_id).name.as_ptr(),
                    amount,
                );
                action = 1;
            }
        }
    } // write guard dropped before pe-based calls

    if action == 0 {
        clif_senddelitem(pe, id, type_);
    } else {
        clif_sendminitext(pe, buf.as_ptr());
        clif_sendadditem(pe, id);
    }
    0
}

// ─── pc_dropitemmap ───────────────────────────────────────────────────────────

/// Drop one (or all) units
/// of inventory slot `id` onto the map floor.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_dropitemmap(pe: &PlayerEntity, id: i32, type_: i32) -> i32 {
    let id_u = id as usize;
    let player_id;
    let map_m;
    let map_x;
    let map_y;

    // Phase 1: validate and copy inventory item into floor item.
    {
        let sd = pe.read();
        if id > sd.player.inventory.max_inv as i32 {
            return 0;
        }
        if sd.player.inventory.inventory[id_u].id == 0 {
            return 0;
        }
        if sd.player.inventory.inventory[id_u].amount <= 0 {
            drop(sd);
            clif_senddelitem(pe, id, 1);
            return 0;
        }
        player_id = sd.id;
        map_m = sd.m;
        map_x = sd.x;
        map_y = sd.y;
    }

    let mut def = [0i32; 2];
    let mut fl = Box::new(unsafe { mem::zeroed::<FloorItemData>() });

    fl.m = map_m;
    fl.x = map_x;
    fl.y = map_y;

    // Phase 2: copy inventory item data and try stacking, mutate inventory.
    let do_full_drop;
    {
        let mut sd = pe.write();
        libc::memcpy(
            &mut fl.data as *mut _ as *mut libc::c_void,
            &sd.player.inventory.inventory[id_u] as *const Item as *const libc::c_void,
            mem::size_of::<Item>(),
        );

        // Attempt to stack onto an existing floor item at full durability.
        if fl.data.dura == item_db::search(fl.data.id as u32).dura {
            if let Some(grid) = block_grid::get_grid(fl.m as usize) {
                let cell_ids = grid.ids_at_tile(fl.x, fl.y);
                for cell_id in cell_ids {
                    if let Some(fl_arc) = map_server::map_id2fl_ref(cell_id) {
                        let mut fl_existing = fl_arc.write();
                        pc_addtocurrent_inner(
                            &mut *fl_existing as *mut FloorItemData,
                            def.as_mut_ptr(),
                            id,
                            type_,
                            &mut *sd as *mut MapSessionData,
                        );
                    }
                }
            }
        }

        sd.player.inventory.inventory[id_u].amount -= 1;

        do_full_drop = type_ != 0 || sd.player.inventory.inventory[id_u].amount == 0;
        if do_full_drop {
            libc::memset(
                &mut sd.player.inventory.inventory[id_u] as *mut Item as *mut libc::c_void,
                0,
                mem::size_of::<Item>(),
            );
        } else {
            fl.data.amount = 1;
        }
    } // write guard dropped before pe-based calls

    // Phase 3: send client updates (acquires pe locks internally).
    if do_full_drop {
        clif_senddelitem(pe, id, 1);
    } else {
        clif_sendadditem(pe, id);
    }

    // Phase 4: add to map and broadcast (Lua call — no guard held).
    if def[0] == 0 {
        let fl_raw = Box::into_raw(fl);
        map_additem(fl_raw);
        sl_doscript_2_pc("characterLog", Some("dropWrite"), player_id, (*fl_raw).id);
        if let Some(grid) = block_grid::get_grid(map_m as usize) {
            let slot = &*map_db::get_map_ptr(map_m);
            let ids = block_grid::ids_in_area(
                grid,
                map_x as i32,
                map_y as i32,
                AreaType::Area,
                slot.xs as i32,
                slot.ys as i32,
            );
            for id in ids {
                if let Some(tsd_arc) = map_server::map_id2sd_pc(id) {
                    clif_object_look2_item(tsd_arc.fd, tsd_arc.read().player.identity.id, &*fl_raw);
                }
            }
        }
    } else {
        drop(fl);
    }
    0
}

// ─── pc_changeitem ────────────────────────────────────────────────────────────

/// Swap inventory slots `id1`
/// and `id2`, sending the appropriate add/delete packets to the client.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_changeitem(pe: &PlayerEntity, id1: i32, id2: i32) -> i32 {
    {
        let mut sd = pe.write();
        let maxinv = sd.player.inventory.max_inv as i32;
        if id1 >= maxinv {
            return 0;
        }
        if id2 >= maxinv {
            return 0;
        }

        let i1 = id1 as usize;
        let i2 = id2 as usize;

        // Swap using a byte-level copy to preserve the full Item layout.
        sd.player.inventory.inventory.swap(i2, i1);
    } // write guard dropped before pe-based calls

    // Re-read after swap to match original semantics (clif_senddelitem may
    // modify slots between the two blocks).
    let i1_has = pe.read().player.inventory.inventory[id1 as usize].id != 0;
    if i1_has {
        let i2_empty = pe.read().player.inventory.inventory[id2 as usize].id == 0;
        if i2_empty {
            clif_senddelitem(pe, id2, 0);
        }
        clif_sendadditem(pe, id1);
    }
    let i2_has = pe.read().player.inventory.inventory[id2 as usize].id != 0;
    if i2_has {
        let i1_empty = pe.read().player.inventory.inventory[id1 as usize].id == 0;
        if i1_empty {
            clif_senddelitem(pe, id1, 0);
        }
        clif_sendadditem(pe, id2);
    }
    0
}

// ─── pc_useitem ───────────────────────────────────────────────────────────────

/// Use / equip the item in inventory slot `id`.
///
/// Handles all item types: food, usables, consumables, mounts, equipment, etc.
/// Delegates equip logic to `pc_equipitem`.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_useitem(pe: &PlayerEntity, id: i32) -> i32 {
    let id_u = id as usize;
    let player_id = pe.id;

    // Phase 1: validation — read guard is compatible with clif_sendminitext
    // (which only acquires pe.read()).
    let item_id;
    let item_type;
    let gm_level;
    let map_m;
    {
        let sd = pe.read();
        let maxinv = sd.player.inventory.max_inv as i32;
        if id < 0 || id >= maxinv {
            return 0;
        }

        if sd.player.inventory.inventory[id_u].id == 0 {
            return 0;
        }

        // Ownership check.
        if sd.player.inventory.inventory[id_u].owner != 0
            && sd.player.inventory.inventory[id_u].owner != sd.player.identity.id
        {
            drop(sd);
            clif_sendminitext(
                pe,
                c"You cannot use this, it does not belong to you!".as_ptr(),
            );
            return 0;
        }

        item_id = sd.player.inventory.inventory[id_u].id;
        gm_level = sd.player.identity.gm_level;
        map_m = sd.m;

        // Equipment type: check whether the current equip slot can be replaced.
        let equip_type = item_db::search(item_id).typ as i32 - 3;
        if equip_type >= 0
            && (equip_type as usize) < sd.player.inventory.equip.len()
            && sd.player.inventory.equip[equip_type as usize].id > 0
            && gm_level == 0
            && item_db::search(sd.player.inventory.equip[equip_type as usize].id).unequip as i32
                == 1
        {
            drop(sd);
            clif_sendminitext(pe, c"You are unable to unequip that.".as_ptr());
            return 0;
        }

        // Class / path restriction check.
        if item_db::search(item_id).class as i32 != 0 {
            if classdb_path(sd.player.progression.class as i32) == 5 {
                // GM — no restriction
            } else if (item_db::search(item_id).class as i32) < 6 {
                if classdb_path(sd.player.progression.class as i32)
                    != item_db::search(item_id).class as i32
                {
                    drop(sd);
                    clif_sendminitext(pe, map_msg()[MAP_ERRITMPATH].message.as_ptr());
                    return 0;
                }
            } else {
                if sd.player.progression.class as i32 != item_db::search(item_id).class as i32 {
                    drop(sd);
                    clif_sendminitext(pe, map_msg()[MAP_ERRITMPATH].message.as_ptr());
                    return 0;
                }
            }
            if (sd.player.progression.mark as i32) < item_db::search(item_id).rank {
                drop(sd);
                clif_sendminitext(pe, map_msg()[MAP_ERRITMMARK].message.as_ptr());
                return 0;
            }
        }

        // Ghost / mounted state restrictions.
        if sd.player.combat.state == PC_DIE as i8 {
            drop(sd);
            clif_sendminitext(pe, map_msg()[MAP_ERRGHOST].message.as_ptr());
            return 0;
        }
        if sd.player.combat.state == PC_MOUNTED as i8 {
            drop(sd);
            clif_sendminitext(pe, map_msg()[MAP_ERRMOUNT].message.as_ptr());
            return 0;
        }

        item_type = item_db::search(item_id).typ as i32;
    } // read guard dropped

    // Phase 2: set timed expiry if needed.
    if item_db::search(item_id).time as i32 != 0 {
        let need_time = pe.read().player.inventory.inventory[id_u].time == 0;
        if need_time {
            pe.write().player.inventory.inventory[id_u].time = (libc::time(std::ptr::null_mut())
                as u32)
                .wrapping_add(item_db::search(item_id).time as i32 as u32);
        }
    }

    let map_ptr = map_db::get_map_ptr(map_m);

    macro_rules! can_use {
        () => {
            !map_ptr.is_null() && (*map_ptr).can_use != 0 || gm_level != 0
        };
    }
    macro_rules! can_eat {
        () => {
            !map_ptr.is_null() && (*map_ptr).can_eat != 0 || gm_level != 0
        };
    }
    macro_rules! can_smoke {
        () => {
            !map_ptr.is_null() && (*map_ptr).can_smoke != 0 || gm_level != 0
        };
    }
    macro_rules! can_equip {
        () => {
            !map_ptr.is_null() && (*map_ptr).can_equip != 0 || gm_level != 0
        };
    }

    /// Helper: set invslot and free any active async coroutine.
    /// Must be called within its own scope — acquires and drops write guard.
    macro_rules! set_invslot_and_freeco {
        () => {{
            let mut sd = pe.write();
            sd.invslot = id as u8;
            sl_async_freeco(&mut *sd as *mut MapSessionData);
        }};
    }

    // Phase 3: dispatch by item type.
    match item_type {
        t if t == ITM_EAT => {
            if !can_eat!() {
                clif_sendminitext(pe, c"You cannot eat this here.".as_ptr());
                return 0;
            }
            set_invslot_and_freeco!();
            sl_doscript_simple_pc(
                scripting::carray_to_str(&item_db::search(item_id).yname),
                Some("use"),
                player_id,
            );
            sl_doscript_simple_pc("use", None, player_id);
            pc_delitem(pe, id, 1, 2);
        }
        t if t == ITM_USE => {
            if !can_use!() {
                clif_sendminitext(pe, c"You cannot use this here.".as_ptr());
                return 0;
            }
            set_invslot_and_freeco!();
            sl_doscript_simple_pc(
                scripting::carray_to_str(&item_db::search(item_id).yname),
                Some("use"),
                player_id,
            );
            sl_doscript_simple_pc("use", None, player_id);
            pc_delitem(pe, id, 1, 6);
        }
        t if t == ITM_USESPC => {
            if !can_use!() {
                clif_sendminitext(pe, c"You cannot use this here.".as_ptr());
                return 0;
            }
            set_invslot_and_freeco!();
            sl_doscript_simple_pc(
                scripting::carray_to_str(&item_db::search(item_id).yname),
                Some("use"),
                player_id,
            );
            sl_doscript_simple_pc("use", None, player_id);
            // No auto-delete for USESPC — script decides.
        }
        t if t == ITM_BAG => {
            if !can_use!() {
                clif_sendminitext(pe, c"You cannot use this here.".as_ptr());
                return 0;
            }
            set_invslot_and_freeco!();
            sl_doscript_simple_pc(
                scripting::carray_to_str(&item_db::search(item_id).yname),
                Some("use"),
                player_id,
            );
            sl_doscript_simple_pc("use", None, player_id);
        }
        t if t == ITM_MAP => {
            if !can_use!() {
                clif_sendminitext(pe, c"You cannot use this here.".as_ptr());
                return 0;
            }
            set_invslot_and_freeco!();
            sl_doscript_simple_pc(
                scripting::carray_to_str(&item_db::search(item_id).yname),
                Some("use"),
                player_id,
            );
            sl_doscript_simple_pc("maps", Some("use"), player_id);
        }
        t if t == ITM_QUIVER => {
            if !can_use!() {
                clif_sendminitext(pe, c"You cannot use this here.".as_ptr());
                return 0;
            }
            pe.write().invslot = id as u8;
            clif_sendminitext(pe, c"This item is only usable with a bow.".as_ptr());
        }
        t if t == ITM_MOUNT => {
            if !can_use!() {
                clif_sendminitext(pe, c"You cannot use this here.".as_ptr());
                return 0;
            }
            set_invslot_and_freeco!();
            sl_doscript_simple_pc(
                scripting::carray_to_str(&item_db::search(item_id).yname),
                Some("use"),
                player_id,
            );
            sl_doscript_simple_pc("onMountItem", None, player_id);
        }
        t if t == ITM_FACE => {
            if !can_use!() {
                clif_sendminitext(pe, c"You cannot use this here.".as_ptr());
                return 0;
            }
            set_invslot_and_freeco!();
            sl_doscript_simple_pc(
                scripting::carray_to_str(&item_db::search(item_id).yname),
                Some("use"),
                player_id,
            );
            sl_doscript_simple_pc("useFace", None, player_id);
        }
        t if t == ITM_SET => {
            if !can_use!() {
                clif_sendminitext(pe, c"You cannot use this here.".as_ptr());
                return 0;
            }
            set_invslot_and_freeco!();
            sl_doscript_simple_pc(
                scripting::carray_to_str(&item_db::search(item_id).yname),
                Some("use"),
                player_id,
            );
            sl_doscript_simple_pc("useSetItem", None, player_id);
        }
        t if t == ITM_SKIN => {
            if !can_use!() {
                clif_sendminitext(pe, c"You cannot use this here.".as_ptr());
                return 0;
            }
            set_invslot_and_freeco!();
            sl_doscript_simple_pc(
                scripting::carray_to_str(&item_db::search(item_id).yname),
                Some("use"),
                player_id,
            );
            sl_doscript_simple_pc("useSkinItem", None, player_id);
        }
        t if t == ITM_HAIR_DYE => {
            if !can_use!() {
                clif_sendminitext(pe, c"You cannot use this here.".as_ptr());
                return 0;
            }
            set_invslot_and_freeco!();
            sl_doscript_simple_pc(
                scripting::carray_to_str(&item_db::search(item_id).yname),
                Some("use"),
                player_id,
            );
            sl_doscript_simple_pc("useHairDye", None, player_id);
        }
        t if t == ITM_FACEACCTWO => {
            if !can_use!() {
                clif_sendminitext(pe, c"You cannot use this here.".as_ptr());
                return 0;
            }
            set_invslot_and_freeco!();
            sl_doscript_simple_pc(
                scripting::carray_to_str(&item_db::search(item_id).yname),
                Some("use"),
                player_id,
            );
            sl_doscript_simple_pc("useBeardItem", None, player_id);
        }
        t if t == ITM_SMOKE => {
            if !can_smoke!() {
                clif_sendminitext(pe, c"You cannot smoke this here.".as_ptr());
                return 0;
            }
            set_invslot_and_freeco!();
            sl_doscript_simple_pc(
                scripting::carray_to_str(&item_db::search(item_id).yname),
                Some("use"),
                player_id,
            );
            sl_doscript_simple_pc("use", None, player_id);
            let dura_zero = {
                let mut sd = pe.write();
                sd.player.inventory.inventory[id_u].dura -= 1;
                sd.player.inventory.inventory[id_u].dura == 0
            };
            if dura_zero {
                pc_delitem(pe, id, 1, 3);
            } else {
                clif_sendadditem(pe, id);
            }
        }
        // All equip types: ITM_WEAP(3) through ITM_HAND(17) inclusive.
        // This range covers: WEAP, ARMOR, SHIELD, HELM, LEFT, RIGHT, SUBLEFT,
        // SUBRIGHT, FACEACC, CROWN, MANTLE, NECKLACE, BOOTS, COAT, HAND.
        t if (ITM_WEAP..=ITM_HAND).contains(&t) => {
            if !can_equip!() {
                clif_sendminitext(pe, c"You cannot equip/de-equip on this map.".as_ptr());
                return 0;
            }
            pc_equipitem(pe, id);
        }
        t if t == ITM_ETC => {
            if !can_use!() {
                clif_sendminitext(pe, c"You cannot use this here.".as_ptr());
                return 0;
            }
            set_invslot_and_freeco!();
            sl_doscript_simple_pc(
                scripting::carray_to_str(&item_db::search(item_id).yname),
                Some("use"),
                player_id,
            );
            sl_doscript_simple_pc("use", None, player_id);
        }
        _ => {}
    }

    0
}

// ─── pc_runfloor_sub ──────────────────────────────────────────────────────────

/// Check if the player is standing on a FLOOR
/// or sub-2 NPC cell, and if so trigger its script.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_runfloor_sub(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() {
        return 0;
    }

    let npc_id = match block_grid::first_in_cell((*sd).m as usize, (*sd).x, (*sd).y, BL_NPC) {
        Some(id) => id,
        None => return 0,
    };
    let nd_arc = match map_server::map_id2npc_ref(npc_id) {
        Some(n) => n,
        None => return 0,
    };
    let nd = &mut *nd_arc.write() as *mut NpcData;

    if (*nd).subtype != FLOOR && (*nd).subtype != 2 {
        return 0;
    }

    if (*nd).subtype == 2 {
        sl_async_freeco(sd);
        sl_doscript_2_pc(
            scripting::carray_to_str(&(*nd).name),
            Some("click"),
            (*sd).id,
            (*nd).id,
        );
    }
    0
}

// ─── Equipment functions ──────────────────────────────────────────────────────
//

/// Returns the item id in equip slot
/// `type`, or 0 if the slot is empty.
///
/// Bounds-checked: returns 0 for out-of-range `type`.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_isequip(sd: *mut MapSessionData, type_: i32) -> i32 {
    if sd.is_null() {
        return 0;
    }
    if !(0..15).contains(&type_) {
        return 0;
    }
    (&(*sd).player.inventory.equip)[type_ as usize].id as i32
}

/// Send all non-empty inventory slots to the
/// client via `clif_sendadditem`.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_loaditem(pe: &PlayerEntity) -> i32 {
    let maxinv = pe.read().player.inventory.max_inv as usize;
    for i in 0..maxinv {
        let has_item = pe.read().player.inventory.inventory[i].id != 0;
        if has_item {
            clif_sendadditem(pe, i as i32);
        }
    }
    0
}

/// Send all non-empty equip slots to the client
/// via `clif_sendequip`.
///
/// Only slots 0..14 are active equipment positions.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_loadequip(pe: &PlayerEntity) -> i32 {
    for i in 0..14 {
        let has_equip = pe.read().player.inventory.equip[i].id > 0;
        if has_equip {
            clif_sendequip(pe, i as i32);
        }
    }
    0
}

/// Check whether inventory slot `id`
/// can be equipped given the current state of the player.
///
/// Returns a `MAP_ERR*` index on failure, or 0 on success.
///
/// Checks:
/// - Two-handed weapon conflicts with an equipped shield and vice-versa.
/// - Item level requirement.
/// - Might (strength) requirement.
/// - Sex restriction.
///
/// `id` is a slot index into `sd->status.inventory`.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_canequipitem(sd: *mut MapSessionData, id: i32) -> i32 {
    if sd.is_null() {
        return 0;
    }
    let maxinv = (*sd).player.inventory.max_inv as i32;
    if id < 0 || id >= maxinv {
        return 0;
    }

    let itemid = (&(*sd).player.inventory.inventory)[id as usize].id;

    // Two-handed weapon conflicts:
    // If a weapon with look 10000..29999 is equipped, a shield cannot be added.
    if pc_isequip(sd, EQ_WEAP) != 0 {
        let weap_look = item_db::search((&(*sd).player.inventory.equip)[EQ_WEAP as usize].id).look;
        if item_db::search(itemid).typ as i32 == ITM_SHIELD && (10000..=29999).contains(&weap_look)
        {
            return MAP_ERRITM2H as i32;
        }
    }

    // If a shield is equipped, a two-handed weapon cannot be added.
    if pc_isequip(sd, EQ_SHIELD) != 0 {
        let itm_look = item_db::search(itemid).look;
        if item_db::search(itemid).typ as i32 == ITM_WEAP && (10000..=29999).contains(&itm_look) {
            return MAP_ERRITM2H as i32;
        }
    }

    if ((*sd).player.progression.level as i32) < item_db::search(itemid).level as i32 {
        return MAP_ERRITMLEVEL as i32;
    }
    if (*sd).might < item_db::search(itemid).mightreq {
        return MAP_ERRITMMIGHT as i32;
    }
    let item_sex = item_db::search(itemid).sex as i32;
    if ((*sd).player.identity.sex as i32) != item_sex && item_sex != 2 {
        return MAP_ERRITMSEX as i32;
    }

    0
}

/// Check whether an item with item-id
/// `id` can be equipped given the player's current HP/MP totals.
///
/// Returns 1 if allowed, 0 if the vita/mana penalty would reduce hp/mp below 0.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_canequipstats(sd: *mut MapSessionData, id: u32) -> i32 {
    if sd.is_null() {
        return 0;
    }

    let vita = item_db::search(id).vita;
    if vita < 0 && vita.unsigned_abs() > (*sd).max_hp {
        return 0;
    }
    let mana = item_db::search(id).mana;
    if mana < 0 && mana.unsigned_abs() > (*sd).max_mp {
        return 0;
    }

    1
}

/// Begin the equip sequence for inventory
/// slot `id`.
///
/// Validates state, ownership, equip eligibility, and stat requirements before
/// firing the `onEquip` Lua event via `sl_doscript_simple_pc`.  The actual slot
/// assignment happens in `pc_equipscript` which runs from within the Lua hook.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_equipitem(pe: &PlayerEntity, id: i32) -> i32 {
    let id_u = id as usize;
    let item_id;

    // Phase 1: validation — read guard is compatible with clif_sendminitext.
    {
        let sd = pe.read();
        let maxinv = sd.player.inventory.max_inv as i32;
        if id < 0 || id >= maxinv {
            return 0;
        }

        if sd.player.inventory.inventory[id_u].id == 0 {
            return 0;
        }

        // State restrictions (non-GMs only).
        if sd.player.combat.state != 0 && sd.player.identity.gm_level == 0 {
            if sd.player.combat.state == 1 {
                drop(sd);
                clif_sendminitext(pe, c"Spirit's can't do that.".as_ptr());
            } else if sd.player.combat.state == 3 {
                drop(sd);
                clif_sendminitext(pe, c"You can't do that while riding a mount.".as_ptr());
            } else if sd.player.combat.state == 4 {
                drop(sd);
                clif_sendminitext(pe, c"You can't do that while transformed.".as_ptr());
            }
            return 0;
        }

        // Ownership check.
        if sd.player.inventory.inventory[id_u].owner != 0
            && sd.player.inventory.inventory[id_u].owner != pe.id
        {
            drop(sd);
            clif_sendminitext(pe, c"This does not belong to you.".as_ptr());
            return 0;
        }

        item_id = sd.player.inventory.inventory[id_u].id;

        // Equip eligibility (level, might, sex, 2h conflicts).
        let ret = pc_canequipitem(&*sd as *const MapSessionData as *mut MapSessionData, id);
        if ret != 0 {
            drop(sd);
            clif_sendminitext(pe, map_msg()[ret as usize].message.as_ptr());
            return 0;
        }

        // Determine equip slot from item type.
        let slot = item_db::search(item_id).typ as i32 - 3;
        if !(0..=14).contains(&slot) {
            return 0;
        }

        // Stat check.
        if pc_canequipstats(
            &*sd as *const MapSessionData as *mut MapSessionData,
            item_id,
        ) == 0
        {
            drop(sd);
            clif_sendminitext(pe, c"Your stats are too low to equip that.".as_ptr());
            return 0;
        }
    } // read guard dropped

    // Phase 2: store state for pc_equipscript, then call Lua.
    {
        let mut sd = pe.write();
        sd.equipid = item_id;
        sd.invslot = id as u8;
    } // write guard dropped before Lua calls

    sl_doscript_simple_pc("onEquip", None, pe.id);
    sl_doscript_simple_pc(
        scripting::carray_to_str(&item_db::search(item_id).yname),
        Some("onEquip"),
        pe.id,
    );

    0
}

/// Second phase of the equip sequence, called
/// from within the Lua `onEquip` hook.
///
/// Resolves the target slot (handling left/right ring swaps), removes any
/// previously-equipped item in that slot via an `onUnequip` hook, copies the
/// inventory item into the equip array, removes it from the inventory, and then
/// updates client state.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_equipscript(pe: &PlayerEntity) -> i32 {
    let equipid;
    let mut ret;
    let combat_state;
    let gm_level;
    let slot_occupied;

    // Phase 1: resolve target slot, check state restrictions.
    {
        let sd = pe.read();
        equipid = sd.equipid;
        ret = item_db::search(equipid).typ as i32 - 3;

        // Left/right ring slot arbitration: prefer the empty slot.
        if ret == EQ_LEFT {
            ret = if sd.player.inventory.equip[EQ_LEFT as usize].id != 0
                && sd.player.inventory.equip[EQ_RIGHT as usize].id == 0
            {
                EQ_RIGHT
            } else {
                EQ_LEFT
            };
        }
        if ret == EQ_RIGHT {
            ret = if sd.player.inventory.equip[EQ_RIGHT as usize].id != 0
                && sd.player.inventory.equip[EQ_LEFT as usize].id == 0
            {
                EQ_LEFT
            } else {
                EQ_RIGHT
            };
        }
        // Sub-ring slot arbitration.
        if ret == EQ_SUBLEFT {
            ret = if sd.player.inventory.equip[EQ_SUBLEFT as usize].id != 0
                && sd.player.inventory.equip[EQ_SUBRIGHT as usize].id == 0
            {
                EQ_SUBLEFT
            } else {
                EQ_SUBRIGHT
            };
        }
        if ret == EQ_SUBRIGHT {
            ret = if sd.player.inventory.equip[EQ_SUBRIGHT as usize].id != 0
                && sd.player.inventory.equip[EQ_SUBLEFT as usize].id == 0
            {
                EQ_SUBLEFT
            } else {
                EQ_SUBRIGHT
            };
        }

        combat_state = sd.player.combat.state;
        gm_level = sd.player.identity.gm_level;
        slot_occupied = sd.player.inventory.equip[ret as usize].id != 0;
    } // read guard dropped

    // State restrictions (non-GMs only).
    if combat_state != 0 && gm_level == 0 {
        if combat_state == 1 {
            clif_sendminitext(pe, c"Spirits can't do that.".as_ptr());
        }
        if combat_state == 2 {
            clif_sendminitext(pe, c"You can't do that while transformed.".as_ptr());
        }
        if combat_state == 3 {
            clif_sendminitext(pe, c"You can't do that while riding a mount.".as_ptr());
        }
        if combat_state == 4 {
            clif_sendminitext(pe, c"You can't do that while transformed.".as_ptr());
        }
        return 0;
    }

    if slot_occupied {
        // A different item is already in this slot — trigger its unequip hook.
        {
            let mut sd = pe.write();
            sd.target = pe.id as i32;
            sd.attacker = pe.id;
            sd.takeoffid = ret as i8;
        }
        sl_doscript_simple_pc("onUnequip", None, pe.id);
        sl_doscript_simple_pc(
            scripting::carray_to_str(&item_db::search(equipid).yname),
            Some("equip"),
            pe.id,
        );
        pe.write().equipid = 0;
        return 0;
    }

    // Slot is free: copy inventory item → equip slot, remove from inventory.
    let invslot;
    {
        let mut sd = pe.write();
        invslot = sd.invslot as usize;
        libc::memcpy(
            &mut sd.player.inventory.equip[ret as usize] as *mut _ as *mut libc::c_void,
            &sd.player.inventory.inventory[invslot] as *const _ as *const libc::c_void,
            mem::size_of::<Item>(),
        );
    } // write guard dropped before pe-based calls

    pc_delitem(pe, invslot as i32, 1, 6);
    sl_doscript_simple_pc(
        scripting::carray_to_str(&item_db::search(equipid).yname),
        Some("equip"),
        pe.id,
    );
    pe.write().equipid = 0;

    // If a two-handed weapon was equipped, reset enchantment.
    if ret == EQ_WEAP {
        let needs_reset = pe.read().enchanted > 1.0f32;
        if needs_reset {
            {
                let mut sd = pe.write();
                sd.enchanted = 1.0f32;
                sd.flank = 0;
                sd.backstab = 0;
            }
            clif_sendminitext(pe, c"Your weapon loses its enchantment.".as_ptr());
        }
    }

    clif_sendequip(pe, ret);
    pe.write().player.inventory.equip[ret as usize].amount = 1;

    pc_calcstat(pe);
    clif_sendupdatestatus_onequip(pe);
    broadcast_update_state(pe);

    0
}

/// Begin the unequip sequence for equip
/// slot `type`.
///
/// If the slot is empty, returns 1 immediately.  Otherwise stores `takeoffid`
/// and fires the `onUnequip` Lua hook so `pc_unequipscript` can finish.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_unequip(sd: *mut MapSessionData, type_: i32) -> i32 {
    if sd.is_null() {
        return 1;
    }
    if !(0..15).contains(&type_) {
        return 1;
    }

    if (&(*sd).player.inventory.equip)[type_ as usize].id == 0 {
        return 1;
    }

    (*sd).takeoffid = type_ as i8;
    sl_doscript_simple_pc("onUnequip", None, (*sd).id);
    0
}

/// Second phase of the unequip sequence,
/// called from within the Lua `onUnequip` hook.
///
/// If `sd->equipid > 0`, the player is simultaneously equipping a new item
/// (swap): the old equip slot item is moved to inventory and the inventory item
/// occupies the slot.  Otherwise the equip slot is cleared and the item is
/// returned to inventory.
///
/// In both paths the client is updated and `pc_calcstat` recalculates stats.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_unequipscript(pe: &PlayerEntity) -> i32 {
    let (type_, takeoff, equipid, invslot) = {
        let sd = pe.read();
        let t = sd.takeoffid as usize;
        (
            t,
            sd.player.inventory.equip[t].id,
            sd.equipid,
            sd.invslot as usize,
        )
    };

    if equipid > 0 {
        // Swap: move old equip item to inventory, place new inventory item in slot.
        let mut it = mem::zeroed::<Item>();
        {
            let mut sd = pe.write();
            libc::memcpy(
                &mut it as *mut _ as *mut libc::c_void,
                &sd.player.inventory.equip[type_] as *const _ as *const libc::c_void,
                mem::size_of::<Item>(),
            );
            libc::memcpy(
                &mut sd.player.inventory.equip[type_] as *mut _ as *mut libc::c_void,
                &sd.player.inventory.inventory[invslot] as *const _ as *const libc::c_void,
                mem::size_of::<Item>(),
            );
        }

        pc_delitem(pe, invslot as i32, 1, 6);
        pc_additem(pe, &mut it as *mut _);
        clif_sendequip(pe, type_ as i32);
        pe.write().player.inventory.equip[type_].amount = 1;
    } else {
        // Simple unequip: clear slot and return item to inventory.
        let mut it = mem::zeroed::<Item>();
        {
            let sd = pe.read();
            libc::memcpy(
                &mut it as *mut _ as *mut libc::c_void,
                &sd.player.inventory.equip[type_] as *const _ as *const libc::c_void,
                mem::size_of::<Item>(),
            );
        }

        // Guard against a zeroed-out slot (C checks `&it.id <= 0` — bogus pointer
        // arithmetic, but effectively means id==0 due to struct layout).
        if it.id == 0 {
            return 1;
        }

        if pc_additem(pe, &mut it as *mut _) != 0 {
            return 1;
        }

        {
            let mut sd = pe.write();
            libc::memset(
                &mut sd.player.inventory.equip[type_] as *mut _ as *mut libc::c_void,
                0,
                mem::size_of::<Item>(),
            );
            sd.target = pe.id as i32;
            sd.attacker = pe.id;
        }
    }

    // If a two-handed weapon was unequipped, reset enchantment.
    if type_ == EQ_WEAP as usize {
        let needs_reset = pe.read().enchanted > 1.0f32;
        if needs_reset {
            {
                let mut sd = pe.write();
                sd.enchanted = 1.0f32;
                sd.flank = 0;
                sd.backstab = 0;
            }
            clif_sendminitext(pe, c"Your weapon loses its enchantment.".as_ptr());
        }
    }

    // Fire the item's unequip Lua hook.
    sl_doscript_simple_pc(
        scripting::carray_to_str(&item_db::search(takeoff).yname),
        Some("unequip"),
        pe.id,
    );

    pe.write().takeoffid = -1i8;
    pc_calcstat(pe);
    clif_sendupdatestatus_onequip(pe);
    broadcast_update_state(pe);

    0
}

/// Pick up floor item with block-list
/// id `id` and add it to the player's inventory.
///
/// - Gold (item id 0): credited directly to `sd->status.money`.
/// - Non-droppable items (unless player is GM): rejected with a minitext.
/// - Stackable items with `pickuptype==0` and `stackamount==1`: picks up 1 at
///   a time (the floor item keeps the rest).
/// - All other cases: pick up the whole stack.
///
/// `clif_lookgone` + `map_delitem` are called when the floor item is exhausted.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_getitemscript(pe: &PlayerEntity, id: i32) -> i32 {
    let fl_raw = map_id2fl(id as u32);
    if fl_raw.is_null() {
        return 0;
    }
    let fl = fl_raw as *mut FloorItemData;

    if (*fl).data.id == 0 {
        // It's gold — credit the amount and remove from map.
        pe.write().player.inventory.money += (*fl).data.amount as u32;
        clif_sendstatus(pe, SFLAG_XPMONEY);
        clif_lookgone_by_id((*fl).id);
        map_delitem((*fl).id);

        return 0;
    }

    // Non-droppable items are blocked for regular players.
    let gm_level = pe.read().player.identity.gm_level;
    if item_db::search((*fl).data.id).droppable != 0 && gm_level == 0 {
        clif_sendminitext(pe, c"That item cannot be picked up.".as_ptr());
        return 0;
    }

    let mut it = mem::zeroed::<Item>();
    let add: bool;

    let pickuptype = pe.read().pickuptype;
    if pickuptype == 0 && item_db::search((*fl).data.id).stack_amount == 1 && (*fl).data.amount > 1
    {
        // Take only 1 from the stack.
        libc::memcpy(
            &mut it as *mut _ as *mut libc::c_void,
            &(*fl).data as *const _ as *const libc::c_void,
            mem::size_of::<Item>(),
        );
        it.amount = 1;
        (*fl).data.amount -= 1;
        add = true;
    } else {
        // Take the whole stack.
        libc::memcpy(
            &mut it as *mut _ as *mut libc::c_void,
            &(*fl).data as *const _ as *const libc::c_void,
            mem::size_of::<Item>(),
        );
        (*fl).data.amount = 0;
        add = true;
    }

    if (*fl).data.amount <= 0 {
        clif_lookgone_by_id((*fl).id);
        map_delitem((*fl).id);
    }

    if add {
        pc_additem(pe, &mut it as *mut _);
    }

    if pickuptype > 0 && (*fl).data.amount > 0 {
        return 0;
    }

    0
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_loadmagic(sd: *mut MapSessionData) -> i32 {
    for i in 0..MAX_SPELLS {
        if (&(*sd).player.spells.skills)[i] > 0 {
            clif_sendmagic(&mut *sd, i as i32);
        }
    }
    0
}

/// Initialises spell durations at login.
///
/// For each active aether timer, sends the duration bar to the client and
/// calls the `recast` Lua hook on the spell.  Also sends any pending aether
/// (cooldown) values.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_magic_startup(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() {
        return 0;
    }

    for x in 0..MAX_MAGIC_TIMERS {
        let p = &(&(*sd).player.spells.dura_aether)[x];

        if p.id > 0 {
            if p.duration > 0 {
                let caster_pe = if p.caster_id > 0 {
                    map_server::map_id2sd_pc(p.caster_id)
                } else {
                    None
                };
                {
                    let caster_name = caster_pe.as_ref().map(|cpe| cpe.name.as_str());
                    clif_send_duration(
                        (*sd).fd,
                        p.id as i32,
                        p.duration / 1000,
                        caster_name,
                    );
                }

                if let Some(ref cpe) = caster_pe {
                    (*sd).target = p.caster_id as i32;
                    (*sd).attacker = p.caster_id;
                    sl_doscript_2_pc(
                        scripting::carray_to_str(&magic_db::search(p.id as i32).yname),
                        Some("recast"),
                        (*sd).id,
                        cpe.id,
                    );
                } else {
                    (*sd).target = (*sd).player.identity.id as i32;
                    (*sd).attacker = (*sd).player.identity.id;
                    sl_doscript_simple_pc(
                        scripting::carray_to_str(&magic_db::search(p.id as i32).yname),
                        Some("recast"),
                        (*sd).id,
                    );
                }
            }

            if p.aether > 0 {
                clif_send_aether(&mut *sd, p.id as i32, p.aether / 1000);
            }
        }
    }

    0
}

/// Like `pc_magic_startup` but takes `&PlayerEntity`, properly scoping lock
/// guards so they are never held across Lua calls.
pub fn pc_magic_startup_pe(pe: &PlayerEntity) -> i32 {
    let id = pe.id;

    // Snapshot active spells under a read guard (SkillInfo is Copy).
    let spells: Vec<SkillInfo> = {
        let sd = pe.read();
        sd.player.spells.dura_aether.iter().copied().collect()
    };

    for p in &spells {
        if p.id == 0 {
            continue;
        }

        if p.duration > 0 {
            let caster_pe = if p.caster_id > 0 {
                map_server::map_id2sd_pc(p.caster_id)
            } else {
                None
            };

            // Send duration packet — no locks needed, only fd + caster name.
            {
                let caster_name = caster_pe.as_ref().map(|cpe| cpe.name.as_str());
                clif_send_duration(pe.fd, p.id as i32, p.duration / 1000, caster_name);
            }
            // Guard dropped — safe to call Lua.

            let spell_entry = magic_db::search(p.id as i32);
            let spell_name = scripting::carray_to_str(&spell_entry.yname);
            if let Some(ref cpe) = caster_pe {
                {
                    let mut sd = pe.write();
                    sd.target = p.caster_id as i32;
                    sd.attacker = p.caster_id;
                }
                sl_doscript_2_pc(spell_name, Some("recast"), id, cpe.id);
            } else {
                {
                    let mut sd = pe.write();
                    sd.target = sd.player.identity.id as i32;
                    sd.attacker = sd.player.identity.id;
                }
                sl_doscript_simple_pc(spell_name, Some("recast"), id);
            }
        }

        if p.aether > 0 {
            let mut sd = pe.write();
            clif_send_aether(&mut *sd, p.id as i32, p.aether / 1000);
        }
    }

    0
}

/// Resends active aether (spell cooldown)
/// values to the client.  Called when the client reconnects.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn pc_reload_aether(sd: *mut MapSessionData) -> i32 {
    for x in 0..MAX_MAGIC_TIMERS {
        let p = &(&(*sd).player.spells.dura_aether)[x];
        if p.id > 0 && p.aether > 0 {
            clif_send_aether(&mut *sd, p.id as i32, p.aether / 1000);
        }
    }
    0
}
