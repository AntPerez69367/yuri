//! PC (USER) field accessors for Lua scripting.
//! Replaces the sl_pc_* accessor block in `c_src/sl_compat.c`.

use std::os::raw::{c_char, c_int, c_uint, c_short, c_void};
use std::os::raw::c_ulong;
use crate::game::pc::{MapSessionData, EQ_FACEACCTWO};
#[allow(unused_imports)]
use crate::database::map_db::BlockList; // retained for future BL_* field casting

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
