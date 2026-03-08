//! PC (USER) field accessors for Lua scripting.
//! Replaces the sl_pc_* accessor block in `c_src/sl_compat.c`.

use std::os::raw::{c_char, c_int, c_uint, c_short, c_void, c_uchar};
use std::os::raw::c_ulong;
use crate::game::pc::{MapSessionData, EQ_FACEACCTWO, SFLAG_HPMP, SFLAG_FULLSTATS, SFLAG_XPMONEY};
use crate::database::map_db::BlockList;
use crate::game::mob::{MobSpawnData, MAX_THREATCOUNT};
use crate::servers::char::charstatus::{MAX_INVENTORY, MAX_SPELLS, MAX_KILLREG, MAX_EQUIP, MAX_MAGIC_TIMERS, MAX_LEGENDS, MAX_BANK_SLOTS, Legend};

// ─── Read: block_list embedded fields ────────────────────────────────────────

#[no_mangle] pub unsafe extern "C" fn sl_pc_bl_id(sd: *mut c_void) -> c_int   { (*(sd as *mut MapSessionData)).bl.id as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_bl_m(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).bl.m as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_bl_x(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).bl.x as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_bl_y(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).bl.y as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_bl_type(sd: *mut c_void) -> c_int { (*(sd as *mut MapSessionData)).bl.bl_type as c_int }

// ─── Read: status fields ──────────────────────────────────────────────────────
// Mirrors the `sl_pc_status_*` block from `c_src/sl_compat.c` lines 469-532.

#[no_mangle] pub unsafe extern "C" fn sl_pc_status_id(sd: *mut c_void) -> c_int          { (*(sd as *mut MapSessionData)).status.id as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_hp(sd: *mut c_void) -> c_int          { (*(sd as *mut MapSessionData)).status.hp as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_mp(sd: *mut c_void) -> c_int          { (*(sd as *mut MapSessionData)).status.mp as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_level(sd: *mut c_void) -> c_int       { (*(sd as *mut MapSessionData)).status.level as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_exp(sd: *mut c_void) -> c_int         { (*(sd as *mut MapSessionData)).status.exp as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_expsoldmagic(sd: *mut c_void) -> c_int{ (*(sd as *mut MapSessionData)).status.expsold_magic as c_int } // truncates to low 32 bits; matches C
// Field name mapping (C → Rust where they differ):
//   settingFlags → setting_flags   classRank → class_rank   clanRank → clan_rank
//   miniMapToggle → mini_map_toggle   expsoldhealth/stats → expsold_health/stats
// Many numeric fields are u8/u16/u32/u64/i8/i32/f32 in Rust; all cast to c_int.
// sl_pc_status_killspvp reads from sd->killspvp (direct USER field), not status.

#[no_mangle] pub unsafe extern "C" fn sl_pc_status_expsoldhealth(sd: *mut c_void) -> c_int { (*(sd as *mut MapSessionData)).status.expsold_health as c_int } // truncates to low 32 bits; matches C
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_expsoldstats(sd: *mut c_void) -> c_int  { (*(sd as *mut MapSessionData)).status.expsold_stats as c_int } // truncates to low 32 bits; matches C
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_class(sd: *mut c_void) -> c_int     { (*(sd as *mut MapSessionData)).status.class as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_totem(sd: *mut c_void) -> c_int     { (*(sd as *mut MapSessionData)).status.totem as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_tier(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).status.tier as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_mark(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).status.mark as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_country(sd: *mut c_void) -> c_int   { (*(sd as *mut MapSessionData)).status.country as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_clan(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).status.clan as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_gm_level(sd: *mut c_void) -> c_int  { (*(sd as *mut MapSessionData)).status.gm_level as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_sex(sd: *mut c_void) -> c_int       { (*(sd as *mut MapSessionData)).status.sex as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_side(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).status.side as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_state(sd: *mut c_void) -> c_int     { (*(sd as *mut MapSessionData)).status.state as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_face(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).status.face as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_hair(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).status.hair as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_hair_color(sd: *mut c_void) -> c_int  { (*(sd as *mut MapSessionData)).status.hair_color as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_face_color(sd: *mut c_void) -> c_int  { (*(sd as *mut MapSessionData)).status.face_color as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_armor_color(sd: *mut c_void) -> c_int { (*(sd as *mut MapSessionData)).status.armor_color as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_skin_color(sd: *mut c_void) -> c_int  { (*(sd as *mut MapSessionData)).status.skin_color as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_basehp(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).status.basehp as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_basemp(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).status.basemp as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_money(sd: *mut c_void) -> c_int     { (*(sd as *mut MapSessionData)).status.money as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_bankmoney(sd: *mut c_void) -> c_int { (*(sd as *mut MapSessionData)).status.bankmoney as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_maxslots(sd: *mut c_void) -> c_int  { (*(sd as *mut MapSessionData)).status.maxslots as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_maxinv(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).status.maxinv as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_partner(sd: *mut c_void) -> c_int   { (*(sd as *mut MapSessionData)).status.partner as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_pk(sd: *mut c_void) -> c_int        { (*(sd as *mut MapSessionData)).status.pk as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_killedby(sd: *mut c_void) -> c_int  { (*(sd as *mut MapSessionData)).status.killedby as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_killspk(sd: *mut c_void) -> c_int   { (*(sd as *mut MapSessionData)).status.killspk as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_pkduration(sd: *mut c_void) -> c_int{ (*(sd as *mut MapSessionData)).status.pkduration as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_basegrace(sd: *mut c_void) -> c_int { (*(sd as *mut MapSessionData)).status.basegrace as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_basemight(sd: *mut c_void) -> c_int { (*(sd as *mut MapSessionData)).status.basemight as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_basewill(sd: *mut c_void) -> c_int  { (*(sd as *mut MapSessionData)).status.basewill as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_basearmor(sd: *mut c_void) -> c_int { (*(sd as *mut MapSessionData)).status.basearmor as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_tutor(sd: *mut c_void) -> c_int     { (*(sd as *mut MapSessionData)).status.tutor as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_karma(sd: *mut c_void) -> c_int     { (*(sd as *mut MapSessionData)).status.karma as c_int } // truncates float to int; matches C
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_alignment(sd: *mut c_void) -> c_int { (*(sd as *mut MapSessionData)).status.alignment as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_classRank(sd: *mut c_void) -> c_int { (*(sd as *mut MapSessionData)).status.class_rank as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_clanRank(sd: *mut c_void) -> c_int  { (*(sd as *mut MapSessionData)).status.clan_rank as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_novice_chat(sd: *mut c_void) -> c_int { (*(sd as *mut MapSessionData)).status.novice_chat as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_subpath_chat(sd: *mut c_void) -> c_int{ (*(sd as *mut MapSessionData)).status.subpath_chat as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_clan_chat(sd: *mut c_void) -> c_int  { (*(sd as *mut MapSessionData)).status.clan_chat as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_miniMapToggle(sd: *mut c_void) -> c_int{ (*(sd as *mut MapSessionData)).status.mini_map_toggle as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_heroes(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).status.heroes as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_mute(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).status.mute as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_settingFlags(sd: *mut c_void) -> c_int{ (*(sd as *mut MapSessionData)).status.setting_flags as c_int }
// sl_pc_status_killspvp reads from the direct USER field `killspvp`, not status.killspvp
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_killspvp(sd: *mut c_void) -> c_int  { (*(sd as *mut MapSessionData)).killspvp as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_profile_vitastats(sd: *mut c_void) -> c_int  { (*(sd as *mut MapSessionData)).status.profile_vitastats as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_profile_equiplist(sd: *mut c_void) -> c_int  { (*(sd as *mut MapSessionData)).status.profile_equiplist as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_profile_legends(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).status.profile_legends as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_profile_spells(sd: *mut c_void) -> c_int     { (*(sd as *mut MapSessionData)).status.profile_spells as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_profile_inventory(sd: *mut c_void) -> c_int  { (*(sd as *mut MapSessionData)).status.profile_inventory as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_profile_bankitems(sd: *mut c_void) -> c_int  { (*(sd as *mut MapSessionData)).status.profile_bankitems as c_int }

// String getters — status fields are fixed-size [i8; N] arrays; return pointer to first element.
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_name(sd: *mut c_void) -> *const c_char {
    (*(sd as *mut MapSessionData)).status.name.as_ptr()
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_title(sd: *mut c_void) -> *const c_char {
    (*(sd as *mut MapSessionData)).status.title.as_ptr()
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_clan_title(sd: *mut c_void) -> *const c_char {
    (*(sd as *mut MapSessionData)).status.clan_title.as_ptr()
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_afkmessage(sd: *mut c_void) -> *const c_char {
    (*(sd as *mut MapSessionData)).status.afkmessage.as_ptr()
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_status_f1name(sd: *mut c_void) -> *const c_char {
    (*(sd as *mut MapSessionData)).status.f1name.as_ptr()
}

// ─── Read: direct USER fields ────────────────────────────────────────────────
// Mirrors the sl_pc_* block from c_src/sl_compat.c lines 469-569.
// Type notes: c_uint/c_ulong/u8/u16/i8/c_short/f32/f64 fields all cast to c_int.
// f32 fields (rage, enchanted, sleep, deduction, damage, invis, fury, critmult, dmgshield)
// f64 fields (dmgdealt, dmgtaken)

#[no_mangle] pub unsafe extern "C" fn sl_pc_npc_g(sd: *mut c_void) -> c_int        { (*(sd as *mut MapSessionData)).npc_g }
#[no_mangle] pub unsafe extern "C" fn sl_pc_npc_gc(sd: *mut c_void) -> c_int       { (*(sd as *mut MapSessionData)).npc_gc }
#[no_mangle] pub unsafe extern "C" fn sl_pc_groupid(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).groupid as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_time(sd: *mut c_void) -> c_int         { (*(sd as *mut MapSessionData)).time }
#[no_mangle] pub unsafe extern "C" fn sl_pc_fakeDrop(sd: *mut c_void) -> c_int     { (*(sd as *mut MapSessionData)).fakeDrop as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_max_hp(sd: *mut c_void) -> c_int       { (*(sd as *mut MapSessionData)).max_hp as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_max_mp(sd: *mut c_void) -> c_int       { (*(sd as *mut MapSessionData)).max_mp as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_lastvita(sd: *mut c_void) -> c_int     { (*(sd as *mut MapSessionData)).lastvita as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_rage(sd: *mut c_void) -> c_int         { (*(sd as *mut MapSessionData)).rage as c_int } // truncates float to int; matches C
#[no_mangle] pub unsafe extern "C" fn sl_pc_polearm(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).polearm }
#[no_mangle] pub unsafe extern "C" fn sl_pc_last_click(sd: *mut c_void) -> c_int   { (*(sd as *mut MapSessionData)).last_click as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_grace(sd: *mut c_void) -> c_int        { (*(sd as *mut MapSessionData)).grace }
#[no_mangle] pub unsafe extern "C" fn sl_pc_might(sd: *mut c_void) -> c_int        { (*(sd as *mut MapSessionData)).might }
#[no_mangle] pub unsafe extern "C" fn sl_pc_will(sd: *mut c_void) -> c_int         { (*(sd as *mut MapSessionData)).will }
#[no_mangle] pub unsafe extern "C" fn sl_pc_armor(sd: *mut c_void) -> c_int        { (*(sd as *mut MapSessionData)).armor }
#[no_mangle] pub unsafe extern "C" fn sl_pc_dam(sd: *mut c_void) -> c_int          { (*(sd as *mut MapSessionData)).dam }
#[no_mangle] pub unsafe extern "C" fn sl_pc_hit(sd: *mut c_void) -> c_int          { (*(sd as *mut MapSessionData)).hit }
#[no_mangle] pub unsafe extern "C" fn sl_pc_miss(sd: *mut c_void) -> c_int         { (*(sd as *mut MapSessionData)).miss as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_sleep(sd: *mut c_void) -> c_int        { (*(sd as *mut MapSessionData)).sleep as c_int } // truncates float to int; matches C
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_sleep(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).sleep = v as f32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_attack_speed(sd: *mut c_void) -> c_int { (*(sd as *mut MapSessionData)).attack_speed as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_enchanted(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).enchanted as c_int } // truncates float to int; matches C
#[no_mangle] pub unsafe extern "C" fn sl_pc_confused(sd: *mut c_void) -> c_int     { (*(sd as *mut MapSessionData)).confused as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_target(sd: *mut c_void) -> c_int       { (*(sd as *mut MapSessionData)).target }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_target(sd: *mut c_void, v: c_int)  { (*(sd as *mut MapSessionData)).target = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_deduction(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).deduction as c_int } // truncates float to int; matches C
#[no_mangle] pub unsafe extern "C" fn sl_pc_speed(sd: *mut c_void) -> c_int        { (*(sd as *mut MapSessionData)).speed }
#[no_mangle] pub unsafe extern "C" fn sl_pc_disguise(sd: *mut c_void) -> c_int     { (*(sd as *mut MapSessionData)).disguise as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_disguise_color(sd: *mut c_void) -> c_int { (*(sd as *mut MapSessionData)).disguise_color as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_attacker(sd: *mut c_void) -> c_int     { (*(sd as *mut MapSessionData)).attacker as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_invis(sd: *mut c_void) -> c_int        { (*(sd as *mut MapSessionData)).invis as c_int } // truncates float to int; matches C
#[no_mangle] pub unsafe extern "C" fn sl_pc_damage(sd: *mut c_void) -> c_int       { (*(sd as *mut MapSessionData)).damage as c_int } // truncates float to int; matches C
#[no_mangle] pub unsafe extern "C" fn sl_pc_crit(sd: *mut c_void) -> c_int         { (*(sd as *mut MapSessionData)).crit }
#[no_mangle] pub unsafe extern "C" fn sl_pc_critchance(sd: *mut c_void) -> c_int   { (*(sd as *mut MapSessionData)).critchance }
#[no_mangle] pub unsafe extern "C" fn sl_pc_critmult(sd: *mut c_void) -> c_int     { (*(sd as *mut MapSessionData)).critmult as c_int } // truncates float to int; matches C
#[no_mangle] pub unsafe extern "C" fn sl_pc_rangeTarget(sd: *mut c_void) -> c_int  { (*(sd as *mut MapSessionData)).rangeTarget as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_exchange_gold(sd: *mut c_void) -> c_int  { (*(sd as *mut MapSessionData)).exchange.gold as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_exchange_count(sd: *mut c_void) -> c_int { (*(sd as *mut MapSessionData)).exchange.item_count }
#[no_mangle] pub unsafe extern "C" fn sl_pc_bod_count(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).boditems.bod_count }
#[no_mangle] pub unsafe extern "C" fn sl_pc_paralyzed(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).paralyzed }
#[no_mangle] pub unsafe extern "C" fn sl_pc_blind(sd: *mut c_void) -> c_int        { (*(sd as *mut MapSessionData)).blind }
#[no_mangle] pub unsafe extern "C" fn sl_pc_drunk(sd: *mut c_void) -> c_int        { (*(sd as *mut MapSessionData)).drunk }
#[no_mangle] pub unsafe extern "C" fn sl_pc_board(sd: *mut c_void) -> c_int        { (*(sd as *mut MapSessionData)).board }
#[no_mangle] pub unsafe extern "C" fn sl_pc_board_candel(sd: *mut c_void) -> c_int { (*(sd as *mut MapSessionData)).board_candel }
#[no_mangle] pub unsafe extern "C" fn sl_pc_board_canwrite(sd: *mut c_void) -> c_int { (*(sd as *mut MapSessionData)).board_canwrite }
#[no_mangle] pub unsafe extern "C" fn sl_pc_boardshow(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).boardshow as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_boardnameval(sd: *mut c_void) -> c_int { (*(sd as *mut MapSessionData)).boardnameval as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_msPing(sd: *mut c_void) -> c_int       { (*(sd as *mut MapSessionData)).msPing }
#[no_mangle] pub unsafe extern "C" fn sl_pc_pbColor(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).pbColor }
#[no_mangle] pub unsafe extern "C" fn sl_pc_coref(sd: *mut c_void) -> c_int        { (*(sd as *mut MapSessionData)).coref as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_optFlags(sd: *mut c_void) -> c_int     { (*(sd as *mut MapSessionData)).optFlags as c_int } // c_ulong (64-bit); truncates to low 32 bits; matches C
#[no_mangle] pub unsafe extern "C" fn sl_pc_snare(sd: *mut c_void) -> c_int        { (*(sd as *mut MapSessionData)).snare }
#[no_mangle] pub unsafe extern "C" fn sl_pc_silence(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).silence }
#[no_mangle] pub unsafe extern "C" fn sl_pc_extendhit(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).extendhit }
#[no_mangle] pub unsafe extern "C" fn sl_pc_afk(sd: *mut c_void) -> c_int          { (*(sd as *mut MapSessionData)).afk }
#[no_mangle] pub unsafe extern "C" fn sl_pc_afktime(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).afktime }
#[no_mangle] pub unsafe extern "C" fn sl_pc_totalafktime(sd: *mut c_void) -> c_int { (*(sd as *mut MapSessionData)).totalafktime }
#[no_mangle] pub unsafe extern "C" fn sl_pc_backstab(sd: *mut c_void) -> c_int     { (*(sd as *mut MapSessionData)).backstab }
#[no_mangle] pub unsafe extern "C" fn sl_pc_flank(sd: *mut c_void) -> c_int        { (*(sd as *mut MapSessionData)).flank }
#[no_mangle] pub unsafe extern "C" fn sl_pc_healing(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).healing }
#[no_mangle] pub unsafe extern "C" fn sl_pc_minSdam(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).minSdam }
#[no_mangle] pub unsafe extern "C" fn sl_pc_maxSdam(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).maxSdam }
#[no_mangle] pub unsafe extern "C" fn sl_pc_minLdam(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).minLdam }
#[no_mangle] pub unsafe extern "C" fn sl_pc_maxLdam(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).maxLdam }
#[no_mangle] pub unsafe extern "C" fn sl_pc_talktype(sd: *mut c_void) -> c_int     { (*(sd as *mut MapSessionData)).talktype as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_equipid(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).equipid as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_takeoffid(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).takeoffid as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_breakid(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).breakid as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_equipslot(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).equipslot as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_invslot(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).invslot as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_pickuptype(sd: *mut c_void) -> c_int   { (*(sd as *mut MapSessionData)).pickuptype as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_spottraps(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).spottraps as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_fury(sd: *mut c_void) -> c_int         { (*(sd as *mut MapSessionData)).fury as c_int } // truncates float to int; matches C
// status.equip[EQ_FACEACCTWO] — Item.id and Item.custom are both u32
#[no_mangle] pub unsafe extern "C" fn sl_pc_faceacctwo_id(sd: *mut c_void) -> c_int {
    (*(sd as *mut MapSessionData)).status.equip[EQ_FACEACCTWO as usize].id as c_int
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_faceacctwo_custom(sd: *mut c_void) -> c_int {
    (*(sd as *mut MapSessionData)).status.equip[EQ_FACEACCTWO as usize].custom as c_int
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_protection(sd: *mut c_void) -> c_int   { (*(sd as *mut MapSessionData)).protection as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_clone(sd: *mut c_void) -> c_int        { (*(sd as *mut MapSessionData)).clone as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_wisdom(sd: *mut c_void) -> c_int       { (*(sd as *mut MapSessionData)).wisdom }
#[no_mangle] pub unsafe extern "C" fn sl_pc_con(sd: *mut c_void) -> c_int          { (*(sd as *mut MapSessionData)).con as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_deathflag(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).deathflag as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_selfbar(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).selfbar as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_groupbars(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).groupbars as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_mobbars(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).mobbars as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_disptimertick(sd: *mut c_void) -> c_int { (*(sd as *mut MapSessionData)).disptimertick as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_bindmap(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).bindmap as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_bindx(sd: *mut c_void) -> c_int        { (*(sd as *mut MapSessionData)).bindx }
#[no_mangle] pub unsafe extern "C" fn sl_pc_bindy(sd: *mut c_void) -> c_int        { (*(sd as *mut MapSessionData)).bindy }
#[no_mangle] pub unsafe extern "C" fn sl_pc_ambushtimer(sd: *mut c_void) -> c_int  { (*(sd as *mut MapSessionData)).ambushtimer as c_int } // c_ulong (64-bit); truncates to low 32 bits; matches C
#[no_mangle] pub unsafe extern "C" fn sl_pc_dialogtype(sd: *mut c_void) -> c_int   { (*(sd as *mut MapSessionData)).dialogtype as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_dialogtype(sd: *mut c_void, v: c_int) { (*(sd as *mut MapSessionData)).dialogtype = v as i8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_cursed(sd: *mut c_void) -> c_int       { (*(sd as *mut MapSessionData)).cursed as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_action(sd: *mut c_void) -> c_int       { (*(sd as *mut MapSessionData)).action as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_scripttick(sd: *mut c_void) -> c_int   { (*(sd as *mut MapSessionData)).scripttick }
#[no_mangle] pub unsafe extern "C" fn sl_pc_dmgshield(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).dmgshield as c_int } // truncates float to int; matches C
#[no_mangle] pub unsafe extern "C" fn sl_pc_dmgdealt(sd: *mut c_void) -> c_int     { (*(sd as *mut MapSessionData)).dmgdealt as c_int } // truncates float to int; matches C
#[no_mangle] pub unsafe extern "C" fn sl_pc_dmgtaken(sd: *mut c_void) -> c_int     { (*(sd as *mut MapSessionData)).dmgtaken as c_int } // truncates float to int; matches C

// String getters — direct USER char array fields; return pointer to first element.
#[no_mangle] pub unsafe extern "C" fn sl_pc_ipaddress(sd: *mut c_void) -> *const c_char {
    (*(sd as *mut MapSessionData)).ipaddress.as_ptr()
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_speech(sd: *mut c_void) -> *const c_char {
    (*(sd as *mut MapSessionData)).speech.as_ptr()
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_question(sd: *mut c_void) -> *const c_char {
    (*(sd as *mut MapSessionData)).question.as_ptr()
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_mail(sd: *mut c_void) -> *const c_char {
    (*(sd as *mut MapSessionData)).mail.as_ptr()
}

// ─── Read: GFX fields ────────────────────────────────────────────────────────
// Mirrors the sl_pc_gfx_* block from c_src/sl_compat.c lines 571-598.
// GfxViewer name mapping (C camelCase to Rust snake_case):
//   faceAcc -> face_acc   cfaceAcc -> cface_acc
//   faceAccT -> face_acc_t   cfaceAccT -> cface_acc_t
// u16 fields: weapon, armor, helm, face_acc, crown, shield, necklace, mantle, boots, face_acc_t
// u8  fields: cweapon, carmor, chelm, cface_acc, ccrown, cshield, cnecklace, cmantle, cboots,
//             cface_acc_t, hair, chair, face, cface, cskin, dye

#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_face(sd: *mut c_void) -> c_int     { (*(sd as *mut MapSessionData)).gfx.face as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_hair(sd: *mut c_void) -> c_int     { (*(sd as *mut MapSessionData)).gfx.hair as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_chair(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).gfx.chair as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_cface(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).gfx.cface as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_cskin(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).gfx.cskin as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_dye(sd: *mut c_void) -> c_int      { (*(sd as *mut MapSessionData)).gfx.dye as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_weapon(sd: *mut c_void) -> c_int   { (*(sd as *mut MapSessionData)).gfx.weapon as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_cweapon(sd: *mut c_void) -> c_int  { (*(sd as *mut MapSessionData)).gfx.cweapon as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_armor(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).gfx.armor as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_carmor(sd: *mut c_void) -> c_int   { (*(sd as *mut MapSessionData)).gfx.carmor as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_shield(sd: *mut c_void) -> c_int   { (*(sd as *mut MapSessionData)).gfx.shield as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_cshield(sd: *mut c_void) -> c_int  { (*(sd as *mut MapSessionData)).gfx.cshield as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_helm(sd: *mut c_void) -> c_int     { (*(sd as *mut MapSessionData)).gfx.helm as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_chelm(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).gfx.chelm as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_mantle(sd: *mut c_void) -> c_int   { (*(sd as *mut MapSessionData)).gfx.mantle as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_cmantle(sd: *mut c_void) -> c_int  { (*(sd as *mut MapSessionData)).gfx.cmantle as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_crown(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).gfx.crown as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_ccrown(sd: *mut c_void) -> c_int   { (*(sd as *mut MapSessionData)).gfx.ccrown as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_faceAcc(sd: *mut c_void) -> c_int  { (*(sd as *mut MapSessionData)).gfx.face_acc as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_cfaceAcc(sd: *mut c_void) -> c_int { (*(sd as *mut MapSessionData)).gfx.cface_acc as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_faceAccT(sd: *mut c_void) -> c_int { (*(sd as *mut MapSessionData)).gfx.face_acc_t as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_cfaceAccT(sd: *mut c_void) -> c_int{ (*(sd as *mut MapSessionData)).gfx.cface_acc_t as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_boots(sd: *mut c_void) -> c_int    { (*(sd as *mut MapSessionData)).gfx.boots as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_cboots(sd: *mut c_void) -> c_int   { (*(sd as *mut MapSessionData)).gfx.cboots as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_necklace(sd: *mut c_void) -> c_int { (*(sd as *mut MapSessionData)).gfx.necklace as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_cnecklace(sd: *mut c_void) -> c_int{ (*(sd as *mut MapSessionData)).gfx.cnecklace as c_int }
#[no_mangle] pub unsafe extern "C" fn sl_pc_gfx_name(sd: *mut c_void) -> *const c_char {
    (*(sd as *mut MapSessionData)).gfx.name.as_ptr()
}

// ─── Read: computed / indirect fields ────────────────────────────────────────
// Mirrors sl_pc_actid .. sl_pc_classNameMark from c_src/sl_compat.c lines 473-479.
//
// Ownership notes:
//   clif_getaccountemail  — C allocates a 255-byte heap buffer; caller owns it.
//                           Leaked here exactly as the original C code did; Lua
//                           copies the string via lua_pushstring before returning.
//   rust_classdb_name     — returns a CString::into_raw pointer; caller must free
//                           with rust_classdb_free_name.  Leaked here to match the
//                           original C sl_pc_className / sl_pc_baseClassName behaviour.
//   rust_clandb_name      — returns a pointer into an interned static table; never
//                           freed by the caller.

// ─── Method wrappers: extern declarations ────────────────────────────────────

extern "C" {
    // health/state packets (all are #[no_mangle] Rust)
    fn clif_send_pc_healthscript(sd: *mut MapSessionData, damage: c_int, crit: c_int) -> c_int;
    fn clif_sendstatus(sd: *mut MapSessionData, flags: c_int) -> c_int;
    fn clif_sendupdatestatus_onequip(sd: *mut MapSessionData) -> c_int;
    fn clif_send_pc_health(sd: *mut MapSessionData, damage: c_int, critical: c_int) -> c_int;
    fn sl_async_freeco(sd: *mut c_void);
    fn sl_intif_save(sd: *mut c_void) -> c_int;

    // pc_* — all Rust (rust_pc_*) accessed via link_name
    #[link_name = "rust_pc_diescript"] fn pc_diescript(sd: *mut MapSessionData) -> c_int;
    #[link_name = "rust_pc_res"]       fn pc_res(sd: *mut MapSessionData) -> c_int;
    #[link_name = "rust_pc_calcstat"]  fn pc_calcstat(sd: *mut MapSessionData) -> c_int;
    #[link_name = "rust_pc_requestmp"] fn pc_requestmp(sd: *mut MapSessionData) -> c_int;
    #[link_name = "rust_pc_warp"]      fn pc_warp(sd: *mut MapSessionData, m: c_int, x: c_int, y: c_int) -> c_int;
    #[link_name = "rust_pc_setpos"]    fn pc_setpos(sd: *mut MapSessionData, m: c_int, x: c_int, y: c_int) -> c_int;
    #[link_name = "rust_pc_getitemscript"]  fn pc_getitemscript(sd: *mut MapSessionData, id: c_int) -> c_int;
    #[link_name = "rust_pc_loaditem"]       fn pc_loaditem(sd: *mut MapSessionData) -> c_int;
    #[link_name = "rust_pc_equipscript"]    fn pc_equipscript(sd: *mut MapSessionData) -> c_int;
    #[link_name = "rust_pc_unequipscript"]  fn pc_unequipscript(sd: *mut MapSessionData) -> c_int;
    #[link_name = "rust_pc_loadmagic"]      fn pc_loadmagic(sd: *mut MapSessionData) -> c_int;
    #[link_name = "rust_pc_checklevel"]     fn pc_checklevel(sd: *mut MapSessionData) -> c_int;
    fn pc_delitem(sd: *mut MapSessionData, idx: c_int, amount: c_int, flag: c_int) -> c_int;
    #[link_name = "rust_pc_dropitemmap"]
    fn pc_dropitemmap(sd: *mut MapSessionData, id: c_int, all: c_int) -> c_int;
    fn pc_isinvenspace(sd: *mut MapSessionData, id: c_int, owner: c_int,
        engrave: *const c_char, cl: c_uint, clc: c_uint, ci: c_uint, cic: c_uint) -> c_int;

    // movement / display (all #[no_mangle] Rust)
    fn clif_refreshnoclick(sd: *mut MapSessionData) -> c_int;
    fn clif_spawn(sd: *mut MapSessionData) -> c_int;
    fn clif_noparsewalk(sd: *mut MapSessionData, speed: i8) -> c_int;
    fn clif_blockmovement(sd: *mut MapSessionData, flag: c_int) -> c_int;
    fn clif_parseattack(sd: *mut MapSessionData) -> c_int;
    fn clif_throwitem_script(sd: *mut MapSessionData) -> c_int;
    fn clif_sendadditem(sd: *mut MapSessionData, num: c_int) -> c_int;
    fn clif_checkinvbod(sd: *mut MapSessionData) -> c_int;
    fn clif_deductarmor(sd: *mut MapSessionData, hit: c_int) -> c_int;
    fn clif_deductweapon(sd: *mut MapSessionData, hit: c_int) -> c_int;
    fn clif_deductdura(sd: *mut MapSessionData, equip: c_int, val: c_int) -> c_int;
    fn clif_deductduraequip(sd: *mut MapSessionData) -> c_int;
    fn clif_sendminimap(sd: *mut MapSessionData) -> c_int;
    fn clif_guitextsd(msg: *const c_char, sd: *mut MapSessionData) -> c_int;
    fn clif_sendminitext(sd: *mut MapSessionData, msg: *const c_char) -> c_int;
    fn boards_showposts(sd: *mut MapSessionData, id: c_int) -> c_int;
    fn boards_readpost(sd: *mut MapSessionData, id: c_int, post: c_int) -> c_int;
    fn clif_sendxychange(sd: *mut MapSessionData, x: c_int, y: c_int) -> c_int;
    fn clif_sendscriptsay(sd: *mut MapSessionData, msg: *const c_char, len: c_int, kind: c_int) -> c_int;
    fn nmail_sendmail(sd: *mut MapSessionData, to: *const c_char, topic: *const c_char, msg: *const c_char) -> c_int;
    fn clif_mob_damage(sd: *mut MapSessionData, mob: *mut MobSpawnData) -> c_int;
    fn clif_pc_damage(sd: *mut MapSessionData, src: *mut MapSessionData) -> c_int;
    fn clif_sendurl(sd: *mut MapSessionData, kind: c_int, url: *const c_char) -> c_int;
    fn clif_parselookat_scriptsub(sd: *mut MapSessionData, bl: *mut BlockList) -> c_int;
    fn clif_mystaytus(sd: *mut MapSessionData) -> c_int;
    fn clif_send_duration(sd: *mut MapSessionData, id: c_int, time: c_int, tsd: *mut MapSessionData) -> c_int;
    fn clif_send_aether(sd: *mut MapSessionData, id: c_int, time: c_int) -> c_int;
    fn clif_send_timer(sd: *mut MapSessionData, timer_type: i8, length: c_uint);

    // map lookups
    #[link_name = "map_id2bl"]  fn map_id2bl_acc(id: c_uint) -> *mut BlockList;
    #[link_name = "map_id2mob"] fn map_id2mob_acc(id: c_uint) -> *mut MobSpawnData;
    #[link_name = "map_id2sd"]  fn map_id2sd_acc(id: c_uint) -> *mut MapSessionData;

    // magicdb
    #[link_name = "rust_magicdb_id"]    fn magicdb_id(s: *const c_char) -> c_int;
    #[link_name = "rust_magicdb_name"]  fn magicdb_name(id: c_int) -> *mut c_char;
    #[link_name = "rust_magicdb_yname"] fn magicdb_yname(id: c_int) -> *mut c_char;

    // pc item ops
    #[link_name = "rust_pc_additem"] fn pc_additem_acc(sd: *mut MapSessionData, fl: *mut crate::servers::char::charstatus::Item) -> c_int;
}

extern "C" {
    fn clif_isregistered(id: c_uint) -> c_int;
    fn clif_getaccountemail(id: c_uint) -> *const c_char;

    #[link_name = "rust_clandb_name"]
    fn clandb_name_ffi(id: c_int) -> *const c_char;

    #[link_name = "rust_classdb_path"]
    fn classdb_path_ffi(id: c_int) -> c_int;

    /// Returns a caller-owned string; free with rust_classdb_free_name.
    #[link_name = "rust_classdb_name"]
    fn classdb_name_ffi(id: c_int, rank: c_int) -> *mut c_char;
}

/// Returns 1 if the account is registered, 0 otherwise.
/// Delegates to `clif_isregistered` (still in C / map_parse.c).
#[no_mangle]
pub unsafe extern "C" fn sl_pc_actid(sd: *mut c_void) -> c_int {
    clif_isregistered((*(sd as *mut MapSessionData)).status.id as c_uint)
}

/// Returns a heap-allocated email string (or NULL).
/// The pointer is leaked — Lua copies the string immediately, matching C behaviour.
#[no_mangle]
pub unsafe extern "C" fn sl_pc_email(sd: *mut c_void) -> *const c_char {
    clif_getaccountemail((*(sd as *mut MapSessionData)).status.id as c_uint)
}

/// Returns the interned clan name for this character's clan id.
#[no_mangle]
pub unsafe extern "C" fn sl_pc_clanname(sd: *mut c_void) -> *const c_char {
    clandb_name_ffi((*(sd as *mut MapSessionData)).status.clan as c_int)
}

/// Returns the path (base class id) for this character's class.
#[no_mangle]
pub unsafe extern "C" fn sl_pc_baseclass(sd: *mut c_void) -> c_int {
    classdb_path_ffi((*(sd as *mut MapSessionData)).status.class as c_int)
}

/// Returns the display name of the base class (path, rank 0).
/// The returned pointer is a leaked CString — Lua copies it immediately.
#[no_mangle]
pub unsafe extern "C" fn sl_pc_baseClassName(sd: *mut c_void) -> *mut c_char {
    let path = classdb_path_ffi((*(sd as *mut MapSessionData)).status.class as c_int);
    classdb_name_ffi(path, 0)
}

/// Returns the display name of the character's class at rank 0.
/// The returned pointer is a leaked CString — Lua copies it immediately.
#[no_mangle]
pub unsafe extern "C" fn sl_pc_className(sd: *mut c_void) -> *mut c_char {
    classdb_name_ffi((*(sd as *mut MapSessionData)).status.class as c_int, 0)
}

/// Returns the display name of the character's class at their current mark (rank).
/// The returned pointer is a leaked CString — Lua copies it immediately.
#[no_mangle]
pub unsafe extern "C" fn sl_pc_classNameMark(sd: *mut c_void) -> *mut c_char {
    let sd = sd as *mut MapSessionData;
    classdb_name_ffi((*sd).status.class as c_int, (*sd).status.mark as c_int)
}

// ─── Write: direct field setters ─────────────────────────────────────────────
// Each setter takes a c_int and writes the field with the appropriate cast.
// sl_pc_set_sleep, sl_pc_set_target, sl_pc_set_dialogtype are already ported above.
//
// status.* field types (Rust):
//   hp/mp/basehp/basemp/exp/money/bankmoney/maxslots/partner/clan/killedby/killspk/pkduration
//   heroes/mini_map_toggle/basemight/basewill/basegrace → u32
//   level/totem/class/tier/mark/maxinv/pk/profile_*     → u8
//   hair/hair_color/face_color/armor_color/skin_color/face/setting_flags → u16
//   gm_level/sex/country/state/side/clan_chat/novice_chat/subpath_chat/mute/alignment → i8
//   basearmor/clan_rank/class_rank → i32    karma → f32
// Direct USER field types:
//   max_hp/max_mp/attacker/rangeTarget/last_click/coref_container → c_uint
//   rage/invis/damage/deduction/critmult/dmgshield/fury → f32
//   dmgdealt/dmgtaken → f64
//   disguise/disguise_color/bindmap → u16
//   talktype/confused/spottraps/cursed/fakeDrop → u8
//   boardshow/boardnameval/selfbar/groupbars/mobbars/clone/deathflag → i8
//   paralyzed/blind/drunk/snare/silence/extendhit/afk/npc_g/npc_gc/time/polearm
//   speed/crit/critchance/backstab/flank/healing/wisdom/bindx/bindy
//   board_candel/board_canwrite/msPing/pbColor → c_int
//   protection/miss → c_short
//   optFlags/uFlags → c_ulong (XOR ops)

// status.* setters
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_hp(sd: *mut c_void, v: c_int)         { (*(sd as *mut MapSessionData)).status.hp = v as u32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_mp(sd: *mut c_void, v: c_int)         { (*(sd as *mut MapSessionData)).status.mp = v as u32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_exp(sd: *mut c_void, v: c_int)        { (*(sd as *mut MapSessionData)).status.exp = v as u32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_level(sd: *mut c_void, v: c_int)      { (*(sd as *mut MapSessionData)).status.level = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_class(sd: *mut c_void, v: c_int)      { (*(sd as *mut MapSessionData)).status.class = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_totem(sd: *mut c_void, v: c_int)      { (*(sd as *mut MapSessionData)).status.totem = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_tier(sd: *mut c_void, v: c_int)       { (*(sd as *mut MapSessionData)).status.tier = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_mark(sd: *mut c_void, v: c_int)       { (*(sd as *mut MapSessionData)).status.mark = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_country(sd: *mut c_void, v: c_int)    { (*(sd as *mut MapSessionData)).status.country = v as i8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_clan(sd: *mut c_void, v: c_int)       { (*(sd as *mut MapSessionData)).status.clan = v as u32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gm_level(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).status.gm_level = v as i8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_side(sd: *mut c_void, v: c_int)       { (*(sd as *mut MapSessionData)).status.side = v as i8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_state(sd: *mut c_void, v: c_int)      { (*(sd as *mut MapSessionData)).status.state = v as i8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_hair(sd: *mut c_void, v: c_int)       { (*(sd as *mut MapSessionData)).status.hair = v as u16; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_hair_color(sd: *mut c_void, v: c_int) { (*(sd as *mut MapSessionData)).status.hair_color = v as u16; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_face_color(sd: *mut c_void, v: c_int) { (*(sd as *mut MapSessionData)).status.face_color = v as u16; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_armor_color(sd: *mut c_void, v: c_int){ (*(sd as *mut MapSessionData)).status.armor_color = v as u16; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_skin_color(sd: *mut c_void, v: c_int) { (*(sd as *mut MapSessionData)).status.skin_color = v as u16; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_face(sd: *mut c_void, v: c_int)       { (*(sd as *mut MapSessionData)).status.face = v as u16; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_money(sd: *mut c_void, v: c_int)      { (*(sd as *mut MapSessionData)).status.money = v as u32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_bankmoney(sd: *mut c_void, v: c_int)  { (*(sd as *mut MapSessionData)).status.bankmoney = v as u32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_maxslots(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).status.maxslots = v as u32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_maxinv(sd: *mut c_void, v: c_int)     { (*(sd as *mut MapSessionData)).status.maxinv = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_partner(sd: *mut c_void, v: c_int)    { (*(sd as *mut MapSessionData)).status.partner = v as u32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_pk(sd: *mut c_void, v: c_int)         { (*(sd as *mut MapSessionData)).status.pk = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_basehp(sd: *mut c_void, v: c_int)     { (*(sd as *mut MapSessionData)).status.basehp = v as u32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_basemp(sd: *mut c_void, v: c_int)     { (*(sd as *mut MapSessionData)).status.basemp = v as u32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_karma(sd: *mut c_void, v: c_int)      { (*(sd as *mut MapSessionData)).status.karma = v as f32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_alignment(sd: *mut c_void, v: c_int)  { (*(sd as *mut MapSessionData)).status.alignment = v as i8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_basegrace(sd: *mut c_void, v: c_int)  { (*(sd as *mut MapSessionData)).status.basegrace = v as u32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_basemight(sd: *mut c_void, v: c_int)  { (*(sd as *mut MapSessionData)).status.basemight = v as u32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_basewill(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).status.basewill = v as u32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_basearmor(sd: *mut c_void, v: c_int)  { (*(sd as *mut MapSessionData)).status.basearmor = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_novice_chat(sd: *mut c_void, v: c_int){ (*(sd as *mut MapSessionData)).status.novice_chat = v as i8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_subpath_chat(sd: *mut c_void, v: c_int){ (*(sd as *mut MapSessionData)).status.subpath_chat = v as i8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_clan_chat(sd: *mut c_void, v: c_int)  { (*(sd as *mut MapSessionData)).status.clan_chat = v as i8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_tutor(sd: *mut c_void, v: c_int)      { (*(sd as *mut MapSessionData)).status.tutor = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_profile_vitastats(sd: *mut c_void, v: c_int) { (*(sd as *mut MapSessionData)).status.profile_vitastats = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_profile_equiplist(sd: *mut c_void, v: c_int) { (*(sd as *mut MapSessionData)).status.profile_equiplist = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_profile_legends(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).status.profile_legends = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_profile_spells(sd: *mut c_void, v: c_int)    { (*(sd as *mut MapSessionData)).status.profile_spells = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_profile_inventory(sd: *mut c_void, v: c_int) { (*(sd as *mut MapSessionData)).status.profile_inventory = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_profile_bankitems(sd: *mut c_void, v: c_int) { (*(sd as *mut MapSessionData)).status.profile_bankitems = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_mute(sd: *mut c_void, v: c_int)       { (*(sd as *mut MapSessionData)).status.mute = v as i8; }
// C casts to (unsigned int) but Rust field is u16; low 16 bits are preserved identically.
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_settingFlags(sd: *mut c_void, v: c_int) { (*(sd as *mut MapSessionData)).status.setting_flags = v as u16; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_heroshow(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).status.heroes = v as u32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_sex(sd: *mut c_void, v: c_int)        { (*(sd as *mut MapSessionData)).status.sex = v as i8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_classRank(sd: *mut c_void, v: c_int)  { (*(sd as *mut MapSessionData)).status.class_rank = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_clanRank(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).status.clan_rank = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_setminimaptoggle(sd: *mut c_void, v: c_int) { (*(sd as *mut MapSessionData)).status.mini_map_toggle = v as u32; }

// direct USER field setters
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_max_hp(sd: *mut c_void, v: c_int)     { (*(sd as *mut MapSessionData)).max_hp = v as c_uint; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_max_mp(sd: *mut c_void, v: c_int)     { (*(sd as *mut MapSessionData)).max_mp = v as c_uint; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_npc_g(sd: *mut c_void, v: c_int)      { (*(sd as *mut MapSessionData)).npc_g = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_npc_gc(sd: *mut c_void, v: c_int)     { (*(sd as *mut MapSessionData)).npc_gc = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_last_click(sd: *mut c_void, v: c_int) { (*(sd as *mut MapSessionData)).last_click = v as c_uint; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_time(sd: *mut c_void, v: c_int)       { (*(sd as *mut MapSessionData)).time = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_rage(sd: *mut c_void, v: c_int)       { (*(sd as *mut MapSessionData)).rage = v as f32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_polearm(sd: *mut c_void, v: c_int)    { (*(sd as *mut MapSessionData)).polearm = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_deduction(sd: *mut c_void, v: c_int)  { (*(sd as *mut MapSessionData)).deduction = v as f32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_speed(sd: *mut c_void, v: c_int)      { (*(sd as *mut MapSessionData)).speed = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_attacker(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).attacker = v as c_uint; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_invis(sd: *mut c_void, v: c_int)      { (*(sd as *mut MapSessionData)).invis = v as f32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_damage(sd: *mut c_void, v: c_int)     { (*(sd as *mut MapSessionData)).damage = v as f32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_crit(sd: *mut c_void, v: c_int)       { (*(sd as *mut MapSessionData)).crit = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_critchance(sd: *mut c_void, v: c_int) { (*(sd as *mut MapSessionData)).critchance = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_critmult(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).critmult = v as f32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_rangeTarget(sd: *mut c_void, v: c_int){ (*(sd as *mut MapSessionData)).rangeTarget = v as c_uint; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_disguise(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).disguise = v as u16; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_disguise_color(sd: *mut c_void, v: c_int) { (*(sd as *mut MapSessionData)).disguise_color = v as u16; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_paralyzed(sd: *mut c_void, v: c_int)  { (*(sd as *mut MapSessionData)).paralyzed = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_blind(sd: *mut c_void, v: c_int)      { (*(sd as *mut MapSessionData)).blind = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_drunk(sd: *mut c_void, v: c_int)      { (*(sd as *mut MapSessionData)).drunk = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_board_candel(sd: *mut c_void, v: c_int)  { (*(sd as *mut MapSessionData)).board_candel = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_board_canwrite(sd: *mut c_void, v: c_int){ (*(sd as *mut MapSessionData)).board_canwrite = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_boardshow(sd: *mut c_void, v: c_int)  { (*(sd as *mut MapSessionData)).boardshow = v as i8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_boardnameval(sd: *mut c_void, v: c_int){ (*(sd as *mut MapSessionData)).boardnameval = v as i8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_snare(sd: *mut c_void, v: c_int)      { (*(sd as *mut MapSessionData)).snare = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_silence(sd: *mut c_void, v: c_int)    { (*(sd as *mut MapSessionData)).silence = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_extendhit(sd: *mut c_void, v: c_int)  { (*(sd as *mut MapSessionData)).extendhit = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_afk(sd: *mut c_void, v: c_int)        { (*(sd as *mut MapSessionData)).afk = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_confused(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).confused = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_spottraps(sd: *mut c_void, v: c_int)  { (*(sd as *mut MapSessionData)).spottraps = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_selfbar(sd: *mut c_void, v: c_int)    { (*(sd as *mut MapSessionData)).selfbar = v as i8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_groupbars(sd: *mut c_void, v: c_int)  { (*(sd as *mut MapSessionData)).groupbars = v as i8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_mobbars(sd: *mut c_void, v: c_int)    { (*(sd as *mut MapSessionData)).mobbars = v as i8; }
// C uses (unsigned int) for the XOR mask but uFlags is c_ulong; XOR low 32 bits, upper preserved.
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_optFlags_xor(sd: *mut c_void, v: c_int) { (*(sd as *mut MapSessionData)).optFlags ^= v as c_uint as c_ulong; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_uflags_xor(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).uFlags ^= v as c_uint as c_ulong; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_talktype(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).talktype = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_cursed(sd: *mut c_void, v: c_int)     { (*(sd as *mut MapSessionData)).cursed = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_deathflag(sd: *mut c_void, v: c_int)  { (*(sd as *mut MapSessionData)).deathflag = v as i8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_bindmap(sd: *mut c_void, v: c_int)    { (*(sd as *mut MapSessionData)).bindmap = v as u16; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_bindx(sd: *mut c_void, v: c_int)      { (*(sd as *mut MapSessionData)).bindx = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_bindy(sd: *mut c_void, v: c_int)      { (*(sd as *mut MapSessionData)).bindy = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_protection(sd: *mut c_void, v: c_int) { (*(sd as *mut MapSessionData)).protection = v as c_short; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_dmgshield(sd: *mut c_void, v: c_int)  { (*(sd as *mut MapSessionData)).dmgshield = v as f32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_dmgdealt(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).dmgdealt = v as f64; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_dmgtaken(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).dmgtaken = v as f64; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_fakeDrop(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).fakeDrop = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_clone(sd: *mut c_void, v: c_int)      { (*(sd as *mut MapSessionData)).clone = v as i8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_fury(sd: *mut c_void, v: c_int)       { (*(sd as *mut MapSessionData)).fury = v as f32; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_coref_container(sd: *mut c_void, v: c_int) { (*(sd as *mut MapSessionData)).coref_container = v as c_uint; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_wisdom(sd: *mut c_void, v: c_int)     { (*(sd as *mut MapSessionData)).wisdom = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_con(sd: *mut c_void, v: c_int)        { (*(sd as *mut MapSessionData)).con = v as c_short; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_backstab(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).backstab = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_flank(sd: *mut c_void, v: c_int)      { (*(sd as *mut MapSessionData)).flank = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_healing(sd: *mut c_void, v: c_int)    { (*(sd as *mut MapSessionData)).healing = v; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_pbColor(sd: *mut c_void, v: c_int)    { (*(sd as *mut MapSessionData)).pbColor = v; }

// ─── Write: GFX setters ───────────────────────────────────────────────────────
// GfxViewer field types: u16 for weapon/armor/helm/face_acc/crown/shield/necklace/mantle/boots/face_acc_t
//                        u8  for cweapon/carmor/chelm/cface_acc/ccrown/cshield/cnecklace/cmantle/cboots/cface_acc_t
//                        u8  for hair/chair/face/cface/cskin/dye
// sl_pc_set_gfx_name is a string setter — ported below with bounded_copy.

#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_face(sd: *mut c_void, v: c_int)     { (*(sd as *mut MapSessionData)).gfx.face = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_hair(sd: *mut c_void, v: c_int)     { (*(sd as *mut MapSessionData)).gfx.hair = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_chair(sd: *mut c_void, v: c_int)    { (*(sd as *mut MapSessionData)).gfx.chair = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_cface(sd: *mut c_void, v: c_int)    { (*(sd as *mut MapSessionData)).gfx.cface = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_cskin(sd: *mut c_void, v: c_int)    { (*(sd as *mut MapSessionData)).gfx.cskin = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_dye(sd: *mut c_void, v: c_int)      { (*(sd as *mut MapSessionData)).gfx.dye = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_weapon(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).gfx.weapon = v as u16; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_cweapon(sd: *mut c_void, v: c_int)  { (*(sd as *mut MapSessionData)).gfx.cweapon = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_armor(sd: *mut c_void, v: c_int)    { (*(sd as *mut MapSessionData)).gfx.armor = v as u16; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_carmor(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).gfx.carmor = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_shield(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).gfx.shield = v as u16; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_cshield(sd: *mut c_void, v: c_int)  { (*(sd as *mut MapSessionData)).gfx.cshield = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_helm(sd: *mut c_void, v: c_int)     { (*(sd as *mut MapSessionData)).gfx.helm = v as u16; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_chelm(sd: *mut c_void, v: c_int)    { (*(sd as *mut MapSessionData)).gfx.chelm = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_mantle(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).gfx.mantle = v as u16; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_cmantle(sd: *mut c_void, v: c_int)  { (*(sd as *mut MapSessionData)).gfx.cmantle = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_crown(sd: *mut c_void, v: c_int)    { (*(sd as *mut MapSessionData)).gfx.crown = v as u16; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_ccrown(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).gfx.ccrown = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_faceAcc(sd: *mut c_void, v: c_int)  { (*(sd as *mut MapSessionData)).gfx.face_acc = v as u16; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_cfaceAcc(sd: *mut c_void, v: c_int) { (*(sd as *mut MapSessionData)).gfx.cface_acc = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_faceAccT(sd: *mut c_void, v: c_int) { (*(sd as *mut MapSessionData)).gfx.face_acc_t = v as u16; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_cfaceAccT(sd: *mut c_void, v: c_int){ (*(sd as *mut MapSessionData)).gfx.cface_acc_t = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_boots(sd: *mut c_void, v: c_int)    { (*(sd as *mut MapSessionData)).gfx.boots = v as u16; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_cboots(sd: *mut c_void, v: c_int)   { (*(sd as *mut MapSessionData)).gfx.cboots = v as u8; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_necklace(sd: *mut c_void, v: c_int) { (*(sd as *mut MapSessionData)).gfx.necklace = v as u16; }
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_cnecklace(sd: *mut c_void, v: c_int){ (*(sd as *mut MapSessionData)).gfx.cnecklace = v as u8; }

// ─── String setters (bounded_copy) ───────────────────────────────────────────
// Equivalent to: strncpy(dst, src ? src : "", max_len-1); dst[max_len-1] = 0;
// Used for all [i8; N] / [c_char; N] name/title/speech fields.

/// Copies at most `max_len - 1` bytes from `src` into `dst`, then null-terminates.
/// Mirrors the strncpy + explicit null pattern used throughout sl_compat.c.
///
/// # Safety
/// `dst` must point to a buffer of at least `max_len` bytes.
/// `src` may be null (treated as empty string).
unsafe fn bounded_copy(dst: *mut i8, src: *const c_char, max_len: usize) {
    if src.is_null() {
        *dst = 0;
        return;
    }
    let mut n = 0usize;
    while n < max_len - 1 && *src.add(n) != 0 {
        n += 1;
    }
    std::ptr::copy_nonoverlapping(src as *const i8, dst, n);
    *dst.add(n) = 0;
}

#[no_mangle] pub unsafe extern "C" fn sl_pc_set_gfx_name(sd: *mut c_void, v: *const c_char) {
    let sd = &mut *(sd as *mut MapSessionData);
    bounded_copy(sd.gfx.name.as_mut_ptr(), v, sd.gfx.name.len());
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_name(sd: *mut c_void, v: *const c_char) {
    let sd = &mut *(sd as *mut MapSessionData);
    bounded_copy(sd.status.name.as_mut_ptr(), v, sd.status.name.len());
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_title(sd: *mut c_void, v: *const c_char) {
    let sd = &mut *(sd as *mut MapSessionData);
    bounded_copy(sd.status.title.as_mut_ptr(), v, sd.status.title.len());
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_clan_title(sd: *mut c_void, v: *const c_char) {
    let sd = &mut *(sd as *mut MapSessionData);
    bounded_copy(sd.status.clan_title.as_mut_ptr(), v, sd.status.clan_title.len());
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_afkmessage(sd: *mut c_void, v: *const c_char) {
    let sd = &mut *(sd as *mut MapSessionData);
    bounded_copy(sd.status.afkmessage.as_mut_ptr(), v, sd.status.afkmessage.len());
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_speech(sd: *mut c_void, v: *const c_char) {
    let sd = &mut *(sd as *mut MapSessionData);
    bounded_copy(sd.speech.as_mut_ptr(), v, sd.speech.len());
}

// ─── Dispatcher accessors (used by src/game/client/mod.rs) ───────────────────

#[no_mangle] pub unsafe extern "C" fn sl_pc_fd(sd: *mut c_void) -> c_int {
    (*(sd as *mut MapSessionData)).fd
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_chat_timer(sd: *mut c_void) -> c_int {
    (*(sd as *mut MapSessionData)).chat_timer
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_chat_timer(sd: *mut c_void, v: c_int) {
    (*(sd as *mut MapSessionData)).chat_timer = v;
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_attacked(sd: *mut c_void) -> c_int {
    (*(sd as *mut MapSessionData)).attacked as c_int
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_attacked(sd: *mut c_void, v: c_int) {
    (*(sd as *mut MapSessionData)).attacked = v as i8;
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_loaded(sd: *mut c_void) -> c_int {
    (*(sd as *mut MapSessionData)).loaded as c_int
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_inventory_id(sd: *mut c_void, pos: c_int) -> c_uint {
    (*(sd as *mut MapSessionData)).status.inventory[pos as usize].id as c_uint
}

// ─── Regen overflow accumulators and group membership ────────────────────────

#[no_mangle] pub unsafe extern "C" fn sl_pc_set_vregenoverflow(sd: *mut c_void, v: c_int) {
    (*(sd as *mut MapSessionData)).vregenoverflow = v as f32;
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_mregenoverflow(sd: *mut c_void, v: c_int) {
    (*(sd as *mut MapSessionData)).mregenoverflow = v as f32;
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_group_count(sd: *mut c_void, v: c_int) {
    (*(sd as *mut MapSessionData)).group_count = v;
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_group_on(sd: *mut c_void, v: c_int) {
    (*(sd as *mut MapSessionData)).group_on = v;
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_set_group_leader(sd: *mut c_void, v: c_int) {
    (*(sd as *mut MapSessionData)).group_leader = v as c_uint;
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_group_count(sd: *mut c_void) -> c_int {
    (*(sd as *mut MapSessionData)).group_count
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_group_on(sd: *mut c_void) -> c_int {
    (*(sd as *mut MapSessionData)).group_on
}
#[no_mangle] pub unsafe extern "C" fn sl_pc_group_leader(sd: *mut c_void) -> c_int {
    (*(sd as *mut MapSessionData)).group_leader as c_int
}

extern "C" {
    #[link_name = "groups"]
    static mut pc_acc_groups: [c_uint; 65536]; // groups[256][256]
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_getgroup(sd: *mut c_void, out: *mut c_uint, max: c_int) -> c_int {
    const MAX_MEMBERS: usize = 256;
    let user = &*(sd as *mut MapSessionData);
    if user.group_count > 0 {
        let n = user.group_count.min(max) as usize;
        let gid = (user.groupid as usize).min(255);
        for i in 0..n {
            *out.add(i) = pc_acc_groups[gid * MAX_MEMBERS + i];
        }
        return n as c_int;
    }
    if max > 0 { *out = user.status.id; }
    1
}

// ─── sl_pc method wrappers (ported from c_src/sl_compat.c) ───────────────────

// ── Health ────────────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_addhealth(sd: *mut c_void, damage: c_int) {
    let sd = sd as *mut MapSessionData;
    clif_send_pc_healthscript(sd, -damage, 0);
    clif_sendstatus(sd, SFLAG_HPMP);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_removehealth(sd: *mut c_void, damage: c_int, caster: c_int) {
    let sd = sd as *mut MapSessionData;
    if caster > 0 { (*sd).attacker = caster as c_uint; }
    clif_send_pc_healthscript(sd, damage, 0);
    clif_sendstatus(sd, SFLAG_HPMP);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_freeasync(sd: *mut c_void) {
    sl_async_freeco(sd);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_forcesave(sd: *mut c_void) -> c_int {
    sl_intif_save(sd)
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_die(sd: *mut c_void) {
    pc_diescript(sd as *mut MapSessionData);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_resurrect(sd: *mut c_void) {
    pc_res(sd as *mut MapSessionData);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_showhealth(sd: *mut c_void, damage: c_int, kind: c_int) {
    clif_send_pc_health(sd as *mut MapSessionData, damage, kind);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_calcstat(sd: *mut c_void) {
    pc_calcstat(sd as *mut MapSessionData);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_sendstatus(sd: *mut c_void) {
    let sd = sd as *mut MapSessionData;
    pc_requestmp(sd);
    clif_sendstatus(sd, SFLAG_FULLSTATS | SFLAG_HPMP | SFLAG_XPMONEY);
    clif_sendupdatestatus_onequip(sd);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_status(sd: *mut c_void) -> c_int {
    clif_mystaytus(sd as *mut MapSessionData)
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_warp(sd: *mut c_void, m: c_int, x: c_int, y: c_int) {
    pc_warp(sd as *mut MapSessionData, m, x, y);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_refresh(sd: *mut c_void) {
    let sd = sd as *mut MapSessionData;
    pc_setpos(sd, (*sd).bl.m as c_int, (*sd).bl.x as c_int, (*sd).bl.y as c_int);
    clif_refreshnoclick(sd);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_pickup(sd: *mut c_void, id: c_uint) {
    pc_getitemscript(sd as *mut MapSessionData, id as c_int);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_throwitem(sd: *mut c_void) {
    clif_throwitem_script(sd as *mut MapSessionData);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_forcedrop(sd: *mut c_void, id: c_int) {
    pc_dropitemmap(sd as *mut MapSessionData, id, 0);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_lock(sd: *mut c_void) {
    clif_blockmovement(sd as *mut MapSessionData, 0);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_unlock(sd: *mut c_void) {
    clif_blockmovement(sd as *mut MapSessionData, 1);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_swing(sd: *mut c_void) {
    clif_parseattack(sd as *mut MapSessionData);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_respawn(sd: *mut c_void) {
    clif_spawn(sd as *mut MapSessionData);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_sendhealth(sd: *mut c_void, dmgf: f32, critical: c_int) -> c_int {
    let damage = if dmgf > 0.0 { (dmgf + 0.5) as c_int }
                 else if dmgf < 0.0 { (dmgf - 0.5) as c_int }
                 else { 0 };
    let critical = if critical == 1 { 33 } else if critical == 2 { 255 } else { critical };
    clif_send_pc_healthscript(sd as *mut MapSessionData, damage, critical);
    0
}

// ── Movement / UI ─────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_move(sd: *mut c_void, speed: c_int) {
    clif_noparsewalk(sd as *mut MapSessionData, speed as i8);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_lookat(sd: *mut c_void, id: c_int) {
    let bl = map_id2bl_acc(id as c_uint);
    if !bl.is_null() { clif_parselookat_scriptsub(sd as *mut MapSessionData, bl); }
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_minirefresh(sd: *mut c_void) {
    clif_refreshnoclick(sd as *mut MapSessionData);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_refreshinventory(sd: *mut c_void) {
    let sd = sd as *mut MapSessionData;
    for i in 0..MAX_INVENTORY as c_int { clif_sendadditem(sd, i); }
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_updateinv(sd: *mut c_void) {
    pc_loaditem(sd as *mut MapSessionData);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_checkinvbod(sd: *mut c_void) {
    clif_checkinvbod(sd as *mut MapSessionData);
}

// ── Equipment ────────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_equip(sd: *mut c_void) {
    pc_equipscript(sd as *mut MapSessionData);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_takeoff(sd: *mut c_void) {
    pc_unequipscript(sd as *mut MapSessionData);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_deductarmor(sd: *mut c_void, v: c_int) {
    clif_deductarmor(sd as *mut MapSessionData, v);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_deductweapon(sd: *mut c_void, v: c_int) {
    clif_deductweapon(sd as *mut MapSessionData, v);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_deductdura(sd: *mut c_void, eq: c_int, v: c_int) {
    clif_deductdura(sd as *mut MapSessionData, eq, v);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_deductduraequip(sd: *mut c_void) {
    clif_deductduraequip(sd as *mut MapSessionData);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_deductdurainv(sd: *mut c_void, slot: c_int, v: c_int) {
    let sd = sd as *mut MapSessionData;
    if slot >= 0 && (slot as usize) < MAX_INVENTORY {
        (*sd).status.inventory[slot as usize].dura -= v;
    }
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_hasequipped(sd: *mut c_void, item_id: c_uint) -> c_int {
    let sd = sd as *mut MapSessionData;
    for i in 0..MAX_EQUIP {
        if (*sd).status.equip[i].id == item_id { return 1; }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_removeitemslot(sd: *mut c_void, slot: c_int, amount: c_int, kind: c_int) {
    pc_delitem(sd as *mut MapSessionData, slot, amount, kind);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_hasitem(sd: *mut c_void, item_id: c_uint, amount: c_int) -> c_int {
    let sd = sd as *mut MapSessionData;
    let mut found = 0i32;
    for i in 0..MAX_INVENTORY {
        if (*sd).status.inventory[i].id == item_id {
            found += (*sd).status.inventory[i].amount as i32;
        }
    }
    if found >= amount { found } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_hasspace(sd: *mut c_void, item_id: c_uint) -> c_int {
    pc_isinvenspace(sd as *mut MapSessionData, item_id as c_int, 0, std::ptr::null(), 0, 0, 0, 0)
}

// ── Stats ─────────────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_checklevel(sd: *mut c_void) {
    pc_checklevel(sd as *mut MapSessionData);
}

// ── Display / UI ──────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_sendminimap(sd: *mut c_void) {
    clif_sendminimap(sd as *mut MapSessionData);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_popup(sd: *mut c_void, msg: *const c_char) {
    // clif_popup is in visual.rs
    use crate::game::client::visual::clif_popup;
    clif_popup(sd as *mut MapSessionData, msg);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_guitext(sd: *mut c_void, msg: *const c_char) {
    clif_guitextsd(msg, sd as *mut MapSessionData);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_sendminitext(sd: *mut c_void, msg: *const c_char) {
    clif_sendminitext(sd as *mut MapSessionData, msg);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_powerboard(_sd: *mut c_void) { /* stub */ }

#[no_mangle]
pub unsafe extern "C" fn sl_pc_showboard(sd: *mut c_void, id: c_int) {
    boards_showposts(sd as *mut MapSessionData, id);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_showpost(sd: *mut c_void, id: c_int, post: c_int) {
    boards_readpost(sd as *mut MapSessionData, id, post);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_changeview(sd: *mut c_void, x: c_int, y: c_int) {
    clif_sendxychange(sd as *mut MapSessionData, x, y);
}

// ── Social ────────────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_speak(sd: *mut c_void, msg: *const c_char, len: c_int, kind: c_int) {
    clif_sendscriptsay(sd as *mut MapSessionData, msg, len, kind);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_sendmail(sd: *mut c_void, to: *const c_char, topic: *const c_char, msg: *const c_char) -> c_int {
    nmail_sendmail(sd as *mut MapSessionData, to, topic, msg)
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_sendurl(sd: *mut c_void, kind: c_int, url: *const c_char) {
    clif_sendurl(sd as *mut MapSessionData, kind, url);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_swingtarget(sd: *mut c_void, id: c_int) {
    use crate::game::mob::BL_MOB;
    let bl = map_id2bl_acc(id as c_uint);
    if bl.is_null() { return; }
    let sd = sd as *mut MapSessionData;
    if (*bl).bl_type as c_int == BL_MOB {
        clif_mob_damage(sd, bl as *mut MobSpawnData);
    } else {
        clif_pc_damage(sd, bl as *mut MapSessionData);
    }
}

// ── Kill registry ─────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_killcount(sd: *mut c_void, mob_id: c_int) -> c_int {
    let sd = sd as *mut MapSessionData;
    for x in 0..MAX_KILLREG {
        if (*sd).status.killreg[x].mob_id == mob_id as u32 {
            return (*sd).status.killreg[x].amount as c_int;
        }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_setkillcount(sd: *mut c_void, mob_id: c_int, amount: c_int) {
    let sd = sd as *mut MapSessionData;
    for x in 0..MAX_KILLREG {
        if (*sd).status.killreg[x].mob_id == mob_id as u32 {
            (*sd).status.killreg[x].amount = amount as u32;
            return;
        }
    }
    for x in 0..MAX_KILLREG {
        if (*sd).status.killreg[x].mob_id == 0 {
            (*sd).status.killreg[x].mob_id = mob_id as u32;
            (*sd).status.killreg[x].amount = amount as u32;
            return;
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_flushkills(sd: *mut c_void, mob_id: c_int) {
    let sd = sd as *mut MapSessionData;
    for x in 0..MAX_KILLREG {
        if mob_id == 0 || (*sd).status.killreg[x].mob_id == mob_id as u32 {
            (*sd).status.killreg[x].mob_id = 0;
            (*sd).status.killreg[x].amount = 0;
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_flushallkills(sd: *mut c_void) {
    sl_pc_flushkills(sd, 0);
}

// ── Threat ────────────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_addthreat(sd: *mut c_void, mob_id: c_uint, amount: c_uint) {
    let mob = map_id2mob_acc(mob_id);
    if mob.is_null() { return; }
    let uid = (*(sd as *mut MapSessionData)).bl.id;
    (*mob).lastaction = libc::time(std::ptr::null_mut()) as c_int;
    for x in 0..MAX_THREATCOUNT {
        if (*mob).threat[x].user == uid { (*mob).threat[x].amount += amount; return; }
        if (*mob).threat[x].user == 0  { (*mob).threat[x].user = uid; (*mob).threat[x].amount = amount; return; }
    }
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_setthreat(sd: *mut c_void, mob_id: c_uint, amount: c_uint) {
    let mob = map_id2mob_acc(mob_id);
    if mob.is_null() { return; }
    let uid = (*(sd as *mut MapSessionData)).bl.id;
    (*mob).lastaction = libc::time(std::ptr::null_mut()) as c_int;
    for x in 0..MAX_THREATCOUNT {
        if (*mob).threat[x].user == uid { (*mob).threat[x].amount = amount; return; }
        if (*mob).threat[x].user == 0  { (*mob).threat[x].user = uid; (*mob).threat[x].amount = amount; return; }
    }
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_addthreatgeneral(_sd: *mut c_void, _amount: c_uint) { /* stub */ }

// ── Spell list ────────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_hasspell(sd: *mut c_void, name: *const c_char) -> c_int {
    let id = magicdb_id(name); if id <= 0 { return 0; }
    let sd = sd as *mut MapSessionData;
    for i in 0..MAX_SPELLS {
        if (*sd).status.skill[i] == id as u16 { return 1; }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_addspell(sd: *mut c_void, spell_id: c_int) {
    let sd = sd as *mut MapSessionData;
    for i in 0..MAX_SPELLS {
        if (*sd).status.skill[i] == 0 {
            (*sd).status.skill[i] = spell_id as u16;
            pc_loadmagic(sd);
            return;
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_removespell(sd: *mut c_void, spell_id: c_int) {
    let sd = sd as *mut MapSessionData;
    for i in 0..MAX_SPELLS {
        if (*sd).status.skill[i] == spell_id as u16 { (*sd).status.skill[i] = 0; }
    }
}

// ── Duration system ───────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_hasduration(sd: *mut c_void, name: *const c_char) -> c_int {
    let id = magicdb_id(name); if id <= 0 { return 0; }
    let sd = sd as *mut MapSessionData;
    for i in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[i].id == id as u16 && (*sd).status.dura_aether[i].duration > 0 { return 1; }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_hasdurationid(sd: *mut c_void, name: *const c_char, caster_id: c_int) -> c_int {
    let id = magicdb_id(name); if id <= 0 { return 0; }
    let sd = sd as *mut MapSessionData;
    for i in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[i].id == id as u16
            && (*sd).status.dura_aether[i].caster_id == caster_id as c_uint
            && (*sd).status.dura_aether[i].duration > 0 { return 1; }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_getduration(sd: *mut c_void, name: *const c_char) -> c_int {
    let id = magicdb_id(name); if id <= 0 { return 0; }
    let sd = sd as *mut MapSessionData;
    for i in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[i].id == id as u16 { return (*sd).status.dura_aether[i].duration; }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_getdurationid(sd: *mut c_void, name: *const c_char, caster_id: c_int) -> c_int {
    let id = magicdb_id(name); if id <= 0 { return 0; }
    let sd = sd as *mut MapSessionData;
    for i in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[i].id == id as u16
            && (*sd).status.dura_aether[i].caster_id == caster_id as c_uint {
            return (*sd).status.dura_aether[i].duration;
        }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_durationamount(sd: *mut c_void, name: *const c_char) -> c_int {
    let id = magicdb_id(name); if id <= 0 { return 0; }
    let sd = sd as *mut MapSessionData;
    let mut count = 0;
    for i in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[i].id == id as u16 && (*sd).status.dura_aether[i].duration > 0 { count += 1; }
    }
    count
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_setduration(sd: *mut c_void, name: *const c_char, mut time_ms: c_int, caster_id: c_int, recast: c_int) {
    let sd = sd as *mut MapSessionData;
    let id = magicdb_id(name); if id <= 0 { return; }
    if time_ms > 0 && time_ms < 1000 { time_ms = 1000; }
    let mut alreadycast = false;
    for x in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[x].id == id as u16
            && (*sd).status.dura_aether[x].caster_id == caster_id as c_uint
            && (*sd).status.dura_aether[x].duration > 0 { alreadycast = true; break; }
    }
    for x in 0..MAX_MAGIC_TIMERS {
        let da = &mut (*sd).status.dura_aether[x];
        if da.id == id as u16 && time_ms <= 0 && da.caster_id == caster_id as c_uint && alreadycast {
            let tsd = map_id2sd_acc(da.caster_id);
            clif_send_duration(sd, id, time_ms, tsd);
            da.duration = 0; da.caster_id = 0;
            if da.aether == 0 { da.id = 0; }
            return;
        } else if da.id == id as u16 && da.caster_id == caster_id as c_uint
            && da.aether > 0 && da.duration <= 0 {
            da.duration = time_ms;
            clif_send_duration(sd, id, time_ms / 1000, map_id2sd_acc(da.caster_id));
            return;
        } else if da.id == id as u16 && da.caster_id == caster_id as c_uint
            && (da.duration > time_ms || recast != 0) && alreadycast {
            da.duration = time_ms;
            clif_send_duration(sd, id, time_ms / 1000, map_id2sd_acc(da.caster_id));
            return;
        } else if da.id == 0 && da.duration == 0 && time_ms != 0 && !alreadycast {
            da.id = id as u16; da.duration = time_ms; da.caster_id = caster_id as c_uint;
            clif_send_duration(sd, id, time_ms / 1000, map_id2sd_acc(da.caster_id));
            return;
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_flushduration(sd: *mut c_void, _dispel_level: c_int, min_id: c_int, max_id: c_int) {
    let sd = sd as *mut MapSessionData;
    for x in 0..MAX_MAGIC_TIMERS {
        let id = (*sd).status.dura_aether[x].id as c_int;
        if id == 0 || (*sd).status.dura_aether[x].duration <= 0 { continue; }
        if min_id > 0 && id < min_id { continue; }
        if max_id > 0 && id > max_id { continue; }
        let tsd = map_id2sd_acc((*sd).status.dura_aether[x].caster_id);
        clif_send_duration(sd, id, 0, tsd);
        (*sd).status.dura_aether[x].duration = 0; (*sd).status.dura_aether[x].caster_id = 0;
        if (*sd).status.dura_aether[x].aether == 0 { (*sd).status.dura_aether[x].id = 0; }
    }
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_flushdurationnouncast(sd: *mut c_void, dispel_level: c_int, min_id: c_int, max_id: c_int) {
    sl_pc_flushduration(sd, dispel_level, min_id, max_id);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_refreshdurations(sd: *mut c_void) {
    let sd = sd as *mut MapSessionData;
    for x in 0..MAX_MAGIC_TIMERS {
        let da = (*sd).status.dura_aether[x];
        if da.id > 0 && da.duration > 0 {
            let tsd = map_id2sd_acc(da.caster_id);
            clif_send_duration(sd, da.id as c_int, da.duration / 1000, tsd);
        }
    }
}

// ── Aether system ─────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_setaether(sd: *mut c_void, name: *const c_char, mut time_ms: c_int) {
    let sd = sd as *mut MapSessionData;
    let id = magicdb_id(name); if id <= 0 { return; }
    if time_ms > 0 && time_ms < 1000 { time_ms = 1000; }
    let mut alreadycast = false;
    for x in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[x].id == id as u16 { alreadycast = true; break; }
    }
    for x in 0..MAX_MAGIC_TIMERS {
        let da = &mut (*sd).status.dura_aether[x];
        if da.id == id as u16 && time_ms <= 0 {
            clif_send_aether(sd, id, time_ms);
            if da.duration == 0 { da.id = 0; }
            da.aether = 0; return;
        } else if da.id == id as u16 && (da.aether > time_ms || da.duration > 0) {
            da.aether = time_ms;
            clif_send_aether(sd, id, time_ms / 1000); return;
        } else if da.id == 0 && da.aether == 0 && time_ms != 0 && !alreadycast {
            da.id = id as u16; da.aether = time_ms;
            clif_send_aether(sd, id, time_ms / 1000); return;
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_hasaether(sd: *mut c_void, name: *const c_char) -> c_int {
    let id = magicdb_id(name); if id <= 0 { return 0; }
    let sd = sd as *mut MapSessionData;
    for i in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[i].id == id as u16 && (*sd).status.dura_aether[i].aether > 0 { return 1; }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_getaether(sd: *mut c_void, name: *const c_char) -> c_int {
    let id = magicdb_id(name); if id <= 0 { return 0; }
    let sd = sd as *mut MapSessionData;
    for i in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[i].id == id as u16 { return (*sd).status.dura_aether[i].aether; }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_flushaether(sd: *mut c_void) {
    let sd = sd as *mut MapSessionData;
    for i in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[i].aether > 0 {
            clif_send_aether(sd, (*sd).status.dura_aether[i].id as c_int, 0);
            (*sd).status.dura_aether[i].aether = 0;
            if (*sd).status.dura_aether[i].duration == 0 { (*sd).status.dura_aether[i].id = 0; }
        }
    }
}

// ── Misc ──────────────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_addclan(_sd: *mut c_void, _name: *const c_char) { /* stub */ }

#[no_mangle]
pub unsafe extern "C" fn sl_pc_updatepath(sd: *mut c_void, path: c_int, mark: c_int) {
    let id = (*(sd as *mut MapSessionData)).status.id;
    let _ = crate::database::blocking_run(async move {
        sqlx::query("UPDATE `Character` SET `ChaPthId`=?,`ChaMark`=? WHERE `ChaId`=?")
            .bind(path).bind(mark).bind(id)
            .execute(crate::database::get_pool()).await
    });
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_updatecountry(sd: *mut c_void, country: c_int) {
    let id = (*(sd as *mut MapSessionData)).status.id;
    let _ = crate::database::blocking_run(async move {
        sqlx::query("UPDATE `Character` SET `ChaNation`=? WHERE `ChaId`=?")
            .bind(country).bind(id)
            .execute(crate::database::get_pool()).await
    });
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_getcasterid(_sd: *mut c_void, name: *const c_char) -> c_int {
    magicdb_id(name)
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_settimer(sd: *mut c_void, kind: c_int, length: c_uint) {
    clif_send_timer(sd as *mut MapSessionData, kind as i8, length);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_addtime(sd: *mut c_void, v: c_int) {
    let sd = sd as *mut MapSessionData;
    (*sd).disptimertick = (*sd).disptimertick.wrapping_add(v as u32);
    clif_send_timer(sd, (*sd).disptimertype, (*sd).disptimertick);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_removetime(sd: *mut c_void, v: c_int) {
    let sd = sd as *mut MapSessionData;
    (*sd).disptimertick = (*sd).disptimertick.saturating_sub(v as u32);
    clif_send_timer(sd, (*sd).disptimertype, (*sd).disptimertick);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_setheroshow(sd: *mut c_void, flag: c_int) {
    let sd = sd as *mut MapSessionData;
    (*sd).status.heroes = flag as u32;
    let id = (*sd).status.id;
    let _ = crate::database::blocking_run(async move {
        sqlx::query("UPDATE `Character` SET `ChaHeroShow`=? WHERE `ChaId`=?")
            .bind(flag).bind(id)
            .execute(crate::database::get_pool()).await
    });
}

// ── Legends ───────────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_addlegend(
    sd: *mut c_void, text: *const c_char, name: *const c_char,
    icon: c_int, color: c_int, tchaid: c_uint,
) {
    use crate::servers::char::charstatus::MAX_LEGENDS;
    let sd = sd as *mut MapSessionData;
    for x in 0..MAX_LEGENDS {
        let empty_now  = (*sd).status.legends[x].name[0] == 0;
        let empty_next = x + 1 >= MAX_LEGENDS || (*sd).status.legends[x + 1].name[0] == 0;
        if empty_now && empty_next {
            let leg = &mut (*sd).status.legends[x];
            bounded_copy(leg.text.as_mut_ptr(), text, leg.text.len());
            bounded_copy(leg.name.as_mut_ptr(), name, leg.name.len());
            leg.icon   = icon as u16;
            leg.color  = color as u16;
            leg.tchaid = tchaid;
            return;
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_haslegend(sd: *mut c_void, name: *const c_char) -> c_int {
    use crate::servers::char::charstatus::MAX_LEGENDS;
    let sd = sd as *mut MapSessionData;
    let cmp = if name.is_null() { b"" as &[u8] } else { std::ffi::CStr::from_ptr(name).to_bytes() };
    for i in 0..MAX_LEGENDS {
        let leg_name = (*sd).status.legends[i].name;
        if leg_name[0] != 0 {
            let leg_bytes = std::ffi::CStr::from_ptr(leg_name.as_ptr()).to_bytes();
            if leg_bytes.eq_ignore_ascii_case(cmp) { return 1; }
        }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_removelegendbyname(sd: *mut c_void, name: *const c_char) {
    let sd = sd as *mut MapSessionData;
    let legs = &mut (*sd).status.legends;
    let cmp = if name.is_null() { b"".as_ref() } else { std::ffi::CStr::from_ptr(name).to_bytes() };
    // zero all matching entries
    for x in 0..MAX_LEGENDS {
        let leg_name = &legs[x].name;
        if leg_name[0] != 0 {
            let n = std::ffi::CStr::from_ptr(leg_name.as_ptr()).to_bytes();
            if n.eq_ignore_ascii_case(cmp) {
                legs[x].name[0] = 0;
                legs[x].text[0] = 0;
                legs[x].icon = 0;
                legs[x].color = 0;
                legs[x].tchaid = 0;
            }
        }
    }
    // compact: shift non-empty entries forward over gaps
    for x in 0..MAX_LEGENDS - 1 {
        if legs[x].name[0] == 0 && legs[x + 1].name[0] != 0 {
            legs[x] = legs[x + 1];
            legs[x + 1].name[0] = 0;
            legs[x + 1].text[0] = 0;
            legs[x + 1].icon = 0;
            legs[x + 1].color = 0;
            legs[x + 1].tchaid = 0;
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_removelegendbycolor(sd: *mut c_void, color: c_int) {
    let sd = sd as *mut MapSessionData;
    let legs = &mut (*sd).status.legends;
    let color = color as u16;
    // copy non-matching entries forward, skipping matched ones
    let mut count = 0usize;
    for x in 0..MAX_LEGENDS {
        if legs[x].color == color && legs[x].name[0] != 0 {
            count += 1;
        }
        if x + count < MAX_LEGENDS {
            legs[x] = legs[x + count];
        }
    }
    // compact trailing empties
    for x in 0..MAX_LEGENDS - 1 {
        if legs[x].name[0] == 0 && legs[x + 1].name[0] != 0 {
            legs[x] = legs[x + 1];
            legs[x + 1].name[0] = 0;
            legs[x + 1].text[0] = 0;
            legs[x + 1].icon = 0;
            legs[x + 1].color = 0;
            legs[x + 1].tchaid = 0;
        }
    }
}

// ── PK list ────────────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_getpk(sd: *mut c_void, id: c_int) -> c_int {
    let sd = sd as *mut MapSessionData;
    for x in 0..20 {
        if (*sd).pvp[x][0] == id as c_uint { return 1; }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_vregenoverflow(sd: *mut c_void) -> c_int {
    (*(sd as *mut MapSessionData)).vregenoverflow as c_int
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_mregenoverflow(sd: *mut c_void) -> c_int {
    (*(sd as *mut MapSessionData)).mregenoverflow as c_int
}

// ─── sl_user_* accessors (ported from sl_compat.c) ───────────────────────────

#[no_mangle] pub unsafe extern "C" fn sl_user_coref(sd: *mut c_void) -> c_uint {
    (*(sd as *mut MapSessionData)).coref
}
#[no_mangle] pub unsafe extern "C" fn sl_user_set_coref(sd: *mut c_void, v: c_uint) {
    (*(sd as *mut MapSessionData)).coref = v;
}
#[no_mangle] pub unsafe extern "C" fn sl_user_coref_container(sd: *mut c_void) -> c_uint {
    (*(sd as *mut MapSessionData)).coref_container
}
#[no_mangle] pub unsafe extern "C" fn sl_user_map_id2sd(id: c_uint) -> *mut c_void {
    crate::game::map_server::map_id2sd(id)
}

// ─── Mana / gold / time helpers (ported from sl_compat.c) ────────────────────

/// addMagic / addMana — add `amount` to sd->status.mp and send HP/MP status.
#[no_mangle]
pub unsafe extern "C" fn sl_pc_addmagic(sd: *mut c_void, amount: c_int) {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() { return; }
    (*sd).status.mp = ((*sd).status.mp as i32).wrapping_add(amount) as u32;
    clif_sendstatus(sd, SFLAG_HPMP);
}

/// addManaExtend — alias for addMagic.
#[no_mangle]
pub unsafe extern "C" fn sl_pc_addmanaextend(sd: *mut c_void, amount: c_int) {
    sl_pc_addmagic(sd, amount);
}

/// addGold — add gold to sd->status.money and send XP/money status.
#[no_mangle]
pub unsafe extern "C" fn sl_pc_addgold(sd: *mut c_void, amount: c_int) {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() { return; }
    (*sd).status.money = ((*sd).status.money as i32).wrapping_add(amount) as u32;
    clif_sendstatus(sd, SFLAG_XPMONEY);
}

/// removeGold — subtract gold (floor at 0) and send XP/money status.
#[no_mangle]
pub unsafe extern "C" fn sl_pc_removegold(sd: *mut c_void, amount: c_int) {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() { return; }
    if (*sd).status.money < amount as u32 {
        (*sd).status.money = 0;
    } else {
        (*sd).status.money -= amount as u32;
    }
    clif_sendstatus(sd, SFLAG_XPMONEY);
}

/// setTimeValues — prepend newval to the timevalues ring buffer.
#[no_mangle]
pub unsafe extern "C" fn sl_pc_settimevalues(sd: *mut c_void, newval: c_uint) {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() { return; }
    let n = (*sd).timevalues.len();
    for i in (1..n).rev() {
        (*sd).timevalues[i] = (*sd).timevalues[i - 1];
    }
    (*sd).timevalues[0] = newval;
}

/// addHealth (extend variant) — heal by amount (negative damage).
#[no_mangle]
pub unsafe extern "C" fn sl_pc_addhealth_extend(sd: *mut c_void, amount: c_int) {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() { return; }
    clif_send_pc_healthscript(sd, -amount, 0);
    clif_sendstatus(sd, SFLAG_HPMP);
}

/// removeHealth (extend variant) — damage by amount, skipped if dead.
#[no_mangle]
pub unsafe extern "C" fn sl_pc_removehealth_extend(sd: *mut c_void, damage: c_int) {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() { return; }
    use crate::game::pc::PC_DIE;
    if (*sd).status.state != PC_DIE as i8 {
        clif_send_pc_healthscript(sd, damage, 0);
        clif_sendstatus(sd, SFLAG_HPMP);
    }
}

/// getEquippedDura — return durability of equipped item at slot, or -1 if not found.
#[no_mangle]
pub unsafe extern "C" fn sl_pc_getequippeddura(sd: *mut c_void, id: c_uint, slot: c_int) -> c_int {
    use crate::servers::char::charstatus::MAX_EQUIP;
    let sd = sd as *mut MapSessionData;
    if sd.is_null() { return -1; }
    if slot >= 0 && (slot as usize) < MAX_EQUIP {
        let s = slot as usize;
        if (*sd).status.equip[s].id == id { return (*sd).status.equip[s].dura; }
    } else {
        for x in 0..MAX_EQUIP {
            if (*sd).status.equip[x].id == id { return (*sd).status.equip[x].dura; }
        }
    }
    -1
}

// ─── No-op stubs (ported from sl_compat.c) ───────────────────────────────────

#[no_mangle] pub unsafe extern "C" fn sl_pc_addguide(_sd: *mut c_void, _guide: c_int) {}
#[no_mangle] pub unsafe extern "C" fn sl_pc_delguide(_sd: *mut c_void, _guide: c_int) {}
#[no_mangle] pub unsafe extern "C" fn sl_pc_logbuysell(
    _sd: *mut c_void, _item: c_uint, _amount: c_uint, _gold: c_uint, _flag: c_int) {}
#[no_mangle] pub unsafe extern "C" fn sl_pc_calcthrow(_sd: *mut c_void) {}
#[no_mangle] pub unsafe extern "C" fn sl_pc_calcrangeddamage(_sd: *mut c_void, _bl: *mut c_void) -> c_int { 0 }
#[no_mangle] pub unsafe extern "C" fn sl_pc_calcrangedhit(_sd: *mut c_void, _bl: *mut c_void) -> c_int { 0 }

// ─── sl_map_spell (ported from sl_compat.c) ──────────────────────────────────

/// Return map[m].spell (1 = spell-disabled), or 0 if map not loaded.
#[no_mangle]
pub unsafe extern "C" fn sl_map_spell(m: c_int) -> c_int {
    let ptr = crate::ffi::map_db::get_map_ptr(m as u16);
    if ptr.is_null() || (*ptr).xs == 0 { return 0; }
    (*ptr).spell as c_int
}

// ─── Bank field reads ─────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_checkbankitems(sd: *mut c_void, slot: c_int) -> c_int {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() || slot < 0 || slot as usize >= MAX_BANK_SLOTS { return 0; }
    (*sd).status.banks[slot as usize].item_id as c_int
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_checkbankamounts(sd: *mut c_void, slot: c_int) -> c_int {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() || slot < 0 || slot as usize >= MAX_BANK_SLOTS { return 0; }
    (*sd).status.banks[slot as usize].amount as c_int
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_checkbankowners(sd: *mut c_void, slot: c_int) -> c_int {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() || slot < 0 || slot as usize >= MAX_BANK_SLOTS { return 0; }
    (*sd).status.banks[slot as usize].owner as c_int
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_checkbankengraves(sd: *mut c_void, slot: c_int) -> *const c_char {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() || slot < 0 || slot as usize >= MAX_BANK_SLOTS { return c"".as_ptr(); }
    (*sd).status.banks[slot as usize].real_name.as_ptr() as *const c_char
}

// ─── Bank deposit / withdraw ──────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_bankdeposit(
    sd: *mut c_void, item: c_uint, amount: c_uint, owner: c_uint, engrave: *const c_char,
) {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() { return; }
    let engrave_bytes: &[u8] = if engrave.is_null() { b"\0" } else {
        std::slice::from_raw_parts(engrave as *const u8,
            libc::strlen(engrave) + 1)
    };
    // Find existing matching slot, else find empty slot.
    let mut deposit: Option<usize> = None;
    for x in 0..MAX_BANK_SLOTS {
        let b = &(*sd).status.banks[x];
        if b.item_id == item && b.owner == owner {
            let rn = b.real_name.as_ptr() as *const u8;
            let rn_len = libc::strlen(rn as *const c_char) + 1;
            let rn_bytes = std::slice::from_raw_parts(rn, rn_len);
            if engrave_bytes.len() == rn_bytes.len()
                && engrave_bytes.eq_ignore_ascii_case(rn_bytes)
            {
                deposit = Some(x);
                break;
            }
        }
    }
    if let Some(x) = deposit {
        (*sd).status.banks[x].amount = (*sd).status.banks[x].amount.wrapping_add(amount);
    } else {
        for x in 0..MAX_BANK_SLOTS {
            if (*sd).status.banks[x].item_id == 0 {
                let b = &mut (*sd).status.banks[x];
                b.item_id = item;
                b.amount = amount;
                b.owner = owner;
                let src = if engrave.is_null() { c"".as_ptr() } else { engrave };
                libc::strncpy(b.real_name.as_mut_ptr() as *mut libc::c_char, src,
                              b.real_name.len() - 1);
                break;
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_bankwithdraw(
    sd: *mut c_void, item: c_uint, amount: c_uint, owner: c_uint, engrave: *const c_char,
) {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() { return; }
    let engrave_bytes: &[u8] = if engrave.is_null() { b"\0" } else {
        std::slice::from_raw_parts(engrave as *const u8,
            libc::strlen(engrave) + 1)
    };
    let mut deposit: Option<usize> = None;
    for x in 0..MAX_BANK_SLOTS {
        let b = &(*sd).status.banks[x];
        if b.item_id == item && b.owner == owner {
            let rn = b.real_name.as_ptr() as *const u8;
            let rn_len = libc::strlen(rn as *const c_char) + 1;
            let rn_bytes = std::slice::from_raw_parts(rn, rn_len);
            if engrave_bytes.len() == rn_bytes.len()
                && engrave_bytes.eq_ignore_ascii_case(rn_bytes)
            {
                deposit = Some(x);
                break;
            }
        }
    }
    let Some(x) = deposit else { return; };
    if (*sd).status.banks[x].amount <= amount {
        (*sd).status.banks[x] = std::mem::zeroed();
    } else {
        (*sd).status.banks[x].amount -= amount;
    }
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_bankcheckamount(
    sd: *mut c_void, item: c_uint, _amount: c_uint, owner: c_uint, engrave: *const c_char,
) -> c_int {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() { return 0; }
    let engrave_bytes: &[u8] = if engrave.is_null() { b"\0" } else {
        std::slice::from_raw_parts(engrave as *const u8,
            libc::strlen(engrave) + 1)
    };
    let mut total: u32 = 0;
    for x in 0..MAX_BANK_SLOTS {
        let b = &(*sd).status.banks[x];
        if b.item_id == item && b.owner == owner {
            let rn = b.real_name.as_ptr() as *const u8;
            let rn_len = libc::strlen(rn as *const c_char) + 1;
            let rn_bytes = std::slice::from_raw_parts(rn, rn_len);
            if engrave_bytes.len() == rn_bytes.len()
                && engrave_bytes.eq_ignore_ascii_case(rn_bytes)
            {
                total = total.wrapping_add(b.amount);
            }
        }
    }
    total as c_int
}

// ─── Clan bank — no-ops (SQL-backed; deposit/withdraw handled in scripting.c) ─

#[no_mangle]
pub unsafe extern "C" fn sl_pc_clanbankdeposit(
    _sd: *mut c_void, _item: c_uint, _amount: c_uint, _owner: c_uint, _engrave: *const c_char,
) {}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_clanbankwithdraw(
    _sd: *mut c_void, _item: c_uint, _amount: c_uint, _owner: c_uint, _engrave: *const c_char,
) {}

// ─── No-op stubs ──────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_getunknownspells(
    _sd: *mut c_void, _out_ids: *mut c_int, _max: c_int,
) -> c_int { 0 }

#[no_mangle]
pub unsafe extern "C" fn sl_pc_getparcel(_sd: *mut c_void) -> *mut c_void { std::ptr::null_mut() }

#[no_mangle]
pub unsafe extern "C" fn sl_pc_getparcellist(
    _sd: *mut c_void, _out: *mut *mut c_void, _max: c_int,
) -> c_int { 0 }

// ─── Kill registry ────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_killrank(sd: *mut c_void, mob_id: c_int) -> c_int {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() { return 0; }
    for x in 0..MAX_KILLREG {
        if (*sd).status.killreg[x].mob_id as c_int == mob_id {
            return (*sd).status.killreg[x].amount as c_int;
        }
    }
    0
}

// ─── Misc PC helpers ──────────────────────────────────────────────────────────

extern "C" {
    #[link_name = "clif_sendmsg"]
    fn clif_sendmsg_pc(sd: *mut MapSessionData, color: c_int, msg: *const c_char) -> c_int;
    #[link_name = "clif_broadcast"]
    fn clif_broadcast_pc(msg: *const c_char, m: c_int) -> c_int;
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_gmmsg(sd: *mut c_void, msg: *const c_char) {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() || msg.is_null() { return; }
    clif_sendmsg_pc(sd, 0, msg);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_talkself(sd: *mut c_void, color: c_int, msg: *const c_char) {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() || msg.is_null() { return; }
    clif_sendmsg_pc(sd, color, msg);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_broadcast_sd(
    _sd: *mut c_void, msg: *const c_char, m: c_int,
) {
    if msg.is_null() { return; }
    clif_broadcast_pc(msg, m);
}

// ─── Inventory / equip slot pointers ─────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_getinventoryitem(sd: *mut c_void, slot: c_int) -> *mut c_void {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() || slot < 0 || slot as usize >= MAX_INVENTORY { return std::ptr::null_mut(); }
    if (*sd).status.inventory[slot as usize].id == 0 { return std::ptr::null_mut(); }
    &mut (*sd).status.inventory[slot as usize] as *mut _ as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_getequippeditem_sd(sd: *mut c_void, slot: c_int) -> *mut c_void {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() || slot < 0 || slot as usize >= MAX_EQUIP { return std::ptr::null_mut(); }
    if (*sd).status.equip[slot as usize].id == 0 { return std::ptr::null_mut(); }
    &mut (*sd).status.equip[slot as usize] as *mut _ as *mut c_void
}

// ─── Inventory mutation: add / remove items ───────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_additem(
    sd_ptr: *mut c_void,
    id: c_uint, amount: c_uint,
    dura: c_int, owner: c_uint,
    engrave: *const c_char,
) {
    let sd = sd_ptr as *mut MapSessionData;
    if sd.is_null() { return; }
    let mut fl: crate::servers::char::charstatus::Item = std::mem::zeroed();
    fl.id     = id;
    fl.amount = amount as i32;
    fl.owner  = owner;
    fl.dura   = if dura != 0 { dura } else { crate::ffi::item_db::rust_itemdb_dura(id) };
    fl.protected = crate::ffi::item_db::rust_itemdb_protected(id) as u32;
    if !engrave.is_null() && *engrave != 0 {
        libc::strncpy(fl.real_name.as_mut_ptr(), engrave, fl.real_name.len() - 1);
    }
    pc_additem_acc(sd, &mut fl);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_removeitem(
    sd_ptr: *mut c_void,
    id: c_uint, mut amount: c_uint,
    type_: c_int, owner: c_uint,
    engrave: *const c_char,
) {
    let sd = sd_ptr as *mut MapSessionData;
    if sd.is_null() { return; }
    let engrave = if engrave.is_null() { c"".as_ptr() } else { engrave };
    let maxinv = (*sd).status.maxinv as usize;
    for x in 0..maxinv {
        if amount == 0 { break; }
        let inv = &(*sd).status.inventory[x];
        if inv.id != id { continue; }
        if owner != 0 && inv.owner != owner { continue; }
        if libc::strcasecmp(inv.real_name.as_ptr(), engrave) != 0 { continue; }
        let avail = inv.amount as u32;
        if avail == 0 { continue; }
        let take = avail.min(amount);
        crate::game::pc::rust_pc_delitem(sd, x as c_int, take as c_int, type_);
        amount -= take;
    }
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_removeitemdura(
    sd_ptr: *mut c_void,
    id: c_uint, mut amount: c_uint,
    type_: c_int,
) {
    let sd = sd_ptr as *mut MapSessionData;
    if sd.is_null() { return; }
    let max_dura = crate::ffi::item_db::rust_itemdb_dura(id);
    let maxinv = (*sd).status.maxinv as usize;
    for x in 0..maxinv {
        if amount == 0 { break; }
        let inv = &(*sd).status.inventory[x];
        if inv.id != id { continue; }
        if inv.dura != max_dura { continue; }
        let avail = inv.amount as u32;
        if avail == 0 { continue; }
        let take = avail.min(amount);
        crate::game::pc::rust_pc_delitem(sd, x as c_int, take as c_int, type_);
        amount -= take;
    }
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_hasitemdura(
    sd_ptr: *mut c_void, id: c_uint, mut amount: c_uint,
) -> c_int {
    let sd = sd_ptr as *mut MapSessionData;
    if sd.is_null() { return amount as c_int; }
    let max_dura = crate::ffi::item_db::rust_itemdb_dura(id);
    let maxinv = (*sd).status.maxinv as usize;
    for x in 0..maxinv {
        if amount == 0 { break; }
        let inv = &(*sd).status.inventory[x];
        if inv.id != id { continue; }
        if inv.dura != max_dura { continue; }
        let avail = inv.amount as u32;
        if avail == 0 { continue; }
        if avail >= amount { return 0; }
        amount -= avail;
    }
    amount as c_int
}

// ─── Spell lists ──────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_getspells(
    sd: *mut c_void, out_ids: *mut c_int, max: c_int,
) -> c_int {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() || out_ids.is_null() { return 0; }
    let mut count = 0i32;
    for x in 0..MAX_SPELLS {
        if count >= max { break; }
        if (*sd).status.skill[x] != 0 {
            *out_ids.add(count as usize) = (*sd).status.skill[x] as c_int;
            count += 1;
        }
    }
    count
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_getspellnames(
    sd: *mut c_void, out_names: *mut *const c_char, max: c_int,
) -> c_int {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() || out_names.is_null() { return 0; }
    let mut count = 0i32;
    for x in 0..MAX_SPELLS {
        if count >= max { break; }
        if (*sd).status.skill[x] != 0 {
            *out_names.add(count as usize) = magicdb_name((*sd).status.skill[x] as c_int);
            count += 1;
        }
    }
    count
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_getalldurations(
    sd: *mut c_void, out_names: *mut *const c_char, max: c_int,
) -> c_int {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() || out_names.is_null() { return 0; }
    let mut count = 0i32;
    for i in 0..MAX_MAGIC_TIMERS {
        if count >= max { break; }
        let da = &(*sd).status.dura_aether[i];
        if da.id > 0 && da.duration > 0 {
            *out_names.add(count as usize) = magicdb_yname(da.id as c_int);
            count += 1;
        }
    }
    count
}

// ─── Legends ──────────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_getlegend(
    sd: *mut c_void, name: *const c_char,
) -> *const c_char {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() || name.is_null() { return std::ptr::null(); }
    for x in 0..MAX_LEGENDS {
        if libc::strcasecmp((*sd).status.legends[x].name.as_ptr(), name) == 0 {
            return (*sd).status.legends[x].text.as_ptr() as *const c_char;
        }
    }
    std::ptr::null()
}

// ─── Active spell check ───────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_activespells(sd: *mut c_void, name: *const c_char) -> c_int {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() || name.is_null() { return 0; }
    let id = magicdb_id(name);
    for x in 0..MAX_MAGIC_TIMERS {
        let da = &(*sd).status.dura_aether[x];
        if da.id as c_int == id && da.duration > 0 { return 1; }
    }
    0
}

// ─── Give XP ─────────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_givexp(sd: *mut c_void, amount: c_uint) {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() { return; }
    extern "C" { #[link_name = "xp_rate"] static mut xp_rate_inner: c_int; }
    crate::game::pc::rust_pc_givexp(sd, amount, xp_rate_inner as c_uint);
}

// ─── Clan bank reads ──────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_getclanitems(sd: *mut c_void, slot: c_int) -> c_int {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() { return 0; }
    let clan = crate::ffi::clan_db::rust_clandb_search((*sd).status.clan as c_int);
    if clan.is_null() || (*clan).clanbanks.is_null() { return 0; }
    if slot < 0 || slot >= 255 { return 0; }
    (*(*clan).clanbanks.add(slot as usize)).item_id as c_int
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_getclanamounts(sd: *mut c_void, slot: c_int) -> c_int {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() { return 0; }
    let clan = crate::ffi::clan_db::rust_clandb_search((*sd).status.clan as c_int);
    if clan.is_null() || (*clan).clanbanks.is_null() { return 0; }
    if slot < 0 || slot >= 255 { return 0; }
    (*(*clan).clanbanks.add(slot as usize)).amount as c_int
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_checkclankitemamounts(
    sd: *mut c_void, item: c_int, _amount: c_int,
) -> c_int {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() { return 0; }
    let clan = crate::ffi::clan_db::rust_clandb_search((*sd).status.clan as c_int);
    if clan.is_null() || (*clan).clanbanks.is_null() { return 0; }
    let mut total: u32 = 0;
    for x in 0..255usize {
        let b = &*(*clan).clanbanks.add(x);
        if b.item_id as c_int == item { total = total.wrapping_add(b.amount); }
    }
    total as c_int
}

// ─── Creation packet reads ────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_getcreationitems(
    sd: *mut c_void, len: c_int, out: *mut c_uint,
) -> c_int {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() || out.is_null() { return 0; }
    let curitem = (*crate::ffi::session::rust_session_rdata_ptr((*sd).fd, len as usize)) as i32 - 1;
    let maxinv = (*sd).status.maxinv as i32;
    if curitem >= 0 && curitem < maxinv && (*sd).status.inventory[curitem as usize].id != 0 {
        *out = (*sd).status.inventory[curitem as usize].id;
        return 1;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_getcreationamounts(
    sd: *mut c_void, len: c_int, item_id: c_uint,
) -> c_int {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() { return 1; }
    let t = crate::ffi::item_db::rust_itemdb_type(item_id);
    if t < 3 || t > 17 {
        (*crate::ffi::session::rust_session_rdata_ptr((*sd).fd, len as usize)) as c_int
    } else {
        1
    }
}

// ─── Dialog send helpers ──────────────────────────────────────────────────────

extern "C" {
    #[link_name = "clif_input"]
    fn clif_input_pc(sd: *mut MapSessionData, npc_id: c_uint, msg: *const c_char, empty: *const c_char);
    #[link_name = "clif_scriptmes"]
    fn clif_scriptmes_pc(sd: *mut MapSessionData, npc_id: c_uint, msg: *const c_char, prev: c_int, next: c_int);
    #[link_name = "clif_inputseq"]
    fn clif_inputseq_pc(sd: *mut MapSessionData, npc_id: c_uint, title: *const c_char,
                        subtitle: *const c_char, body: *const c_char,
                        opts: *const *const c_char, n: c_int, prev: c_int, next: c_int);
    #[link_name = "clif_scriptmenuseq"]
    fn clif_scriptmenuseq_pc(sd: *mut MapSessionData, npc_id: c_uint, msg: *const c_char,
                             buf: *const *const c_char, n: c_int, prev: c_int, next: c_int);
    #[link_name = "clif_buydialog"]
    fn clif_buydialog_pc(sd: *mut MapSessionData, npc_id: c_uint, msg: *const c_char,
                         ilist: *const crate::servers::char::charstatus::Item,
                         prices: *const c_int, n: c_int);
    #[link_name = "clif_selldialog"]
    fn clif_selldialog_pc(sd: *mut MapSessionData, npc_id: c_uint, msg: *const c_char,
                          slots: *const c_int, count: c_int);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_input_send(sd: *mut c_void, msg: *const c_char) {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() { return; }
    clif_input_pc(sd, (*sd).last_click, msg, c"".as_ptr());
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_dialog_send(
    sd: *mut c_void, msg: *const c_char, graphics: *const c_int, ngraphics: c_int,
) {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() { return; }
    let prev = if !graphics.is_null() && ngraphics > 0 { *graphics.add(0) } else { 0 };
    let next = if !graphics.is_null() && ngraphics > 1 { *graphics.add(1) } else { 0 };
    clif_scriptmes_pc(sd, (*sd).last_click, msg, prev, next);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_dialogseq_send(
    sd: *mut c_void, entries: *const *const c_char, n: c_int, can_continue: c_int,
) {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() { return; }
    let title    = if !entries.is_null() && n > 0 { *entries.add(0) } else { c"".as_ptr() };
    let subtitle = if !entries.is_null() && n > 1 { *entries.add(1) } else { c"".as_ptr() };
    let body     = if !entries.is_null() && n > 2 { *entries.add(2) } else { c"".as_ptr() };
    let nopts    = (n - 3).max(0) as usize;
    let opts_ptr = if nopts > 0 && !entries.is_null() { entries.add(3) } else { std::ptr::null() };
    clif_inputseq_pc(sd, (*sd).last_click, title, subtitle, body,
                     opts_ptr, nopts as c_int, 0, can_continue);
}

/// Build 1-indexed option array (buf[0]=NULL, buf[1..n]=options[0..n-1]) and call clif.
unsafe fn menu_send_1idx(
    sd: *mut MapSessionData, msg: *const c_char,
    options: *const *const c_char, n: c_int,
) {
    let nu = n as usize;
    let mut buf: Vec<*const c_char> = Vec::with_capacity(nu + 1);
    buf.push(std::ptr::null());
    for i in 0..nu { buf.push(if options.is_null() { std::ptr::null() } else { *options.add(i) }); }
    clif_scriptmenuseq_pc(sd, (*sd).last_click, msg, buf.as_ptr(), n, 0, 0);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_menu_send(
    sd: *mut c_void, msg: *const c_char, options: *const *const c_char, n: c_int,
) {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() { return; }
    menu_send_1idx(sd, msg, options, n);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_menuseq_send(
    sd: *mut c_void, msg: *const c_char, options: *const *const c_char, n: c_int,
) {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() { return; }
    menu_send_1idx(sd, msg, options, n);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_menustring_send(
    sd: *mut c_void, msg: *const c_char, options: *const *const c_char, n: c_int,
) {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() { return; }
    menu_send_1idx(sd, msg, options, n);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_menustring2_send(
    _sd: *mut c_void, _msg: *const c_char, _options: *const *const c_char, _n: c_int,
) {} // no matching clif_ packet

#[no_mangle]
pub unsafe extern "C" fn sl_pc_buy_send(
    sd: *mut c_void, msg: *const c_char,
    items: *const c_int, values: *const c_int,
    displaynames: *const *const c_char, buytext: *const *const c_char,
    n: c_int,
) {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() || n <= 0 { return; }
    let nu = n as usize;
    let mut ilist: Vec<crate::servers::char::charstatus::Item> = vec![std::mem::zeroed(); nu];
    for i in 0..nu {
        ilist[i].id = *items.add(i) as c_uint;
        if !displaynames.is_null() && !(*displaynames.add(i)).is_null() {
            libc::strncpy(ilist[i].real_name.as_mut_ptr(), *displaynames.add(i),
                          ilist[i].real_name.len() - 1);
        }
        if !buytext.is_null() && !(*buytext.add(i)).is_null() {
            libc::strncpy(ilist[i].buytext.as_mut_ptr() as *mut libc::c_char,
                          *buytext.add(i), ilist[i].buytext.len() - 1);
        }
    }
    clif_buydialog_pc(sd, (*sd).last_click, msg, ilist.as_ptr(), values, n);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_buydialog_send(
    sd: *mut c_void, msg: *const c_char, items: *const c_int, n: c_int,
) {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() || n <= 0 { return; }
    let nu = n as usize;
    let mut ilist: Vec<crate::servers::char::charstatus::Item> = vec![std::mem::zeroed(); nu];
    for i in 0..nu { ilist[i].id = *items.add(i) as c_uint; }
    clif_buydialog_pc(sd, (*sd).last_click, msg, ilist.as_ptr(), std::ptr::null(), n);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_buyextend_send(
    sd: *mut c_void, msg: *const c_char,
    items: *const c_int, prices: *const c_int,
    _maxamounts: *const c_int, n: c_int,
) {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() || n <= 0 { return; }
    let nu = n as usize;
    let mut ilist: Vec<crate::servers::char::charstatus::Item> = vec![std::mem::zeroed(); nu];
    for i in 0..nu { ilist[i].id = *items.add(i) as c_uint; }
    clif_buydialog_pc(sd, (*sd).last_click, msg, ilist.as_ptr(), prices, n);
}

unsafe fn sell_send_inner(sd: *mut MapSessionData, msg: *const c_char, items: *const c_int, n: c_int) {
    let nu = n as usize;
    let maxinv = (*sd).status.maxinv as usize;
    let mut slots: Vec<c_int> = Vec::with_capacity(nu * 4);
    for j in 0..nu {
        let item_id = *items.add(j) as c_uint;
        for x in 0..maxinv {
            if (*sd).status.inventory[x].id == item_id { slots.push(x as c_int); }
        }
    }
    clif_selldialog_pc(sd, (*sd).last_click, msg, slots.as_ptr(), slots.len() as c_int);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_sell_send(
    sd: *mut c_void, msg: *const c_char, items: *const c_int, n: c_int,
) {
    let sd = sd as *mut MapSessionData;
    if sd.is_null() || n <= 0 { return; }
    sell_send_inner(sd, msg, items, n);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_sell2_send(
    sd: *mut c_void, msg: *const c_char, items: *const c_int, n: c_int,
) {
    sl_pc_sell_send(sd, msg, items, n);
}

#[no_mangle]
pub unsafe extern "C" fn sl_pc_sellextend_send(
    sd: *mut c_void, msg: *const c_char, items: *const c_int, n: c_int,
) {
    sl_pc_sell_send(sd, msg, items, n);
}

// Bank/clan bank/repair UI — no clif_ packet exists; all are no-ops.
#[no_mangle] pub unsafe extern "C" fn sl_pc_showbank_send(_sd: *mut c_void, _msg: *const c_char) {}
#[no_mangle] pub unsafe extern "C" fn sl_pc_showbankadd_send(_sd: *mut c_void) {}
#[no_mangle] pub unsafe extern "C" fn sl_pc_bankaddmoney_send(_sd: *mut c_void) {}
#[no_mangle] pub unsafe extern "C" fn sl_pc_bankwithdrawmoney_send(_sd: *mut c_void) {}
#[no_mangle] pub unsafe extern "C" fn sl_pc_clanshowbank_send(_sd: *mut c_void, _msg: *const c_char) {}
#[no_mangle] pub unsafe extern "C" fn sl_pc_clanshowbankadd_send(_sd: *mut c_void) {}
#[no_mangle] pub unsafe extern "C" fn sl_pc_clanbankaddmoney_send(_sd: *mut c_void) {}
#[no_mangle] pub unsafe extern "C" fn sl_pc_clanbankwithdrawmoney_send(_sd: *mut c_void) {}
#[no_mangle] pub unsafe extern "C" fn sl_pc_clanviewbank_send(_sd: *mut c_void) {}
#[no_mangle] pub unsafe extern "C" fn sl_pc_repairextend_send(_sd: *mut c_void) {}
#[no_mangle] pub unsafe extern "C" fn sl_pc_repairall_send(_sd: *mut c_void, _npc_bl: *mut c_void) {}

// ─── Extra extern declarations for later-ported functions ────────────────────

extern "C" {
    fn clif_getchararea(sd: *mut MapSessionData) -> c_int;
    #[link_name = "rust_itemdb_name"] fn itemdb_name_item(id: c_uint) -> *mut c_char;
    #[link_name = "rust_itemdb_time"] fn itemdb_time_item(id: c_uint) -> c_int;
    #[link_name = "rust_pc_unequip"]  fn pc_unequip_slot(sd: *mut MapSessionData, slot: c_int) -> c_int;
    #[link_name = "sl_doscript_blargs"]
    fn sl_doscript_blargs_acc(root: *const c_char, method: *const c_char, nargs: c_int, ...) -> c_int;
}

// ─── Parcel removal ───────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn sl_pc_removeparcel(
    sd_ptr: *mut c_void,
    _sender: c_int, _item: c_uint, _amount: c_uint,
    pos: c_int, _owner: c_uint,
    _engrave: *const c_char, _npcflag: c_int,
) {
    let sd = sd_ptr as *mut MapSessionData;
    if sd.is_null() { return; }
    let char_id = (*sd).status.id;
    let _ = crate::database::blocking_run(async move {
        sqlx::query(
            "DELETE FROM `Parcels` WHERE `ParChaIdDestination`=? AND `ParPosition`=?"
        )
        .bind(char_id)
        .bind(pos)
        .execute(crate::database::get_pool()).await
    });
}

// ─── PvP / combat helpers ─────────────────────────────────────────────────────

/// Record `id` in sd->pvp[]; mirrors pcl_setpk.
///
/// Mirrors `sl_pc_setpk` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_pc_setpk(sd_ptr: *mut c_void, id: c_int) {
    let sd = sd_ptr as *mut MapSessionData;
    if sd.is_null() { return; }
    let mut exist = -1i32;
    for x in 0..20usize {
        if (*sd).pvp[x][0] as i32 == id { exist = x as i32; break; }
    }
    if exist != -1 {
        (*sd).pvp[exist as usize][1] = libc::time(std::ptr::null_mut()) as u32;
    } else {
        for x in 0..20usize {
            if (*sd).pvp[x][0] == 0 {
                (*sd).pvp[x][0] = id as u32;
                (*sd).pvp[x][1] = libc::time(std::ptr::null_mut()) as u32;
                clif_getchararea(sd);
                break;
            }
        }
    }
}

/// Reduce HP without displaying a damage number.
///
/// Mirrors `sl_pc_removehealth_nodmgnum` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_pc_removehealth_nodmgnum(sd_ptr: *mut c_void, damage: c_int, type_: c_int) {
    use crate::game::pc::PC_DIE;
    let sd = sd_ptr as *mut MapSessionData;
    if sd.is_null() { return; }
    if ((*sd).status.state as c_int) != PC_DIE {
        clif_send_pc_health(sd, damage, type_);
    }
}

/// Expire timed items in inventory and equipped slots.
///
/// Mirrors `sl_pc_expireitem` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_pc_expireitem(sd_ptr: *mut c_void) {
    let sd = sd_ptr as *mut MapSessionData;
    if sd.is_null() { return; }
    let t = libc::time(std::ptr::null_mut()) as u32;

    for x in 0..(*sd).status.maxinv as usize {
        let id = (*sd).status.inventory[x].id;
        if id == 0 { continue; }
        let item_t = itemdb_time_item(id) as u32;
        let slot_t = (*sd).status.inventory[x].time;
        if (slot_t > 0 && slot_t < t) || (item_t > 0 && item_t < t) {
            let name = itemdb_name_item(id);
            let msg = format!("Your {} has expired! Please visit the cash shop to purchase another.",
                std::ffi::CStr::from_ptr(name).to_string_lossy());
            if let Ok(cmsg) = std::ffi::CString::new(msg) {
                pc_delitem(sd, x as c_int, 1, 8);
                clif_sendminitext(sd, cmsg.as_ptr());
            }
        }
    }

    // Find first empty inventory slot (receives the item moved by pc_unequip)
    let mut eqdel = -1i32;
    for x in 0..(*sd).status.maxinv as usize {
        if (*sd).status.inventory[x].id == 0 { eqdel = x as i32; break; }
    }

    for x in 0..MAX_EQUIP {
        let id = (*sd).status.equip[x].id;
        if id == 0 { continue; }
        let item_t = itemdb_time_item(id) as u32;
        let slot_t = (*sd).status.equip[x].time;
        if (slot_t > 0 && slot_t < t) || (item_t > 0 && item_t < t) {
            let name = itemdb_name_item(id);
            let msg = format!("Your {} has expired! Please visit the cash shop to purchase another.",
                std::ffi::CStr::from_ptr(name).to_string_lossy());
            if let Ok(cmsg) = std::ffi::CString::new(msg) {
                pc_unequip_slot(sd, x as c_int);
                if eqdel >= 0 { pc_delitem(sd, eqdel, 1, 8); }
                clif_sendminitext(sd, cmsg.as_ptr());
            }
        }
    }
}

/// Heal sd by `amount`; triggers `on_healed` Lua hook if attacker is set.
///
/// Mirrors `sl_pc_addhealth2` in `c_src/sl_compat.c`.
#[no_mangle]
pub unsafe extern "C" fn sl_pc_addhealth2(sd_ptr: *mut c_void, amount: c_int, _type: c_int) {
    let sd = sd_ptr as *mut MapSessionData;
    if sd.is_null() { return; }
    let bl_ptr = map_id2bl_acc((*sd).attacker) as *mut crate::database::map_db::BlockList;
    if !bl_ptr.is_null() && amount > 0 {
        sl_doscript_blargs_acc(
            c"player_combat".as_ptr(), c"on_healed".as_ptr(), 2,
            &mut (*sd).bl as *mut crate::database::map_db::BlockList,
            bl_ptr,
        );
    } else if amount > 0 {
        sl_doscript_blargs_acc(
            c"player_combat".as_ptr(), c"on_healed".as_ptr(), 1,
            &mut (*sd).bl as *mut crate::database::map_db::BlockList,
        );
    }
    clif_send_pc_healthscript(sd, -amount, 0);
    clif_sendstatus(sd, SFLAG_HPMP);
}
