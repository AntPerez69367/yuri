//! PcObject — Lua UserData wrapping a C `USER*` player pointer.
//! Mirrors the `pcl_*` class from `c_src/scripting.c`.
#![allow(unused_variables)]

use mlua::{MetaMethod, UserData, UserDataMethods};
use std::ffi::{c_char, CStr, CString};
use std::os::raw::{c_float, c_int, c_uint, c_void};

use crate::database::map_db::BlockList;
use crate::game::scripting::ffi as sffi;
use crate::game::scripting::types::mob::MobObject;
use crate::game::scripting::types::npc::NpcObject;
use crate::game::scripting::types::registry::{
    GameRegObject, MapRegObject, NpcRegObject, QuestRegObject, RegObject, RegStringObject,
};
use crate::game::scripting::types::shared;

pub struct PcObject {
    pub ptr: *mut c_void,
}
unsafe impl Send for PcObject {}

fn val_to_int(v: &mlua::Value) -> c_int {
    match v {
        mlua::Value::Integer(i) => *i as c_int,
        mlua::Value::Number(f) => *f as c_int,
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

unsafe fn cstr_to_lua(lua: &mlua::Lua, p: *const c_char) -> mlua::Result<mlua::Value> {
    if p.is_null() {
        return Ok(mlua::Value::Nil);
    }
    let s = CStr::from_ptr(p).to_str().unwrap_or("");
    Ok(mlua::Value::String(lua.create_string(s)?))
}

// ─── C accessor externs (declared in sl_compat.c) ────────────────────────────
extern "C" {
    // vitals
    fn sl_pc_status_id(sd: *mut c_void) -> c_int;
    fn sl_pc_status_hp(sd: *mut c_void) -> c_int;
    fn sl_pc_status_mp(sd: *mut c_void) -> c_int;
    fn sl_pc_status_level(sd: *mut c_void) -> c_int;
    fn sl_pc_status_exp(sd: *mut c_void) -> c_int;
    fn sl_pc_status_expsoldmagic(sd: *mut c_void) -> c_int;
    fn sl_pc_status_expsoldhealth(sd: *mut c_void) -> c_int;
    fn sl_pc_status_expsoldstats(sd: *mut c_void) -> c_int;
    fn sl_pc_status_class(sd: *mut c_void) -> c_int;
    fn sl_pc_status_totem(sd: *mut c_void) -> c_int;
    fn sl_pc_status_tier(sd: *mut c_void) -> c_int;
    fn sl_pc_status_mark(sd: *mut c_void) -> c_int;
    fn sl_pc_status_country(sd: *mut c_void) -> c_int;
    fn sl_pc_status_clan(sd: *mut c_void) -> c_int;
    fn sl_pc_status_gm_level(sd: *mut c_void) -> c_int;
    fn sl_pc_status_sex(sd: *mut c_void) -> c_int;
    fn sl_pc_status_side(sd: *mut c_void) -> c_int;
    fn sl_pc_status_state(sd: *mut c_void) -> c_int;
    fn sl_pc_status_face(sd: *mut c_void) -> c_int;
    fn sl_pc_status_hair(sd: *mut c_void) -> c_int;
    fn sl_pc_status_hair_color(sd: *mut c_void) -> c_int;
    fn sl_pc_status_face_color(sd: *mut c_void) -> c_int;
    fn sl_pc_status_armor_color(sd: *mut c_void) -> c_int;
    fn sl_pc_status_skin_color(sd: *mut c_void) -> c_int;
    fn sl_pc_status_basehp(sd: *mut c_void) -> c_int;
    fn sl_pc_status_basemp(sd: *mut c_void) -> c_int;
    fn sl_pc_status_money(sd: *mut c_void) -> c_int;
    fn sl_pc_status_bankmoney(sd: *mut c_void) -> c_int;
    fn sl_pc_status_maxslots(sd: *mut c_void) -> c_int;
    fn sl_pc_status_maxinv(sd: *mut c_void) -> c_int;
    fn sl_pc_status_partner(sd: *mut c_void) -> c_int;
    fn sl_pc_status_pk(sd: *mut c_void) -> c_int;
    fn sl_pc_status_killedby(sd: *mut c_void) -> c_int;
    fn sl_pc_status_killspk(sd: *mut c_void) -> c_int;
    fn sl_pc_status_pkduration(sd: *mut c_void) -> c_int;
    fn sl_pc_status_basegrace(sd: *mut c_void) -> c_int;
    fn sl_pc_status_basemight(sd: *mut c_void) -> c_int;
    fn sl_pc_status_basewill(sd: *mut c_void) -> c_int;
    fn sl_pc_status_basearmor(sd: *mut c_void) -> c_int;
    fn sl_pc_status_tutor(sd: *mut c_void) -> c_int;
    fn sl_pc_status_karma(sd: *mut c_void) -> c_int;
    fn sl_pc_status_alignment(sd: *mut c_void) -> c_int;
    fn sl_pc_status_classRank(sd: *mut c_void) -> c_int;
    fn sl_pc_status_clanRank(sd: *mut c_void) -> c_int;
    fn sl_pc_status_novice_chat(sd: *mut c_void) -> c_int;
    fn sl_pc_status_subpath_chat(sd: *mut c_void) -> c_int;
    fn sl_pc_status_clan_chat(sd: *mut c_void) -> c_int;
    fn sl_pc_status_miniMapToggle(sd: *mut c_void) -> c_int;
    fn sl_pc_status_heroes(sd: *mut c_void) -> c_int;
    fn sl_pc_status_mute(sd: *mut c_void) -> c_int;
    fn sl_pc_status_settingFlags(sd: *mut c_void) -> c_int;
    fn sl_pc_status_killspvp(sd: *mut c_void) -> c_int;
    fn sl_pc_status_profile_vitastats(sd: *mut c_void) -> c_int;
    fn sl_pc_status_profile_equiplist(sd: *mut c_void) -> c_int;
    fn sl_pc_status_profile_legends(sd: *mut c_void) -> c_int;
    fn sl_pc_status_profile_spells(sd: *mut c_void) -> c_int;
    fn sl_pc_status_profile_inventory(sd: *mut c_void) -> c_int;
    fn sl_pc_status_profile_bankitems(sd: *mut c_void) -> c_int;
    fn sl_pc_status_name(sd: *mut c_void) -> *const c_char;
    fn sl_pc_status_title(sd: *mut c_void) -> *const c_char;
    fn sl_pc_status_clan_title(sd: *mut c_void) -> *const c_char;
    fn sl_pc_status_afkmessage(sd: *mut c_void) -> *const c_char;
    fn sl_pc_status_f1name(sd: *mut c_void) -> *const c_char;
    // direct fields
    fn sl_pc_bl_id(sd: *mut c_void) -> c_int;
    fn sl_pc_bl_m(sd: *mut c_void) -> c_int;
    fn sl_pc_bl_x(sd: *mut c_void) -> c_int;
    fn sl_pc_bl_y(sd: *mut c_void) -> c_int;
    fn sl_pc_bl_type(sd: *mut c_void) -> c_int;
    fn sl_pc_groupid(sd: *mut c_void) -> c_int;
    fn sl_pc_npc_g(sd: *mut c_void) -> c_int;
    fn sl_pc_npc_gc(sd: *mut c_void) -> c_int;
    fn sl_pc_time(sd: *mut c_void) -> c_int;
    fn sl_pc_fakeDrop(sd: *mut c_void) -> c_int;
    fn sl_pc_max_hp(sd: *mut c_void) -> c_int;
    fn sl_pc_max_mp(sd: *mut c_void) -> c_int;
    fn sl_pc_lastvita(sd: *mut c_void) -> c_int;
    fn sl_pc_rage(sd: *mut c_void) -> c_int;
    fn sl_pc_polearm(sd: *mut c_void) -> c_int;
    fn sl_pc_last_click(sd: *mut c_void) -> c_int;
    fn sl_pc_grace(sd: *mut c_void) -> c_int;
    fn sl_pc_might(sd: *mut c_void) -> c_int;
    fn sl_pc_will(sd: *mut c_void) -> c_int;
    fn sl_pc_armor(sd: *mut c_void) -> c_int;
    fn sl_pc_dam(sd: *mut c_void) -> c_int;
    fn sl_pc_hit(sd: *mut c_void) -> c_int;
    fn sl_pc_miss(sd: *mut c_void) -> c_int;
    fn sl_pc_sleep(sd: *mut c_void) -> c_int;
    fn sl_pc_attack_speed(sd: *mut c_void) -> c_int;
    fn sl_pc_enchanted(sd: *mut c_void) -> c_int;
    fn sl_pc_confused(sd: *mut c_void) -> c_int;
    fn sl_pc_target(sd: *mut c_void) -> c_int;
    fn sl_pc_set_target(sd: *mut c_void, v: c_int);
    fn sl_pc_deduction(sd: *mut c_void) -> c_int;
    fn sl_pc_speed(sd: *mut c_void) -> c_int;
    fn sl_pc_disguise(sd: *mut c_void) -> c_int;
    fn sl_pc_disguise_color(sd: *mut c_void) -> c_int;
    fn sl_pc_attacker(sd: *mut c_void) -> c_int;
    fn sl_pc_invis(sd: *mut c_void) -> c_int;
    fn sl_pc_damage(sd: *mut c_void) -> c_int;
    fn sl_pc_crit(sd: *mut c_void) -> c_int;
    fn sl_pc_critchance(sd: *mut c_void) -> c_int;
    fn sl_pc_critmult(sd: *mut c_void) -> c_int;
    fn sl_pc_rangeTarget(sd: *mut c_void) -> c_int;
    fn sl_pc_exchange_gold(sd: *mut c_void) -> c_int;
    fn sl_pc_exchange_count(sd: *mut c_void) -> c_int;
    fn sl_pc_bod_count(sd: *mut c_void) -> c_int;
    fn sl_pc_paralyzed(sd: *mut c_void) -> c_int;
    fn sl_pc_blind(sd: *mut c_void) -> c_int;
    fn sl_pc_drunk(sd: *mut c_void) -> c_int;
    fn sl_pc_board(sd: *mut c_void) -> c_int;
    fn sl_pc_board_candel(sd: *mut c_void) -> c_int;
    fn sl_pc_board_canwrite(sd: *mut c_void) -> c_int;
    fn sl_pc_boardshow(sd: *mut c_void) -> c_int;
    fn sl_pc_boardnameval(sd: *mut c_void) -> c_int;
    fn sl_pc_msPing(sd: *mut c_void) -> c_int;
    fn sl_pc_pbColor(sd: *mut c_void) -> c_int;
    fn sl_pc_coref(sd: *mut c_void) -> c_int;
    fn sl_pc_optFlags(sd: *mut c_void) -> c_int;
    fn sl_pc_snare(sd: *mut c_void) -> c_int;
    fn sl_pc_silence(sd: *mut c_void) -> c_int;
    fn sl_pc_extendhit(sd: *mut c_void) -> c_int;
    fn sl_pc_afk(sd: *mut c_void) -> c_int;
    fn sl_pc_afktime(sd: *mut c_void) -> c_int;
    fn sl_pc_totalafktime(sd: *mut c_void) -> c_int;
    fn sl_pc_backstab(sd: *mut c_void) -> c_int;
    fn sl_pc_flank(sd: *mut c_void) -> c_int;
    fn sl_pc_healing(sd: *mut c_void) -> c_int;
    fn sl_pc_minSdam(sd: *mut c_void) -> c_int;
    fn sl_pc_maxSdam(sd: *mut c_void) -> c_int;
    fn sl_pc_minLdam(sd: *mut c_void) -> c_int;
    fn sl_pc_maxLdam(sd: *mut c_void) -> c_int;
    fn sl_pc_talktype(sd: *mut c_void) -> c_int;
    fn sl_pc_equipid(sd: *mut c_void) -> c_int;
    fn sl_pc_takeoffid(sd: *mut c_void) -> c_int;
    fn sl_pc_breakid(sd: *mut c_void) -> c_int;
    fn sl_pc_equipslot(sd: *mut c_void) -> c_int;
    fn sl_pc_invslot(sd: *mut c_void) -> c_int;
    fn sl_pc_pickuptype(sd: *mut c_void) -> c_int;
    fn sl_pc_spottraps(sd: *mut c_void) -> c_int;
    fn sl_pc_fury(sd: *mut c_void) -> c_int;
    fn sl_pc_faceacctwo_id(sd: *mut c_void) -> c_int;
    fn sl_pc_faceacctwo_custom(sd: *mut c_void) -> c_int;
    fn sl_pc_protection(sd: *mut c_void) -> c_int;
    fn sl_pc_clone(sd: *mut c_void) -> c_int;
    fn sl_pc_wisdom(sd: *mut c_void) -> c_int;
    fn sl_pc_con(sd: *mut c_void) -> c_int;
    fn sl_pc_deathflag(sd: *mut c_void) -> c_int;
    fn sl_pc_selfbar(sd: *mut c_void) -> c_int;
    fn sl_pc_groupbars(sd: *mut c_void) -> c_int;
    fn sl_pc_mobbars(sd: *mut c_void) -> c_int;
    fn sl_pc_disptimertick(sd: *mut c_void) -> c_int;
    fn sl_pc_bindmap(sd: *mut c_void) -> c_int;
    fn sl_pc_bindx(sd: *mut c_void) -> c_int;
    fn sl_pc_bindy(sd: *mut c_void) -> c_int;
    fn sl_pc_ambushtimer(sd: *mut c_void) -> c_int;
    fn sl_pc_dialogtype(sd: *mut c_void) -> c_int;
    fn sl_pc_cursed(sd: *mut c_void) -> c_int;
    fn sl_pc_action(sd: *mut c_void) -> c_int;
    fn sl_pc_scripttick(sd: *mut c_void) -> c_int;
    fn sl_pc_dmgshield(sd: *mut c_void) -> c_int;
    fn sl_pc_dmgdealt(sd: *mut c_void) -> c_int;
    fn sl_pc_dmgtaken(sd: *mut c_void) -> c_int;
    fn sl_pc_ipaddress(sd: *mut c_void) -> *const c_char;
    fn sl_pc_speech(sd: *mut c_void) -> *const c_char;
    fn sl_pc_question(sd: *mut c_void) -> *const c_char;
    fn sl_pc_mail(sd: *mut c_void) -> *const c_char;
    // gfx
    fn sl_pc_gfx_face(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_hair(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_chair(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_cface(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_cskin(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_dye(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_weapon(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_cweapon(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_armor(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_carmor(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_shield(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_cshield(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_helm(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_chelm(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_mantle(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_cmantle(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_crown(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_ccrown(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_faceAcc(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_cfaceAcc(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_faceAccT(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_cfaceAccT(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_boots(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_cboots(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_necklace(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_cnecklace(sd: *mut c_void) -> c_int;
    fn sl_pc_gfx_name(sd: *mut c_void) -> *const c_char;
    // computed
    fn sl_pc_actid(sd: *mut c_void) -> c_int;
    fn sl_pc_email(sd: *mut c_void) -> *const c_char;
    fn sl_pc_clanname(sd: *mut c_void) -> *const c_char;
    fn sl_pc_baseclass(sd: *mut c_void) -> c_int;
    fn sl_pc_baseClassName(sd: *mut c_void) -> *const c_char;
    fn sl_pc_className(sd: *mut c_void) -> *const c_char;
    fn sl_pc_classNameMark(sd: *mut c_void) -> *const c_char;
    // setters — integers
    fn sl_pc_set_hp(sd: *mut c_void, v: c_int);
    fn sl_pc_set_mp(sd: *mut c_void, v: c_int);
    fn sl_pc_set_max_hp(sd: *mut c_void, v: c_int);
    fn sl_pc_set_max_mp(sd: *mut c_void, v: c_int);
    fn sl_pc_set_exp(sd: *mut c_void, v: c_int);
    fn sl_pc_set_level(sd: *mut c_void, v: c_int);
    fn sl_pc_set_class(sd: *mut c_void, v: c_int);
    fn sl_pc_set_totem(sd: *mut c_void, v: c_int);
    fn sl_pc_set_tier(sd: *mut c_void, v: c_int);
    fn sl_pc_set_mark(sd: *mut c_void, v: c_int);
    fn sl_pc_set_country(sd: *mut c_void, v: c_int);
    fn sl_pc_set_clan(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gm_level(sd: *mut c_void, v: c_int);
    fn sl_pc_set_side(sd: *mut c_void, v: c_int);
    fn sl_pc_set_state(sd: *mut c_void, v: c_int);
    fn sl_pc_set_hair(sd: *mut c_void, v: c_int);
    fn sl_pc_set_hair_color(sd: *mut c_void, v: c_int);
    fn sl_pc_set_face_color(sd: *mut c_void, v: c_int);
    fn sl_pc_set_armor_color(sd: *mut c_void, v: c_int);
    fn sl_pc_set_skin_color(sd: *mut c_void, v: c_int);
    fn sl_pc_set_face(sd: *mut c_void, v: c_int);
    fn sl_pc_set_money(sd: *mut c_void, v: c_int);
    fn sl_pc_set_bankmoney(sd: *mut c_void, v: c_int);
    fn sl_pc_set_maxslots(sd: *mut c_void, v: c_int);
    fn sl_pc_set_maxinv(sd: *mut c_void, v: c_int);
    fn sl_pc_set_partner(sd: *mut c_void, v: c_int);
    fn sl_pc_set_pk(sd: *mut c_void, v: c_int);
    fn sl_pc_set_basehp(sd: *mut c_void, v: c_int);
    fn sl_pc_set_basemp(sd: *mut c_void, v: c_int);
    fn sl_pc_set_karma(sd: *mut c_void, v: c_int);
    fn sl_pc_set_alignment(sd: *mut c_void, v: c_int);
    fn sl_pc_set_basegrace(sd: *mut c_void, v: c_int);
    fn sl_pc_set_basemight(sd: *mut c_void, v: c_int);
    fn sl_pc_set_basewill(sd: *mut c_void, v: c_int);
    fn sl_pc_set_basearmor(sd: *mut c_void, v: c_int);
    fn sl_pc_set_novice_chat(sd: *mut c_void, v: c_int);
    fn sl_pc_set_subpath_chat(sd: *mut c_void, v: c_int);
    fn sl_pc_set_clan_chat(sd: *mut c_void, v: c_int);
    fn sl_pc_set_tutor(sd: *mut c_void, v: c_int);
    fn sl_pc_set_profile_vitastats(sd: *mut c_void, v: c_int);
    fn sl_pc_set_profile_equiplist(sd: *mut c_void, v: c_int);
    fn sl_pc_set_profile_legends(sd: *mut c_void, v: c_int);
    fn sl_pc_set_profile_spells(sd: *mut c_void, v: c_int);
    fn sl_pc_set_profile_inventory(sd: *mut c_void, v: c_int);
    fn sl_pc_set_profile_bankitems(sd: *mut c_void, v: c_int);
    fn sl_pc_set_npc_g(sd: *mut c_void, v: c_int);
    fn sl_pc_set_npc_gc(sd: *mut c_void, v: c_int);
    fn sl_pc_set_last_click(sd: *mut c_void, v: c_int);
    fn sl_pc_set_time(sd: *mut c_void, v: c_int);
    fn sl_pc_set_rage(sd: *mut c_void, v: c_int);
    fn sl_pc_set_polearm(sd: *mut c_void, v: c_int);
    fn sl_pc_set_deduction(sd: *mut c_void, v: c_int);
    fn sl_pc_set_speed(sd: *mut c_void, v: c_int);
    fn sl_pc_set_attacker(sd: *mut c_void, v: c_int);
    fn sl_pc_set_invis(sd: *mut c_void, v: c_int);
    fn sl_pc_set_damage(sd: *mut c_void, v: c_int);
    fn sl_pc_set_crit(sd: *mut c_void, v: c_int);
    fn sl_pc_set_critchance(sd: *mut c_void, v: c_int);
    fn sl_pc_set_critmult(sd: *mut c_void, v: c_int);
    fn sl_pc_set_rangeTarget(sd: *mut c_void, v: c_int);
    fn sl_pc_set_disguise(sd: *mut c_void, v: c_int);
    fn sl_pc_set_disguise_color(sd: *mut c_void, v: c_int);
    fn sl_pc_set_paralyzed(sd: *mut c_void, v: c_int);
    fn sl_pc_set_blind(sd: *mut c_void, v: c_int);
    fn sl_pc_set_drunk(sd: *mut c_void, v: c_int);
    fn sl_pc_set_board_candel(sd: *mut c_void, v: c_int);
    fn sl_pc_set_board_canwrite(sd: *mut c_void, v: c_int);
    fn sl_pc_set_boardshow(sd: *mut c_void, v: c_int);
    fn sl_pc_set_boardnameval(sd: *mut c_void, v: c_int);
    fn sl_pc_set_snare(sd: *mut c_void, v: c_int);
    fn sl_pc_set_silence(sd: *mut c_void, v: c_int);
    fn sl_pc_set_extendhit(sd: *mut c_void, v: c_int);
    fn sl_pc_set_afk(sd: *mut c_void, v: c_int);
    fn sl_pc_set_confused(sd: *mut c_void, v: c_int);
    fn sl_pc_set_spottraps(sd: *mut c_void, v: c_int);
    fn sl_pc_set_selfbar(sd: *mut c_void, v: c_int);
    fn sl_pc_set_groupbars(sd: *mut c_void, v: c_int);
    fn sl_pc_set_mobbars(sd: *mut c_void, v: c_int);
    fn sl_pc_set_mute(sd: *mut c_void, v: c_int);
    fn sl_pc_set_settingFlags(sd: *mut c_void, v: c_int);
    fn sl_pc_set_optFlags_xor(sd: *mut c_void, v: c_int);
    fn sl_pc_set_uflags_xor(sd: *mut c_void, v: c_int);
    fn sl_pc_set_talktype(sd: *mut c_void, v: c_int);
    fn sl_pc_set_cursed(sd: *mut c_void, v: c_int);
    fn sl_pc_set_deathflag(sd: *mut c_void, v: c_int);
    fn sl_pc_set_bindmap(sd: *mut c_void, v: c_int);
    fn sl_pc_set_bindx(sd: *mut c_void, v: c_int);
    fn sl_pc_set_bindy(sd: *mut c_void, v: c_int);
    fn sl_pc_set_protection(sd: *mut c_void, v: c_int);
    fn sl_pc_set_dmgshield(sd: *mut c_void, v: c_int);
    fn sl_pc_set_dmgdealt(sd: *mut c_void, v: c_int);
    fn sl_pc_set_dmgtaken(sd: *mut c_void, v: c_int);
    fn sl_pc_set_heroshow(sd: *mut c_void, v: c_int);
    fn sl_pc_set_fakeDrop(sd: *mut c_void, v: c_int);
    fn sl_pc_set_sex(sd: *mut c_void, v: c_int);
    fn sl_pc_set_clone(sd: *mut c_void, v: c_int);
    fn sl_pc_set_classRank(sd: *mut c_void, v: c_int);
    fn sl_pc_set_clanRank(sd: *mut c_void, v: c_int);
    fn sl_pc_set_fury(sd: *mut c_void, v: c_int);
    fn sl_pc_set_coref_container(sd: *mut c_void, v: c_int);
    fn sl_pc_set_wisdom(sd: *mut c_void, v: c_int);
    fn sl_pc_set_con(sd: *mut c_void, v: c_int);
    fn sl_pc_set_backstab(sd: *mut c_void, v: c_int);
    fn sl_pc_set_flank(sd: *mut c_void, v: c_int);
    fn sl_pc_set_healing(sd: *mut c_void, v: c_int);
    fn sl_pc_set_pbColor(sd: *mut c_void, v: c_int);
    // setters — gfx
    fn sl_pc_set_gfx_face(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_hair(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_chair(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_cface(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_cskin(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_dye(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_weapon(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_cweapon(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_armor(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_carmor(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_shield(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_cshield(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_helm(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_chelm(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_mantle(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_cmantle(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_crown(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_ccrown(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_faceAcc(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_cfaceAcc(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_faceAccT(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_cfaceAccT(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_boots(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_cboots(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_necklace(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_cnecklace(sd: *mut c_void, v: c_int);
    fn sl_pc_set_gfx_name(sd: *mut c_void, v: *const c_char);
    // setters — strings
    fn sl_pc_set_name(sd: *mut c_void, v: *const c_char);
    fn sl_pc_set_title(sd: *mut c_void, v: *const c_char);
    fn sl_pc_set_clan_title(sd: *mut c_void, v: *const c_char);
    fn sl_pc_set_afkmessage(sd: *mut c_void, v: *const c_char);
    fn sl_pc_set_speech(sd: *mut c_void, v: *const c_char);
    // methods — group 1 (task 12)
    fn sl_pc_freeasync(sd: *mut c_void);
    fn sl_pc_forcesave(sd: *mut c_void) -> c_int;
    fn sl_pc_addhealth(sd: *mut c_void, damage: c_int);
    fn sl_pc_removehealth(sd: *mut c_void, damage: c_int, caster: c_int);
    fn sl_pc_die(sd: *mut c_void);
    fn sl_pc_resurrect(sd: *mut c_void);
    fn sl_pc_showhealth(sd: *mut c_void, damage: c_int, typ: c_int);
    fn sl_pc_calcstat(sd: *mut c_void);
    fn sl_pc_sendstatus(sd: *mut c_void);
    fn sl_pc_status(sd: *mut c_void) -> c_int;
    fn sl_pc_warp(sd: *mut c_void, m: c_int, x: c_int, y: c_int);
    fn sl_pc_refresh(sd: *mut c_void);
    fn sl_pc_pickup(sd: *mut c_void, id: c_uint);
    fn sl_pc_throwitem(sd: *mut c_void);
    fn sl_pc_forcedrop(sd: *mut c_void, id: c_int);
    fn sl_pc_lock(sd: *mut c_void);
    fn sl_pc_unlock(sd: *mut c_void);
    fn sl_pc_swing(sd: *mut c_void);
    fn sl_pc_respawn(sd: *mut c_void);
    fn sl_pc_sendhealth(sd: *mut c_void, dmg: c_float, critical: c_int) -> c_int;
    // methods — group 2 (task 13)
    fn sl_pc_move(sd: *mut c_void, speed: c_int);
    fn sl_pc_lookat(sd: *mut c_void, id: c_int);
    fn sl_pc_minirefresh(sd: *mut c_void);
    fn sl_pc_refreshinventory(sd: *mut c_void);
    fn sl_pc_updateinv(sd: *mut c_void);
    fn sl_pc_checkinvbod(sd: *mut c_void);
    fn sl_pc_equip(sd: *mut c_void);
    fn sl_pc_takeoff(sd: *mut c_void);
    fn sl_pc_deductarmor(sd: *mut c_void, v: c_int);
    fn sl_pc_deductweapon(sd: *mut c_void, v: c_int);
    fn sl_pc_deductdura(sd: *mut c_void, eq: c_int, v: c_int);
    fn sl_pc_deductduraequip(sd: *mut c_void);
    fn sl_pc_deductdurainv(sd: *mut c_void, slot: c_int, v: c_int);
    fn sl_pc_hasequipped(sd: *mut c_void, item_id: c_uint) -> c_int;
    fn sl_pc_removeitemslot(sd: *mut c_void, slot: c_int, amount: c_int, typ: c_int);
    fn sl_pc_hasitem(sd: *mut c_void, item_id: c_uint, amount: c_int) -> c_int;
    fn sl_pc_hasspace(sd: *mut c_void, item_id: c_uint) -> c_int;
    fn sl_pc_checklevel(sd: *mut c_void);
    fn sl_pc_sendminimap(sd: *mut c_void);
    fn sl_pc_setminimaptoggle(sd: *mut c_void, flag: c_int);
    fn sl_pc_popup(sd: *mut c_void, msg: *const c_char);
    fn sl_pc_guitext(sd: *mut c_void, msg: *const c_char);
    fn sl_pc_sendminitext(sd: *mut c_void, msg: *const c_char);
    fn sl_pc_powerboard(sd: *mut c_void);
    fn sl_pc_showboard(sd: *mut c_void, id: c_int);
    fn sl_pc_showpost(sd: *mut c_void, id: c_int, post: c_int);
    fn sl_pc_changeview(sd: *mut c_void, x: c_int, y: c_int);
    fn sl_pc_speak(sd: *mut c_void, msg: *const c_char, len: c_int, typ: c_int);
    fn sl_pc_sendmail(
        sd: *mut c_void,
        to: *const c_char,
        topic: *const c_char,
        msg: *const c_char,
    ) -> c_int;
    fn sl_pc_sendurl(sd: *mut c_void, typ: c_int, url: *const c_char);
    fn sl_pc_swingtarget(sd: *mut c_void, id: c_int);
    fn sl_pc_killcount(sd: *mut c_void, mob_id: c_int) -> c_int;
    fn sl_pc_setkillcount(sd: *mut c_void, mob_id: c_int, amount: c_int);
    fn sl_pc_flushkills(sd: *mut c_void, mob_id: c_int);
    fn sl_pc_flushallkills(sd: *mut c_void);
    fn sl_pc_addthreat(sd: *mut c_void, mob_id: c_uint, amount: c_uint);
    fn sl_pc_setthreat(sd: *mut c_void, mob_id: c_uint, amount: c_uint);
    fn sl_pc_addthreatgeneral(sd: *mut c_void, amount: c_uint);
    fn sl_pc_hasspell(sd: *mut c_void, name: *const c_char) -> c_int;
    fn sl_pc_addspell(sd: *mut c_void, spell_id: c_int);
    fn sl_pc_removespell(sd: *mut c_void, spell_id: c_int);
    fn sl_pc_hasduration(sd: *mut c_void, name: *const c_char) -> c_int;
    fn sl_pc_hasdurationid(sd: *mut c_void, name: *const c_char, caster_id: c_int) -> c_int;
    fn sl_pc_getduration(sd: *mut c_void, name: *const c_char) -> c_int;
    fn sl_pc_getdurationid(sd: *mut c_void, name: *const c_char, caster_id: c_int) -> c_int;
    fn sl_pc_durationamount(sd: *mut c_void, name: *const c_char) -> c_int;
    fn sl_pc_setduration(
        sd: *mut c_void,
        name: *const c_char,
        time_ms: c_int,
        caster_id: c_int,
        recast: c_int,
    );
    fn sl_pc_flushduration(sd: *mut c_void, dispel_level: c_int, min_id: c_int, max_id: c_int);
    fn sl_pc_flushdurationnouncast(
        sd: *mut c_void,
        dispel_level: c_int,
        min_id: c_int,
        max_id: c_int,
    );
    fn sl_pc_refreshdurations(sd: *mut c_void);
    fn sl_pc_setaether(sd: *mut c_void, name: *const c_char, time_ms: c_int);
    fn sl_pc_hasaether(sd: *mut c_void, name: *const c_char) -> c_int;
    fn sl_pc_getaether(sd: *mut c_void, name: *const c_char) -> c_int;
    fn sl_pc_flushaether(sd: *mut c_void);
    fn sl_pc_addclan(sd: *mut c_void, name: *const c_char);
    fn sl_pc_updatepath(sd: *mut c_void, path: c_int, mark: c_int);
    fn sl_pc_updatecountry(sd: *mut c_void, country: c_int);
    fn sl_pc_getcasterid(sd: *mut c_void, name: *const c_char) -> c_int;
    fn sl_pc_settimer(sd: *mut c_void, typ: c_int, length: c_int);
    fn sl_pc_addtime(sd: *mut c_void, v: c_int);
    fn sl_pc_removetime(sd: *mut c_void, v: c_int);
    fn sl_pc_setheroshow(sd: *mut c_void, flag: c_int);
    // legends
    fn sl_pc_addlegend(
        sd: *mut c_void,
        text: *const c_char,
        name: *const c_char,
        icon: c_int,
        color: c_int,
        tchaid: c_uint,
    );
    fn sl_pc_haslegend(sd: *mut c_void, name: *const c_char) -> c_int;
    fn sl_pc_removelegendbyname(sd: *mut c_void, name: *const c_char);
    fn sl_pc_removelegendbycolor(sd: *mut c_void, color: c_int);
}

impl UserData for PcObject {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // ── __index: read PC attributes ───────────────────────────────────────
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            let sd = this.ptr;
            if sd.is_null() {
                return Ok(mlua::Value::Nil);
            }
            macro_rules! int_ {
                ($f:expr) => {
                    Ok(mlua::Value::Integer(unsafe { $f(sd) } as i64))
                };
            }
            macro_rules! bool_ {
                ($f:expr) => {
                    Ok(mlua::Value::Boolean(unsafe { $f(sd) } != 0))
                };
            }
            macro_rules! str_ {
                ($f:expr) => {
                    unsafe { cstr_to_lua(lua, $f(sd)) }
                };
            }
            // Shared map properties (pvp, mapTitle, bgm, etc.) handled before the type-specific match.
            let m = unsafe { sl_pc_bl_m(sd) };
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
                "name" => str_!(sl_pc_status_name),
                "title" => str_!(sl_pc_status_title),
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
                "clanTitle" => str_!(sl_pc_status_clan_title),
                "clanRank" => int_!(sl_pc_status_clanRank),
                "actId" => int_!(sl_pc_actid),
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
                "afkMessage" => str_!(sl_pc_status_afkmessage),
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
                "f1Name" => str_!(sl_pc_status_f1name),
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
                        sffi::sl_pc_getgroup(this.ptr, ids.as_mut_ptr(), MAX_MEMBERS as c_int)
                    };
                    let t = lua.create_table()?;
                    for i in 0..n.max(0) as usize {
                        t.raw_set(i + 1, ids[i] as i64)?;
                    }
                    return Ok(mlua::Value::Table(t));
                }
                // Alias: docs use "spouse", implementation uses "partner"
                "spouse"          => int_!(sl_pc_status_partner),
                // Alias: docs use "ac", implementation uses "armor"
                "ac"              => int_!(sl_pc_armor),
                // Registry sub-objects — mirroring pcl_init from scripting.c.
                "registry" => return lua.pack(RegObject { ptr: sd }),
                "registryString" => return lua.pack(RegStringObject { ptr: sd }),
                "quest" => return lua.pack(QuestRegObject { ptr: sd }),
                "npc" => return lua.pack(NpcRegObject { ptr: sd }),
                "mapRegistry" => return lua.pack(MapRegObject { ptr: sd }),
                "gameRegistry" => {
                    return lua.pack(GameRegObject {
                        ptr: std::ptr::null_mut(),
                    })
                }

                // getUsers() → table of all online PcObjects.
                "getUsers" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        |lua, _: mlua::MultiValue| {
                            const MAX: usize = 4096;
                            let mut ptrs: Vec<*mut c_void> = vec![std::ptr::null_mut(); MAX];
                            let count =
                                unsafe { sffi::sl_g_getusers(ptrs.as_mut_ptr(), MAX as c_int) }
                                    as usize;
                            let tbl = lua.create_table()?;
                            for (i, &bl) in ptrs[..count].iter().enumerate() {
                                let val = unsafe {
                                    crate::game::scripting::bl_to_lua(lua, bl)
                                        .unwrap_or(mlua::Value::Nil)
                                };
                                tbl.raw_set(i + 1, val)?;
                            }
                            Ok(tbl)
                        },
                    )?));
                }
                "getBlock" => {
                    return shared::make_getblock_fn(lua)
                }
                "getObjectsInCell" | "getAliveObjectsInCell" | "getObjectsInCellWithTraps" => {
                    return shared::make_cell_query_fn(lua, key.as_str())
                }
                "getObjectsInArea" | "getAliveObjectsInArea"
                | "getObjectsInSameMap" | "getAliveObjectsInSameMap" => {
                    return shared::make_area_query_fn(lua, key.as_str(), sd)
                }
                "getPK" => {
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_lua, (this, id): (mlua::AnyUserData, c_int)| {
                            let sd = this.borrow::<PcObject>()?.ptr;
                            if sd.is_null() {
                                return Ok(false);
                            }
                            Ok(unsafe { sffi::sl_pc_getpk(sd, id) } != 0)
                        },
                    )?));
                }
                "getObjectsInMap" => return shared::make_map_query_fn(lua),
                "sendAnimation"     => return shared::make_sendanimation_fn(lua, sd),
                "playSound"         => return shared::make_playsound_fn(lua, sd),
                "sendAction"        => return shared::make_sendaction_fn(lua, sd),
                "msg"               => return shared::make_msg_fn(lua, sd),
                "dropItem"          => return shared::make_dropitem_fn(lua, sd),
                "dropItemXY"        => return shared::make_dropitemxy_fn(lua, sd),
                "objectCanMove"     => return shared::make_objectcanmove_fn(lua, sd),
                "objectCanMoveFrom" => return shared::make_objectcanmovefrom_fn(lua, sd),
                "repeatAnimation"   => return shared::make_repeatanimation_fn(lua, sd),
                "selfAnimation"     => return shared::make_selfanimation_fn(lua, sd),
                "selfAnimationXY"   => return shared::make_selfanimationxy_fn(lua, sd),
                "sendParcel"        => return shared::make_sendparcel_fn(lua, sd),
                "throw"             => return shared::make_throwblock_fn(lua, sd),
                "delFromIDDB" => return Ok(mlua::Value::Function(lua.create_function(
                    move |_, _: mlua::MultiValue| {
                        unsafe { sffi::sl_g_deliddb(sd); }
                        Ok(())
                    }
                )?)),
                "addPermanentSpawn" => return Ok(mlua::Value::Function(lua.create_function(
                    move |_, _: mlua::MultiValue| {
                        unsafe { sffi::sl_g_addpermanentspawn(sd); }
                        Ok(())
                    }
                )?)),
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
                let sd = this.ptr;
                if sd.is_null() {
                    return Ok(());
                }
                let v = val_to_int(&val);
                match key.as_str() {
                    "actionTime" => unsafe { sl_pc_set_time(sd, v) },
                    "afk" => unsafe { sl_pc_set_afk(sd, v) },
                    "alignment" => unsafe { sl_pc_set_alignment(sd, v) },
                    "armorColor" => unsafe { sl_pc_set_armor_color(sd, v) },
                    "attacker" => unsafe { sl_pc_set_attacker(sd, v) },
                    "backstab" => unsafe { sl_pc_set_backstab(sd, v) },
                    "bankMoney" => unsafe { sl_pc_set_bankmoney(sd, v) },
                    "baseArmor" => unsafe { sl_pc_set_basearmor(sd, v) },
                    "baseGrace" => unsafe { sl_pc_set_basegrace(sd, v) },
                    "baseHealth" => unsafe { sl_pc_set_basehp(sd, v) },
                    "baseMagic" => unsafe { sl_pc_set_basemp(sd, v) },
                    "baseMight" => unsafe { sl_pc_set_basemight(sd, v) },
                    "baseWill" => unsafe { sl_pc_set_basewill(sd, v) },
                    "bindMap" => unsafe { sl_pc_set_bindmap(sd, v) },
                    "bindX" => unsafe { sl_pc_set_bindx(sd, v) },
                    "bindY" => unsafe { sl_pc_set_bindy(sd, v) },
                    "blind" => unsafe { sl_pc_set_blind(sd, v) },
                    "clan" => unsafe { sl_pc_set_clan(sd, v) },
                    "clanChat" => unsafe { sl_pc_set_clan_chat(sd, v) },
                    "clanRank" => unsafe { sl_pc_set_clanRank(sd, v) },
                    "class" => unsafe { sl_pc_set_class(sd, v) },
                    "classRank" => unsafe { sl_pc_set_classRank(sd, v) },
                    "coContainer" => unsafe { sl_pc_set_coref_container(sd, v) },
                    "con" => unsafe { sl_pc_set_con(sd, v) },
                    "confused" => unsafe { sl_pc_set_confused(sd, v) },
                    "country" => unsafe { sl_pc_set_country(sd, v) },
                    "crit" => unsafe { sl_pc_set_crit(sd, v) },
                    "critChance" => unsafe { sl_pc_set_critchance(sd, v) },
                    "critMult" => unsafe { sl_pc_set_critmult(sd, v) },
                    "cursed" => unsafe { sl_pc_set_cursed(sd, v) },
                    "damage" => unsafe { sl_pc_set_damage(sd, v) },
                    "deathFlag" => unsafe { sl_pc_set_deathflag(sd, v) },
                    "deduction" => unsafe { sl_pc_set_deduction(sd, v) },
                    "disguise" => unsafe { sl_pc_set_disguise(sd, v) },
                    "disguiseColor" => unsafe { sl_pc_set_disguise_color(sd, v) },
                    "dmgDealt" => unsafe { sl_pc_set_dmgdealt(sd, v) },
                    "dmgShield" => unsafe { sl_pc_set_dmgshield(sd, v) },
                    "dmgTaken" => unsafe { sl_pc_set_dmgtaken(sd, v) },
                    "drunk" => unsafe { sl_pc_set_drunk(sd, v) },
                    "exp" => unsafe { sl_pc_set_exp(sd, v) },
                    "extendHit" => unsafe { sl_pc_set_extendhit(sd, v) },
                    "face" => unsafe { sl_pc_set_face(sd, v) },
                    "faceColor" => unsafe { sl_pc_set_face_color(sd, v) },
                    "fakeDrop" => unsafe { sl_pc_set_fakeDrop(sd, v) },
                    "flank" => unsafe { sl_pc_set_flank(sd, v) },
                    "fury" => unsafe { sl_pc_set_fury(sd, v) },
                    "gfxArmor" => unsafe { sl_pc_set_gfx_armor(sd, v) },
                    "gfxArmorC" => unsafe { sl_pc_set_gfx_carmor(sd, v) },
                    "gfxBoots" => unsafe { sl_pc_set_gfx_boots(sd, v) },
                    "gfxBootsC" => unsafe { sl_pc_set_gfx_cboots(sd, v) },
                    "gfxClone" => unsafe { sl_pc_set_clone(sd, v) },
                    "gfxCrown" => unsafe { sl_pc_set_gfx_crown(sd, v) },
                    "gfxCrownC" => unsafe { sl_pc_set_gfx_ccrown(sd, v) },
                    "gfxDye" => unsafe { sl_pc_set_gfx_dye(sd, v) },
                    "gfxFace" => unsafe { sl_pc_set_gfx_face(sd, v) },
                    "gfxFaceA" => unsafe { sl_pc_set_gfx_faceAcc(sd, v) },
                    "gfxFaceAC" => unsafe { sl_pc_set_gfx_cfaceAcc(sd, v) },
                    "gfxFaceAT" => unsafe { sl_pc_set_gfx_faceAccT(sd, v) },
                    "gfxFaceATC" => unsafe { sl_pc_set_gfx_cfaceAccT(sd, v) },
                    "gfxFaceC" => unsafe { sl_pc_set_gfx_cface(sd, v) },
                    "gfxHair" => unsafe { sl_pc_set_gfx_hair(sd, v) },
                    "gfxHairC" => unsafe { sl_pc_set_gfx_chair(sd, v) },
                    "gfxHelm" => unsafe { sl_pc_set_gfx_helm(sd, v) },
                    "gfxHelmC" => unsafe { sl_pc_set_gfx_chelm(sd, v) },
                    "gfxMantle" => unsafe { sl_pc_set_gfx_mantle(sd, v) },
                    "gfxMantleC" => unsafe { sl_pc_set_gfx_cmantle(sd, v) },
                    "gfxNeck" => unsafe { sl_pc_set_gfx_necklace(sd, v) },
                    "gfxNeckC" => unsafe { sl_pc_set_gfx_cnecklace(sd, v) },
                    "gfxShield" => unsafe { sl_pc_set_gfx_shield(sd, v) },
                    "gfxShieldC" => unsafe { sl_pc_set_gfx_cshield(sd, v) },
                    "gfxSkinC" => unsafe { sl_pc_set_gfx_cskin(sd, v) },
                    "gfxWeap" => unsafe { sl_pc_set_gfx_weapon(sd, v) },
                    "gfxWeapC" => unsafe { sl_pc_set_gfx_cweapon(sd, v) },
                    "gmLevel" => unsafe { sl_pc_set_gm_level(sd, v) },
                    "groupBars" => unsafe { sl_pc_set_groupbars(sd, v) },
                    "hair" => unsafe { sl_pc_set_hair(sd, v) },
                    "hairColor" => unsafe { sl_pc_set_hair_color(sd, v) },
                    "healing" => unsafe { sl_pc_set_healing(sd, v) },
                    "health" => unsafe { sl_pc_set_hp(sd, v) },
                    "heroShow" => unsafe { sl_pc_set_heroshow(sd, v) },
                    "invis" => unsafe { sl_pc_set_invis(sd, v) },
                    "karma" => unsafe { sl_pc_set_karma(sd, v) },
                    "lastClick" => unsafe { sl_pc_set_last_click(sd, v) },
                    "level" => unsafe { sl_pc_set_level(sd, v) },
                    "magic" => unsafe { sl_pc_set_mp(sd, v) },
                    "mark" => unsafe { sl_pc_set_mark(sd, v) },
                    "maxHealth" => unsafe { sl_pc_set_max_hp(sd, v) },
                    "maxInv" => unsafe { sl_pc_set_maxinv(sd, v) },
                    "maxMagic" => unsafe { sl_pc_set_max_mp(sd, v) },
                    "maxSlots" => unsafe { sl_pc_set_maxslots(sd, v) },
                    "miniMapToggle" => unsafe { sl_pc_set_settingFlags(sd, v) },
                    "mobBars" => unsafe { sl_pc_set_mobbars(sd, v) },
                    "money" => unsafe { sl_pc_set_money(sd, v) },
                    "mute" => unsafe { sl_pc_set_mute(sd, v) },
                    "noviceChat" => unsafe { sl_pc_set_novice_chat(sd, v) },
                    "npcColor" => unsafe { sl_pc_set_npc_gc(sd, v) },
                    "npcGraphic" => unsafe { sl_pc_set_npc_g(sd, v) },
                    "optFlags" => unsafe { sl_pc_set_optFlags_xor(sd, v) },
                    "paralyzed" => unsafe { sl_pc_set_paralyzed(sd, v) },
                    "partner" => unsafe { sl_pc_set_partner(sd, v) },
                    "pbColor" => unsafe { sl_pc_set_pbColor(sd, v) },
                    "PK" => unsafe { sl_pc_set_pk(sd, v) },
                    "polearm" => unsafe { sl_pc_set_polearm(sd, v) },
                    "profileBankItems" => unsafe { sl_pc_set_profile_bankitems(sd, v) },
                    "profileEquipList" => unsafe { sl_pc_set_profile_equiplist(sd, v) },
                    "profileInventory" => unsafe { sl_pc_set_profile_inventory(sd, v) },
                    "profileLegends" => unsafe { sl_pc_set_profile_legends(sd, v) },
                    "profileSpells" => unsafe { sl_pc_set_profile_spells(sd, v) },
                    "profileVitaStats" => unsafe { sl_pc_set_profile_vitastats(sd, v) },
                    "protection" => unsafe { sl_pc_set_protection(sd, v) },
                    "rage" => unsafe { sl_pc_set_rage(sd, v) },
                    "rangeTarget" => unsafe { sl_pc_set_rangeTarget(sd, v) },
                    "selfBar" => unsafe { sl_pc_set_selfbar(sd, v) },
                    "settings" => unsafe { sl_pc_set_settingFlags(sd, v) },
                    "sex" => unsafe { sl_pc_set_sex(sd, v) },
                    "side" => unsafe { sl_pc_set_side(sd, v) },
                    "silence" => unsafe { sl_pc_set_silence(sd, v) },
                    "skinColor" => unsafe { sl_pc_set_skin_color(sd, v) },
                    "snare" => unsafe { sl_pc_set_snare(sd, v) },
                    "speed" => unsafe { sl_pc_set_speed(sd, v) },
                    "spotTraps" => unsafe { sl_pc_set_spottraps(sd, v) },
                    "state" => unsafe { sl_pc_set_state(sd, v) },
                    "subpathChat" => unsafe { sl_pc_set_subpath_chat(sd, v) },
                    "talkType" => unsafe { sl_pc_set_talktype(sd, v) },
                    "target"   => unsafe { sl_pc_set_target(sd, v) },
                    "tier" => unsafe { sl_pc_set_tier(sd, v) },
                    "totem" => unsafe { sl_pc_set_totem(sd, v) },
                    "tutor" => unsafe { sl_pc_set_tutor(sd, v) },
                    "uflags" => unsafe { sl_pc_set_uflags_xor(sd, v) },
                    "wisdom" => unsafe { sl_pc_set_wisdom(sd, v) },
                    // Task 4: new attribute bindings
                    "vRegenOverflow"  => unsafe { sl_pc_set_vregenoverflow(sd, v) },
                    "mRegenOverflow"  => unsafe { sl_pc_set_mregenoverflow(sd, v) },
                    "groupCount"      => unsafe { sl_pc_set_group_count(sd, v) },
                    "groupOn"         => unsafe { sl_pc_set_group_on(sd, v) },
                    "groupLeader"     => unsafe { sl_pc_set_group_leader(sd, v) },
                    "spouse"          => unsafe { sl_pc_set_partner(sd, v) },
                    "ac"              => unsafe { sl_pc_set_basearmor(sd, v) },
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
        methods.add_method("addHealth", |_, this, damage: c_int| {
            unsafe { sl_pc_addhealth(this.ptr, damage) };
            Ok(())
        });
        methods.add_method(
            "removeHealth",
            |_, this, (damage, caster): (c_int, c_int)| {
                unsafe { sl_pc_removehealth(this.ptr, damage, caster) };
                Ok(())
            },
        );
        methods.add_method("die", |_, this, ()| {
            unsafe { sl_pc_die(this.ptr) };
            Ok(())
        });
        methods.add_method("resurrect", |_, this, ()| {
            unsafe { sl_pc_resurrect(this.ptr) };
            Ok(())
        });
        methods.add_method("showHealth", |_, this, (damage, typ): (c_int, c_int)| {
            unsafe { sl_pc_showhealth(this.ptr, damage, typ) };
            Ok(())
        });
        methods.add_method("freeAsync", |_, this, ()| {
            unsafe { sl_pc_freeasync(this.ptr) };
            Ok(())
        });
        methods.add_method("forceSave", |_, this, ()| {
            Ok(unsafe { sl_pc_forcesave(this.ptr) })
        });
        methods.add_method("calcStat", |_, this, ()| {
            unsafe { sl_pc_calcstat(this.ptr) };
            Ok(())
        });
        methods.add_method("sendStatus", |_, this, ()| {
            unsafe { sl_pc_sendstatus(this.ptr) };
            Ok(())
        });
        methods.add_method("status", |_, this, ()| {
            Ok(unsafe { sl_pc_status(this.ptr) })
        });
        methods.add_method("warp", |_, this, (m, x, y): (c_int, c_int, c_int)| {
            unsafe { sl_pc_warp(this.ptr, m, x, y) };
            Ok(())
        });
        methods.add_method("refresh", |_, this, ()| {
            unsafe { sl_pc_refresh(this.ptr) };
            Ok(())
        });
        methods.add_method("pickUp", |_, this, id: c_uint| {
            unsafe { sl_pc_pickup(this.ptr, id) };
            Ok(())
        });
        methods.add_method("throwItem", |_, this, ()| {
            unsafe { sl_pc_throwitem(this.ptr) };
            Ok(())
        });
        methods.add_method("forceDrop", |_, this, id: c_int| {
            unsafe { sl_pc_forcedrop(this.ptr, id) };
            Ok(())
        });
        methods.add_method("lock", |_, this, ()| {
            unsafe { sl_pc_lock(this.ptr) };
            Ok(())
        });
        methods.add_method("unlock", |_, this, ()| {
            unsafe { sl_pc_unlock(this.ptr) };
            Ok(())
        });
        methods.add_method("swing", |_, this, ()| {
            unsafe { sl_pc_swing(this.ptr) };
            Ok(())
        });
        methods.add_method("respawn", |_, this, ()| {
            unsafe { sl_pc_respawn(this.ptr) };
            Ok(())
        });
        methods.add_method("sendHealth", |_, this, (dmg, crit): (f32, c_int)| {
            Ok(unsafe { sl_pc_sendhealth(this.ptr, dmg, crit) })
        });

        // ── Movement ──────────────────────────────────────────────────────────
        methods.add_method("move", |_, this, speed: c_int| {
            unsafe { sl_pc_move(this.ptr, speed) };
            Ok(())
        });
        methods.add_method("lookAt", |_, this, id: c_int| {
            unsafe { sl_pc_lookat(this.ptr, id) };
            Ok(())
        });
        methods.add_method("miniRefresh", |_, this, ()| {
            unsafe { sl_pc_minirefresh(this.ptr) };
            Ok(())
        });
        methods.add_method("refreshInventory", |_, this, ()| {
            unsafe { sl_pc_refreshinventory(this.ptr) };
            Ok(())
        });
        methods.add_method("updateInv", |_, this, ()| {
            unsafe { sl_pc_updateinv(this.ptr) };
            Ok(())
        });
        methods.add_method("checkInvBod", |_, this, ()| {
            unsafe { sl_pc_checkinvbod(this.ptr) };
            Ok(())
        });

        // ── Equipment ────────────────────────────────────────────────────────
        methods.add_method("equip", |_, this, ()| {
            unsafe { sl_pc_equip(this.ptr) };
            Ok(())
        });
        methods.add_method("takeOff", |_, this, ()| {
            unsafe { sl_pc_takeoff(this.ptr) };
            Ok(())
        });
        methods.add_method("deductArmor", |_, this, v: c_int| {
            unsafe { sl_pc_deductarmor(this.ptr, v) };
            Ok(())
        });
        methods.add_method("deductWeapon", |_, this, v: c_int| {
            unsafe { sl_pc_deductweapon(this.ptr, v) };
            Ok(())
        });
        methods.add_method("deductDura", |_, this, (eq, v): (c_int, c_int)| {
            unsafe { sl_pc_deductdura(this.ptr, eq, v) };
            Ok(())
        });
        methods.add_method("deductDuraEquip", |_, this, ()| {
            unsafe { sl_pc_deductduraequip(this.ptr) };
            Ok(())
        });
        methods.add_method("deductDuraInv", |_, this, (slot, v): (c_int, c_int)| {
            unsafe { sl_pc_deductdurainv(this.ptr, slot, v) };
            Ok(())
        });
        methods.add_method("hasEquipped", |_, this, id: c_uint| {
            Ok(unsafe { sl_pc_hasequipped(this.ptr, id) } != 0)
        });
        methods.add_method(
            "removeItemSlot",
            |_, this, (slot, amount, typ): (c_int, c_int, c_int)| {
                unsafe { sl_pc_removeitemslot(this.ptr, slot, amount, typ) };
                Ok(())
            },
        );
        methods.add_method("hasItem", |_, this, (id, amount): (c_uint, c_int)| {
            Ok(unsafe { sl_pc_hasitem(this.ptr, id, amount) } != 0)
        });
        methods.add_method("hasSpace", |_, this, id: c_uint| {
            Ok(unsafe { sl_pc_hasspace(this.ptr, id) } != 0)
        });

        // ── Stats ────────────────────────────────────────────────────────────
        methods.add_method("checkLevel", |_, this, ()| {
            unsafe { sl_pc_checklevel(this.ptr) };
            Ok(())
        });

        // ── UI / display ─────────────────────────────────────────────────────
        methods.add_method("sendMiniMap", |_, this, ()| {
            unsafe { sl_pc_sendminimap(this.ptr) };
            Ok(())
        });
        methods.add_method("setMiniMapToggle", |_, this, flag: c_int| {
            unsafe { sl_pc_setminimaptoggle(this.ptr, flag) };
            Ok(())
        });
        methods.add_method("popup", |_, this, msg: String| {
            if let Ok(cs) = CString::new(msg.as_bytes()) {
                unsafe { sl_pc_popup(this.ptr, cs.as_ptr()) };
            }
            Ok(())
        });
        methods.add_method("popUp", |_, this, msg: String| {
            if let Ok(cs) = CString::new(msg.as_bytes()) {
                unsafe { sl_pc_popup(this.ptr, cs.as_ptr()) };
            }
            Ok(())
        });
        methods.add_method("guiText", |_, this, msg: String| {
            if let Ok(cs) = CString::new(msg.as_bytes()) {
                unsafe { sl_pc_guitext(this.ptr, cs.as_ptr()) };
            }
            Ok(())
        });
        methods.add_method("sendMiniText", |_, this, msg: String| {
            if let Ok(cs) = CString::new(msg.as_bytes()) {
                unsafe { sl_pc_sendminitext(this.ptr, cs.as_ptr()) };
            }
            Ok(())
        });
        methods.add_method("sendMinitext", |_, this, msg: String| {
            if let Ok(cs) = CString::new(msg.as_bytes()) {
                unsafe { sl_pc_sendminitext(this.ptr, cs.as_ptr()) };
            }
            Ok(())
        });
        methods.add_method("powerBoard", |_, this, ()| {
            unsafe { sl_pc_powerboard(this.ptr) };
            Ok(())
        });
        methods.add_method("showBoard", |_, this, id: c_int| {
            unsafe { sl_pc_showboard(this.ptr, id) };
            Ok(())
        });
        methods.add_method("showPost", |_, this, (id, post): (c_int, c_int)| {
            unsafe { sl_pc_showpost(this.ptr, id, post) };
            Ok(())
        });
        methods.add_method("changeView", |_, this, (x, y): (c_int, c_int)| {
            unsafe { sl_pc_changeview(this.ptr, x, y) };
            Ok(())
        });

        // ── Social / network ─────────────────────────────────────────────────
        methods.add_method("speak", |_, this, (msg, typ): (String, c_int)| {
            if let Ok(cs) = CString::new(msg.as_bytes()) {
                let len = cs.as_bytes().len() as c_int;
                unsafe { sl_pc_speak(this.ptr, cs.as_ptr(), len, typ) };
            }
            Ok(())
        });
        methods.add_method(
            "sendMail",
            |_, this, (to, topic, msg): (String, String, String)| {
                let to_cs = CString::new(to.as_bytes()).ok();
                let topic_cs = CString::new(topic.as_bytes()).ok();
                let msg_cs = CString::new(msg.as_bytes()).ok();
                if let (Some(t), Some(s), Some(m)) = (to_cs, topic_cs, msg_cs) {
                    unsafe { sl_pc_sendmail(this.ptr, t.as_ptr(), s.as_ptr(), m.as_ptr()) };
                }
                Ok(())
            },
        );
        methods.add_method("sendUrl", |_, this, (typ, url): (c_int, String)| {
            if let Ok(cs) = CString::new(url.as_bytes()) {
                unsafe { sl_pc_sendurl(this.ptr, typ, cs.as_ptr()) };
            }
            Ok(())
        });
        methods.add_method("swingTarget", |_, this, val: mlua::Value| {
            // swing.lua passes MobObject/PcObject userdata, not a raw integer.
            let id: c_int = match val {
                mlua::Value::Integer(n) => n as c_int,
                mlua::Value::Number(n) => n as c_int,
                mlua::Value::UserData(ud) => {
                    if let Ok(mob) = ud.borrow::<MobObject>() {
                        let mob_data = unsafe {
                            &*(mob.ptr as *const crate::game::mob::MobSpawnData)
                        };
                        mob_data.bl.id as c_int
                    } else if let Ok(pc) = ud.borrow::<PcObject>() {
                        unsafe { sl_pc_bl_id(pc.ptr) }
                    } else {
                        return Ok(());
                    }
                }
                _ => return Ok(()),
            };
            unsafe { sl_pc_swingtarget(this.ptr, id) };
            Ok(())
        });

        // ── Kill registry ─────────────────────────────────────────────────────
        methods.add_method("killCount", |_, this, mob_id: c_int| {
            Ok(unsafe { sl_pc_killcount(this.ptr, mob_id) })
        });
        methods.add_method(
            "setKillCount",
            |_, this, (mob_id, amount): (c_int, c_int)| {
                unsafe { sl_pc_setkillcount(this.ptr, mob_id, amount) };
                Ok(())
            },
        );
        methods.add_method("flushKills", |_, this, mob_id: c_int| {
            unsafe { sl_pc_flushkills(this.ptr, mob_id) };
            Ok(())
        });
        methods.add_method("flushAllKills", |_, this, ()| {
            unsafe { sl_pc_flushallkills(this.ptr) };
            Ok(())
        });

        // ── Threat ───────────────────────────────────────────────────────────
        methods.add_method(
            "addThreat",
            |_, this, (mob_id, amount): (c_uint, c_uint)| {
                unsafe { sl_pc_addthreat(this.ptr, mob_id, amount) };
                Ok(())
            },
        );
        methods.add_method(
            "setThreat",
            |_, this, (mob_id, amount): (c_uint, c_uint)| {
                unsafe { sl_pc_setthreat(this.ptr, mob_id, amount) };
                Ok(())
            },
        );
        methods.add_method("addThreatGeneral", |_, this, amount: c_uint| {
            unsafe { sl_pc_addthreatgeneral(this.ptr, amount) };
            Ok(())
        });

        // ── Spell list ───────────────────────────────────────────────────────
        methods.add_method("hasSpell", |_, this, name: String| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| unsafe { sl_pc_hasspell(this.ptr, c.as_ptr()) }) != 0)
        });
        methods.add_method("addSpell", |_, this, spell_id: c_int| {
            unsafe { sl_pc_addspell(this.ptr, spell_id) };
            Ok(())
        });
        methods.add_method("removeSpell", |_, this, spell_id: c_int| {
            unsafe { sl_pc_removespell(this.ptr, spell_id) };
            Ok(())
        });

        // ── Duration system ──────────────────────────────────────────────────
        methods.add_method("hasDuration", |_, this, name: String| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| unsafe { sl_pc_hasduration(this.ptr, c.as_ptr()) }) != 0)
        });
        methods.add_method(
            "hasDurationId",
            |_, this, (name, caster): (String, c_int)| {
                let cs = CString::new(name.as_bytes()).ok();
                Ok(cs.map_or(0, |c| unsafe {
                    sl_pc_hasdurationid(this.ptr, c.as_ptr(), caster)
                }) != 0)
            },
        );
        methods.add_method(
            "hasDurationID",
            |_, this, (name, caster): (String, c_int)| {
                let cs = CString::new(name.as_bytes()).ok();
                Ok(cs.map_or(0, |c| unsafe {
                    sl_pc_hasdurationid(this.ptr, c.as_ptr(), caster)
                }) != 0)
            },
        );
        methods.add_method("getDuration", |_, this, name: String| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| unsafe { sl_pc_getduration(this.ptr, c.as_ptr()) }))
        });
        methods.add_method(
            "getDurationId",
            |_, this, (name, caster): (String, c_int)| {
                let cs = CString::new(name.as_bytes()).ok();
                Ok(cs.map_or(0, |c| unsafe {
                    sl_pc_getdurationid(this.ptr, c.as_ptr(), caster)
                }))
            },
        );
        methods.add_method(
            "getDurationID",
            |_, this, (name, caster): (String, c_int)| {
                let cs = CString::new(name.as_bytes()).ok();
                Ok(cs.map_or(0, |c| unsafe {
                    sl_pc_getdurationid(this.ptr, c.as_ptr(), caster)
                }))
            },
        );
        methods.add_method("durationAmount", |_, this, name: String| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| unsafe { sl_pc_durationamount(this.ptr, c.as_ptr()) }))
        });
        methods.add_method(
            "setDuration",
            |_, this, (name, time_ms, caster, recast): (String, c_int, c_int, c_int)| {
                if let Ok(cs) = CString::new(name.as_bytes()) {
                    unsafe { sl_pc_setduration(this.ptr, cs.as_ptr(), time_ms, caster, recast) };
                }
                Ok(())
            },
        );
        methods.add_method(
            "flushDuration",
            |_, this, (level, min_id, max_id): (c_int, c_int, c_int)| {
                unsafe { sl_pc_flushduration(this.ptr, level, min_id, max_id) };
                Ok(())
            },
        );
        methods.add_method(
            "flushDurationNoUncast",
            |_, this, (level, min_id, max_id): (c_int, c_int, c_int)| {
                unsafe { sl_pc_flushdurationnouncast(this.ptr, level, min_id, max_id) };
                Ok(())
            },
        );
        methods.add_method("refreshDurations", |_, this, ()| {
            unsafe { sl_pc_refreshdurations(this.ptr) };
            Ok(())
        });

        // ── Aether system ────────────────────────────────────────────────────
        methods.add_method("setAether", |_, this, (name, time_ms): (String, c_int)| {
            if let Ok(cs) = CString::new(name.as_bytes()) {
                unsafe { sl_pc_setaether(this.ptr, cs.as_ptr(), time_ms) };
            }
            Ok(())
        });
        methods.add_method("hasAether", |_, this, name: String| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| unsafe { sl_pc_hasaether(this.ptr, c.as_ptr()) }) != 0)
        });
        methods.add_method("getAether", |_, this, name: String| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| unsafe { sl_pc_getaether(this.ptr, c.as_ptr()) }))
        });
        methods.add_method("flushAether", |_, this, ()| {
            unsafe { sl_pc_flushaether(this.ptr) };
            Ok(())
        });

        // ── Clan / path ──────────────────────────────────────────────────────
        methods.add_method("addClan", |_, this, name: String| {
            if let Ok(cs) = CString::new(name.as_bytes()) {
                unsafe { sl_pc_addclan(this.ptr, cs.as_ptr()) };
            }
            Ok(())
        });
        methods.add_method("updatePath", |_, this, (path, mark): (c_int, c_int)| {
            unsafe { sl_pc_updatepath(this.ptr, path, mark) };
            Ok(())
        });
        methods.add_method("updateCountry", |_, this, country: c_int| {
            unsafe { sl_pc_updatecountry(this.ptr, country) };
            Ok(())
        });

        // ── Misc ─────────────────────────────────────────────────────────────
        methods.add_method("getCasterId", |_, this, name: String| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| unsafe { sl_pc_getcasterid(this.ptr, c.as_ptr()) }))
        });
        methods.add_method("getCasterID", |_, this, name: String| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| unsafe { sl_pc_getcasterid(this.ptr, c.as_ptr()) }))
        });
        methods.add_method("setTimer", |_, this, (typ, length): (c_int, c_int)| {
            unsafe { sl_pc_settimer(this.ptr, typ, length) };
            Ok(())
        });
        methods.add_method("addTime", |_, this, v: c_int| {
            unsafe { sl_pc_addtime(this.ptr, v) };
            Ok(())
        });
        methods.add_method("removeTime", |_, this, v: c_int| {
            unsafe { sl_pc_removetime(this.ptr, v) };
            Ok(())
        });
        methods.add_method("setHeroShow", |_, this, flag: c_int| {
            unsafe { sl_pc_setheroshow(this.ptr, flag) };
            Ok(())
        });

        // ── Legends ──────────────────────────────────────────────────────────
        methods.add_method(
            "addLegend",
            |_, this, (text, name, icon, color, tchaid): (String, String, c_int, c_int, c_uint)| {
                let t = CString::new(text.as_bytes()).ok();
                let n = CString::new(name.as_bytes()).ok();
                if let (Some(tc), Some(nc)) = (t, n) {
                    unsafe {
                        sl_pc_addlegend(this.ptr, tc.as_ptr(), nc.as_ptr(), icon, color, tchaid)
                    };
                }
                Ok(())
            },
        );
        methods.add_method("hasLegend", |_, this, name: String| {
            let cs = CString::new(name.as_bytes()).ok();
            Ok(cs.map_or(0, |c| unsafe { sl_pc_haslegend(this.ptr, c.as_ptr()) }) != 0)
        });
        methods.add_method("removeLegendByName", |_, this, name: String| {
            if let Ok(cs) = CString::new(name.as_bytes()) {
                unsafe { sl_pc_removelegendbyname(this.ptr, cs.as_ptr()) };
            }
            Ok(())
        });
        methods.add_method("removeLegendByColor", |_, this, color: c_int| {
            unsafe { sl_pc_removelegendbycolor(this.ptr, color) };
            Ok(())
        });
    }
}
