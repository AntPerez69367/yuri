//! Mob item / drop helpers.

#![allow(non_snake_case)]

use crate::common::constants::entity::item::FLOORITEM_START_NUM;
use crate::common::constants::entity::mob::{MAX_INVENTORY, MOB_START_NUM};
use crate::common::constants::world::MAX_GROUP_MEMBERS;
use crate::common::traits::LegacyEntity;
use crate::database::map_db::get_map_ptr as ffi_get_map_ptr;
use crate::game::block::AreaType;
use crate::game::block_grid;
use crate::game::map_parse::visual::clif_object_look2_item;
use crate::game::map_server::{groups as groups_mob, map_additem, map_id2fl_ref, map_id2mob_ref, map_id2sd_pc};
use crate::game::pc::MapSessionData;
use crate::game::scripting::types::floor::FloorItemData;

use super::entity::{map_id2sd_mob, sl_doscript_2, MobSpawnData};

// ─── Item / drop helpers ──────────────────────────────────────────────────────

/// Typed inner: sets def[0]=1 on first hit (used as a foreachincell "any-present" test).
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_thing_yeah_inner(_entity_id: u32, def: *mut i32) -> i32 {
    if !def.is_null() {
        *def = 1;
    }
    0
}

/// Typed inner: merge item `fl2` into an existing floor-item `fl` if IDs match.
/// Args: `int* def`, `int id` (unused), `FLOORITEM* fl2`, `USER* sd` (unused).
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_addtocurrent_inner(
    fl: *mut FloorItemData,
    def: *mut i32,
    _id: i32,
    fl2: *mut FloorItemData,
    _sd: *mut MapSessionData,
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
    if (*fl).data.id == (*fl2).data.id {
        (*fl).data.amount += (*fl2).data.amount;
        *def = 1;
    }
    0
}

/// Parameters describing the item being dropped from a mob.
#[derive(Clone, Copy)]
pub struct DropItemSpec {
    pub id: u32,
    pub amount: i32,
    pub dura: i32,
    pub protected_: i32,
    pub owner: i32,
}

/// Drop an item onto the ground at (m, x, y).
/// Reads `attacker->group_count` and `groups[]` to populate floor-item looters.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mob_dropitem(
    blockid: u32,
    item: DropItemSpec,
    m: i32,
    x: i32,
    y: i32,
    sd: *mut MapSessionData,
) -> i32 {
    let DropItemSpec {
        id,
        amount,
        dura,
        protected_,
        owner,
    } = item;
    let mob_arc_holder = if (MOB_START_NUM..FLOORITEM_START_NUM).contains(&blockid) {
        map_id2mob_ref(blockid)
    } else {
        None
    };
    let mob: *mut MobSpawnData = match mob_arc_holder {
        Some(ref arc) => &mut *arc.write() as *mut MobSpawnData,
        None => std::ptr::null_mut(),
    };

    let mut def: i32 = 0;
    let mut fl = Box::new(unsafe { std::mem::zeroed::<FloorItemData>() });
    fl.m = m as u16;
    fl.x = x as u16;
    fl.y = y as u16;
    fl.data.id = id;
    fl.data.amount = amount;
    fl.data.dura = dura;
    fl.data.protected = protected_ as u32;
    fl.data.owner = owner as u32;

    if let Some(grid) = block_grid::get_grid(m as usize) {
        let def_ptr = &raw mut def;
        let fl_ptr = fl.as_mut() as *mut FloorItemData;
        let sd_ptr = sd;
        let cell_ids = grid.ids_at_tile(x as u16, y as u16);
        for cid in cell_ids {
            if let Some(fl_arc) = map_id2fl_ref(cid) {
                mob_addtocurrent_inner(
                    &mut *fl_arc.write() as *mut FloorItemData,
                    def_ptr,
                    id as i32,
                    fl_ptr,
                    sd_ptr,
                );
            }
        }
    }

    fl.timer = libc::time(std::ptr::null_mut()) as u32;
    // looters is already zeroed by mem::zeroed()

    if !mob.is_null() {
        let attacker = map_id2sd_mob((*mob).attacker);
        if !attacker.is_null() {
            if (*attacker).group_count > 0 {
                let safe_count = if (*attacker).group_count < MAX_GROUP_MEMBERS as i32 {
                    (*attacker).group_count as usize
                } else {
                    MAX_GROUP_MEMBERS
                };
                let gid = (*attacker).groupid as usize;
                if gid < 256 {
                    let grp = groups_mob();
                    for z in 0..safe_count {
                        let idx = gid * MAX_GROUP_MEMBERS + z;
                        if idx < grp.len() {
                            fl.looters[z] = grp[idx];
                        }
                    }
                }
            } else {
                fl.looters[0] = (*attacker).id;
            }
        }
    }

    if def == 0 {
        let fl_raw = Box::into_raw(fl);
        map_additem(fl_raw);
        if let Some(grid) = block_grid::get_grid(m as usize) {
            let slot = &*ffi_get_map_ptr(m as u16);
            let ids =
                block_grid::ids_in_area(grid, x, y, AreaType::Area, slot.xs as i32, slot.ys as i32);
            for id in ids {
                if let Some(pe) = map_id2sd_pc(id) {
                    clif_object_look2_item(pe.fd, pe.read().player.identity.id, &*fl_raw);
                }
            }
        }
    } else {
        drop(fl);
    }
    0
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn mobdb_drops(mob: *mut MobSpawnData, sd: *mut MapSessionData) -> i32 {
    sl_doscript_2("mobDrops", None, (*sd).id, (*mob).id);
    for i in 0..MAX_INVENTORY {
        let slot = &(*mob).inventory[i];
        if slot.id != 0 && slot.amount >= 1 {
            mob_dropitem(
                (*mob).id,
                DropItemSpec {
                    id: slot.id,
                    amount: slot.amount,
                    dura: slot.dura,
                    protected_: slot.protected as i32,
                    owner: slot.owner as i32,
                },
                (*mob).m as i32,
                (*mob).x as i32,
                (*mob).y as i32,
                sd,
            );
            (*mob).inventory[i].id = 0;
            (*mob).inventory[i].amount = 0;
            (*mob).inventory[i].owner = 0;
            (*mob).inventory[i].dura = 0;
            (*mob).inventory[i].protected = 0;
        }
    }
    0
}
