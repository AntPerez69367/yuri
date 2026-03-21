//! PcObject — Lua UserData wrapping a C `USER*` player pointer.

#![allow(unused_variables)]

use mlua::{MetaMethod, UserData, UserDataMethods};
use std::ffi::{CStr, CString};

use crate::common::traits::LegacyEntity;

use crate::game::map_parse::visual::clif_spawn;
use crate::game::scripting::pc_accessors::{
    sl_pc_getpk, sl_pc_getgroup,
    sl_pc_input_send, sl_pc_dialog_send, sl_pc_dialogseq_send,
    sl_pc_menu_send, sl_pc_menuseq_send,
    sl_pc_menustring_send, sl_pc_menustring2_send,
    sl_pc_buy_send, sl_pc_buydialog_send, sl_pc_buyextend_send,
    sl_pc_sell_send, sl_pc_sell2_send, sl_pc_sellextend_send,
    sl_pc_showbank_send, sl_pc_showbankadd_send,
    sl_pc_bankaddmoney_send, sl_pc_bankwithdrawmoney_send,
    sl_pc_clanshowbank_send, sl_pc_clanshowbankadd_send,
    sl_pc_clanbankaddmoney_send, sl_pc_clanbankwithdrawmoney_send,
    sl_pc_clanviewbank_send,
    sl_pc_repairextend_send, sl_pc_repairall_send,
};
use crate::game::scripting::map_globals::{
    sl_g_getusers_ids, sl_g_deliddb, sl_g_addpermanentspawn,
};
use crate::game::scripting::types::mob::MobObject;
use crate::game::scripting::types::registry::{
    GameRegObject, MapRegObject, NpcRegObject, QuestRegObject, RegObject, RegStringObject,
};
use crate::game::scripting::types::shared;

pub struct PcObject {
    pub id: u32,
}
// u32 is Send — no unsafe impl needed.

fn val_to_int(v: &mlua::Value) -> i32 {
    match v {
        mlua::Value::Integer(i) => *i as i32,
        mlua::Value::Number(f) => *f as i32,
        mlua::Value::Boolean(b) => {
            if *b {
                1
            } else {
                0
            }
        }
        _ => 0,
    }
}

fn val_to_str(v: &mlua::Value) -> Option<CString> {
    match v {
        mlua::Value::String(s) => s
            .to_str()
            .ok()
            .and_then(|bs| CString::new(bs.as_bytes()).ok()),
        _ => None,
    }
}

unsafe fn cstr_to_lua(lua: &mlua::Lua, p: *const i8) -> mlua::Result<mlua::Value> {
    if p.is_null() {
        return Ok(mlua::Value::Nil);
    }
    let s = CStr::from_ptr(p).to_str().unwrap_or("");
    Ok(mlua::Value::String(lua.create_string(s)?))
}

#[allow(unused_imports)]
use crate::game::scripting::pc_accessors::*;
use crate::game::client::visual::broadcast_update_state;


// ─── Task 10: async yield helpers ────────────────────────────────────────────

fn lua_table_to_cstrings(tbl: &mlua::Table) -> mlua::Result<Vec<CString>> {
    lua_table_to_cstrings_from(tbl, 1)
}

fn lua_table_to_cstrings_from(tbl: &mlua::Table, start: i64) -> mlua::Result<Vec<CString>> {
    let mut out = Vec::new();
    let len = tbl.raw_len() as i64;
    for i in start..=len {
        let s: String = tbl.raw_get(i)?;
        out.push(CString::new(s.as_bytes()).map_err(mlua::Error::external)?);
    }
    Ok(out)
}

fn lua_table_to_ints(tbl: &mlua::Table) -> mlua::Result<Vec<i32>> {
    let mut out = Vec::new();
    let len = tbl.raw_len();
    for i in 1..=len {
        let v: i64 = tbl.raw_get(i)?;
        out.push(v as i32);
    }
    Ok(out)
}

fn cstring_ptrs(v: &[CString]) -> Vec<*const i8> {
    v.iter().map(|s| s.as_ptr()).collect()
}


impl PcObject {
    /// Execute a closure with a write guard on the player's session data.
    /// Returns `default` if the player is no longer online.
    #[inline]
    fn with_sd<R, F: FnOnce(&mut crate::game::pc::MapSessionData) -> R>(&self, default: R, f: F) -> R {
        match crate::game::map_server::map_id2sd_pc(self.id) {
            Some(pe) => f(&mut pe.write()),
            None => default,
        }
    }

    /// Execute a closure with the PlayerEntity reference.
    /// Returns `default` if the player is no longer online.
    #[inline]
    fn with_pe<R, F: FnOnce(&crate::game::player::PlayerEntity) -> R>(&self, default: R, f: F) -> R {
        match crate::game::map_server::map_id2sd_pc(self.id) {
            Some(pe) => f(&pe),
            None => default,
        }
    }

    /// Raw pointer to session data — only for interop with functions that require `*mut c_void`.
    #[inline]
    fn sd_ptr_raw(&self) -> *mut crate::game::pc::MapSessionData {
        crate::game::map_server::map_id2sd_pc(self.id)
            .map(|arc| &mut *arc.write() as *mut crate::game::pc::MapSessionData)
            .unwrap_or(std::ptr::null_mut())
    }
}

impl UserData for PcObject {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // ── __index: read PC attributes ───────────────────────────────────────
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            let arc = match crate::game::map_server::map_id2sd_pc(this.id) {
                Some(arc) => arc,
                None => return Ok(mlua::Value::Nil),
            };
            let mut guard = arc.write();
            let sd = &mut *guard;
            let entity_id = this.id;
            macro_rules! int_ {
                ($f:expr) => {
                    Ok(mlua::Value::Integer($f(sd) as i64))
                };
            }
            macro_rules! bool_ {
                ($f:expr) => {
                    Ok(mlua::Value::Boolean($f(sd) != 0))
                };
            }
            macro_rules! str_ {
                ($f:expr) => {
                    unsafe { cstr_to_lua(lua, $f(sd)) }
                };
            }
            // New — for migrated &str getters (no unsafe, no CStr parsing)
            macro_rules! str_ref {
                ($f:expr) => {
                    Ok(mlua::Value::String(lua.create_string($f(sd))?))
                };
            }
            // Shared map properties (pvp, mapTitle, bgm, etc.) handled before the type-specific match.
            let m = sl_pc_bl_m(sd);
            if let Some(v) = unsafe { shared::map_field(lua, m, key.as_str()) } {
                return v;
            }
            match key.as_str() {
                "ID" => int_!(sl_pc_bl_id),
                "id" => int_!(sl_pc_status_id),
                "mapId" | "m" => int_!(sl_pc_bl_m),
                "x" => int_!(sl_pc_bl_x),
                "y" => int_!(sl_pc_bl_y),
                "blType" => int_!(sl_pc_bl_type),
                "groupID" => int_!(sl_pc_groupid),
                "health" => int_!(sl_pc_status_hp),
                "magic" => int_!(sl_pc_status_mp),
                "maxHealth" => int_!(sl_pc_max_hp),
                "maxMagic" => int_!(sl_pc_max_mp),
                "lastHealth" => int_!(sl_pc_lastvita),
                "baseHealth" => int_!(sl_pc_status_basehp),
                "baseMagic" => int_!(sl_pc_status_basemp),
                "level" => int_!(sl_pc_status_level),
                "exp" => int_!(sl_pc_status_exp),
                "expSoldMagic" => int_!(sl_pc_status_expsoldmagic),
                "expSoldHealth" => int_!(sl_pc_status_expsoldhealth),
                "expSoldStats" => int_!(sl_pc_status_expsoldstats),
                "class" => int_!(sl_pc_status_class),
                "baseClass" => int_!(sl_pc_baseclass),
                "baseClassName" => str_!(sl_pc_baseClassName),
                "className" => str_!(sl_pc_className),
                "classNameMark" => str_!(sl_pc_classNameMark),
                "classRank" => int_!(sl_pc_status_classRank),
                "totem" => int_!(sl_pc_status_totem),
                "tier" => int_!(sl_pc_status_tier),
                "mark" => int_!(sl_pc_status_mark),
                "name" => str_ref!(sl_pc_status_name),
                "title" => str_ref!(sl_pc_status_title),
                "sex" => int_!(sl_pc_status_sex),
                "country" => int_!(sl_pc_status_country),
                "side" => int_!(sl_pc_status_side),
                "partner" => int_!(sl_pc_status_partner),
                "tutor" => int_!(sl_pc_status_tutor),
                "karma" => int_!(sl_pc_status_karma),
                "alignment" => int_!(sl_pc_status_alignment),
                "email" => str_!(sl_pc_email),
                "ipaddress" => str_!(sl_pc_ipaddress),
                "clan" => int_!(sl_pc_status_clan),
                "clanName" => str_!(sl_pc_clanname),
                "clanTitle" => str_ref!(sl_pc_status_clan_title),
                "clanRank" => int_!(sl_pc_status_clanRank),
                "actId" => Ok(mlua::Value::Integer(unsafe { sl_pc_actid(sd) } as i64)),
                "gmLevel" => int_!(sl_pc_status_gm_level),
                "PK" => int_!(sl_pc_status_pk),
                "killedBy" => int_!(sl_pc_status_killedby),
                "killsPK" => int_!(sl_pc_status_killspk),
                "killsPVP" => int_!(sl_pc_status_killspvp),
                "durationPK" => int_!(sl_pc_status_pkduration),
                "face" => int_!(sl_pc_status_face),
                "hair" => int_!(sl_pc_status_hair),
                "hairColor" => int_!(sl_pc_status_hair_color),
                "faceColor" => int_!(sl_pc_status_face_color),
                "armorColor" => int_!(sl_pc_status_armor_color),
                "skinColor" => int_!(sl_pc_status_skin_color),
                "faceAccessoryTwo" => int_!(sl_pc_faceacctwo_id),
                "faceAccessoryTwoColor" => int_!(sl_pc_faceacctwo_custom),
                "money" => int_!(sl_pc_status_money),
                "bankMoney" => int_!(sl_pc_status_bankmoney),
                "exchangeMoney" => int_!(sl_pc_exchange_gold),
                "exchangeItemCount" => int_!(sl_pc_exchange_count),
                "BODItemCount" => int_!(sl_pc_bod_count),
                "maxSlots" => int_!(sl_pc_status_maxslots),
                "maxInv" => int_!(sl_pc_status_maxinv),
                "grace" => int_!(sl_pc_grace),
                "baseGrace" => int_!(sl_pc_status_basegrace),
                "might" => int_!(sl_pc_might),
                "baseMight" => int_!(sl_pc_status_basemight),
                "will" => int_!(sl_pc_will),
                "baseWill" => int_!(sl_pc_status_basewill),
                "armor" => int_!(sl_pc_armor),
                "baseArmor" => int_!(sl_pc_status_basearmor),
                "dam" => int_!(sl_pc_dam),
                "hit" => int_!(sl_pc_hit),
                "miss" => int_!(sl_pc_miss),
                "crit" => int_!(sl_pc_crit),
                "critChance" => int_!(sl_pc_critchance),
                "critMult" => int_!(sl_pc_critmult),
                "attackSpeed" => int_!(sl_pc_attack_speed),
                "healing" => int_!(sl_pc_healing),
                "rage" => int_!(sl_pc_rage),
                "minSDam" => int_!(sl_pc_minSdam),
                "maxSDam" => int_!(sl_pc_maxSdam),
                "minLDam" => int_!(sl_pc_minLdam),
                "maxLDam" => int_!(sl_pc_maxLdam),
                "protection" => int_!(sl_pc_protection),
                "dmgShield" => int_!(sl_pc_dmgshield),
                "dmgDealt" => int_!(sl_pc_dmgdealt),
                "dmgTaken" => int_!(sl_pc_dmgtaken),
                "state" => int_!(sl_pc_status_state),
                "paralyzed" => bool_!(sl_pc_paralyzed),
                "blind" => bool_!(sl_pc_blind),
                "drunk" => int_!(sl_pc_drunk),
                "confused" => bool_!(sl_pc_confused),
                "snare" => bool_!(sl_pc_snare),
                "silence" => bool_!(sl_pc_silence),
                "extendHit" => bool_!(sl_pc_extendhit),
                "afk" => bool_!(sl_pc_afk),
                "afkTime" => int_!(sl_pc_afktime),
                "afkTimeTotal" => int_!(sl_pc_totalafktime),
                "afkMessage" => str_ref!(sl_pc_status_afkmessage),
                "backstab" => bool_!(sl_pc_backstab),
                "flank" => bool_!(sl_pc_flank),
                "spotTraps" => bool_!(sl_pc_spottraps),
                "mute" => bool_!(sl_pc_status_mute),
                "selfBar" => bool_!(sl_pc_selfbar),
                "groupBars" => bool_!(sl_pc_groupbars),
                "mobBars" => bool_!(sl_pc_mobbars),
                "target" => int_!(sl_pc_target),
                "attacker" => int_!(sl_pc_attacker),
                "rangeTarget" => int_!(sl_pc_rangeTarget),
                "damage" => int_!(sl_pc_damage),
                "sleep" => int_!(sl_pc_sleep),
                "deduction" => int_!(sl_pc_deduction),
                "speed" => int_!(sl_pc_speed),
                "invis" => int_!(sl_pc_invis),
                "disguise" => int_!(sl_pc_disguise),
                "disguiseColor" => int_!(sl_pc_disguise_color),
                "board" => int_!(sl_pc_board),
                "boardDel" => int_!(sl_pc_board_candel),
                "boardWrite" => int_!(sl_pc_board_canwrite),
                "boardShow" => int_!(sl_pc_boardshow),
                "boardNameVal" => int_!(sl_pc_boardnameval),
                "talkType" => int_!(sl_pc_talktype),
                "speech" => str_!(sl_pc_speech),
                "question" => str_!(sl_pc_question),
                "enchant" => int_!(sl_pc_enchanted),
                "actionTime" => int_!(sl_pc_time),
                "polearm" => int_!(sl_pc_polearm),
                "lastClick" => int_!(sl_pc_last_click),
                "noviceChat" => int_!(sl_pc_status_novice_chat),
                "subpathChat" => int_!(sl_pc_status_subpath_chat),
                "clanChat" => int_!(sl_pc_status_clan_chat),
                "fakeDrop" => int_!(sl_pc_fakeDrop),
                "coRef" => int_!(sl_pc_coref),
                "optFlags" => int_!(sl_pc_optFlags),
                "settings" => int_!(sl_pc_status_settingFlags),
                "miniMapToggle" => int_!(sl_pc_status_miniMapToggle),
                "heroShow" => int_!(sl_pc_status_heroes),
                "ping" => int_!(sl_pc_msPing),
                "pbColor" => int_!(sl_pc_pbColor),
                "equipID" => int_!(sl_pc_equipid),
                "takeOffID" => int_!(sl_pc_takeoffid),
                "breakID" => int_!(sl_pc_breakid),
                "equipSlot" => int_!(sl_pc_equipslot),
                "invSlot" => int_!(sl_pc_invslot),
                "pickUpType" => int_!(sl_pc_pickuptype),
                "profileVitaStats" => int_!(sl_pc_status_profile_vitastats),
                "profileEquipList" => int_!(sl_pc_status_profile_equiplist),
                "profileLegends" => int_!(sl_pc_status_profile_legends),
                "profileSpells" => int_!(sl_pc_status_profile_spells),
                "profileInventory" => int_!(sl_pc_status_profile_inventory),
                "profileBankItems" => int_!(sl_pc_status_profile_bankitems),
                "timerTick" => int_!(sl_pc_scripttick),
                "displayTimeLeft" => int_!(sl_pc_disptimertick),
                "fury" => int_!(sl_pc_fury),
                "f1Name" => str_ref!(sl_pc_status_f1name),
                "mail" => str_!(sl_pc_mail),
                "cursed" => int_!(sl_pc_cursed),
                "dialogType" => int_!(sl_pc_dialogtype),
                "ambushTimer" => int_!(sl_pc_ambushtimer),
                "bindMap" => int_!(sl_pc_bindmap),
                "bindX" => int_!(sl_pc_bindx),
                "bindY" => int_!(sl_pc_bindy),
                "deathFlag" => int_!(sl_pc_deathflag),
                "wisdom" => int_!(sl_pc_wisdom),
                "con" => int_!(sl_pc_con),
                "action" => int_!(sl_pc_action),
                "gfxClone" => int_!(sl_pc_clone),
                "npcGraphic" => int_!(sl_pc_npc_g),
                "npcColor" => int_!(sl_pc_npc_gc),
                "gfxFace" => int_!(sl_pc_gfx_face),
                "gfxHair" => int_!(sl_pc_gfx_hair),
                "gfxHairC" => int_!(sl_pc_gfx_chair),
                "gfxFaceC" => int_!(sl_pc_gfx_cface),
                "gfxSkinC" => int_!(sl_pc_gfx_cskin),
                "gfxDye" => int_!(sl_pc_gfx_dye),
                "gfxTitleColor" => int_!(sl_pc_gfx_dye),
                "gfxWeap" => int_!(sl_pc_gfx_weapon),
                "gfxWeapC" => int_!(sl_pc_gfx_cweapon),
                "gfxArmor" => int_!(sl_pc_gfx_armor),
                "gfxArmorC" => int_!(sl_pc_gfx_carmor),
                "gfxShield" => int_!(sl_pc_gfx_shield),
                "gfxShieldC" => int_!(sl_pc_gfx_cshield),
                "gfxHelm" => int_!(sl_pc_gfx_helm),
                "gfxHelmC" => int_!(sl_pc_gfx_chelm),
                "gfxMantle" => int_!(sl_pc_gfx_mantle),
                "gfxMantleC" => int_!(sl_pc_gfx_cmantle),
                "gfxCrown" => int_!(sl_pc_gfx_crown),
                "gfxCrownC" => int_!(sl_pc_gfx_ccrown),
                "gfxFaceA" => int_!(sl_pc_gfx_faceAcc),
                "gfxFaceAC" => int_!(sl_pc_gfx_cfaceAcc),
                "gfxFaceAT" => int_!(sl_pc_gfx_faceAccT),
                "gfxFaceATC" => int_!(sl_pc_gfx_cfaceAccT),
                "gfxBoots" => int_!(sl_pc_gfx_boots),
                "gfxBootsC" => int_!(sl_pc_gfx_cboots),
                "gfxNeck" => int_!(sl_pc_gfx_necklace),
                "gfxNeckC" => int_!(sl_pc_gfx_cnecklace),
                "gfxName" => str_!(sl_pc_gfx_name),
                // Task 4: new attribute bindings
                "vRegenOverflow"  => int_!(sl_pc_vregenoverflow),
                "mRegenOverflow"  => int_!(sl_pc_mregenoverflow),
                "groupCount"      => int_!(sl_pc_group_count),
                "groupOn"         => int_!(sl_pc_group_on),
                "groupLeader"     => int_!(sl_pc_group_leader),
                "group" => {
                    const MAX_MEMBERS: usize = 256;
                    let mut ids = [0u32; MAX_MEMBERS];
                    let n = unsafe {
                        sl_pc_getgroup(sd, ids.as_mut_ptr(), MAX_MEMBERS as i32)
                    };
                    let t = lua.create_table()?;
                    for (i, id) in ids.iter().enumerate().take(n.max(0) as usize) {
                        t.raw_set(i + 1, *id as i64)?;
                    }
                    Ok(mlua::Value::Table(t))
                }
                // Alias: docs use "spouse", implementation uses "partner"
                "spouse"          => int_!(sl_pc_status_partner),
                // Alias: docs use "ac", implementation uses "armor"
                "ac"              => int_!(sl_pc_armor),
                // Registry sub-objects — mirroring pcl_init from scripting.c.
                "registry" => lua.pack(RegObject { ptr: sd as *mut _ as *mut std::ffi::c_void }),
                "registryString" => lua.pack(RegStringObject { ptr: sd as *mut _ as *mut std::ffi::c_void }),
                "quest" => lua.pack(QuestRegObject { ptr: sd as *mut _ as *mut std::ffi::c_void }),
                "npc" => lua.pack(NpcRegObject { ptr: sd as *mut _ as *mut std::ffi::c_void }),
                "mapRegistry" => lua.pack(MapRegObject { ptr: sd as *mut _ as *mut std::ffi::c_void }),
                "gameRegistry" => {
                    lua.pack(GameRegObject {
                        ptr: std::ptr::null_mut(),
                    })
                }

                // getUsers() → table of all online PcObjects.
                "getUsers" => {
                    Ok(mlua::Value::Function(lua.create_function(
                        |lua, _: mlua::MultiValue| {
                            let ids = sl_g_getusers_ids();
                            let tbl = lua.create_table()?;
                            for (i, &id) in ids.iter().enumerate() {
                                let val = crate::game::scripting::id_to_lua(lua, id)?;
                                tbl.raw_set(i + 1, val)?;
                            }
                            Ok(tbl)
                        },
                    )?))
                }
                "getBlock" => {
                    shared::make_getblock_fn(lua)
                }
                "getObjectsInCell" | "getAliveObjectsInCell" | "getObjectsInCellWithTraps" => {
                    shared::make_cell_query_fn(lua, key.as_str())
                }
                "getObjectsInArea" | "getAliveObjectsInArea"
                | "getObjectsInSameMap" | "getAliveObjectsInSameMap" => {
                    shared::make_area_query_fn(lua, key.as_str(), entity_id)
                }
                "getPK" => {
                    let capture_id = this.id;
                    Ok(mlua::Value::Function(lua.create_function(
                        move |_lua, (_this, id): (mlua::AnyUserData, i32)| {
                            let arc = match crate::game::map_server::map_id2sd_pc(capture_id) {
                                Some(arc) => arc,
                                None => return Ok(false),
                            };
                            let mut guard = arc.write();
                            Ok(sl_pc_getpk(&mut guard, id) != 0)
                        },
                    )?))
                }
                "getObjectsInMap" => shared::make_map_query_fn(lua),
                "sendAnimation"     => shared::make_sendanimation_fn(lua, entity_id),
                "playSound"         => shared::make_playsound_fn(lua, entity_id),
                "sendAction"        => shared::make_sendaction_fn(lua, entity_id),
                "msg"               => shared::make_msg_fn(lua, entity_id),
                "dropItem"          => shared::make_dropitem_fn(lua, entity_id),
                "dropItemXY"        => shared::make_dropitemxy_fn(lua, entity_id),
                "objectCanMove"     => shared::make_objectcanmove_fn(lua, entity_id),
                "objectCanMoveFrom" => shared::make_objectcanmovefrom_fn(lua, entity_id),
                "repeatAnimation"   => shared::make_repeatanimation_fn(lua, entity_id),
                "selfAnimation"     => shared::make_selfanimation_fn(lua, entity_id),
                "selfAnimationXY"   => shared::make_selfanimationxy_fn(lua, entity_id),
                "sendParcel"        => shared::make_sendparcel_fn(lua, entity_id),
                "throw"             => shared::make_throwblock_fn(lua, entity_id),
                "delFromIDDB" => {
                    let capture_id = this.id;
                    Ok(mlua::Value::Function(lua.create_function(
                        move |_, _: mlua::MultiValue| {
                            sl_g_deliddb(capture_id);
                            Ok(())
                        }
                    )?))
                }
                "addPermanentSpawn" => {
                    let capture_id = this.id;
                    Ok(mlua::Value::Function(lua.create_function(
                        move |_, _: mlua::MultiValue| {
                            sl_g_addpermanentspawn(capture_id);
                            Ok(())
                        }
                    )?))
                }
                _ => {
                    // Delegate to the global Player table for Lua-defined methods (e.g. Player.regen).
                    if let Ok(tbl) = lua.globals().get::<mlua::Table>("Player") {
                        if let Ok(v) = tbl.get::<mlua::Value>(key.as_str()) {
                            if !matches!(v, mlua::Value::Nil) {
                                return Ok(v);
                            }
                        }
                    }
                    tracing::debug!("[scripting] PcObject: unimplemented __index key={key:?}");
                    Ok(mlua::Value::Nil)
                }
            }
        });

        // ── __newindex: write PC attributes ──────────────────────────────────
        methods.add_meta_method_mut(
            MetaMethod::NewIndex,
            |_lua, this, (key, val): (String, mlua::Value)| {
                let pe = match crate::game::map_server::map_id2sd_pc(this.id) {
                    Some(pe) => pe,
                    None => return Ok(()),
                };
                let mut guard = pe.write();
                let sd = &mut *guard;
                let v = val_to_int(&val);
                match key.as_str() {
                    "actionTime" => sl_pc_set_time(sd, v),
                    "afk" => sl_pc_set_afk(sd, v),
                    "alignment" => sl_pc_set_alignment(sd, v),
                    "armorColor" => sl_pc_set_armor_color(sd, v),
                    "attacker" => sl_pc_set_attacker(sd, v),
                    "backstab" => sl_pc_set_backstab(sd, v),
                    "bankMoney" => sl_pc_set_bankmoney(sd, v),
                    "baseArmor" => sl_pc_set_basearmor(sd, v),
                    "baseGrace" => sl_pc_set_basegrace(sd, v),
                    "baseHealth" => sl_pc_set_basehp(sd, v),
                    "baseMagic" => sl_pc_set_basemp(sd, v),
                    "baseMight" => sl_pc_set_basemight(sd, v),
                    "baseWill" => sl_pc_set_basewill(sd, v),
                    "bindMap" => sl_pc_set_bindmap(sd, v),
                    "bindX" => sl_pc_set_bindx(sd, v),
                    "bindY" => sl_pc_set_bindy(sd, v),
                    "blind" => sl_pc_set_blind(sd, v),
                    "boardDel" => sl_pc_set_board_candel(sd, v),
                    "boardNameVal" => sl_pc_set_boardnameval(sd, v),
                    "boardShow" => sl_pc_set_boardshow(sd, v),
                    "boardWrite" => sl_pc_set_board_canwrite(sd, v),
                    "clan" => sl_pc_set_clan(sd, v),
                    "clanChat" => sl_pc_set_clan_chat(sd, v),
                    "clanRank" => sl_pc_set_clanRank(sd, v),
                    "class" => sl_pc_set_class(&pe, sd, v),
                    "classRank" => sl_pc_set_classRank(sd, v),
                    "coContainer" => sl_pc_set_coref_container(sd, v),
                    "con" => sl_pc_set_con(sd, v),
                    "confused" => sl_pc_set_confused(sd, v),
                    "country" => sl_pc_set_country(sd, v),
                    "crit" => sl_pc_set_crit(sd, v),
                    "critChance" => sl_pc_set_critchance(sd, v),
                    "critMult" => sl_pc_set_critmult(sd, v),
                    "cursed" => sl_pc_set_cursed(sd, v),
                    "damage" => sl_pc_set_damage(sd, v),
                    "deathFlag" => sl_pc_set_deathflag(sd, v),
                    "deduction" => sl_pc_set_deduction(sd, v),
                    "disguise" => sl_pc_set_disguise(sd, v),
                    "disguiseColor" => sl_pc_set_disguise_color(sd, v),
                    "dmgDealt" => sl_pc_set_dmgdealt(sd, v),
                    "dmgShield" => sl_pc_set_dmgshield(sd, v),
                    "dmgTaken" => sl_pc_set_dmgtaken(sd, v),
                    "drunk" => sl_pc_set_drunk(sd, v),
                    "exp" => sl_pc_set_exp(&pe, sd, v),
                    "extendHit" => sl_pc_set_extendhit(sd, v),
                    "face" => sl_pc_set_face(sd, v),
                    "faceColor" => sl_pc_set_face_color(sd, v),
                    "fakeDrop" => sl_pc_set_fakeDrop(sd, v),
                    "flank" => sl_pc_set_flank(sd, v),
                    "fury" => sl_pc_set_fury(sd, v),
                    "gfxArmor" => sl_pc_set_gfx_armor(sd, v),
                    "gfxArmorC" => sl_pc_set_gfx_carmor(sd, v),
                    "gfxBoots" => sl_pc_set_gfx_boots(sd, v),
                    "gfxBootsC" => sl_pc_set_gfx_cboots(sd, v),
                    "gfxClone" => sl_pc_set_clone(sd, v),
                    "gfxCrown" => sl_pc_set_gfx_crown(sd, v),
                    "gfxCrownC" => sl_pc_set_gfx_ccrown(sd, v),
                    "gfxDye" => sl_pc_set_gfx_dye(sd, v),
                    "gfxFace" => sl_pc_set_gfx_face(sd, v),
                    "gfxFaceA" => sl_pc_set_gfx_faceAcc(sd, v),
                    "gfxFaceAC" => sl_pc_set_gfx_cfaceAcc(sd, v),
                    "gfxFaceAT" => sl_pc_set_gfx_faceAccT(sd, v),
                    "gfxFaceATC" => sl_pc_set_gfx_cfaceAccT(sd, v),
                    "gfxFaceC" => sl_pc_set_gfx_cface(sd, v),
                    "gfxHair" => sl_pc_set_gfx_hair(sd, v),
                    "gfxHairC" => sl_pc_set_gfx_chair(sd, v),
                    "gfxHelm" => sl_pc_set_gfx_helm(sd, v),
                    "gfxHelmC" => sl_pc_set_gfx_chelm(sd, v),
                    "gfxMantle" => sl_pc_set_gfx_mantle(sd, v),
                    "gfxMantleC" => sl_pc_set_gfx_cmantle(sd, v),
                    "gfxNeck" => sl_pc_set_gfx_necklace(sd, v),
                    "gfxNeckC" => sl_pc_set_gfx_cnecklace(sd, v),
                    "gfxShield" => sl_pc_set_gfx_shield(sd, v),
                    "gfxShieldC" => sl_pc_set_gfx_cshield(sd, v),
                    "gfxSkinC" => sl_pc_set_gfx_cskin(sd, v),
                    "gfxWeap" => sl_pc_set_gfx_weapon(sd, v),
                    "gfxWeapC" => sl_pc_set_gfx_cweapon(sd, v),
                    "gmLevel" => sl_pc_set_gm_level(sd, v),
                    "groupBars" => sl_pc_set_groupbars(sd, v),
                    "hair" => sl_pc_set_hair(sd, v),
                    "hairColor" => sl_pc_set_hair_color(sd, v),
                    "healing" => sl_pc_set_healing(sd, v),
                    "health" => sl_pc_set_hp(sd, v),
                    "heroShow" => sl_pc_set_heroshow(sd, v),
                    "invis" => sl_pc_set_invis(sd, v),
                    "karma" => sl_pc_set_karma(sd, v),
                    "lastClick" => sl_pc_set_last_click(sd, v),
                    "level" => sl_pc_set_level(&pe, sd, v),
                    "magic" => sl_pc_set_mp(sd, v),
                    "mark" => sl_pc_set_mark(sd, v),
                    "maxHealth" => sl_pc_set_max_hp(sd, v),
                    "maxInv" => sl_pc_set_maxinv(sd, v),
                    "maxMagic" => sl_pc_set_max_mp(sd, v),
                    "maxSlots" => sl_pc_set_maxslots(sd, v),
                    "miniMapToggle" => sl_pc_set_settingFlags(sd, v),
                    "mobBars" => sl_pc_set_mobbars(sd, v),
                    "money" => sl_pc_set_money(sd, v),
                    "mute" => sl_pc_set_mute(sd, v),
                    "noviceChat" => sl_pc_set_novice_chat(sd, v),
                    "npcColor" => sl_pc_set_npc_gc(sd, v),
                    "npcGraphic" => sl_pc_set_npc_g(sd, v),
                    "optFlags" => sl_pc_set_optFlags_xor(sd, v),
                    "paralyzed" => sl_pc_set_paralyzed(sd, v),
                    "partner" => sl_pc_set_partner(sd, v),
                    "pbColor" => sl_pc_set_pbColor(sd, v),
                    "PK" => sl_pc_set_pk(sd, v),
                    "polearm" => sl_pc_set_polearm(sd, v),
                    "profileBankItems" => sl_pc_set_profile_bankitems(sd, v),
                    "profileEquipList" => sl_pc_set_profile_equiplist(sd, v),
                    "profileInventory" => sl_pc_set_profile_inventory(sd, v),
                    "profileLegends" => sl_pc_set_profile_legends(sd, v),
                    "profileSpells" => sl_pc_set_profile_spells(sd, v),
                    "profileVitaStats" => sl_pc_set_profile_vitastats(sd, v),
                    "protection" => sl_pc_set_protection(sd, v),
                    "rage" => sl_pc_set_rage(sd, v),
                    "rangeTarget" => sl_pc_set_rangeTarget(sd, v),
                    "selfBar" => sl_pc_set_selfbar(sd, v),
                    "settings" => sl_pc_set_settingFlags(sd, v),
                    "sex" => sl_pc_set_sex(sd, v),
                    "side" => sl_pc_set_side(sd, v),
                    "dialogType" => sl_pc_set_dialogtype(sd, v),
                    "silence" => sl_pc_set_silence(sd, v),
                    "sleep" => sl_pc_set_sleep(sd, v),
                    "skinColor" => sl_pc_set_skin_color(sd, v),
                    "snare" => sl_pc_set_snare(sd, v),
                    "speed" => sl_pc_set_speed(sd, v),
                    "spotTraps" => sl_pc_set_spottraps(sd, v),
                    "state" => sl_pc_set_state(sd, v),
                    "subpathChat" => sl_pc_set_subpath_chat(sd, v),
                    "talkType" => sl_pc_set_talktype(sd, v),
                    "target"   => sl_pc_set_target(sd, v),
                    "tier" => sl_pc_set_tier(sd, v),
                    "totem" => sl_pc_set_totem(sd, v),
                    "tutor" => sl_pc_set_tutor(sd, v),
                    "uflags" => sl_pc_set_uflags_xor(sd, v),
                    "wisdom" => sl_pc_set_wisdom(sd, v),
                    // Task 4: new attribute bindings
                    "vRegenOverflow"  => sl_pc_set_vregenoverflow(sd, v),
                    "mRegenOverflow"  => sl_pc_set_mregenoverflow(sd, v),
                    "groupCount"      => sl_pc_set_group_count(sd, v),
                    "groupOn"         => sl_pc_set_group_on(sd, v),
                    "groupLeader"     => sl_pc_set_group_leader(sd, v),
                    "spouse"          => sl_pc_set_partner(sd, v),
                    "ac"              => sl_pc_set_basearmor(sd, v),
                    // string fields
                    "name" => {
                        if let Some(cs) = val_to_str(&val) {
                            unsafe { sl_pc_set_name(sd, cs.as_ptr()) }
                        }
                    }
                    "title" => {
                        if let Some(cs) = val_to_str(&val) {
                            unsafe { sl_pc_set_title(sd, cs.as_ptr()) }
                        }
                    }
                    "clanTitle" => {
                        if let Some(cs) = val_to_str(&val) {
                            unsafe { sl_pc_set_clan_title(sd, cs.as_ptr()) }
                        }
                    }
                    "afkMessage" => {
                        if let Some(cs) = val_to_str(&val) {
                            unsafe { sl_pc_set_afkmessage(sd, cs.as_ptr()) }
                        }
                    }
                    "speech" => {
                        if let Some(cs) = val_to_str(&val) {
                            unsafe { sl_pc_set_speech(sd, cs.as_ptr()) }
                        }
                    }
                    "gfxName" => {
                        if let Some(cs) = val_to_str(&val) {
                            unsafe { sl_pc_set_gfx_name(sd, cs.as_ptr()) }
                        }
                    }
                    _ => {
                        tracing::debug!(
                            "[scripting] PcObject: unimplemented __newindex key={key:?}"
                        );
                    }
                }
                Ok(())
            },
        );

        // ── Named methods — health/combat ─────────────────────────────────────
        methods.add_method("addHealth", |_, this, damage: i32| {
            this.with_sd((), |sd| unsafe { sl_pc_addhealth(sd, damage) }); Ok(())
        });
        methods.add_method("removeHealth", |_, this, (damage, caster): (i32, i32)| {
            this.with_sd((), |sd| unsafe { sl_pc_removehealth(sd, damage, caster) }); Ok(())
        });
        methods.add_method("die", |_, this, ()| {
            this.with_sd((), |sd| unsafe { sl_pc_die(sd) }); Ok(())
        });
        methods.add_method("resurrect", |_, this, ()| {
            this.with_sd((), |sd| unsafe { sl_pc_resurrect(sd) }); Ok(())
        });
        methods.add_method("showHealth", |_, this, (damage, typ): (i32, i32)| {
            this.with_sd((), |sd| sl_pc_showhealth(sd, damage, typ)); Ok(())
        });
        methods.add_method("freeAsync", |_, this, ()| {
            this.with_sd((), |sd| unsafe { sl_pc_freeasync(sd) }); Ok(())
        });
        methods.add_method("forceSave", |_, this, ()| {
            this.with_pe((), |pe| { sl_pc_forcesave(pe); }); Ok(())
        });
        methods.add_method("calcStat", |_, this, ()| {
            this.with_pe((), |pe| unsafe { sl_pc_calcstat(pe) }); Ok(())
        });
        methods.add_method("sendStatus", |_, this, ()| {
            this.with_pe((), |pe| unsafe { sl_pc_sendstatus(pe) }); Ok(())
        });
        methods.add_method("status", |_, this, ()| {
            if let Some(pe) = crate::game::map_server::map_id2sd_pc(this.id) {
                unsafe { sl_pc_status(&pe); }
            }
            Ok(())
        });
        methods.add_method("warp", |_, this, (m, x, y): (i32, i32, i32)| {
            this.with_pe((), |pe| unsafe { sl_pc_warp(pe, m, x, y) }); Ok(())
        });
        methods.add_method("refresh", |_, this, ()| {
            this.with_sd((), |sd| unsafe { sl_pc_refresh(sd) }); Ok(())
        });
        methods.add_method("pickUp", |_, this, id: u32| {
            this.with_sd((), |sd| unsafe { sl_pc_pickup(sd, id) }); Ok(())
        });
        methods.add_method("throwItem", |_, this, ()| {
            this.with_sd((), |sd| unsafe { sl_pc_throwitem(sd) }); Ok(())
        });
        methods.add_method("forceDrop", |_, this, id: i32| {
            this.with_sd((), |sd| unsafe { sl_pc_forcedrop(sd, id) }); Ok(())
        });
        methods.add_method("lock", |_, this, ()| {
            this.with_sd((), |sd| unsafe { sl_pc_lock(sd) }); Ok(())
        });
        methods.add_method("unlock", |_, this, ()| {
            this.with_sd((), |sd| unsafe { sl_pc_unlock(sd) }); Ok(())
        });
        methods.add_method("swing", |_, this, ()| {
            this.with_sd((), |sd| unsafe { sl_pc_swing(sd) }); Ok(())
        });
        methods.add_method("respawn", |_, this, ()| {
            this.with_pe((), |pe| { clif_spawn(pe); }); Ok(())
        });
        methods.add_method("sendHealth", |_, this, (dmg, crit): (f32, i32)| {
            Ok(this.with_sd(0, |sd| unsafe { sl_pc_sendhealth(sd, dmg, crit) }))
        });

        // ── Movement ──────────────────────────────────────────────────────────
        methods.add_method("move", |_, this, speed: i32| {
            this.with_sd((), |sd| unsafe { sl_pc_move(sd, speed) }); Ok(())
        });
        methods.add_method("lookAt", |_, this, id: i32| {
            this.with_sd((), |sd| unsafe { sl_pc_lookat(sd, id) }); Ok(())
        });
        methods.add_method("miniRefresh", |_, this, ()| {
            this.with_sd((), |sd| unsafe { sl_pc_minirefresh(sd) }); Ok(())
        });
        methods.add_method("refreshInventory", |_, this, ()| {
            this.with_sd((), |sd| unsafe { sl_pc_refreshinventory(sd) }); Ok(())
        });
        methods.add_method("updateInv", |_, this, ()| {
            this.with_sd((), |sd| unsafe { sl_pc_updateinv(sd) }); Ok(())
        });
        methods.add_method("checkInvBod", |_, this, ()| {
            this.with_sd((), |sd| unsafe { sl_pc_checkinvbod(sd) }); Ok(())
        });

        // ── Equipment ────────────────────────────────────────────────────────
        methods.add_method("equip", |_, this, ()| {
            this.with_sd((), |sd| unsafe { sl_pc_equip(sd) }); Ok(())
        });
        methods.add_method("takeOff", |_, this, ()| {
            this.with_sd((), |sd| unsafe { sl_pc_takeoff(sd) }); Ok(())
        });
        methods.add_method("deductArmor", |_, this, v: i32| {
            this.with_sd((), |sd| unsafe { sl_pc_deductarmor(sd, v) }); Ok(())
        });
        methods.add_method("deductWeapon", |_, this, v: i32| {
            this.with_sd((), |sd| sl_pc_deductweapon(sd, v)); Ok(())
        });
        methods.add_method("deductDura", |_, this, (eq, v): (i32, i32)| {
            this.with_sd((), |sd| unsafe { sl_pc_deductdura(sd, eq, v) }); Ok(())
        });
        methods.add_method("deductDuraEquip", |_, this, ()| {
            this.with_sd((), sl_pc_deductduraequip); Ok(())
        });
        methods.add_method("deductDuraInv", |_, this, (slot, v): (i32, i32)| {
            this.with_sd((), |sd| sl_pc_deductdurainv(sd, slot, v)); Ok(())
        });
        methods.add_method("hasEquipped", |_, this, id: u32| {
            Ok(this.with_sd(0, |sd| sl_pc_hasequipped(sd, id)) != 0)
        });
        methods.add_method("removeItemSlot", |_, this, (slot, amount, typ): (i32, i32, i32)| {
            this.with_sd((), |sd| unsafe { sl_pc_removeitemslot(sd, slot, amount, typ) }); Ok(())
        });
        methods.add_method("hasItem", |_, this, (id, amount): (u32, i32)| {
            Ok(this.with_sd(0, |sd| sl_pc_hasitem(sd, id, amount)) != 0)
        });
        methods.add_method("hasSpace", |_, this, id: u32| {
            Ok(this.with_sd(0, |sd| unsafe { sl_pc_hasspace(sd, id) }) != 0)
        });

        // ── Stats ────────────────────────────────────────────────────────────
        methods.add_method("checkLevel", |_, this, ()| {
            this.with_sd((), |sd| unsafe { sl_pc_checklevel(sd) }); Ok(())
        });

        // ── UI / display ─────────────────────────────────────────────────────
        methods.add_method("sendMiniMap", |_, this, ()| {
            this.with_sd((), |sd| unsafe { sl_pc_sendminimap(sd) }); Ok(())
        });
        methods.add_method("setMiniMapToggle", |_, this, flag: i32| {
            this.with_sd((), |sd| sl_pc_setminimaptoggle(sd, flag)); Ok(())
        });
        methods.add_method("popup", |_, this, msg: String| {
            if let Ok(cs) = CString::new(msg.as_bytes()) {
                this.with_sd((), |sd| unsafe { sl_pc_popup(sd, cs.as_ptr()) });
            }
            Ok(())
        });
        methods.add_method("popUp", |_, this, msg: String| {
            if let Ok(cs) = CString::new(msg.as_bytes()) {
                this.with_sd((), |sd| unsafe { sl_pc_popup(sd, cs.as_ptr()) });
            }
            Ok(())
        });
        methods.add_method("guiText", |_, this, msg: String| {
            if let Ok(cs) = CString::new(msg.as_bytes()) {
                this.with_sd((), |sd| unsafe { sl_pc_guitext(sd, cs.as_ptr()) });
            }
            Ok(())
        });
        methods.add_method("sendMiniText", |_, this, msg: String| {
            if let Ok(cs) = CString::new(msg.as_bytes()) {
                this.with_sd((), |sd| unsafe { sl_pc_sendminitext(sd, cs.as_ptr()) });
            }
            Ok(())
        });
        methods.add_method("sendMinitext", |_, this, msg: String| {
            if let Ok(cs) = CString::new(msg.as_bytes()) {
                this.with_sd((), |sd| unsafe { sl_pc_sendminitext(sd, cs.as_ptr()) });
            }
            Ok(())
        });
        methods.add_method("powerBoard", |_, this, ()| {
            this.with_sd((), sl_pc_powerboard); Ok(())
        });
        methods.add_method("showBoard", |_, this, id: i32| {
            this.with_sd((), |sd| unsafe { sl_pc_showboard(sd, id) }); Ok(())
        });
        methods.add_method("showPost", |_, this, (id, post): (i32, i32)| {
            this.with_sd((), |sd| unsafe { sl_pc_showpost(sd, id, post) }); Ok(())
        });
        methods.add_method("changeView", |_, this, (x, y): (i32, i32)| {
            this.with_sd((), |sd| unsafe { sl_pc_changeview(sd, x, y) }); Ok(())
        });

        // ── Social / network ─────────────────────────────────────────────────
        methods.add_method("speak", |_, this, (msg, typ): (String, i32)| {
            if let Ok(cs) = CString::new(msg.as_bytes()) {
                let len = cs.as_bytes().len() as i32;
                this.with_sd((), |sd| unsafe { sl_pc_speak(sd, cs.as_ptr(), len, typ) });
            }
            Ok(())
        });
        methods.add_method("sendMail", |_, this, (to, topic, msg): (String, String, String)| {
            let to_cs = CString::new(to.as_bytes()).ok();
            let topic_cs = CString::new(topic.as_bytes()).ok();
            let msg_cs = CString::new(msg.as_bytes()).ok();
            if let (Some(t), Some(s), Some(m)) = (to_cs, topic_cs, msg_cs) {
                this.with_sd((), |sd| unsafe { sl_pc_sendmail(sd, t.as_ptr(), s.as_ptr(), m.as_ptr()); });
            }
            Ok(())
        });
        methods.add_method("sendUrl", |_, this, (typ, url): (i32, String)| {
            if let Ok(cs) = CString::new(url.as_bytes()) {
                this.with_sd((), |sd| unsafe { sl_pc_sendurl(sd, typ, cs.as_ptr()) });
            }
            Ok(())
        });
        methods.add_method("swingTarget", |_, this, val: mlua::Value| {
            // swing.lua passes MobObject/PcObject userdata, not a raw integer.
            let target_id: i32 = match val {
                mlua::Value::Integer(n) => n as i32,
                mlua::Value::Number(n) => n as i32,
                mlua::Value::UserData(ud) => {
                    if let Ok(mob) = ud.borrow::<MobObject>() {
                        match crate::game::map_server::map_id2mob_ref(mob.id) {
                            Some(arc) => arc.read().id as i32,
                            None => return Ok(()),
                        }
                    } else if let Ok(pc) = ud.borrow::<PcObject>() {
                        pc.id as i32
                    } else {
                        return Ok(());
                    }
                }
                _ => return Ok(()),
            };
            let arc = match crate::game::map_server::map_id2sd_pc(this.id) {
                Some(arc) => arc,
                None => return Ok(()),
            };
            let mut guard = arc.write();
            unsafe { sl_pc_swingtarget(&mut guard, target_id) };
            Ok(())
        });

        // ── Kill registry ─────────────────────────────────────────────────────
        methods.add_method("killCount", |_, this, mob_id: i32| {
            Ok(this.with_sd(0, |sd| sl_pc_killcount(sd, mob_id)))
        });
        methods.add_method("setKillCount", |_, this, (mob_id, amount): (i32, i32)| {
            this.with_sd((), |sd| sl_pc_setkillcount(sd, mob_id, amount)); Ok(())
        });
        methods.add_method("flushKills", |_, this, mob_id: i32| {
            this.with_sd((), |sd| sl_pc_flushkills(sd, mob_id)); Ok(())
        });
        methods.add_method("flushAllKills", |_, this, ()| {
            this.with_sd((), sl_pc_flushallkills); Ok(())
        });

        // ── Threat ───────────────────────────────────────────────────────────
        methods.add_method("addThreat", |_, this, (mob_id, amount): (u32, u32)| {
            this.with_sd((), |sd| unsafe { sl_pc_addthreat(sd, mob_id, amount) }); Ok(())
        });
        methods.add_method("setThreat", |_, this, (mob_id, amount): (u32, u32)| {
            this.with_sd((), |sd| unsafe { sl_pc_setthreat(sd, mob_id, amount) }); Ok(())
        });
        methods.add_method("addThreatGeneral", |_, this, amount: u32| {
            this.with_sd((), |sd| unsafe { sl_pc_addthreatgeneral(sd, amount) }); Ok(())
        });

        // ── Spell list ───────────────────────────────────────────────────────
        methods.add_method("hasSpell", |_, this, name: String| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| this.with_sd(0, |sd| unsafe { sl_pc_hasspell(sd, c.as_ptr()) })) != 0)
        });
        methods.add_method("addSpell", |_, this, spell_id: i32| {
            this.with_sd((), |sd| unsafe { sl_pc_addspell(sd, spell_id) }); Ok(())
        });
        methods.add_method("removeSpell", |_, this, spell_id: i32| {
            this.with_sd((), |sd| sl_pc_removespell(sd, spell_id)); Ok(())
        });

        // ── Duration system ──────────────────────────────────────────────────
        methods.add_method("hasDuration", |_, this, name: String| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| this.with_sd(0, |sd| unsafe { sl_pc_hasduration(sd, c.as_ptr()) })) != 0)
        });
        methods.add_method("hasDurationId", |_, this, (name, caster): (String, i32)| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| this.with_sd(0, |sd| unsafe { sl_pc_hasdurationid(sd, c.as_ptr(), caster) })) != 0)
        });
        methods.add_method("hasDurationID", |_, this, (name, caster): (String, i32)| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| this.with_sd(0, |sd| unsafe { sl_pc_hasdurationid(sd, c.as_ptr(), caster) })) != 0)
        });
        methods.add_method("getDuration", |_, this, name: String| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| this.with_sd(0, |sd| unsafe { sl_pc_getduration(sd, c.as_ptr()) })))
        });
        methods.add_method("getDurationId", |_, this, (name, caster): (String, i32)| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| this.with_sd(0, |sd| unsafe { sl_pc_getdurationid(sd, c.as_ptr(), caster) })))
        });
        methods.add_method("getDurationID", |_, this, (name, caster): (String, i32)| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| this.with_sd(0, |sd| unsafe { sl_pc_getdurationid(sd, c.as_ptr(), caster) })))
        });
        methods.add_method("durationAmount", |_, this, name: String| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| this.with_sd(0, |sd| unsafe { sl_pc_durationamount(sd, c.as_ptr()) })))
        });
        methods.add_method("setDuration", |_, this, (name, time_ms, caster, recast): (String, i32, Option<i32>, Option<i32>)| {
            if let Ok(cs) = CString::new(name.as_bytes()) {
                this.with_sd((), |sd| unsafe { sl_pc_setduration(sd, cs.as_ptr(), time_ms, caster.unwrap_or(0), recast.unwrap_or(0)) });
            }
            Ok(())
        });
        methods.add_method("flushDuration", |_, this, (level, min_id, max_id): (i32, Option<i32>, Option<i32>)| {
            this.with_sd((), |sd| unsafe { sl_pc_flushduration(sd, level, min_id.unwrap_or(0), max_id.unwrap_or(i32::MAX)) }); Ok(())
        });
        methods.add_method("flushDurationNoUncast", |_, this, (level, min_id, max_id): (i32, Option<i32>, Option<i32>)| {
            this.with_sd((), |sd| unsafe { sl_pc_flushdurationnouncast(sd, level, min_id.unwrap_or(0), max_id.unwrap_or(i32::MAX)) }); Ok(())
        });
        methods.add_method("refreshDurations", |_, this, ()| {
            this.with_sd((), |sd| unsafe { sl_pc_refreshdurations(sd) }); Ok(())
        });

        // ── Aether system ────────────────────────────────────────────────────
        methods.add_method("setAether", |_, this, (name, time_ms): (String, i32)| {
            if let Ok(cs) = CString::new(name.as_bytes()) {
                this.with_sd((), |sd| unsafe { sl_pc_setaether(sd, cs.as_ptr(), time_ms) });
            }
            Ok(())
        });
        methods.add_method("hasAether", |_, this, name: String| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| this.with_sd(0, |sd| unsafe { sl_pc_hasaether(sd, c.as_ptr()) })) != 0)
        });
        methods.add_method("getAether", |_, this, name: String| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| this.with_sd(0, |sd| unsafe { sl_pc_getaether(sd, c.as_ptr()) })))
        });
        methods.add_method("flushAether", |_, this, ()| {
            this.with_sd((), |sd| unsafe { sl_pc_flushaether(sd) }); Ok(())
        });

        // ── Clan / path ──────────────────────────────────────────────────────
        methods.add_method("addClan", |_, this, name: String| {
            if let Ok(cs) = CString::new(name.as_bytes()) {
                this.with_sd((), |sd| sl_pc_addclan(sd, cs.as_ptr()));
            }
            Ok(())
        });
        methods.add_method("updatePath", |_, this, (path, mark): (i32, i32)| {
            this.with_sd((), |sd| sl_pc_updatepath(sd, path, mark)); Ok(())
        });
        methods.add_method("updateCountry", |_, this, country: i32| {
            this.with_sd((), |sd| sl_pc_updatecountry(sd, country)); Ok(())
        });

        // ── Misc ─────────────────────────────────────────────────────────────
        methods.add_method("getCasterId", |_, this, name: String| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| this.with_sd(0, |sd| unsafe { sl_pc_getcasterid(sd, c.as_ptr()) })))
        });
        methods.add_method("getCasterID", |_, this, name: String| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| this.with_sd(0, |sd| unsafe { sl_pc_getcasterid(sd, c.as_ptr()) })))
        });
        methods.add_method("setTimer", |_, this, (typ, length): (i32, i32)| {
            this.with_sd((), |sd| unsafe { sl_pc_settimer(sd, typ, length as u32) }); Ok(())
        });
        methods.add_method("addTime", |_, this, v: i32| {
            this.with_sd((), |sd| unsafe { sl_pc_addtime(sd, v) }); Ok(())
        });
        methods.add_method("removeTime", |_, this, v: i32| {
            this.with_sd((), |sd| unsafe { sl_pc_removetime(sd, v) }); Ok(())
        });
        methods.add_method("setHeroShow", |_, this, flag: i32| {
            this.with_sd((), |sd| sl_pc_setheroshow(sd, flag)); Ok(())
        });

        // ── Legends ──────────────────────────────────────────────────────────
        methods.add_method("addLegend", |_, this, (text, name, icon, color, tchaid): (String, String, i32, i32, u32)| {
            let t = CString::new(text.as_bytes()).ok();
            let n = CString::new(name.as_bytes()).ok();
            if let (Some(tc), Some(nc)) = (t, n) {
                this.with_sd((), |sd| unsafe { sl_pc_addlegend(sd, tc.as_ptr(), nc.as_ptr(), icon, color, tchaid) });
            }
            Ok(())
        });
        methods.add_method("hasLegend", |_, this, name: String| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| this.with_sd(0, |sd| unsafe { sl_pc_haslegend(sd, c.as_ptr()) })) != 0)
        });
        methods.add_method("removeLegendByName", |_, this, name: String| {
            if let Ok(cs) = CString::new(name.as_bytes()) {
                this.with_sd((), |sd| unsafe { sl_pc_removelegendbyname(sd, cs.as_ptr()) });
            }
            Ok(())
        });
        methods.add_method("removeLegendbyName", |_, this, name: String| {
            if let Ok(cs) = CString::new(name.as_bytes()) {
                this.with_sd((), |sd| unsafe { sl_pc_removelegendbyname(sd, cs.as_ptr()) });
            }
            Ok(())
        });
        methods.add_method("removeLegendByColor", |_, this, color: i32| {
            this.with_sd((), |sd| sl_pc_removelegendbycolor(sd, color)); Ok(())
        });
        methods.add_method("removeLegendbyColor", |_, this, color: i32| {
            this.with_sd((), |sd| sl_pc_removelegendbycolor(sd, color)); Ok(())
        });

        // ── Inventory ────────────────────────────────────────────────────────────
        methods.add_method("addItem", |_, this, (id, amount, dura, owner, engrave): (i32, i32, i32, i32, String)| {
            if let Ok(cs) = CString::new(engrave.as_bytes()) {
                this.with_sd((), |sd| unsafe { sl_pc_additem(sd, id as u32, amount as u32, dura, owner as u32, cs.as_ptr()) });
            }
            Ok(())
        });
        methods.add_method("getInventoryItem", |lua, this, slot: i32| {
            if !(0..52).contains(&slot) { return Ok(mlua::Value::Nil); }
            let ptr = this.with_sd(std::ptr::null_mut(), |sd| unsafe { sl_pc_getinventoryitem(sd, slot) });
            if ptr.is_null() { return Ok(mlua::Value::Nil); }
            Ok(mlua::Value::UserData(lua.create_userdata(
                crate::game::scripting::types::item::BItemObject { ptr }
            )?))
        });
        methods.add_method("getEquippedItem", |lua, this, slot: i32| {
            if !(0..15).contains(&slot) { return Ok(mlua::Value::Nil); }
            let ptr = this.with_sd(std::ptr::null_mut(), |sd| unsafe { sl_pc_getequippeditem_sd(sd, slot) });
            if ptr.is_null() { return Ok(mlua::Value::Nil); }
            Ok(mlua::Value::UserData(lua.create_userdata(
                crate::game::scripting::types::item::BItemObject { ptr }
            )?))
        });
        methods.add_method("removeItem", |_, this, (id, amount, typ): (i32, i32, i32)| {
            this.with_sd((), |sd| unsafe { sl_pc_removeitem(sd, id as u32, amount as u32, typ, 0, std::ptr::null()) }); Ok(())
        });
        methods.add_method("removeItemDura", |_, this, (id, typ): (i32, i32)| {
            this.with_sd((), |sd| unsafe { sl_pc_removeitemdura(sd, id as u32, 1, typ) }); Ok(())
        });
        methods.add_method("hasItemDura", |_, this, (id, amount): (i32, i32)| {
            Ok(this.with_sd(0, |sd| unsafe { sl_pc_hasitemdura(sd, id as u32, amount as u32) }) != 0)
        });

        // ── Bank ─────────────────────────────────────────────────────────────────
        methods.add_method("checkBankItems", |_, this, slot: i32| {
            if !(0..255).contains(&slot) { return Ok(0i32); }
            Ok(this.with_sd(0, |sd| sl_pc_checkbankitems(sd, slot)))
        });
        methods.add_method("checkBankAmounts", |_, this, slot: i32| {
            if !(0..255).contains(&slot) { return Ok(0i32); }
            Ok(this.with_sd(0, |sd| sl_pc_checkbankamounts(sd, slot)))
        });
        methods.add_method("checkBankOwners", |_, this, slot: i32| {
            if !(0..255).contains(&slot) { return Ok(0i32); }
            Ok(this.with_sd(0, |sd| sl_pc_checkbankowners(sd, slot)))
        });
        methods.add_method("checkBankEngraves", |lua, this, slot: i32| {
            if !(0..255).contains(&slot) { return Ok(mlua::Value::Nil); }
            this.with_sd(Ok(mlua::Value::Nil), |sd| unsafe { cstr_to_lua(lua, sl_pc_checkbankengraves(sd, slot)) })
        });
        methods.add_method("bankDeposit", |_, this, (item, amount, owner, engrave): (i32, i32, i32, String)| {
            if let Ok(cs) = CString::new(engrave.as_bytes()) {
                this.with_sd((), |sd| unsafe { sl_pc_bankdeposit(sd, item as u32, amount as u32, owner as u32, cs.as_ptr()) });
            }
            Ok(())
        });
        methods.add_method("bankWithdraw", |_, this, (item, amount, owner, engrave): (i32, i32, i32, String)| {
            if let Ok(cs) = CString::new(engrave.as_bytes()) {
                this.with_sd((), |sd| unsafe { sl_pc_bankwithdraw(sd, item as u32, amount as u32, owner as u32, cs.as_ptr()) });
            }
            Ok(())
        });
        methods.add_method("bankCheckAmount", |_, this, (item, amount, owner, engrave): (i32, i32, i32, String)| {
            let cs = CString::new(engrave.as_bytes()).ok();
            Ok(cs.map_or(0, |c| this.with_sd(0, |sd| unsafe { sl_pc_bankcheckamount(sd, item as u32, amount as u32, owner as u32, c.as_ptr()) })))
        });

        // ── Clan bank ────────────────────────────────────────────────────────────
        methods.add_method("getClanItems",         |_, this, slot: i32| Ok(this.with_sd(0, |sd| unsafe { sl_pc_getclanitems(sd, slot) })));
        methods.add_method("getClanAmounts",       |_, this, slot: i32| Ok(this.with_sd(0, |sd| unsafe { sl_pc_getclanamounts(sd, slot) })));
        methods.add_method("clanBankDeposit",      |_, this, (item, amount): (i32, i32)| { this.with_sd((), |sd| unsafe { sl_pc_clanbankdeposit(sd, item as u32, amount as u32, 0, std::ptr::null()) }); Ok(()) });
        methods.add_method("clanBankWithdraw",     |_, this, (item, amount): (i32, i32)| { this.with_sd((), |sd| unsafe { sl_pc_clanbankwithdraw(sd, item as u32, amount as u32, 0, std::ptr::null()) }); Ok(()) });
        methods.add_method("checkClanItemAmounts", |_, this, (item, amount): (i32, i32)| Ok(this.with_sd(0, |sd| unsafe { sl_pc_checkclankitemamounts(sd, item, amount) })));

        // ── Spell lists ──────────────────────────────────────────────────────────
        methods.add_method("getAllDurations", |lua, this, ()| {
            const MAX: usize = 200;
            let mut ptrs: Vec<*const i8> = vec![std::ptr::null(); MAX];
            let count = this.with_sd(0, |sd| unsafe { sl_pc_getalldurations(sd, ptrs.as_mut_ptr(), MAX as i32) }) as usize;
            let tbl = lua.create_table()?;
            for (i, &p) in ptrs[..count].iter().enumerate() {
                if !p.is_null() {
                    let s = unsafe { CStr::from_ptr(p).to_str().unwrap_or("") };
                    tbl.raw_set(i + 1, s)?;
                }
            }
            Ok(tbl)
        });
        methods.add_method("getSpells", |lua, this, ()| {
            const MAX: usize = 52;
            let mut ids: Vec<i32> = vec![0; MAX];
            let count = this.with_sd(0, |sd| unsafe { sl_pc_getspells(sd, ids.as_mut_ptr(), MAX as i32) }) as usize;
            let tbl = lua.create_table()?;
            for (i, &id) in ids[..count].iter().enumerate() { tbl.raw_set(i + 1, id as i64)?; }
            Ok(tbl)
        });
        methods.add_method("getSpellName", |lua, this, ()| {
            const MAX: usize = 52;
            let mut ptrs: Vec<*const i8> = vec![std::ptr::null(); MAX];
            let count = this.with_sd(0, |sd| unsafe { sl_pc_getspellnames(sd, ptrs.as_mut_ptr(), MAX as i32) }) as usize;
            let tbl = lua.create_table()?;
            for (i, &p) in ptrs[..count].iter().enumerate() {
                if !p.is_null() {
                    let s = unsafe { CStr::from_ptr(p).to_str().unwrap_or("") };
                    tbl.raw_set(i + 1, s)?;
                }
            }
            Ok(tbl)
        });
        methods.add_method("getUnknownSpells", |lua, this, ()| {
            const MAX: usize = 52;
            let mut ids: Vec<i32> = vec![0; MAX];
            let count = this.with_sd(0, |sd| unsafe { sl_pc_getunknownspells(sd, ids.as_mut_ptr(), MAX as i32) }) as usize;
            let tbl = lua.create_table()?;
            for (i, &id) in ids[..count].iter().enumerate() { tbl.raw_set(i + 1, id as i64)?; }
            Ok(tbl)
        });

        // ── Legends ──────────────────────────────────────────────────────────────
        methods.add_method("getLegend", |lua, this, name: String| {
            let cs = CString::new(name.as_bytes()).ok();
            let p = cs.map_or(std::ptr::null(), |c| this.with_sd(std::ptr::null(), |sd| unsafe { sl_pc_getlegend(sd, c.as_ptr()) }));
            unsafe { cstr_to_lua(lua, p) }
        });

        // ── Combat ───────────────────────────────────────────────────────────────
        methods.add_method("giveXP",        |_, this, amount: i32| { this.with_sd((), |sd| unsafe { sl_pc_givexp(sd, amount as u32) }); Ok(()) });
        methods.add_method("updateState",   |_, this, ()| { if let Some(arc) = crate::game::map_server::map_id2sd_pc(this.id) { unsafe { broadcast_update_state(&arc) } }; Ok(()) });
        methods.add_method("addMagic",      |_, this, amount: i32| { this.with_sd((), |sd| unsafe { sl_pc_addmagic(sd, amount) }); Ok(()) });
        methods.add_method("addManaExtend", |_, this, amount: i32| { this.with_sd((), |sd| unsafe { sl_pc_addmanaextend(sd, amount) }); Ok(()) });
        methods.add_method("setTimeValues", |_, this, newval: i32| { this.with_sd((), |sd| sl_pc_settimevalues(sd, newval as u32)); Ok(()) });
        methods.add_method("setPK",         |_, this, id: i32| { this.with_sd((), |sd| unsafe { sl_pc_setpk(sd, id) }); Ok(()) });
        methods.add_method("activeSpells",  |_, this, name: String| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| this.with_sd(0, |sd| unsafe { sl_pc_activespells(sd, c.as_ptr()) })) != 0)
        });
        methods.add_method("getEquippedDura", |_, this, (id, slot): (i32, i32)| {
            Ok(this.with_sd(0, |sd| sl_pc_getequippeddura(sd, id as u32, slot)))
        });
        methods.add_method("addHealthExtend", |_, this, (dmg, _sleep, _deduct, _ac, _ds, _print): (i32, i32, i32, i32, i32, i32)| {
            this.with_sd((), |sd| unsafe { sl_pc_addhealth_extend(sd, dmg) }); Ok(())
        });
        methods.add_method("removeHealthExtend", |_, this, (dmg, _sleep, _deduct, _ac, _ds, _print): (i32, i32, i32, i32, i32, i32)| {
            this.with_sd((), |sd| unsafe { sl_pc_removehealth_extend(sd, dmg) }); Ok(())
        });
        methods.add_method("addHealth2", |_, this, (amount, typ): (i32, i32)| {
            this.with_sd((), |sd| unsafe { sl_pc_addhealth2(sd, amount, typ) }); Ok(())
        });
        methods.add_method("removeHealthWithoutDamageNumbers", |_, this, (dmg, typ): (i32, i32)| {
            this.with_sd((), |sd| unsafe { sl_pc_removehealth_nodmgnum(sd, dmg, typ) }); Ok(())
        });

        // ── Economy ──────────────────────────────────────────────────────────────
        methods.add_method("addGold",    |_, this, amount: i32| { this.with_sd((), |sd| unsafe { sl_pc_addgold(sd, amount) }); Ok(()) });
        methods.add_method("removeGold", |_, this, amount: i32| { this.with_sd((), |sd| unsafe { sl_pc_removegold(sd, amount) }); Ok(()) });
        methods.add_method("logBuySell", |_, this, (item, amount, gold, flag): (i32, i32, i32, i32)| {
            this.with_sd((), |sd| sl_pc_logbuysell(sd, item as u32, amount as u32, gold as u32, flag)); Ok(())
        });

        // ── Ranged ───────────────────────────────────────────────────────────────
        methods.add_method("calcThrow", |_, this, ()| { this.with_sd((), sl_pc_calcthrow); Ok(()) });
        methods.add_method("calcRangedDamage", |_, this, bl: mlua::AnyUserData| {
            let bl_ptr = extract_bl_ptr(&bl);
            if bl_ptr.is_null() {
                return Err(mlua::Error::external("calcRangedDamage: bl pointer is null"));
            }
            Ok(this.with_sd(0, |sd| sl_pc_calcrangeddamage(sd, bl_ptr)))
        });
        methods.add_method("calcRangedHit", |_, this, bl: mlua::AnyUserData| {
            let bl_ptr = extract_bl_ptr(&bl);
            if bl_ptr.is_null() {
                return Err(mlua::Error::external("calcRangedHit: bl pointer is null"));
            }
            Ok(this.with_sd(0, |sd| sl_pc_calcrangedhit(sd, bl_ptr)))
        });

        // ── Misc ─────────────────────────────────────────────────────────────────
        methods.add_method("talkSelf", |_, this, (color, msg): (i32, String)| {
            if let Ok(cs) = CString::new(msg.as_bytes()) {
                this.with_sd((), |sd| unsafe { sl_pc_talkself(sd, color, cs.as_ptr()) });
            }
            Ok(())
        });
        methods.add_method("gmMsg", |_, this, msg: String| {
            if let Ok(cs) = CString::new(msg.as_bytes()) {
                this.with_sd((), |sd| unsafe { sl_pc_gmmsg(sd, cs.as_ptr()) });
            }
            Ok(())
        });
        methods.add_method("broadcast", |_, this, (msg, m): (String, i32)| {
            if let Ok(cs) = CString::new(msg.as_bytes()) {
                this.with_sd((), |sd| unsafe { sl_pc_broadcast_sd(sd, cs.as_ptr(), m) });
            }
            Ok(())
        });
        methods.add_method("killRank", |_, this, mob_id: i32| Ok(this.with_sd(0, |sd| sl_pc_killrank(sd, mob_id))));
        methods.add_method("getParcel", |lua, this, ()| {
            let ptr = this.with_sd(std::ptr::null_mut(), |sd| unsafe { sl_pc_getparcel(sd) });
            if ptr.is_null() { return Ok(mlua::Value::Nil); }
            Ok(mlua::Value::UserData(lua.create_userdata(
                crate::game::scripting::types::item::ParcelObject { ptr }
            )?))
        });
        methods.add_method("getParcelList", |lua, this, ()| {
            const MAX: usize = 64;
            let mut ptrs: Vec<*mut std::ffi::c_void> = vec![std::ptr::null_mut(); MAX];
            let count = this.with_sd(0, |sd| unsafe { sl_pc_getparcellist(sd, ptrs.as_mut_ptr(), MAX as i32) }) as usize;
            let tbl = lua.create_table()?;
            for (i, &p) in ptrs[..count].iter().enumerate() {
                if !p.is_null() {
                    tbl.raw_set(i + 1, lua.create_userdata(
                        crate::game::scripting::types::item::ParcelObject { ptr: p }
                    )?)?;
                }
            }
            Ok(tbl)
        });
        methods.add_method("removeParcel", |_, this, (sender, item, amount, pos, owner, engrave, npcflag): (i32, i32, i32, i32, i32, String, i32)| {
            if let Ok(cs) = CString::new(engrave.as_bytes()) {
                this.with_sd((), |sd| unsafe { sl_pc_removeparcel(sd, pos, crate::game::scripting::map_globals::ParcelSpec { sender, item, amount, owner, engrave: cs.as_ptr(), npcflag }) });
            }
            Ok(())
        });
        methods.add_method("expireItem", |_, this, ()| { this.with_sd((), |sd| unsafe { sl_pc_expireitem(sd) }); Ok(()) });
        methods.add_method("addGuide", |_, this, _guide: String| {
            this.with_sd((), |sd| sl_pc_addguide(sd, 0)); Ok(())
        });
        methods.add_method("delGuide", |_, this, _guide: String| {
            this.with_sd((), |sd| sl_pc_delguide(sd, 0)); Ok(())
        });
        methods.add_method("mapSelection", |_, _this, _: mlua::MultiValue| Ok(mlua::Value::Nil));
        methods.add_method("getCreationItems", |lua, this, len: i32| {
            let max = (len.max(0) as usize).min(52);
            let mut out: Vec<u32> = vec![0; max.max(1)];
            let count = this.with_sd(0, |sd| unsafe { sl_pc_getcreationitems(sd, len, out.as_mut_ptr()) }) as usize;
            let tbl = lua.create_table()?;
            for (i, &v) in out[..count.min(max)].iter().enumerate() { tbl.raw_set(i + 1, v as i64)?; }
            Ok(tbl)
        });
        methods.add_method("getCreationAmounts", |_, this, (len, item_id): (i32, i32)| {
            Ok(this.with_sd(0, |sd| unsafe { sl_pc_getcreationamounts(sd, len, item_id as u32) }))
        });

        // ── Coroutine-based NPC interaction methods ─────────────────────────

        // ── Packet-send methods for coroutine-based NPC interactions ──────
        //
        // These methods ONLY send the packet to the client and return
        // immediately.  The Lua-side wrapper (in sys.lua) calls
        // coroutine.yield() after calling the _send method, and the
        // response is delivered by resuming the thread from Rust via
        // thread_registry::resume().

        methods.add_method("_input_send", |_, this, msg: String| {
            let cs = CString::new(msg.as_bytes()).map_err(mlua::Error::external)?;
            this.with_sd((), |sd| unsafe { sl_pc_input_send(sd, cs.as_ptr()) });
            Ok(())
        });

        // inputSeq sends no packet — the client is already in sequential-input
        // mode.  The _send method is a no-op; the yield waits for the response.
        methods.add_method("_inputSeq_send", |_, _this, ()| {
            Ok(())
        });

        methods.add_method("_dialog_send", |_, this, (msg, opts_tbl): (String, mlua::Table)| {
            let mut has_prev = 0i32;
            let mut has_next = 0i32;
            let len = opts_tbl.raw_len();
            for i in 1..=len {
                if let Ok(s) = opts_tbl.raw_get::<String>(i) {
                    match s.as_str() {
                        "previous" => has_prev = 1,
                        "next" => has_next = 1,
                        _ => {}
                    }
                }
            }
            let cs = CString::new(msg.as_bytes()).map_err(mlua::Error::external)?;
            this.with_sd((), |sd| unsafe { sl_pc_dialog_send(sd, cs.as_ptr(), has_prev, has_next) });
            Ok(())
        });

        methods.add_method("_dialogSeq_send", |lua, this, args: mlua::MultiValue| {
            let entries_tbl: mlua::Table = args.front()
                .and_then(|v| lua.unpack::<mlua::Table>(v.clone()).ok())
                .unwrap_or_else(|| lua.create_table().unwrap());
            let can_continue = args.get(1)
                .map(|v| match v {
                    mlua::Value::Boolean(b) => *b,
                    mlua::Value::Integer(n) => *n != 0,
                    mlua::Value::Number(n) => *n != 0.0,
                    _ => false,
                })
                .unwrap_or(false);
            let strs = lua_table_to_cstrings_from(&entries_tbl, 2).unwrap_or_default();
            let ptrs = cstring_ptrs(&strs);
            this.with_sd((), |sd| unsafe { sl_pc_dialogseq_send(sd, ptrs.as_ptr(), ptrs.len() as i32, can_continue as i32) });
            Ok(())
        });

        methods.add_method("_menu_send", |_, this, (msg, opts_tbl): (String, mlua::Table)| {
            let cs = CString::new(msg.as_bytes()).map_err(mlua::Error::external)?;
            let strs = lua_table_to_cstrings(&opts_tbl).unwrap_or_default();
            let ptrs = cstring_ptrs(&strs);
            this.with_sd((), |sd| unsafe { sl_pc_menu_send(sd, cs.as_ptr(), ptrs.as_ptr(), ptrs.len() as i32) });
            Ok(())
        });

        methods.add_method("_menuSeq_send", |_, this, (msg, opts_tbl): (String, mlua::Table)| {
            let cs = CString::new(msg.as_bytes()).map_err(mlua::Error::external)?;
            let strs = lua_table_to_cstrings(&opts_tbl).unwrap_or_default();
            let ptrs = cstring_ptrs(&strs);
            this.with_sd((), |sd| unsafe { sl_pc_menuseq_send(sd, cs.as_ptr(), ptrs.as_ptr(), ptrs.len() as i32) });
            Ok(())
        });

        methods.add_method("_menuString_send", |_, this, (msg, opts_tbl): (String, mlua::Table)| {
            let cs = CString::new(msg.as_bytes()).map_err(mlua::Error::external)?;
            let strs = lua_table_to_cstrings(&opts_tbl).unwrap_or_default();
            let strings: Vec<String> = strs.iter()
                .map(|c| c.to_str().unwrap_or("").to_owned())
                .collect();
            let ptrs = cstring_ptrs(&strs);
            this.with_sd((), |sd| unsafe {
                sl_pc_menustring_send(sd, cs.as_ptr(), ptrs.as_ptr(), ptrs.len() as i32);
                crate::game::scripting::async_coro::store_menu_opts(sd as *const _ as *mut std::ffi::c_void, strings);
            });
            Ok(())
        });

        methods.add_method("_menuString2_send", |_, this, (msg, opts_tbl): (String, mlua::Table)| {
            let cs = CString::new(msg.as_bytes()).map_err(mlua::Error::external)?;
            let strs = lua_table_to_cstrings(&opts_tbl).unwrap_or_default();
            let strings: Vec<String> = strs.iter()
                .map(|c| c.to_str().unwrap_or("").to_owned())
                .collect();
            let ptrs = cstring_ptrs(&strs);
            this.with_sd((), |sd| {
                sl_pc_menustring2_send(sd, cs.as_ptr(), ptrs.as_ptr(), ptrs.len() as i32);
                crate::game::scripting::async_coro::store_menu_opts(sd as *const _ as *mut std::ffi::c_void, strings);
            });
            Ok(())
        });

        methods.add_method("_buy_send", |_, this, (msg, items_tbl, values_tbl, dn_tbl, bt_tbl):
            (String, mlua::Table, mlua::Table, mlua::Table, mlua::Table)| {
            let cs = CString::new(msg.as_bytes()).map_err(mlua::Error::external)?;
            let items  = lua_table_to_ints(&items_tbl).unwrap_or_default();
            let values = lua_table_to_ints(&values_tbl).unwrap_or_default();
            let dn     = lua_table_to_cstrings(&dn_tbl).unwrap_or_default();
            let bt     = lua_table_to_cstrings(&bt_tbl).unwrap_or_default();
            let dn_p = cstring_ptrs(&dn);
            let bt_p = cstring_ptrs(&bt);
            this.with_sd((), |sd| unsafe {
                sl_pc_buy_send(sd, cs.as_ptr(),
                    items.as_ptr(), values.as_ptr(),
                    dn_p.as_ptr(), bt_p.as_ptr(), items.len() as i32);
            });
            Ok(())
        });

        methods.add_method("_buyDialog_send", |_, this, (msg, items_tbl): (String, mlua::Table)| {
            let cs = CString::new(msg.as_bytes()).map_err(mlua::Error::external)?;
            let items = lua_table_to_ints(&items_tbl).unwrap_or_default();
            this.with_sd((), |sd| unsafe { sl_pc_buydialog_send(sd, cs.as_ptr(), items.as_ptr(), items.len() as i32) });
            Ok(())
        });

        methods.add_method("_buyExtend_send", |_, this, (msg, items_tbl, prices_tbl, max_tbl):
            (String, mlua::Table, mlua::Table, mlua::Table)| {
            let cs = CString::new(msg.as_bytes()).map_err(mlua::Error::external)?;
            let items  = lua_table_to_ints(&items_tbl).unwrap_or_default();
            let prices = lua_table_to_ints(&prices_tbl).unwrap_or_default();
            let maxs   = lua_table_to_ints(&max_tbl).unwrap_or_default();
            this.with_sd((), |sd| unsafe {
                sl_pc_buyextend_send(sd, cs.as_ptr(),
                    items.as_ptr(), prices.as_ptr(), maxs.as_ptr(), items.len() as i32);
            });
            Ok(())
        });

        methods.add_method("_sell_send", |_, this, (msg, items_tbl): (String, mlua::Table)| {
            let cs = CString::new(msg.as_bytes()).map_err(mlua::Error::external)?;
            let items = lua_table_to_ints(&items_tbl).unwrap_or_default();
            this.with_sd((), |sd| unsafe { sl_pc_sell_send(sd, cs.as_ptr(), items.as_ptr(), items.len() as i32) });
            Ok(())
        });

        methods.add_method("_sell2_send", |_, this, (msg, items_tbl): (String, mlua::Table)| {
            let cs = CString::new(msg.as_bytes()).map_err(mlua::Error::external)?;
            let items = lua_table_to_ints(&items_tbl).unwrap_or_default();
            this.with_sd((), |sd| unsafe { sl_pc_sell2_send(sd, cs.as_ptr(), items.as_ptr(), items.len() as i32) });
            Ok(())
        });

        methods.add_method("_sellExtend_send", |_, this, (msg, items_tbl): (String, mlua::Table)| {
            let cs = CString::new(msg.as_bytes()).map_err(mlua::Error::external)?;
            let items = lua_table_to_ints(&items_tbl).unwrap_or_default();
            this.with_sd((), |sd| unsafe { sl_pc_sellextend_send(sd, cs.as_ptr(), items.as_ptr(), items.len() as i32) });
            Ok(())
        });

        methods.add_method("_showBank_send", |_, this, msg: String| {
            let cs = CString::new(msg.as_bytes()).map_err(mlua::Error::external)?;
            this.with_sd((), |sd| sl_pc_showbank_send(sd, cs.as_ptr()));
            Ok(())
        });

        methods.add_method("_showBankAdd_send", |_, this, ()| {
            this.with_sd((), sl_pc_showbankadd_send); Ok(())
        });

        methods.add_method("_bankAddMoney_send", |_, this, ()| {
            this.with_sd((), sl_pc_bankaddmoney_send); Ok(())
        });

        methods.add_method("_bankWithdrawMoney_send", |_, this, ()| {
            this.with_sd((), sl_pc_bankwithdrawmoney_send); Ok(())
        });

        methods.add_method("_clanShowBank_send", |_, this, msg: String| {
            let cs = CString::new(msg.as_bytes()).map_err(mlua::Error::external)?;
            this.with_sd((), |sd| sl_pc_clanshowbank_send(sd, cs.as_ptr()));
            Ok(())
        });

        methods.add_method("_clanShowBankAdd_send", |_, this, ()| {
            this.with_sd((), sl_pc_clanshowbankadd_send); Ok(())
        });

        methods.add_method("_clanBankAddMoney_send", |_, this, ()| {
            this.with_sd((), sl_pc_clanbankaddmoney_send); Ok(())
        });

        methods.add_method("_clanBankWithdrawMoney_send", |_, this, ()| {
            this.with_sd((), sl_pc_clanbankwithdrawmoney_send); Ok(())
        });

        methods.add_method("_clanViewBank_send", |_, this, ()| {
            this.with_sd((), sl_pc_clanviewbank_send); Ok(())
        });

        methods.add_method("_repairExtend_send", |_, this, ()| {
            this.with_sd((), sl_pc_repairextend_send); Ok(())
        });

        methods.add_method("_repairAll_send", |_, this, npc_bl: mlua::AnyUserData| {
            let npc_ptr = if let Ok(npc) = npc_bl.borrow::<crate::game::scripting::types::npc::NpcObject>() {
                crate::game::map_server::map_id2npc_ref(npc.id)
                    .map(|arc| arc.legacy.data_ptr() as *mut std::ffi::c_void)
                    .unwrap_or(std::ptr::null_mut())
            } else {
                std::ptr::null_mut()
            };
            this.with_sd((), |sd| sl_pc_repairall_send(sd, npc_ptr));
            Ok(())
        });
    }
}

fn extract_bl_ptr(ud: &mlua::AnyUserData) -> *mut std::ffi::c_void {
    if let Ok(pc) = ud.borrow::<PcObject>() { return pc.sd_ptr_raw() as *mut std::ffi::c_void; }
    if let Ok(mob) = ud.borrow::<crate::game::scripting::types::mob::MobObject>() {
        return crate::game::map_server::map_id2mob_ref(mob.id)
            .map(|arc| arc.legacy.data_ptr() as *mut std::ffi::c_void)
            .unwrap_or(std::ptr::null_mut());
    }
    if let Ok(npc) = ud.borrow::<crate::game::scripting::types::npc::NpcObject>() {
        return crate::game::map_server::map_id2npc_ref(npc.id)
            .map(|arc| arc.legacy.data_ptr() as *mut std::ffi::c_void)
            .unwrap_or(std::ptr::null_mut());
    }
    std::ptr::null_mut()
}
