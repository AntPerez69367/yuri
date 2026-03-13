//! PC (USER) field accessors for Lua scripting.

use crate::game::map_parse::packet::rfifob;
use crate::game::pc::{MapSessionData, EQ_FACEACCTWO, SFLAG_HPMP, SFLAG_FULLSTATS, SFLAG_XPMONEY};
use crate::session::SessionId;
use crate::database::map_db::BlockList;
use crate::game::mob::{MobSpawnData, MAX_THREATCOUNT};
use crate::servers::char::charstatus::{MAX_INVENTORY, MAX_SPELLS, MAX_KILLREG, MAX_EQUIP, MAX_MAGIC_TIMERS, MAX_LEGENDS, MAX_BANK_SLOTS};

// ─── Read: block_list embedded fields ────────────────────────────────────────

pub fn sl_pc_bl_id(sd: &mut MapSessionData) -> i32   { sd.bl.id as i32 }
pub fn sl_pc_bl_m(sd: &mut MapSessionData) -> i32    { sd.bl.m as i32 }
pub fn sl_pc_bl_x(sd: &mut MapSessionData) -> i32    { sd.bl.x as i32 }
pub fn sl_pc_bl_y(sd: &mut MapSessionData) -> i32    { sd.bl.y as i32 }
pub fn sl_pc_bl_type(sd: &mut MapSessionData) -> i32 { sd.bl.bl_type as i32 }

// ─── Read: status fields ──────────────────────────────────────────────────────

pub fn sl_pc_status_id(sd: &mut MapSessionData) -> i32          { sd.status.id as i32 }
pub fn sl_pc_status_hp(sd: &mut MapSessionData) -> i32          { sd.status.hp as i32 }
pub fn sl_pc_status_mp(sd: &mut MapSessionData) -> i32          { sd.status.mp as i32 }
pub fn sl_pc_status_level(sd: &mut MapSessionData) -> i32       { sd.status.level as i32 }
pub fn sl_pc_status_exp(sd: &mut MapSessionData) -> i32         { sd.status.exp as i32 }
pub fn sl_pc_status_expsoldmagic(sd: &mut MapSessionData) -> i32{ sd.status.expsold_magic as i32 } // truncates to low 32 bits; matches C
// Field name mapping (C → Rust where they differ):
//   settingFlags → setting_flags   classRank → class_rank   clanRank → clan_rank
//   miniMapToggle → mini_map_toggle   expsoldhealth/stats → expsold_health/stats
// Many numeric fields are u8/u16/u32/u64/i8/i32/f32 in Rust; all cast to i32.
// sl_pc_status_killspvp reads from sd->killspvp (direct USER field), not status.

pub fn sl_pc_status_expsoldhealth(sd: &mut MapSessionData) -> i32 { sd.status.expsold_health as i32 } // truncates to low 32 bits; matches C
pub fn sl_pc_status_expsoldstats(sd: &mut MapSessionData) -> i32  { sd.status.expsold_stats as i32 } // truncates to low 32 bits; matches C
pub fn sl_pc_status_class(sd: &mut MapSessionData) -> i32     { sd.status.class as i32 }
pub fn sl_pc_status_totem(sd: &mut MapSessionData) -> i32     { sd.status.totem as i32 }
pub fn sl_pc_status_tier(sd: &mut MapSessionData) -> i32      { sd.status.tier as i32 }
pub fn sl_pc_status_mark(sd: &mut MapSessionData) -> i32      { sd.status.mark as i32 }
pub fn sl_pc_status_country(sd: &mut MapSessionData) -> i32   { sd.status.country as i32 }
pub fn sl_pc_status_clan(sd: &mut MapSessionData) -> i32      { sd.status.clan as i32 }
pub fn sl_pc_status_gm_level(sd: &mut MapSessionData) -> i32  { sd.status.gm_level as i32 }
pub fn sl_pc_status_sex(sd: &mut MapSessionData) -> i32       { sd.status.sex as i32 }
pub fn sl_pc_status_side(sd: &mut MapSessionData) -> i32      { sd.status.side as i32 }
pub fn sl_pc_status_state(sd: &mut MapSessionData) -> i32     { sd.status.state as i32 }
pub fn sl_pc_status_face(sd: &mut MapSessionData) -> i32      { sd.status.face as i32 }
pub fn sl_pc_status_hair(sd: &mut MapSessionData) -> i32      { sd.status.hair as i32 }
pub fn sl_pc_status_hair_color(sd: &mut MapSessionData) -> i32  { sd.status.hair_color as i32 }
pub fn sl_pc_status_face_color(sd: &mut MapSessionData) -> i32  { sd.status.face_color as i32 }
pub fn sl_pc_status_armor_color(sd: &mut MapSessionData) -> i32 { sd.status.armor_color as i32 }
pub fn sl_pc_status_skin_color(sd: &mut MapSessionData) -> i32  { sd.status.skin_color as i32 }
pub fn sl_pc_status_basehp(sd: &mut MapSessionData) -> i32    { sd.status.basehp as i32 }
pub fn sl_pc_status_basemp(sd: &mut MapSessionData) -> i32    { sd.status.basemp as i32 }
pub fn sl_pc_status_money(sd: &mut MapSessionData) -> i32     { sd.status.money as i32 }
pub fn sl_pc_status_bankmoney(sd: &mut MapSessionData) -> i32 { sd.status.bankmoney as i32 }
pub fn sl_pc_status_maxslots(sd: &mut MapSessionData) -> i32  { sd.status.maxslots as i32 }
pub fn sl_pc_status_maxinv(sd: &mut MapSessionData) -> i32    { sd.status.maxinv as i32 }
pub fn sl_pc_status_partner(sd: &mut MapSessionData) -> i32   { sd.status.partner as i32 }
pub fn sl_pc_status_pk(sd: &mut MapSessionData) -> i32        { sd.status.pk as i32 }
pub fn sl_pc_status_killedby(sd: &mut MapSessionData) -> i32  { sd.status.killedby as i32 }
pub fn sl_pc_status_killspk(sd: &mut MapSessionData) -> i32   { sd.status.killspk as i32 }
pub fn sl_pc_status_pkduration(sd: &mut MapSessionData) -> i32{ sd.status.pkduration as i32 }
pub fn sl_pc_status_basegrace(sd: &mut MapSessionData) -> i32 { sd.status.basegrace as i32 }
pub fn sl_pc_status_basemight(sd: &mut MapSessionData) -> i32 { sd.status.basemight as i32 }
pub fn sl_pc_status_basewill(sd: &mut MapSessionData) -> i32  { sd.status.basewill as i32 }
pub fn sl_pc_status_basearmor(sd: &mut MapSessionData) -> i32 { sd.status.basearmor as i32 }
pub fn sl_pc_status_tutor(sd: &mut MapSessionData) -> i32     { sd.status.tutor as i32 }
pub fn sl_pc_status_karma(sd: &mut MapSessionData) -> i32     { sd.status.karma as i32 } // truncates float to int; matches C
pub fn sl_pc_status_alignment(sd: &mut MapSessionData) -> i32 { sd.status.alignment as i32 }
pub fn sl_pc_status_classRank(sd: &mut MapSessionData) -> i32 { sd.status.class_rank as i32 }
pub fn sl_pc_status_clanRank(sd: &mut MapSessionData) -> i32  { sd.status.clan_rank as i32 }
pub fn sl_pc_status_novice_chat(sd: &mut MapSessionData) -> i32 { sd.status.novice_chat as i32 }
pub fn sl_pc_status_subpath_chat(sd: &mut MapSessionData) -> i32{ sd.status.subpath_chat as i32 }
pub fn sl_pc_status_clan_chat(sd: &mut MapSessionData) -> i32  { sd.status.clan_chat as i32 }
pub fn sl_pc_status_miniMapToggle(sd: &mut MapSessionData) -> i32{ sd.status.mini_map_toggle as i32 }
pub fn sl_pc_status_heroes(sd: &mut MapSessionData) -> i32    { sd.status.heroes as i32 }
pub fn sl_pc_status_mute(sd: &mut MapSessionData) -> i32      { sd.status.mute as i32 }
pub fn sl_pc_status_settingFlags(sd: &mut MapSessionData) -> i32{ sd.status.setting_flags as i32 }
// sl_pc_status_killspvp reads from the direct USER field `killspvp`, not status.killspvp
pub fn sl_pc_status_killspvp(sd: &mut MapSessionData) -> i32  { sd.killspvp as i32 }
pub fn sl_pc_status_profile_vitastats(sd: &mut MapSessionData) -> i32  { sd.status.profile_vitastats as i32 }
pub fn sl_pc_status_profile_equiplist(sd: &mut MapSessionData) -> i32  { sd.status.profile_equiplist as i32 }
pub fn sl_pc_status_profile_legends(sd: &mut MapSessionData) -> i32    { sd.status.profile_legends as i32 }
pub fn sl_pc_status_profile_spells(sd: &mut MapSessionData) -> i32     { sd.status.profile_spells as i32 }
pub fn sl_pc_status_profile_inventory(sd: &mut MapSessionData) -> i32  { sd.status.profile_inventory as i32 }
pub fn sl_pc_status_profile_bankitems(sd: &mut MapSessionData) -> i32  { sd.status.profile_bankitems as i32 }

// String getters — status fields are fixed-size [i8; N] arrays; return pointer to first element.
pub fn sl_pc_status_name(sd: &mut MapSessionData) -> *const i8 {
    sd.status.name.as_ptr()
}
pub fn sl_pc_status_title(sd: &mut MapSessionData) -> *const i8 {
    sd.status.title.as_ptr()
}
pub fn sl_pc_status_clan_title(sd: &mut MapSessionData) -> *const i8 {
    sd.status.clan_title.as_ptr()
}
pub fn sl_pc_status_afkmessage(sd: &mut MapSessionData) -> *const i8 {
    sd.status.afkmessage.as_ptr()
}
pub fn sl_pc_status_f1name(sd: &mut MapSessionData) -> *const i8 {
    sd.status.f1name.as_ptr()
}

// ─── Read: direct USER fields ────────────────────────────────────────────────
// Type notes: u32/u64/u8/u16/i8/i16/f32/f64 fields all cast to i32.
// f32 fields (rage, enchanted, sleep, deduction, damage, invis, fury, critmult, dmgshield)
// f64 fields (dmgdealt, dmgtaken)

pub fn sl_pc_npc_g(sd: &mut MapSessionData) -> i32        { sd.npc_g }
pub fn sl_pc_npc_gc(sd: &mut MapSessionData) -> i32       { sd.npc_gc }
pub fn sl_pc_groupid(sd: &mut MapSessionData) -> i32      { sd.groupid as i32 }
pub fn sl_pc_time(sd: &mut MapSessionData) -> i32         { sd.time }
pub fn sl_pc_fakeDrop(sd: &mut MapSessionData) -> i32     { sd.fakeDrop as i32 }
pub fn sl_pc_max_hp(sd: &mut MapSessionData) -> i32       { sd.max_hp as i32 }
pub fn sl_pc_max_mp(sd: &mut MapSessionData) -> i32       { sd.max_mp as i32 }
pub fn sl_pc_lastvita(sd: &mut MapSessionData) -> i32     { sd.lastvita as i32 }
pub fn sl_pc_rage(sd: &mut MapSessionData) -> i32         { sd.rage as i32 } // truncates float to int; matches C
pub fn sl_pc_polearm(sd: &mut MapSessionData) -> i32      { sd.polearm }
pub fn sl_pc_last_click(sd: &mut MapSessionData) -> i32   { sd.last_click as i32 }
pub fn sl_pc_grace(sd: &mut MapSessionData) -> i32        { sd.grace }
pub fn sl_pc_might(sd: &mut MapSessionData) -> i32        { sd.might }
pub fn sl_pc_will(sd: &mut MapSessionData) -> i32         { sd.will }
pub fn sl_pc_armor(sd: &mut MapSessionData) -> i32        { sd.armor }
pub fn sl_pc_dam(sd: &mut MapSessionData) -> i32          { sd.dam }
pub fn sl_pc_hit(sd: &mut MapSessionData) -> i32          { sd.hit }
pub fn sl_pc_miss(sd: &mut MapSessionData) -> i32         { sd.miss as i32 }
pub fn sl_pc_sleep(sd: &mut MapSessionData) -> i32        { sd.sleep as i32 } // truncates float to int; matches C
pub fn sl_pc_set_sleep(sd: &mut MapSessionData, v: i32)   { sd.sleep = v as f32; }
pub fn sl_pc_attack_speed(sd: &mut MapSessionData) -> i32 { sd.attack_speed as i32 }
pub fn sl_pc_enchanted(sd: &mut MapSessionData) -> i32    { sd.enchanted as i32 } // truncates float to int; matches C
pub fn sl_pc_confused(sd: &mut MapSessionData) -> i32     { sd.confused as i32 }
pub fn sl_pc_target(sd: &mut MapSessionData) -> i32       { sd.target }
pub fn sl_pc_set_target(sd: &mut MapSessionData, v: i32)  { sd.target = v; }
pub fn sl_pc_deduction(sd: &mut MapSessionData) -> i32    { sd.deduction as i32 } // truncates float to int; matches C
pub fn sl_pc_speed(sd: &mut MapSessionData) -> i32        { sd.speed }
pub fn sl_pc_disguise(sd: &mut MapSessionData) -> i32     { sd.disguise as i32 }
pub fn sl_pc_disguise_color(sd: &mut MapSessionData) -> i32 { sd.disguise_color as i32 }
pub fn sl_pc_attacker(sd: &mut MapSessionData) -> i32     { sd.attacker as i32 }
pub fn sl_pc_invis(sd: &mut MapSessionData) -> i32        { sd.invis as i32 } // truncates float to int; matches C
pub fn sl_pc_damage(sd: &mut MapSessionData) -> i32       { sd.damage as i32 } // truncates float to int; matches C
pub fn sl_pc_crit(sd: &mut MapSessionData) -> i32         { sd.crit }
pub fn sl_pc_critchance(sd: &mut MapSessionData) -> i32   { sd.critchance }
pub fn sl_pc_critmult(sd: &mut MapSessionData) -> i32     { sd.critmult as i32 } // truncates float to int; matches C
pub fn sl_pc_rangeTarget(sd: &mut MapSessionData) -> i32  { sd.rangeTarget as i32 }
pub fn sl_pc_exchange_gold(sd: &mut MapSessionData) -> i32  { sd.exchange.gold as i32 }
pub fn sl_pc_exchange_count(sd: &mut MapSessionData) -> i32 { sd.exchange.item_count }
pub fn sl_pc_bod_count(sd: &mut MapSessionData) -> i32    { sd.boditems.bod_count }
pub fn sl_pc_paralyzed(sd: &mut MapSessionData) -> i32    { sd.paralyzed }
pub fn sl_pc_blind(sd: &mut MapSessionData) -> i32        { sd.blind }
pub fn sl_pc_drunk(sd: &mut MapSessionData) -> i32        { sd.drunk }
pub fn sl_pc_board(sd: &mut MapSessionData) -> i32        { sd.board }
pub fn sl_pc_board_candel(sd: &mut MapSessionData) -> i32 { sd.board_candel }
pub fn sl_pc_board_canwrite(sd: &mut MapSessionData) -> i32 { sd.board_canwrite }
pub fn sl_pc_boardshow(sd: &mut MapSessionData) -> i32    { sd.boardshow as i32 }
pub fn sl_pc_boardnameval(sd: &mut MapSessionData) -> i32 { sd.boardnameval as i32 }
pub fn sl_pc_msPing(sd: &mut MapSessionData) -> i32       { sd.msPing }
pub fn sl_pc_pbColor(sd: &mut MapSessionData) -> i32      { sd.pbColor }
pub fn sl_pc_coref(sd: &mut MapSessionData) -> i32        { sd.coref as i32 }
pub fn sl_pc_optFlags(sd: &mut MapSessionData) -> i32     { sd.optFlags as i32 } // u64 (64-bit); truncates to low 32 bits; matches C
pub fn sl_pc_snare(sd: &mut MapSessionData) -> i32        { sd.snare }
pub fn sl_pc_silence(sd: &mut MapSessionData) -> i32      { sd.silence }
pub fn sl_pc_extendhit(sd: &mut MapSessionData) -> i32    { sd.extendhit }
pub fn sl_pc_afk(sd: &mut MapSessionData) -> i32          { sd.afk }
pub fn sl_pc_afktime(sd: &mut MapSessionData) -> i32      { sd.afktime }
pub fn sl_pc_totalafktime(sd: &mut MapSessionData) -> i32 { sd.totalafktime }
pub fn sl_pc_backstab(sd: &mut MapSessionData) -> i32     { sd.backstab }
pub fn sl_pc_flank(sd: &mut MapSessionData) -> i32        { sd.flank }
pub fn sl_pc_healing(sd: &mut MapSessionData) -> i32      { sd.healing }
pub fn sl_pc_minSdam(sd: &mut MapSessionData) -> i32      { sd.minSdam }
pub fn sl_pc_maxSdam(sd: &mut MapSessionData) -> i32      { sd.maxSdam }
pub fn sl_pc_minLdam(sd: &mut MapSessionData) -> i32      { sd.minLdam }
pub fn sl_pc_maxLdam(sd: &mut MapSessionData) -> i32      { sd.maxLdam }
pub fn sl_pc_talktype(sd: &mut MapSessionData) -> i32     { sd.talktype as i32 }
pub fn sl_pc_equipid(sd: &mut MapSessionData) -> i32      { sd.equipid as i32 }
pub fn sl_pc_takeoffid(sd: &mut MapSessionData) -> i32    { sd.takeoffid as i32 }
pub fn sl_pc_breakid(sd: &mut MapSessionData) -> i32      { sd.breakid as i32 }
pub fn sl_pc_equipslot(sd: &mut MapSessionData) -> i32    { sd.equipslot as i32 }
pub fn sl_pc_invslot(sd: &mut MapSessionData) -> i32      { sd.invslot as i32 }
pub fn sl_pc_pickuptype(sd: &mut MapSessionData) -> i32   { sd.pickuptype as i32 }
pub fn sl_pc_spottraps(sd: &mut MapSessionData) -> i32    { sd.spottraps as i32 }
pub fn sl_pc_fury(sd: &mut MapSessionData) -> i32         { sd.fury as i32 } // truncates float to int; matches C
// status.equip[EQ_FACEACCTWO] — Item.id and Item.custom are both u32
pub fn sl_pc_faceacctwo_id(sd: &mut MapSessionData) -> i32 {
    sd.status.equip[EQ_FACEACCTWO as usize].id as i32
}
pub fn sl_pc_faceacctwo_custom(sd: &mut MapSessionData) -> i32 {
    sd.status.equip[EQ_FACEACCTWO as usize].custom as i32
}
pub fn sl_pc_protection(sd: &mut MapSessionData) -> i32   { sd.protection as i32 }
pub fn sl_pc_clone(sd: &mut MapSessionData) -> i32        { sd.clone as i32 }
pub fn sl_pc_wisdom(sd: &mut MapSessionData) -> i32       { sd.wisdom }
pub fn sl_pc_con(sd: &mut MapSessionData) -> i32          { sd.con as i32 }
pub fn sl_pc_deathflag(sd: &mut MapSessionData) -> i32    { sd.deathflag as i32 }
pub fn sl_pc_selfbar(sd: &mut MapSessionData) -> i32      { sd.selfbar as i32 }
pub fn sl_pc_groupbars(sd: &mut MapSessionData) -> i32    { sd.groupbars as i32 }
pub fn sl_pc_mobbars(sd: &mut MapSessionData) -> i32      { sd.mobbars as i32 }
pub fn sl_pc_disptimertick(sd: &mut MapSessionData) -> i32 { sd.disptimertick as i32 }
pub fn sl_pc_bindmap(sd: &mut MapSessionData) -> i32      { sd.bindmap as i32 }
pub fn sl_pc_bindx(sd: &mut MapSessionData) -> i32        { sd.bindx }
pub fn sl_pc_bindy(sd: &mut MapSessionData) -> i32        { sd.bindy }
pub fn sl_pc_ambushtimer(sd: &mut MapSessionData) -> i32  { sd.ambushtimer as i32 } // u64 (64-bit); truncates to low 32 bits; matches C
pub fn sl_pc_dialogtype(sd: &mut MapSessionData) -> i32   { sd.dialogtype as i32 }
pub fn sl_pc_set_dialogtype(sd: &mut MapSessionData, v: i32) { sd.dialogtype = v as i8; }
pub fn sl_pc_cursed(sd: &mut MapSessionData) -> i32       { sd.cursed as i32 }
pub fn sl_pc_action(sd: &mut MapSessionData) -> i32       { sd.action as i32 }
pub fn sl_pc_scripttick(sd: &mut MapSessionData) -> i32   { sd.scripttick }
pub fn sl_pc_dmgshield(sd: &mut MapSessionData) -> i32    { sd.dmgshield as i32 } // truncates float to int; matches C
pub fn sl_pc_dmgdealt(sd: &mut MapSessionData) -> i32     { sd.dmgdealt as i32 } // truncates float to int; matches C
pub fn sl_pc_dmgtaken(sd: &mut MapSessionData) -> i32     { sd.dmgtaken as i32 } // truncates float to int; matches C

// String getters — direct USER char array fields; return pointer to first element.
pub fn sl_pc_ipaddress(sd: &mut MapSessionData) -> *const i8 {
    sd.ipaddress.as_ptr()
}
pub fn sl_pc_speech(sd: &mut MapSessionData) -> *const i8 {
    sd.speech.as_ptr()
}
pub fn sl_pc_question(sd: &mut MapSessionData) -> *const i8 {
    sd.question.as_ptr()
}
pub fn sl_pc_mail(sd: &mut MapSessionData) -> *const i8 {
    sd.mail.as_ptr()
}

// ─── Read: GFX fields ────────────────────────────────────────────────────────
// GfxViewer name mapping (C camelCase to Rust snake_case):
//   faceAcc -> face_acc   cfaceAcc -> cface_acc
//   faceAccT -> face_acc_t   cfaceAccT -> cface_acc_t
// u16 fields: weapon, armor, helm, face_acc, crown, shield, necklace, mantle, boots, face_acc_t
// u8  fields: cweapon, carmor, chelm, cface_acc, ccrown, cshield, cnecklace, cmantle, cboots,
//             cface_acc_t, hair, chair, face, cface, cskin, dye

pub fn sl_pc_gfx_face(sd: &mut MapSessionData) -> i32     { sd.gfx.face as i32 }
pub fn sl_pc_gfx_hair(sd: &mut MapSessionData) -> i32     { sd.gfx.hair as i32 }
pub fn sl_pc_gfx_chair(sd: &mut MapSessionData) -> i32    { sd.gfx.chair as i32 }
pub fn sl_pc_gfx_cface(sd: &mut MapSessionData) -> i32    { sd.gfx.cface as i32 }
pub fn sl_pc_gfx_cskin(sd: &mut MapSessionData) -> i32    { sd.gfx.cskin as i32 }
pub fn sl_pc_gfx_dye(sd: &mut MapSessionData) -> i32      { sd.gfx.dye as i32 }
pub fn sl_pc_gfx_weapon(sd: &mut MapSessionData) -> i32   { sd.gfx.weapon as i32 }
pub fn sl_pc_gfx_cweapon(sd: &mut MapSessionData) -> i32  { sd.gfx.cweapon as i32 }
pub fn sl_pc_gfx_armor(sd: &mut MapSessionData) -> i32    { sd.gfx.armor as i32 }
pub fn sl_pc_gfx_carmor(sd: &mut MapSessionData) -> i32   { sd.gfx.carmor as i32 }
pub fn sl_pc_gfx_shield(sd: &mut MapSessionData) -> i32   { sd.gfx.shield as i32 }
pub fn sl_pc_gfx_cshield(sd: &mut MapSessionData) -> i32  { sd.gfx.cshield as i32 }
pub fn sl_pc_gfx_helm(sd: &mut MapSessionData) -> i32     { sd.gfx.helm as i32 }
pub fn sl_pc_gfx_chelm(sd: &mut MapSessionData) -> i32    { sd.gfx.chelm as i32 }
pub fn sl_pc_gfx_mantle(sd: &mut MapSessionData) -> i32   { sd.gfx.mantle as i32 }
pub fn sl_pc_gfx_cmantle(sd: &mut MapSessionData) -> i32  { sd.gfx.cmantle as i32 }
pub fn sl_pc_gfx_crown(sd: &mut MapSessionData) -> i32    { sd.gfx.crown as i32 }
pub fn sl_pc_gfx_ccrown(sd: &mut MapSessionData) -> i32   { sd.gfx.ccrown as i32 }
pub fn sl_pc_gfx_faceAcc(sd: &mut MapSessionData) -> i32  { sd.gfx.face_acc as i32 }
pub fn sl_pc_gfx_cfaceAcc(sd: &mut MapSessionData) -> i32 { sd.gfx.cface_acc as i32 }
pub fn sl_pc_gfx_faceAccT(sd: &mut MapSessionData) -> i32 { sd.gfx.face_acc_t as i32 }
pub fn sl_pc_gfx_cfaceAccT(sd: &mut MapSessionData) -> i32{ sd.gfx.cface_acc_t as i32 }
pub fn sl_pc_gfx_boots(sd: &mut MapSessionData) -> i32    { sd.gfx.boots as i32 }
pub fn sl_pc_gfx_cboots(sd: &mut MapSessionData) -> i32   { sd.gfx.cboots as i32 }
pub fn sl_pc_gfx_necklace(sd: &mut MapSessionData) -> i32 { sd.gfx.necklace as i32 }
pub fn sl_pc_gfx_cnecklace(sd: &mut MapSessionData) -> i32{ sd.gfx.cnecklace as i32 }
pub fn sl_pc_gfx_name(sd: &mut MapSessionData) -> *const i8 {
    sd.gfx.name.as_ptr()
}

// ─── Read: computed / indirect fields ────────────────────────────────────────
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

// ─── Method wrappers: direct Rust imports ────────────────────────────────────

use crate::game::map_parse::combat::{
    clif_send_pc_healthscript, clif_send_pc_health,
    clif_deductarmor, clif_deductweapon, clif_deductdura, clif_deductduraequip,
    clif_mob_damage, clif_pc_damage, clif_send_duration, clif_send_aether,
    clif_parseattack,
};
use crate::game::map_parse::player_state::{
    clif_sendstatus, clif_sendminimap, clif_sendxychange,
};
use crate::game::client::visual::{clif_sendupdatestatus_onequip, clif_sendurl};
use crate::game::scripting::rust_sl_async_freeco as sl_async_freeco;
use crate::game::map_char::intif_save_impl::rust_sl_intif_save as sl_intif_save;
use crate::game::pc::{
    rust_pc_diescript as pc_diescript, rust_pc_res as pc_res,
    rust_pc_calcstat as pc_calcstat, rust_pc_requestmp as pc_requestmp,
    rust_pc_warp_sync as pc_warp, rust_pc_setpos as pc_setpos,
    rust_pc_getitemscript as pc_getitemscript, rust_pc_loaditem as pc_loaditem,
    rust_pc_equipscript as pc_equipscript, rust_pc_unequipscript as pc_unequipscript,
    rust_pc_loadmagic as pc_loadmagic, rust_pc_checklevel as pc_checklevel,
    rust_pc_delitem as pc_delitem, rust_pc_dropitemmap as pc_dropitemmap,
    rust_pc_isinvenspace as pc_isinvenspace,
    rust_pc_additem as pc_additem_acc,
};
use crate::game::map_parse::movement::{
    clif_refreshnoclick, clif_noparsewalk, clif_blockmovement,
    clif_parselookat_scriptsub,
};
use crate::game::map_parse::visual::clif_spawn;
use crate::game::map_parse::items::{clif_throwitem_script, clif_sendadditem, clif_checkinvbod};
use crate::game::map_parse::chat::{clif_guitextsd, clif_sendminitext, clif_sendscriptsay};
use crate::game::map_server::{boards_showposts, boards_readpost, nmail_sendmail};
use crate::game::map_parse::dialogs::clif_send_timer;
// map lookups — use typed versions
use crate::game::mob::map_id2bl;
use crate::database::magic_db::{rust_magicdb_id as magicdb_id, rust_magicdb_name as magicdb_name, rust_magicdb_yname as magicdb_yname};
use crate::game::client::handlers::{clif_isregistered_sync as clif_isregistered, clif_getaccountemail_sync as clif_getaccountemail};
use crate::database::clan_db::rust_clandb_name as clandb_name_ffi;
use crate::database::class_db::{rust_classdb_path as classdb_path_ffi, rust_classdb_name as classdb_name_ffi};

// Inline helpers for map_id2sd_acc and map_id2mob_acc with correct return types
#[inline(always)]
fn map_id2sd_acc(id: u32) -> *mut MapSessionData {
    crate::game::map_server::map_id2sd_pc(id)
        .map(|arc| &*arc.write() as *const MapSessionData as *mut MapSessionData)
        .unwrap_or(std::ptr::null_mut())
}
#[inline(always)]
fn map_id2bl_acc(id: u32) -> *mut BlockList {
    map_id2bl(id)
}
#[inline(always)]
fn map_id2mob_acc(id: u32) -> *mut MobSpawnData {
    crate::game::map_server::map_id2mob_ref(id)
        .map(|arc| &*arc.write() as *const MobSpawnData as *mut MobSpawnData)
        .unwrap_or(std::ptr::null_mut())
}

/// Returns 1 if the account is registered, 0 otherwise.
/// Delegates to `clif_isregistered` (still in C / map_parse.c).
pub unsafe fn sl_pc_actid(sd: &mut MapSessionData) -> i32 {
    clif_isregistered(sd.status.id as u32)
}

/// Returns a heap-allocated email string (or NULL).
/// The pointer is leaked — Lua copies the string immediately, matching C behaviour.
pub unsafe fn sl_pc_email(sd: &mut MapSessionData) -> *const i8 {
    clif_getaccountemail(sd.status.id as u32)
}

/// Returns the interned clan name for this character's clan id.
pub fn sl_pc_clanname(sd: &mut MapSessionData) -> *const i8 {
    clandb_name_ffi(sd.status.clan as i32)
}

/// Returns the path (base class id) for this character's class.
pub fn sl_pc_baseclass(sd: &mut MapSessionData) -> i32 {
    classdb_path_ffi(sd.status.class as i32)
}

/// Returns the display name of the base class (path, rank 0).
/// The returned pointer is a leaked CString — Lua copies it immediately.
pub fn sl_pc_baseClassName(sd: &mut MapSessionData) -> *mut i8 {
    let path = classdb_path_ffi(sd.status.class as i32);
    classdb_name_ffi(path, 0)
}

/// Returns the display name of the character's class at rank 0.
/// The returned pointer is a leaked CString — Lua copies it immediately.
pub fn sl_pc_className(sd: &mut MapSessionData) -> *mut i8 {
    classdb_name_ffi(sd.status.class as i32, 0)
}

/// Returns the display name of the character's class at their current mark (rank).
/// The returned pointer is a leaked CString — Lua copies it immediately.
pub fn sl_pc_classNameMark(sd: &mut MapSessionData) -> *mut i8 {
    classdb_name_ffi(sd.status.class as i32, sd.status.mark as i32)
}

// ─── Write: direct field setters ─────────────────────────────────────────────
// Each setter takes a i32 and writes the field with the appropriate cast.
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
//   max_hp/max_mp/attacker/rangeTarget/last_click/coref_container → u32
//   rage/invis/damage/deduction/critmult/dmgshield/fury → f32
//   dmgdealt/dmgtaken → f64
//   disguise/disguise_color/bindmap → u16
//   talktype/confused/spottraps/cursed/fakeDrop → u8
//   boardshow/boardnameval/selfbar/groupbars/mobbars/clone/deathflag → i8
//   paralyzed/blind/drunk/snare/silence/extendhit/afk/npc_g/npc_gc/time/polearm
//   speed/crit/critchance/backstab/flank/healing/wisdom/bindx/bindy
//   board_candel/board_canwrite/msPing/pbColor → i32
//   protection/miss → i16
//   optFlags/uFlags → u64 (XOR ops)

// status.* setters
pub fn sl_pc_set_hp(sd: &mut MapSessionData, v: i32)         { sd.status.hp = v as u32; }
pub fn sl_pc_set_mp(sd: &mut MapSessionData, v: i32)         { sd.status.mp = v as u32; }
pub fn sl_pc_set_exp(sd: &mut MapSessionData, v: i32)        { sd.status.exp = v as u32; }
pub fn sl_pc_set_level(sd: &mut MapSessionData, v: i32)      { sd.status.level = v as u8; }
pub fn sl_pc_set_class(sd: &mut MapSessionData, v: i32)      { sd.status.class = v as u8; }
pub fn sl_pc_set_totem(sd: &mut MapSessionData, v: i32)      { sd.status.totem = v as u8; }
pub fn sl_pc_set_tier(sd: &mut MapSessionData, v: i32)       { sd.status.tier = v as u8; }
pub fn sl_pc_set_mark(sd: &mut MapSessionData, v: i32)       { sd.status.mark = v as u8; }
pub fn sl_pc_set_country(sd: &mut MapSessionData, v: i32)    { sd.status.country = v as i8; }
pub fn sl_pc_set_clan(sd: &mut MapSessionData, v: i32)       { sd.status.clan = v as u32; }
pub fn sl_pc_set_gm_level(sd: &mut MapSessionData, v: i32)   { sd.status.gm_level = v as i8; }
pub fn sl_pc_set_side(sd: &mut MapSessionData, v: i32)       { sd.status.side = v as i8; }
pub fn sl_pc_set_state(sd: &mut MapSessionData, v: i32)      { sd.status.state = v as i8; }
pub fn sl_pc_set_hair(sd: &mut MapSessionData, v: i32)       { sd.status.hair = v as u16; }
pub fn sl_pc_set_hair_color(sd: &mut MapSessionData, v: i32) { sd.status.hair_color = v as u16; }
pub fn sl_pc_set_face_color(sd: &mut MapSessionData, v: i32) { sd.status.face_color = v as u16; }
pub fn sl_pc_set_armor_color(sd: &mut MapSessionData, v: i32){ sd.status.armor_color = v as u16; }
pub fn sl_pc_set_skin_color(sd: &mut MapSessionData, v: i32) { sd.status.skin_color = v as u16; }
pub fn sl_pc_set_face(sd: &mut MapSessionData, v: i32)       { sd.status.face = v as u16; }
pub fn sl_pc_set_money(sd: &mut MapSessionData, v: i32)      { sd.status.money = v as u32; }
pub fn sl_pc_set_bankmoney(sd: &mut MapSessionData, v: i32)  { sd.status.bankmoney = v as u32; }
pub fn sl_pc_set_maxslots(sd: &mut MapSessionData, v: i32)   { sd.status.maxslots = v as u32; }
pub fn sl_pc_set_maxinv(sd: &mut MapSessionData, v: i32)     { sd.status.maxinv = v as u8; }
pub fn sl_pc_set_partner(sd: &mut MapSessionData, v: i32)    { sd.status.partner = v as u32; }
pub fn sl_pc_set_pk(sd: &mut MapSessionData, v: i32)         { sd.status.pk = v as u8; }
pub fn sl_pc_set_basehp(sd: &mut MapSessionData, v: i32)     { sd.status.basehp = v as u32; }
pub fn sl_pc_set_basemp(sd: &mut MapSessionData, v: i32)     { sd.status.basemp = v as u32; }
pub fn sl_pc_set_karma(sd: &mut MapSessionData, v: i32)      { sd.status.karma = v as f32; }
pub fn sl_pc_set_alignment(sd: &mut MapSessionData, v: i32)  { sd.status.alignment = v as i8; }
pub fn sl_pc_set_basegrace(sd: &mut MapSessionData, v: i32)  { sd.status.basegrace = v as u32; }
pub fn sl_pc_set_basemight(sd: &mut MapSessionData, v: i32)  { sd.status.basemight = v as u32; }
pub fn sl_pc_set_basewill(sd: &mut MapSessionData, v: i32)   { sd.status.basewill = v as u32; }
pub fn sl_pc_set_basearmor(sd: &mut MapSessionData, v: i32)  { sd.status.basearmor = v; }
pub fn sl_pc_set_novice_chat(sd: &mut MapSessionData, v: i32){ sd.status.novice_chat = v as i8; }
pub fn sl_pc_set_subpath_chat(sd: &mut MapSessionData, v: i32){ sd.status.subpath_chat = v as i8; }
pub fn sl_pc_set_clan_chat(sd: &mut MapSessionData, v: i32)  { sd.status.clan_chat = v as i8; }
pub fn sl_pc_set_tutor(sd: &mut MapSessionData, v: i32)      { sd.status.tutor = v as u8; }
pub fn sl_pc_set_profile_vitastats(sd: &mut MapSessionData, v: i32) { sd.status.profile_vitastats = v as u8; }
pub fn sl_pc_set_profile_equiplist(sd: &mut MapSessionData, v: i32) { sd.status.profile_equiplist = v as u8; }
pub fn sl_pc_set_profile_legends(sd: &mut MapSessionData, v: i32)   { sd.status.profile_legends = v as u8; }
pub fn sl_pc_set_profile_spells(sd: &mut MapSessionData, v: i32)    { sd.status.profile_spells = v as u8; }
pub fn sl_pc_set_profile_inventory(sd: &mut MapSessionData, v: i32) { sd.status.profile_inventory = v as u8; }
pub fn sl_pc_set_profile_bankitems(sd: &mut MapSessionData, v: i32) { sd.status.profile_bankitems = v as u8; }
pub fn sl_pc_set_mute(sd: &mut MapSessionData, v: i32)       { sd.status.mute = v as i8; }
// C casts to (unsigned int) but Rust field is u16; low 16 bits are preserved identically.
pub fn sl_pc_set_settingFlags(sd: &mut MapSessionData, v: i32) { sd.status.setting_flags = v as u16; }
pub fn sl_pc_set_heroshow(sd: &mut MapSessionData, v: i32)   { sd.status.heroes = v as u32; }
pub fn sl_pc_set_sex(sd: &mut MapSessionData, v: i32)        { sd.status.sex = v as i8; }
pub fn sl_pc_set_classRank(sd: &mut MapSessionData, v: i32)  { sd.status.class_rank = v; }
pub fn sl_pc_set_clanRank(sd: &mut MapSessionData, v: i32)   { sd.status.clan_rank = v; }
pub fn sl_pc_setminimaptoggle(sd: &mut MapSessionData, v: i32) { sd.status.mini_map_toggle = v as u32; }

// direct USER field setters
pub fn sl_pc_set_max_hp(sd: &mut MapSessionData, v: i32)     { sd.max_hp = v as u32; }
pub fn sl_pc_set_max_mp(sd: &mut MapSessionData, v: i32)     { sd.max_mp = v as u32; }
pub fn sl_pc_set_npc_g(sd: &mut MapSessionData, v: i32)      { sd.npc_g = v; }
pub fn sl_pc_set_npc_gc(sd: &mut MapSessionData, v: i32)     { sd.npc_gc = v; }
pub fn sl_pc_set_last_click(sd: &mut MapSessionData, v: i32) { sd.last_click = v as u32; }
pub fn sl_pc_set_time(sd: &mut MapSessionData, v: i32)       { sd.time = v; }
pub fn sl_pc_set_rage(sd: &mut MapSessionData, v: i32)       { sd.rage = v as f32; }
pub fn sl_pc_set_polearm(sd: &mut MapSessionData, v: i32)    { sd.polearm = v; }
pub fn sl_pc_set_deduction(sd: &mut MapSessionData, v: i32)  { sd.deduction = v as f32; }
pub fn sl_pc_set_speed(sd: &mut MapSessionData, v: i32)      { sd.speed = v; }
pub fn sl_pc_set_attacker(sd: &mut MapSessionData, v: i32)   { sd.attacker = v as u32; }
pub fn sl_pc_set_invis(sd: &mut MapSessionData, v: i32)      { sd.invis = v as f32; }
pub fn sl_pc_set_damage(sd: &mut MapSessionData, v: i32)     { sd.damage = v as f32; }
pub fn sl_pc_set_crit(sd: &mut MapSessionData, v: i32)       { sd.crit = v; }
pub fn sl_pc_set_critchance(sd: &mut MapSessionData, v: i32) { sd.critchance = v; }
pub fn sl_pc_set_critmult(sd: &mut MapSessionData, v: i32)   { sd.critmult = v as f32; }
pub fn sl_pc_set_rangeTarget(sd: &mut MapSessionData, v: i32){ sd.rangeTarget = v as u32; }
pub fn sl_pc_set_disguise(sd: &mut MapSessionData, v: i32)   { sd.disguise = v as u16; }
pub fn sl_pc_set_disguise_color(sd: &mut MapSessionData, v: i32) { sd.disguise_color = v as u16; }
pub fn sl_pc_set_paralyzed(sd: &mut MapSessionData, v: i32)  { sd.paralyzed = v; }
pub fn sl_pc_set_blind(sd: &mut MapSessionData, v: i32)      { sd.blind = v; }
pub fn sl_pc_set_drunk(sd: &mut MapSessionData, v: i32)      { sd.drunk = v; }
pub fn sl_pc_set_board_candel(sd: &mut MapSessionData, v: i32)  { sd.board_candel = v; }
pub fn sl_pc_set_board_canwrite(sd: &mut MapSessionData, v: i32){ sd.board_canwrite = v; }
pub fn sl_pc_set_boardshow(sd: &mut MapSessionData, v: i32)  { sd.boardshow = v as i8; }
pub fn sl_pc_set_boardnameval(sd: &mut MapSessionData, v: i32){ sd.boardnameval = v as i8; }
pub fn sl_pc_set_snare(sd: &mut MapSessionData, v: i32)      { sd.snare = v; }
pub fn sl_pc_set_silence(sd: &mut MapSessionData, v: i32)    { sd.silence = v; }
pub fn sl_pc_set_extendhit(sd: &mut MapSessionData, v: i32)  { sd.extendhit = v; }
pub fn sl_pc_set_afk(sd: &mut MapSessionData, v: i32)        { sd.afk = v; }
pub fn sl_pc_set_confused(sd: &mut MapSessionData, v: i32)   { sd.confused = v as u8; }
pub fn sl_pc_set_spottraps(sd: &mut MapSessionData, v: i32)  { sd.spottraps = v as u8; }
pub fn sl_pc_set_selfbar(sd: &mut MapSessionData, v: i32)    { sd.selfbar = v as i8; }
pub fn sl_pc_set_groupbars(sd: &mut MapSessionData, v: i32)  { sd.groupbars = v as i8; }
pub fn sl_pc_set_mobbars(sd: &mut MapSessionData, v: i32)    { sd.mobbars = v as i8; }
// C uses (unsigned int) for the XOR mask but uFlags is u64; XOR low 32 bits, upper preserved.
pub fn sl_pc_set_optFlags_xor(sd: &mut MapSessionData, v: i32) { sd.optFlags ^= v as u32 as u64; }
pub fn sl_pc_set_uflags_xor(sd: &mut MapSessionData, v: i32)   { sd.uFlags ^= v as u32 as u64; }
pub fn sl_pc_set_talktype(sd: &mut MapSessionData, v: i32)   { sd.talktype = v as u8; }
pub fn sl_pc_set_cursed(sd: &mut MapSessionData, v: i32)     { sd.cursed = v as u8; }
pub fn sl_pc_set_deathflag(sd: &mut MapSessionData, v: i32)  { sd.deathflag = v as i8; }
pub fn sl_pc_set_bindmap(sd: &mut MapSessionData, v: i32)    { sd.bindmap = v as u16; }
pub fn sl_pc_set_bindx(sd: &mut MapSessionData, v: i32)      { sd.bindx = v; }
pub fn sl_pc_set_bindy(sd: &mut MapSessionData, v: i32)      { sd.bindy = v; }
pub fn sl_pc_set_protection(sd: &mut MapSessionData, v: i32) { sd.protection = v as i16; }
pub fn sl_pc_set_dmgshield(sd: &mut MapSessionData, v: i32)  { sd.dmgshield = v as f32; }
pub fn sl_pc_set_dmgdealt(sd: &mut MapSessionData, v: i32)   { sd.dmgdealt = v as f64; }
pub fn sl_pc_set_dmgtaken(sd: &mut MapSessionData, v: i32)   { sd.dmgtaken = v as f64; }
pub fn sl_pc_set_fakeDrop(sd: &mut MapSessionData, v: i32)   { sd.fakeDrop = v as u8; }
pub fn sl_pc_set_clone(sd: &mut MapSessionData, v: i32)      { sd.clone = v as i8; }
pub fn sl_pc_set_fury(sd: &mut MapSessionData, v: i32)       { sd.fury = v as f32; }
pub fn sl_pc_set_coref_container(sd: &mut MapSessionData, v: i32) { sd.coref_container = v as u32; }
pub fn sl_pc_set_wisdom(sd: &mut MapSessionData, v: i32)     { sd.wisdom = v; }
pub fn sl_pc_set_con(sd: &mut MapSessionData, v: i32)        { sd.con = v as i16; }
pub fn sl_pc_set_backstab(sd: &mut MapSessionData, v: i32)   { sd.backstab = v; }
pub fn sl_pc_set_flank(sd: &mut MapSessionData, v: i32)      { sd.flank = v; }
pub fn sl_pc_set_healing(sd: &mut MapSessionData, v: i32)    { sd.healing = v; }
pub fn sl_pc_set_pbColor(sd: &mut MapSessionData, v: i32)    { sd.pbColor = v; }

// ─── Write: GFX setters ───────────────────────────────────────────────────────
// GfxViewer field types: u16 for weapon/armor/helm/face_acc/crown/shield/necklace/mantle/boots/face_acc_t
//                        u8  for cweapon/carmor/chelm/cface_acc/ccrown/cshield/cnecklace/cmantle/cboots/cface_acc_t
//                        u8  for hair/chair/face/cface/cskin/dye
// sl_pc_set_gfx_name is a string setter — ported below with bounded_copy.

pub fn sl_pc_set_gfx_face(sd: &mut MapSessionData, v: i32)     { sd.gfx.face = v as u8; }
pub fn sl_pc_set_gfx_hair(sd: &mut MapSessionData, v: i32)     { sd.gfx.hair = v as u8; }
pub fn sl_pc_set_gfx_chair(sd: &mut MapSessionData, v: i32)    { sd.gfx.chair = v as u8; }
pub fn sl_pc_set_gfx_cface(sd: &mut MapSessionData, v: i32)    { sd.gfx.cface = v as u8; }
pub fn sl_pc_set_gfx_cskin(sd: &mut MapSessionData, v: i32)    { sd.gfx.cskin = v as u8; }
pub fn sl_pc_set_gfx_dye(sd: &mut MapSessionData, v: i32)      { sd.gfx.dye = v as u8; }
pub fn sl_pc_set_gfx_weapon(sd: &mut MapSessionData, v: i32)   { sd.gfx.weapon = v as u16; }
pub fn sl_pc_set_gfx_cweapon(sd: &mut MapSessionData, v: i32)  { sd.gfx.cweapon = v as u8; }
pub fn sl_pc_set_gfx_armor(sd: &mut MapSessionData, v: i32)    { sd.gfx.armor = v as u16; }
pub fn sl_pc_set_gfx_carmor(sd: &mut MapSessionData, v: i32)   { sd.gfx.carmor = v as u8; }
pub fn sl_pc_set_gfx_shield(sd: &mut MapSessionData, v: i32)   { sd.gfx.shield = v as u16; }
pub fn sl_pc_set_gfx_cshield(sd: &mut MapSessionData, v: i32)  { sd.gfx.cshield = v as u8; }
pub fn sl_pc_set_gfx_helm(sd: &mut MapSessionData, v: i32)     { sd.gfx.helm = v as u16; }
pub fn sl_pc_set_gfx_chelm(sd: &mut MapSessionData, v: i32)    { sd.gfx.chelm = v as u8; }
pub fn sl_pc_set_gfx_mantle(sd: &mut MapSessionData, v: i32)   { sd.gfx.mantle = v as u16; }
pub fn sl_pc_set_gfx_cmantle(sd: &mut MapSessionData, v: i32)  { sd.gfx.cmantle = v as u8; }
pub fn sl_pc_set_gfx_crown(sd: &mut MapSessionData, v: i32)    { sd.gfx.crown = v as u16; }
pub fn sl_pc_set_gfx_ccrown(sd: &mut MapSessionData, v: i32)   { sd.gfx.ccrown = v as u8; }
pub fn sl_pc_set_gfx_faceAcc(sd: &mut MapSessionData, v: i32)  { sd.gfx.face_acc = v as u16; }
pub fn sl_pc_set_gfx_cfaceAcc(sd: &mut MapSessionData, v: i32) { sd.gfx.cface_acc = v as u8; }
pub fn sl_pc_set_gfx_faceAccT(sd: &mut MapSessionData, v: i32) { sd.gfx.face_acc_t = v as u16; }
pub fn sl_pc_set_gfx_cfaceAccT(sd: &mut MapSessionData, v: i32){ sd.gfx.cface_acc_t = v as u8; }
pub fn sl_pc_set_gfx_boots(sd: &mut MapSessionData, v: i32)    { sd.gfx.boots = v as u16; }
pub fn sl_pc_set_gfx_cboots(sd: &mut MapSessionData, v: i32)   { sd.gfx.cboots = v as u8; }
pub fn sl_pc_set_gfx_necklace(sd: &mut MapSessionData, v: i32) { sd.gfx.necklace = v as u16; }
pub fn sl_pc_set_gfx_cnecklace(sd: &mut MapSessionData, v: i32){ sd.gfx.cnecklace = v as u8; }

// ─── String setters (bounded_copy) ───────────────────────────────────────────
// Equivalent to: strncpy(dst, src ? src : "", max_len-1); dst[max_len-1] = 0;
// Used for all [i8; N] / [i8; N] name/title/speech fields.

/// Copies at most `max_len - 1` bytes from `src` into `dst`, then null-terminates.
/// Copies a string into a fixed-size buffer with explicit null termination.
///
/// # Safety
/// `dst` must point to a buffer of at least `max_len` bytes.
/// `src` may be null (treated as empty string).
unsafe fn bounded_copy(dst: *mut i8, src: *const i8, max_len: usize) {
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

pub unsafe fn sl_pc_set_gfx_name(sd: &mut MapSessionData, v: *const i8) {
    bounded_copy(sd.gfx.name.as_mut_ptr(), v, sd.gfx.name.len());
}
pub unsafe fn sl_pc_set_name(sd: &mut MapSessionData, v: *const i8) {
    bounded_copy(sd.status.name.as_mut_ptr(), v, sd.status.name.len());
}
pub unsafe fn sl_pc_set_title(sd: &mut MapSessionData, v: *const i8) {
    bounded_copy(sd.status.title.as_mut_ptr(), v, sd.status.title.len());
}
pub unsafe fn sl_pc_set_clan_title(sd: &mut MapSessionData, v: *const i8) {
    bounded_copy(sd.status.clan_title.as_mut_ptr(), v, sd.status.clan_title.len());
}
pub unsafe fn sl_pc_set_afkmessage(sd: &mut MapSessionData, v: *const i8) {
    bounded_copy(sd.status.afkmessage.as_mut_ptr(), v, sd.status.afkmessage.len());
}
pub unsafe fn sl_pc_set_speech(sd: &mut MapSessionData, v: *const i8) {
    bounded_copy(sd.speech.as_mut_ptr(), v, sd.speech.len());
}

// ─── Dispatcher accessors (used by src/game/client/mod.rs) ───────────────────

pub fn sl_pc_fd(sd: &mut MapSessionData) -> SessionId {
    sd.fd
}
pub fn sl_pc_chat_timer(sd: &mut MapSessionData) -> i32 {
    sd.chat_timer
}
pub fn sl_pc_set_chat_timer(sd: &mut MapSessionData, v: i32) {
    sd.chat_timer = v;
}
pub fn sl_pc_attacked(sd: &mut MapSessionData) -> i32 {
    sd.attacked as i32
}
pub fn sl_pc_set_attacked(sd: &mut MapSessionData, v: i32) {
    sd.attacked = v as i8;
}
pub fn sl_pc_loaded(sd: &mut MapSessionData) -> i32 {
    sd.loaded as i32
}
pub fn sl_pc_inventory_id(sd: &mut MapSessionData, pos: i32) -> u32 {
    sd.status.inventory[pos as usize].id as u32
}

// ─── Regen overflow accumulators and group membership ────────────────────────

pub fn sl_pc_set_vregenoverflow(sd: &mut MapSessionData, v: i32) {
    sd.vregenoverflow = v as f32;
}
pub fn sl_pc_set_mregenoverflow(sd: &mut MapSessionData, v: i32) {
    sd.mregenoverflow = v as f32;
}
pub fn sl_pc_set_group_count(sd: &mut MapSessionData, v: i32) {
    sd.group_count = v;
}
pub fn sl_pc_set_group_on(sd: &mut MapSessionData, v: i32) {
    sd.group_on = v;
}
pub fn sl_pc_set_group_leader(sd: &mut MapSessionData, v: i32) {
    sd.group_leader = v as u32;
}
pub fn sl_pc_group_count(sd: &mut MapSessionData) -> i32 {
    sd.group_count
}
pub fn sl_pc_group_on(sd: &mut MapSessionData) -> i32 {
    sd.group_on
}
pub fn sl_pc_group_leader(sd: &mut MapSessionData) -> i32 {
    sd.group_leader as i32
}

use crate::game::map_server::groups as pc_acc_groups;

pub unsafe fn sl_pc_getgroup(sd: &mut MapSessionData, out: *mut u32, max: i32) -> i32 {
    const MAX_MEMBERS: usize = 256;
    let user = &*(sd);
    if user.group_count > 0 {
        let n = user.group_count.min(max) as usize;
        let gid = (user.groupid as usize).min(255);
        let grp = pc_acc_groups();
        for i in 0..n {
            *out.add(i) = grp[gid * MAX_MEMBERS + i];
        }
        return n as i32;
    }
    if max > 0 { *out = user.status.id; }
    1
}

// ─── sl_pc method wrappers ───────────────────

// ── Health ────────────────────────────────────────────────────────────────────

pub unsafe fn sl_pc_addhealth(sd: &mut MapSessionData, damage: i32) {
    clif_send_pc_healthscript(&mut *sd, -damage, 0);
    clif_sendstatus(sd, SFLAG_HPMP);
}

pub unsafe fn sl_pc_removehealth(sd: &mut MapSessionData, damage: i32, caster: i32) {
    if caster > 0 { sd.attacker = caster as u32; }
    clif_send_pc_healthscript(&mut *sd, damage, 0);
    clif_sendstatus(sd, SFLAG_HPMP);
}

pub unsafe fn sl_pc_freeasync(sd: &mut MapSessionData) {
    sl_async_freeco(sd as *mut MapSessionData);
}

pub unsafe fn sl_pc_forcesave(sd: &mut MapSessionData) -> i32 {
    sl_intif_save(sd as *mut MapSessionData)
}

pub unsafe fn sl_pc_die(sd: &mut MapSessionData) {
    pc_diescript(sd);
}

pub unsafe fn sl_pc_resurrect(sd: &mut MapSessionData) {
    pc_res(sd);
}

pub fn sl_pc_showhealth(sd: &mut MapSessionData, damage: i32, kind: i32) {
    clif_send_pc_health(&mut *sd, damage, kind);
}

pub unsafe fn sl_pc_calcstat(sd: &mut MapSessionData) {
    pc_calcstat(sd);
}

pub unsafe fn sl_pc_sendstatus(sd: &mut MapSessionData) {
    pc_requestmp(sd);
    clif_sendstatus(sd, SFLAG_FULLSTATS | SFLAG_HPMP | SFLAG_XPMONEY);
    clif_sendupdatestatus_onequip(sd);
}

pub fn sl_pc_status(sd: &mut MapSessionData) -> i32 {
    crate::database::blocking_run_async(
        crate::game::map_parse::player_state::clif_mystaytus_by_addr(sd as *mut MapSessionData as usize)
    )
}

pub unsafe fn sl_pc_warp(sd: &mut MapSessionData, m: i32, x: i32, y: i32) {
    pc_warp(sd, m, x, y);
}

pub unsafe fn sl_pc_refresh(sd: &mut MapSessionData) {
    pc_setpos(sd, sd.bl.m as i32, sd.bl.x as i32, sd.bl.y as i32);
    clif_refreshnoclick(sd);
}

pub unsafe fn sl_pc_pickup(sd: &mut MapSessionData, id: u32) {
    pc_getitemscript(sd, id as i32);
}

pub unsafe fn sl_pc_throwitem(sd: &mut MapSessionData) {
    clif_throwitem_script(sd);
}

pub unsafe fn sl_pc_forcedrop(sd: &mut MapSessionData, id: i32) {
    pc_dropitemmap(sd, id, 0);
}

pub unsafe fn sl_pc_lock(sd: &mut MapSessionData) {
    clif_blockmovement(sd, 0);
}

pub unsafe fn sl_pc_unlock(sd: &mut MapSessionData) {
    clif_blockmovement(sd, 1);
}

pub unsafe fn sl_pc_swing(sd: &mut MapSessionData) {
    clif_parseattack(&mut *sd);
}

pub unsafe fn sl_pc_respawn(sd: &mut MapSessionData) {
    clif_spawn(sd);
}

pub unsafe fn sl_pc_sendhealth(sd: &mut MapSessionData, dmgf: f32, critical: i32) -> i32 {
    let damage = if dmgf > 0.0 { (dmgf + 0.5) as i32 }
                 else if dmgf < 0.0 { (dmgf - 0.5) as i32 }
                 else { 0 };
    let critical = if critical == 1 { 33 } else if critical == 2 { 255 } else { critical };
    clif_send_pc_healthscript(&mut *sd, damage, critical);
    0
}

// ── Movement / UI ─────────────────────────────────────────────────────────────

pub unsafe fn sl_pc_move(sd: &mut MapSessionData, speed: i32) {
    clif_noparsewalk(sd, speed as i8);
}

pub unsafe fn sl_pc_lookat(sd: &mut MapSessionData, id: i32) {
    let bl = map_id2bl_acc(id as u32);
    if !bl.is_null() { clif_parselookat_scriptsub(sd, bl); }
}

pub unsafe fn sl_pc_minirefresh(sd: &mut MapSessionData) {
    clif_refreshnoclick(sd);
}

pub unsafe fn sl_pc_refreshinventory(sd: &mut MapSessionData) {
    for i in 0..MAX_INVENTORY as i32 {
        clif_sendadditem(sd, i);
    }
}

pub unsafe fn sl_pc_updateinv(sd: &mut MapSessionData) {
    pc_loaditem(sd);
}

pub unsafe fn sl_pc_checkinvbod(sd: &mut MapSessionData) {
    clif_checkinvbod(sd);
}

// ── Equipment ────────────────────────────────────────────────────────────────

pub unsafe fn sl_pc_equip(sd: &mut MapSessionData) {
    pc_equipscript(sd);
}

pub unsafe fn sl_pc_takeoff(sd: &mut MapSessionData) {
    pc_unequipscript(sd);
}

pub unsafe fn sl_pc_deductarmor(sd: &mut MapSessionData, v: i32) {
    clif_deductarmor(&mut *sd, v);
}

pub fn sl_pc_deductweapon(sd: &mut MapSessionData, v: i32) {
    clif_deductweapon(&mut *sd, v);
}

pub unsafe fn sl_pc_deductdura(sd: &mut MapSessionData, eq: i32, v: i32) {
    clif_deductdura(&mut *sd, eq, v);
}

pub fn sl_pc_deductduraequip(sd: &mut MapSessionData) {
    clif_deductduraequip(&mut *sd);
}

pub fn sl_pc_deductdurainv(sd: &mut MapSessionData, slot: i32, v: i32) {
    if slot >= 0 && (slot as usize) < MAX_INVENTORY {
        sd.status.inventory[slot as usize].dura -= v;
    }
}

pub fn sl_pc_hasequipped(sd: &mut MapSessionData, item_id: u32) -> i32 {
    for i in 0..MAX_EQUIP {
        if sd.status.equip[i].id == item_id { return 1; }
    }
    0
}

pub unsafe fn sl_pc_removeitemslot(sd: &mut MapSessionData, slot: i32, amount: i32, kind: i32) {
    pc_delitem(sd, slot, amount, kind);
}

pub fn sl_pc_hasitem(sd: &mut MapSessionData, item_id: u32, amount: i32) -> i32 {
    let mut found = 0i32;
    for i in 0..MAX_INVENTORY {
        if sd.status.inventory[i].id == item_id {
            found += sd.status.inventory[i].amount as i32;
        }
    }
    if found >= amount { found } else { 0 }
}

pub unsafe fn sl_pc_hasspace(sd: &mut MapSessionData, item_id: u32) -> i32 {
    pc_isinvenspace(sd, item_id as i32, 0, std::ptr::null(), 0, 0, 0, 0)
}

// ── Stats ─────────────────────────────────────────────────────────────────────

pub unsafe fn sl_pc_checklevel(sd: &mut MapSessionData) {
    pc_checklevel(sd);
}

// ── Display / UI ──────────────────────────────────────────────────────────────

pub unsafe fn sl_pc_sendminimap(sd: &mut MapSessionData) {
    clif_sendminimap(sd);
}

pub unsafe fn sl_pc_popup(sd: &mut MapSessionData, msg: *const i8) {
    // clif_popup is in visual.rs
    use crate::game::client::visual::clif_popup;
    clif_popup(sd, msg);
}

pub unsafe fn sl_pc_guitext(sd: &mut MapSessionData, msg: *const i8) {
    clif_guitextsd(msg, sd);
}

pub unsafe fn sl_pc_sendminitext(sd: &mut MapSessionData, msg: *const i8) {
    clif_sendminitext(sd, msg);
}

pub fn sl_pc_powerboard(_sd: &mut MapSessionData) { /* stub */ }

pub unsafe fn sl_pc_showboard(sd: &mut MapSessionData, id: i32) {
    boards_showposts(sd, id);
}

pub unsafe fn sl_pc_showpost(sd: &mut MapSessionData, id: i32, post: i32) {
    boards_readpost(sd, id, post);
}

pub unsafe fn sl_pc_changeview(sd: &mut MapSessionData, x: i32, y: i32) {
    clif_sendxychange(sd, x, y);
}

// ── Social ────────────────────────────────────────────────────────────────────

pub unsafe fn sl_pc_speak(sd: &mut MapSessionData, msg: *const i8, len: i32, kind: i32) {
    clif_sendscriptsay(sd, msg, len, kind);
}

pub unsafe fn sl_pc_sendmail(sd: &mut MapSessionData, to: *const i8, topic: *const i8, msg: *const i8) -> i32 {
    nmail_sendmail(sd, to, topic, msg)
}

pub unsafe fn sl_pc_sendurl(sd: &mut MapSessionData, kind: i32, url: *const i8) {
    clif_sendurl(sd, kind, url);
}

pub unsafe fn sl_pc_swingtarget(sd: &mut MapSessionData, id: i32) {
    use crate::game::mob::BL_MOB;
    let bl = map_id2bl_acc(id as u32);
    if bl.is_null() { return; }
    if (*bl).bl_type as i32 == BL_MOB {
        clif_mob_damage(&mut *sd, &mut *(bl as *mut MobSpawnData));
    } else {
        clif_pc_damage(&mut *sd, &mut *(bl as *mut MapSessionData));
    }
}

// ── Kill registry ─────────────────────────────────────────────────────────────

pub unsafe fn sl_pc_killcount(sd: &mut MapSessionData, mob_id: i32) -> i32 {
    for x in 0..MAX_KILLREG {
        if sd.status.killreg[x].mob_id == mob_id as u32 {
            return sd.status.killreg[x].amount as i32;
        }
    }
    0
}

pub unsafe fn sl_pc_setkillcount(sd: &mut MapSessionData, mob_id: i32, amount: i32) {
    for x in 0..MAX_KILLREG {
        if sd.status.killreg[x].mob_id == mob_id as u32 {
            sd.status.killreg[x].amount = amount as u32;
            return;
        }
    }
    for x in 0..MAX_KILLREG {
        if sd.status.killreg[x].mob_id == 0 {
            sd.status.killreg[x].mob_id = mob_id as u32;
            sd.status.killreg[x].amount = amount as u32;
            return;
        }
    }
}

pub unsafe fn sl_pc_flushkills(sd: &mut MapSessionData, mob_id: i32) {
    for x in 0..MAX_KILLREG {
        if mob_id == 0 || sd.status.killreg[x].mob_id == mob_id as u32 {
            sd.status.killreg[x].mob_id = 0;
            sd.status.killreg[x].amount = 0;
        }
    }
}

pub unsafe fn sl_pc_flushallkills(sd: &mut MapSessionData) {
    sl_pc_flushkills(sd, 0);
}

// ── Threat ────────────────────────────────────────────────────────────────────

pub unsafe fn sl_pc_addthreat(sd: &mut MapSessionData, mob_id: u32, amount: u32) {
    let mob = map_id2mob_acc(mob_id);
    if mob.is_null() { return; }
    let uid = sd.bl.id;
    (*mob).lastaction = libc::time(std::ptr::null_mut()) as i32;
    for x in 0..MAX_THREATCOUNT {
        if (*mob).threat[x].user == uid { (*mob).threat[x].amount += amount; return; }
        if (*mob).threat[x].user == 0  { (*mob).threat[x].user = uid; (*mob).threat[x].amount = amount; return; }
    }
}

pub unsafe fn sl_pc_setthreat(sd: &mut MapSessionData, mob_id: u32, amount: u32) {
    let mob = map_id2mob_acc(mob_id);
    if mob.is_null() { return; }
    let uid = sd.bl.id;
    (*mob).lastaction = libc::time(std::ptr::null_mut()) as i32;
    for x in 0..MAX_THREATCOUNT {
        if (*mob).threat[x].user == uid { (*mob).threat[x].amount = amount; return; }
        if (*mob).threat[x].user == 0  { (*mob).threat[x].user = uid; (*mob).threat[x].amount = amount; return; }
    }
}

pub unsafe fn sl_pc_addthreatgeneral(_sd: &mut MapSessionData, _amount: u32) { /* stub */ }

// ── Spell list ────────────────────────────────────────────────────────────────

pub unsafe fn sl_pc_hasspell(sd: &mut MapSessionData, name: *const i8) -> i32 {
    let id = magicdb_id(name); if id <= 0 { return 0; }
    for i in 0..MAX_SPELLS {
        if sd.status.skill[i] == id as u16 { return 1; }
    }
    0
}

pub unsafe fn sl_pc_addspell(sd: &mut MapSessionData, spell_id: i32) {
    for i in 0..MAX_SPELLS {
        if sd.status.skill[i] == 0 {
            sd.status.skill[i] = spell_id as u16;
            pc_loadmagic(sd);
            return;
        }
    }
}

pub fn sl_pc_removespell(sd: &mut MapSessionData, spell_id: i32) {
    for i in 0..MAX_SPELLS {
        if sd.status.skill[i] == spell_id as u16 { sd.status.skill[i] = 0; }
    }
}

// ── Duration system ───────────────────────────────────────────────────────────

pub unsafe fn sl_pc_hasduration(sd: &mut MapSessionData, name: *const i8) -> i32 {
    let id = magicdb_id(name); if id <= 0 { return 0; }
    for i in 0..MAX_MAGIC_TIMERS {
        if sd.status.dura_aether[i].id == id as u16 && sd.status.dura_aether[i].duration > 0 { return 1; }
    }
    0
}

pub unsafe fn sl_pc_hasdurationid(sd: &mut MapSessionData, name: *const i8, caster_id: i32) -> i32 {
    let id = magicdb_id(name); if id <= 0 { return 0; }
    for i in 0..MAX_MAGIC_TIMERS {
        if sd.status.dura_aether[i].id == id as u16
            && sd.status.dura_aether[i].caster_id == caster_id as u32
            && sd.status.dura_aether[i].duration > 0 { return 1; }
    }
    0
}

pub unsafe fn sl_pc_getduration(sd: &mut MapSessionData, name: *const i8) -> i32 {
    let id = magicdb_id(name); if id <= 0 { return 0; }
    for i in 0..MAX_MAGIC_TIMERS {
        if sd.status.dura_aether[i].id == id as u16 { return sd.status.dura_aether[i].duration; }
    }
    0
}

pub unsafe fn sl_pc_getdurationid(sd: &mut MapSessionData, name: *const i8, caster_id: i32) -> i32 {
    let id = magicdb_id(name); if id <= 0 { return 0; }
    for i in 0..MAX_MAGIC_TIMERS {
        if sd.status.dura_aether[i].id == id as u16
            && sd.status.dura_aether[i].caster_id == caster_id as u32 {
            return sd.status.dura_aether[i].duration;
        }
    }
    0
}

pub unsafe fn sl_pc_durationamount(sd: &mut MapSessionData, name: *const i8) -> i32 {
    let id = magicdb_id(name); if id <= 0 { return 0; }
    let mut count = 0;
    for i in 0..MAX_MAGIC_TIMERS {
        if sd.status.dura_aether[i].id == id as u16 && sd.status.dura_aether[i].duration > 0 { count += 1; }
    }
    count
}

pub unsafe fn sl_pc_setduration(sd: &mut MapSessionData, name: *const i8, mut time_ms: i32, caster_id: i32, recast: i32) {
    let id = magicdb_id(name); if id <= 0 { return; }
    if time_ms > 0 && time_ms < 1000 { time_ms = 1000; }
    let mut alreadycast = false;
    for x in 0..MAX_MAGIC_TIMERS {
        if sd.status.dura_aether[x].id == id as u16
            && sd.status.dura_aether[x].caster_id == caster_id as u32
            && sd.status.dura_aether[x].duration > 0 { alreadycast = true; break; }
    }
    for x in 0..MAX_MAGIC_TIMERS {
        let da_id = sd.status.dura_aether[x].id;
        let da_caster = sd.status.dura_aether[x].caster_id;
        let da_aether = sd.status.dura_aether[x].aether;
        let da_duration = sd.status.dura_aether[x].duration;
        if da_id == id as u16 && time_ms <= 0 && da_caster == caster_id as u32 && alreadycast {
            let tsd = map_id2sd_acc(da_caster);
            clif_send_duration(&mut *sd, id, time_ms, tsd);
            sd.status.dura_aether[x].duration = 0; sd.status.dura_aether[x].caster_id = 0;
            if da_aether == 0 { sd.status.dura_aether[x].id = 0; }
            return;
        } else if da_id == id as u16 && da_caster == caster_id as u32
            && da_aether > 0 && da_duration <= 0 {
            sd.status.dura_aether[x].duration = time_ms;
            clif_send_duration(&mut *sd, id, time_ms / 1000, map_id2sd_acc(da_caster));
            return;
        } else if da_id == id as u16 && da_caster == caster_id as u32
            && (da_duration > time_ms || recast != 0) && alreadycast {
            sd.status.dura_aether[x].duration = time_ms;
            clif_send_duration(&mut *sd, id, time_ms / 1000, map_id2sd_acc(da_caster));
            return;
        } else if da_id == 0 && da_duration == 0 && time_ms != 0 && !alreadycast {
            sd.status.dura_aether[x].id = id as u16;
            sd.status.dura_aether[x].duration = time_ms;
            sd.status.dura_aether[x].caster_id = caster_id as u32;
            clif_send_duration(&mut *sd, id, time_ms / 1000, map_id2sd_acc(caster_id as u32));
            return;
        }
    }
}

pub unsafe fn sl_pc_flushduration(sd: &mut MapSessionData, _dispel_level: i32, min_id: i32, max_id: i32) {
    for x in 0..MAX_MAGIC_TIMERS {
        let id = sd.status.dura_aether[x].id as i32;
        if id == 0 || sd.status.dura_aether[x].duration <= 0 { continue; }
        if min_id > 0 && id < min_id { continue; }
        if max_id > 0 && id > max_id { continue; }
        let tsd = map_id2sd_acc(sd.status.dura_aether[x].caster_id);
        clif_send_duration(&mut *sd, id, 0, tsd);
        sd.status.dura_aether[x].duration = 0; sd.status.dura_aether[x].caster_id = 0;
        if sd.status.dura_aether[x].aether == 0 { sd.status.dura_aether[x].id = 0; }
    }
}

pub unsafe fn sl_pc_flushdurationnouncast(sd: &mut MapSessionData, dispel_level: i32, min_id: i32, max_id: i32) {
    sl_pc_flushduration(sd, dispel_level, min_id, max_id);
}

pub unsafe fn sl_pc_refreshdurations(sd: &mut MapSessionData) {
    for x in 0..MAX_MAGIC_TIMERS {
        let da = sd.status.dura_aether[x];
        if da.id > 0 && da.duration > 0 {
            let tsd = map_id2sd_acc(da.caster_id);
            clif_send_duration(&mut *sd, da.id as i32, da.duration / 1000, tsd);
        }
    }
}

// ── Aether system ─────────────────────────────────────────────────────────────

pub unsafe fn sl_pc_setaether(sd: &mut MapSessionData, name: *const i8, mut time_ms: i32) {
    let id = magicdb_id(name); if id <= 0 { return; }
    if time_ms > 0 && time_ms < 1000 { time_ms = 1000; }
    let mut alreadycast = false;
    for x in 0..MAX_MAGIC_TIMERS {
        if sd.status.dura_aether[x].id == id as u16 { alreadycast = true; break; }
    }
    for x in 0..MAX_MAGIC_TIMERS {
        let da_id = sd.status.dura_aether[x].id;
        let da_aether = sd.status.dura_aether[x].aether;
        let da_duration = sd.status.dura_aether[x].duration;
        if da_id == id as u16 && time_ms <= 0 {
            clif_send_aether(&mut *sd, id, time_ms);
            if da_duration == 0 { sd.status.dura_aether[x].id = 0; }
            sd.status.dura_aether[x].aether = 0; return;
        } else if da_id == id as u16 && (da_aether > time_ms || da_duration > 0) {
            sd.status.dura_aether[x].aether = time_ms;
            clif_send_aether(&mut *sd, id, time_ms / 1000); return;
        } else if da_id == 0 && da_aether == 0 && time_ms != 0 && !alreadycast {
            sd.status.dura_aether[x].id = id as u16; sd.status.dura_aether[x].aether = time_ms;
            clif_send_aether(&mut *sd, id, time_ms / 1000); return;
        }
    }
}

pub unsafe fn sl_pc_hasaether(sd: &mut MapSessionData, name: *const i8) -> i32 {
    let id = magicdb_id(name); if id <= 0 { return 0; }
    for i in 0..MAX_MAGIC_TIMERS {
        if sd.status.dura_aether[i].id == id as u16 && sd.status.dura_aether[i].aether > 0 { return 1; }
    }
    0
}

pub unsafe fn sl_pc_getaether(sd: &mut MapSessionData, name: *const i8) -> i32 {
    let id = magicdb_id(name); if id <= 0 { return 0; }
    for i in 0..MAX_MAGIC_TIMERS {
        if sd.status.dura_aether[i].id == id as u16 { return sd.status.dura_aether[i].aether; }
    }
    0
}

pub unsafe fn sl_pc_flushaether(sd: &mut MapSessionData) {
    for i in 0..MAX_MAGIC_TIMERS {
        if sd.status.dura_aether[i].aether > 0 {
            let aether_id = sd.status.dura_aether[i].id as i32;
            let aether_dur = sd.status.dura_aether[i].duration;
            clif_send_aether(&mut *sd, aether_id, 0);
            sd.status.dura_aether[i].aether = 0;
            if aether_dur == 0 { sd.status.dura_aether[i].id = 0; }
        }
    }
}

// ── Misc ──────────────────────────────────────────────────────────────────────

pub fn sl_pc_addclan(_sd: &mut MapSessionData, _name: *const i8) { /* stub */ }

pub fn sl_pc_updatepath(sd: &mut MapSessionData, path: i32, mark: i32) {
    let id = sd.status.id;
    let _ = crate::database::blocking_run_async(async move {
        sqlx::query("UPDATE `Character` SET `ChaPthId`=?,`ChaMark`=? WHERE `ChaId`=?")
            .bind(path).bind(mark).bind(id)
            .execute(crate::database::get_pool()).await
    });
}

pub fn sl_pc_updatecountry(sd: &mut MapSessionData, country: i32) {
    let id = sd.status.id;
    let _ = crate::database::blocking_run_async(async move {
        sqlx::query("UPDATE `Character` SET `ChaNation`=? WHERE `ChaId`=?")
            .bind(country).bind(id)
            .execute(crate::database::get_pool()).await
    });
}

pub unsafe fn sl_pc_getcasterid(_sd: &mut MapSessionData, name: *const i8) -> i32 {
    magicdb_id(name)
}

pub unsafe fn sl_pc_settimer(sd: &mut MapSessionData, kind: i32, length: u32) {
    clif_send_timer(sd, kind as i8, length);
}

pub unsafe fn sl_pc_addtime(sd: &mut MapSessionData, v: i32) {
    sd.disptimertick = sd.disptimertick.wrapping_add(v as u32);
    clif_send_timer(sd, sd.disptimertype, sd.disptimertick);
}

pub unsafe fn sl_pc_removetime(sd: &mut MapSessionData, v: i32) {
    sd.disptimertick = sd.disptimertick.saturating_sub(v as u32);
    clif_send_timer(sd, sd.disptimertype, sd.disptimertick);
}

pub fn sl_pc_setheroshow(sd: &mut MapSessionData, flag: i32) {
    sd.status.heroes = flag as u32;
    let id = sd.status.id;
    let _ = crate::database::blocking_run_async(async move {
        sqlx::query("UPDATE `Character` SET `ChaHeroShow`=? WHERE `ChaId`=?")
            .bind(flag).bind(id)
            .execute(crate::database::get_pool()).await
    });
}

// ── Legends ───────────────────────────────────────────────────────────────────

pub unsafe fn sl_pc_addlegend(
    sd: &mut MapSessionData, text: *const i8, name: *const i8,
    icon: i32, color: i32, tchaid: u32,
) {
    use crate::servers::char::charstatus::MAX_LEGENDS;
    for x in 0..MAX_LEGENDS {
        let empty_now  = sd.status.legends[x].name[0] == 0;
        let empty_next = x + 1 >= MAX_LEGENDS || sd.status.legends[x + 1].name[0] == 0;
        if empty_now && empty_next {
            let leg = &mut sd.status.legends[x];
            bounded_copy(leg.text.as_mut_ptr(), text, leg.text.len());
            bounded_copy(leg.name.as_mut_ptr(), name, leg.name.len());
            leg.icon   = icon as u16;
            leg.color  = color as u16;
            leg.tchaid = tchaid;
            return;
        }
    }
}

pub unsafe fn sl_pc_haslegend(sd: &mut MapSessionData, name: *const i8) -> i32 {
    use crate::servers::char::charstatus::MAX_LEGENDS;
    let cmp = if name.is_null() { b"" as &[u8] } else { std::ffi::CStr::from_ptr(name).to_bytes() };
    for i in 0..MAX_LEGENDS {
        let leg_name = sd.status.legends[i].name;
        if leg_name[0] != 0 {
            let leg_bytes = std::ffi::CStr::from_ptr(leg_name.as_ptr()).to_bytes();
            if leg_bytes.eq_ignore_ascii_case(cmp) { return 1; }
        }
    }
    0
}

pub unsafe fn sl_pc_removelegendbyname(sd: &mut MapSessionData, name: *const i8) {
    let legs = &mut sd.status.legends;
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

pub fn sl_pc_removelegendbycolor(sd: &mut MapSessionData, color: i32) {
    let legs = &mut sd.status.legends;
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

pub fn sl_pc_getpk(sd: &mut MapSessionData, id: i32) -> i32 {
    for x in 0..20 {
        if sd.pvp[x][0] == id as u32 { return 1; }
    }
    0
}

pub fn sl_pc_vregenoverflow(sd: &mut MapSessionData) -> i32 {
    sd.vregenoverflow as i32
}

pub fn sl_pc_mregenoverflow(sd: &mut MapSessionData) -> i32 {
    sd.mregenoverflow as i32
}

// ─── sl_user_* accessors ───────────────────────────

pub unsafe fn sl_user_coref(sd: &mut MapSessionData) -> u32 {
    sd.coref
}
pub unsafe fn sl_user_set_coref(sd: &mut MapSessionData, v: u32) {
    sd.coref = v;
}
pub unsafe fn sl_user_coref_container(sd: &mut MapSessionData) -> u32 {
    sd.coref_container
}
pub fn sl_user_map_id2sd(id: u32) -> *mut std::ffi::c_void {
    crate::game::map_server::map_id2sd_pc(id)
        .map(|arc| &*arc.write() as *const _ as *mut std::ffi::c_void)
        .unwrap_or(std::ptr::null_mut())
}

// ─── Mana / gold / time helpers ────────────────────

/// addMagic / addMana — add `amount` to sd->status.mp and send HP/MP status.
pub unsafe fn sl_pc_addmagic(sd: &mut MapSessionData, amount: i32) {
    sd.status.mp = (sd.status.mp as i32).wrapping_add(amount) as u32;
    clif_sendstatus(sd, SFLAG_HPMP);
}

/// addManaExtend — alias for addMagic.
pub unsafe fn sl_pc_addmanaextend(sd: &mut MapSessionData, amount: i32) {
    sl_pc_addmagic(sd, amount);
}

/// addGold — add gold to sd->status.money and send XP/money status.
pub unsafe fn sl_pc_addgold(sd: &mut MapSessionData, amount: i32) {
    sd.status.money = (sd.status.money as i32).wrapping_add(amount) as u32;
    clif_sendstatus(sd, SFLAG_XPMONEY);
}

/// removeGold — subtract gold (floor at 0) and send XP/money status.
pub unsafe fn sl_pc_removegold(sd: &mut MapSessionData, amount: i32) {
    if sd.status.money < amount as u32 {
        sd.status.money = 0;
    } else {
        sd.status.money -= amount as u32;
    }
    clif_sendstatus(sd, SFLAG_XPMONEY);
}

/// setTimeValues — prepend newval to the timevalues ring buffer.
pub fn sl_pc_settimevalues(sd: &mut MapSessionData, newval: u32) {
    let n = sd.timevalues.len();
    for i in (1..n).rev() {
        sd.timevalues[i] = sd.timevalues[i - 1];
    }
    sd.timevalues[0] = newval;
}

/// addHealth (extend variant) — heal by amount (negative damage).
pub unsafe fn sl_pc_addhealth_extend(sd: &mut MapSessionData, amount: i32) {
    clif_send_pc_healthscript(&mut *sd, -amount, 0);
    clif_sendstatus(sd, SFLAG_HPMP);
}

/// removeHealth (extend variant) — damage by amount, skipped if dead.
pub unsafe fn sl_pc_removehealth_extend(sd: &mut MapSessionData, damage: i32) {
    use crate::game::pc::PC_DIE;
    if sd.status.state != PC_DIE as i8 {
        clif_send_pc_healthscript(&mut *sd, damage, 0);
        clif_sendstatus(sd, SFLAG_HPMP);
    }
}

/// getEquippedDura — return durability of equipped item at slot, or -1 if not found.
pub fn sl_pc_getequippeddura(sd: &mut MapSessionData, id: u32, slot: i32) -> i32 {
    use crate::servers::char::charstatus::MAX_EQUIP;
    if slot >= 0 && (slot as usize) < MAX_EQUIP {
        let s = slot as usize;
        if sd.status.equip[s].id == id { return sd.status.equip[s].dura; }
    } else {
        for x in 0..MAX_EQUIP {
            if sd.status.equip[x].id == id { return sd.status.equip[x].dura; }
        }
    }
    -1
}

// ─── No-op stubs ───────────────────────────────────

pub fn sl_pc_addguide(_sd: &mut MapSessionData, _guide: i32) {}
pub fn sl_pc_delguide(_sd: &mut MapSessionData, _guide: i32) {}
pub fn sl_pc_logbuysell(
    _sd: &mut MapSessionData, _item: u32, _amount: u32, _gold: u32, _flag: i32) {}
pub fn sl_pc_calcthrow(_sd: &mut MapSessionData) {}
pub fn sl_pc_calcrangeddamage(_sd: &mut MapSessionData, _bl: *mut std::ffi::c_void) -> i32 { 0 }
pub fn sl_pc_calcrangedhit(_sd: &mut MapSessionData, _bl: *mut std::ffi::c_void) -> i32 { 0 }

// ─── sl_map_spell ──────────────────────────────────

/// Return map[m].spell (1 = spell-disabled), or 0 if map not loaded.
pub unsafe fn sl_map_spell(m: i32) -> i32 {
    let ptr = crate::database::map_db::get_map_ptr(m as u16);
    if ptr.is_null() || (*ptr).xs == 0 { return 0; }
    (*ptr).spell as i32
}

// ─── Bank field reads ─────────────────────────────────────────────────────────

pub fn sl_pc_checkbankitems(sd: &mut MapSessionData, slot: i32) -> i32 {
    if slot < 0 || slot as usize >= MAX_BANK_SLOTS { return 0; }
    sd.status.banks[slot as usize].item_id as i32
}

pub fn sl_pc_checkbankamounts(sd: &mut MapSessionData, slot: i32) -> i32 {
    if slot < 0 || slot as usize >= MAX_BANK_SLOTS { return 0; }
    sd.status.banks[slot as usize].amount as i32
}

pub fn sl_pc_checkbankowners(sd: &mut MapSessionData, slot: i32) -> i32 {
    if slot < 0 || slot as usize >= MAX_BANK_SLOTS { return 0; }
    sd.status.banks[slot as usize].owner as i32
}

pub fn sl_pc_checkbankengraves(sd: &mut MapSessionData, slot: i32) -> *const i8 {
    if slot < 0 || slot as usize >= MAX_BANK_SLOTS { return c"".as_ptr(); }
    sd.status.banks[slot as usize].real_name.as_ptr() as *const i8
}

// ─── Bank deposit / withdraw ──────────────────────────────────────────────────

pub unsafe fn sl_pc_bankdeposit(
    sd: &mut MapSessionData, item: u32, amount: u32, owner: u32, engrave: *const i8,
) {
    let engrave_bytes: &[u8] = if engrave.is_null() { b"\0" } else {
        std::slice::from_raw_parts(engrave as *const u8,
            libc::strlen(engrave) + 1)
    };
    // Find existing matching slot, else find empty slot.
    let mut deposit: Option<usize> = None;
    for x in 0..MAX_BANK_SLOTS {
        let b = &sd.status.banks[x];
        if b.item_id == item && b.owner == owner {
            let rn = b.real_name.as_ptr() as *const u8;
            let rn_len = libc::strlen(rn as *const i8) + 1;
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
        sd.status.banks[x].amount = sd.status.banks[x].amount.wrapping_add(amount);
    } else {
        for x in 0..MAX_BANK_SLOTS {
            if sd.status.banks[x].item_id == 0 {
                let b = &mut sd.status.banks[x];
                b.item_id = item;
                b.amount = amount;
                b.owner = owner;
                let src = if engrave.is_null() { c"".as_ptr() } else { engrave };
                libc::strncpy(b.real_name.as_mut_ptr() as *mut i8, src,
                              b.real_name.len() - 1);
                break;
            }
        }
    }
}

pub unsafe fn sl_pc_bankwithdraw(
    sd: &mut MapSessionData, item: u32, amount: u32, owner: u32, engrave: *const i8,
) {
    let engrave_bytes: &[u8] = if engrave.is_null() { b"\0" } else {
        std::slice::from_raw_parts(engrave as *const u8,
            libc::strlen(engrave) + 1)
    };
    let mut deposit: Option<usize> = None;
    for x in 0..MAX_BANK_SLOTS {
        let b = &sd.status.banks[x];
        if b.item_id == item && b.owner == owner {
            let rn = b.real_name.as_ptr() as *const u8;
            let rn_len = libc::strlen(rn as *const i8) + 1;
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
    if sd.status.banks[x].amount <= amount {
        sd.status.banks[x] = std::mem::zeroed();
    } else {
        sd.status.banks[x].amount -= amount;
    }
}

pub unsafe fn sl_pc_bankcheckamount(
    sd: &mut MapSessionData, item: u32, _amount: u32, owner: u32, engrave: *const i8,
) -> i32 {
    let engrave_bytes: &[u8] = if engrave.is_null() { b"\0" } else {
        std::slice::from_raw_parts(engrave as *const u8,
            libc::strlen(engrave) + 1)
    };
    let mut total: u32 = 0;
    for x in 0..MAX_BANK_SLOTS {
        let b = &sd.status.banks[x];
        if b.item_id == item && b.owner == owner {
            let rn = b.real_name.as_ptr() as *const u8;
            let rn_len = libc::strlen(rn as *const i8) + 1;
            let rn_bytes = std::slice::from_raw_parts(rn, rn_len);
            if engrave_bytes.len() == rn_bytes.len()
                && engrave_bytes.eq_ignore_ascii_case(rn_bytes)
            {
                total = total.wrapping_add(b.amount);
            }
        }
    }
    total as i32
}

// ─── Clan bank — no-ops (SQL-backed; deposit/withdraw handled in scripting.c) ─

pub unsafe fn sl_pc_clanbankdeposit(
    _sd: &mut MapSessionData, _item: u32, _amount: u32, _owner: u32, _engrave: *const i8,
) {}

pub unsafe fn sl_pc_clanbankwithdraw(
    _sd: &mut MapSessionData, _item: u32, _amount: u32, _owner: u32, _engrave: *const i8,
) {}

// ─── No-op stubs ──────────────────────────────────────────────────────────────

pub unsafe fn sl_pc_getunknownspells(
    _sd: &mut MapSessionData, _out_ids: *mut i32, _max: i32,
) -> i32 { 0 }

pub unsafe fn sl_pc_getparcel(_sd: &mut MapSessionData) -> *mut std::ffi::c_void { std::ptr::null_mut() }

pub unsafe fn sl_pc_getparcellist(
    _sd: &mut MapSessionData, _out: *mut *mut std::ffi::c_void, _max: i32,
) -> i32 { 0 }

// ─── Kill registry ────────────────────────────────────────────────────────────

pub unsafe fn sl_pc_killrank(sd: &mut MapSessionData, mob_id: i32) -> i32 {
    for x in 0..MAX_KILLREG {
        if sd.status.killreg[x].mob_id as i32 == mob_id {
            return sd.status.killreg[x].amount as i32;
        }
    }
    0
}

// ─── Misc PC helpers ──────────────────────────────────────────────────────────

use crate::game::map_parse::chat::{clif_sendmsg as clif_sendmsg_pc, clif_broadcast as clif_broadcast_pc};

pub unsafe fn sl_pc_gmmsg(sd: &mut MapSessionData, msg: *const i8) {
    if msg.is_null() { return; }
    clif_sendmsg_pc(sd, 0, msg);
}

pub unsafe fn sl_pc_talkself(sd: &mut MapSessionData, color: i32, msg: *const i8) {
    if msg.is_null() { return; }
    clif_sendmsg_pc(sd, color, msg);
}

pub unsafe fn sl_pc_broadcast_sd(
    _sd: &mut MapSessionData, msg: *const i8, m: i32,
) {
    if msg.is_null() { return; }
    clif_broadcast_pc(msg, m);
}

// ─── Inventory / equip slot pointers ─────────────────────────────────────────

pub unsafe fn sl_pc_getinventoryitem(sd: &mut MapSessionData, slot: i32) -> *mut std::ffi::c_void {
    if slot < 0 || slot as usize >= MAX_INVENTORY { return std::ptr::null_mut(); }
    if sd.status.inventory[slot as usize].id == 0 { return std::ptr::null_mut(); }
    &mut sd.status.inventory[slot as usize] as *mut _ as *mut std::ffi::c_void
}

pub unsafe fn sl_pc_getequippeditem_sd(sd: &mut MapSessionData, slot: i32) -> *mut std::ffi::c_void {
    if slot < 0 || slot as usize >= MAX_EQUIP { return std::ptr::null_mut(); }
    if sd.status.equip[slot as usize].id == 0 { return std::ptr::null_mut(); }
    &mut sd.status.equip[slot as usize] as *mut _ as *mut std::ffi::c_void
}

// ─── Inventory mutation: add / remove items ───────────────────────────────────

pub unsafe fn sl_pc_additem(
    sd: &mut MapSessionData,
    id: u32, amount: u32,
    dura: i32, owner: u32,
    engrave: *const i8,
) {
    let mut fl: crate::servers::char::charstatus::Item = std::mem::zeroed();
    fl.id     = id;
    fl.amount = amount as i32;
    fl.owner  = owner;
    fl.dura   = if dura != 0 { dura } else { crate::database::item_db::search(id).dura };
    fl.protected = crate::database::item_db::search(id).protected as u32;
    if !engrave.is_null() && *engrave != 0 {
        libc::strncpy(fl.real_name.as_mut_ptr(), engrave, fl.real_name.len() - 1);
    }
    pc_additem_acc(sd, &mut fl);
}

pub unsafe fn sl_pc_removeitem(
    sd: &mut MapSessionData,
    id: u32, mut amount: u32,
    type_: i32, owner: u32,
    engrave: *const i8,
) {
    let engrave = if engrave.is_null() { c"".as_ptr() } else { engrave };
    let maxinv = sd.status.maxinv as usize;
    for x in 0..maxinv {
        if amount == 0 { break; }
        let inv = &sd.status.inventory[x];
        if inv.id != id { continue; }
        if owner != 0 && inv.owner != owner { continue; }
        if libc::strcasecmp(inv.real_name.as_ptr(), engrave) != 0 { continue; }
        let avail = inv.amount as u32;
        if avail == 0 { continue; }
        let take = avail.min(amount);
        crate::game::pc::rust_pc_delitem(sd, x as i32, take as i32, type_);
        amount -= take;
    }
}

pub unsafe fn sl_pc_removeitemdura(
    sd: &mut MapSessionData,
    id: u32, mut amount: u32,
    type_: i32,
) {
    let max_dura = crate::database::item_db::search(id).dura;
    let maxinv = sd.status.maxinv as usize;
    for x in 0..maxinv {
        if amount == 0 { break; }
        let inv = &sd.status.inventory[x];
        if inv.id != id { continue; }
        if inv.dura != max_dura { continue; }
        let avail = inv.amount as u32;
        if avail == 0 { continue; }
        let take = avail.min(amount);
        crate::game::pc::rust_pc_delitem(sd, x as i32, take as i32, type_);
        amount -= take;
    }
}

pub unsafe fn sl_pc_hasitemdura(
    sd: &mut MapSessionData, id: u32, mut amount: u32,
) -> i32 {
    let max_dura = crate::database::item_db::search(id).dura;
    let maxinv = sd.status.maxinv as usize;
    for x in 0..maxinv {
        if amount == 0 { break; }
        let inv = &sd.status.inventory[x];
        if inv.id != id { continue; }
        if inv.dura != max_dura { continue; }
        let avail = inv.amount as u32;
        if avail == 0 { continue; }
        if avail >= amount { return 0; }
        amount -= avail;
    }
    amount as i32
}

// ─── Spell lists ──────────────────────────────────────────────────────────────

pub unsafe fn sl_pc_getspells(
    sd: &mut MapSessionData, out_ids: *mut i32, max: i32,
) -> i32 {
    if out_ids.is_null() { return 0; }
    let mut count = 0i32;
    for x in 0..MAX_SPELLS {
        if count >= max { break; }
        if sd.status.skill[x] != 0 {
            *out_ids.add(count as usize) = sd.status.skill[x] as i32;
            count += 1;
        }
    }
    count
}

pub unsafe fn sl_pc_getspellnames(
    sd: &mut MapSessionData, out_names: *mut *const i8, max: i32,
) -> i32 {
    if out_names.is_null() { return 0; }
    let mut count = 0i32;
    for x in 0..MAX_SPELLS {
        if count >= max { break; }
        if sd.status.skill[x] != 0 {
            *out_names.add(count as usize) = magicdb_name(sd.status.skill[x] as i32);
            count += 1;
        }
    }
    count
}

pub unsafe fn sl_pc_getalldurations(
    sd: &mut MapSessionData, out_names: *mut *const i8, max: i32,
) -> i32 {
    if out_names.is_null() { return 0; }
    let mut count = 0i32;
    for i in 0..MAX_MAGIC_TIMERS {
        if count >= max { break; }
        let da = &sd.status.dura_aether[i];
        if da.id > 0 && da.duration > 0 {
            *out_names.add(count as usize) = magicdb_yname(da.id as i32);
            count += 1;
        }
    }
    count
}

// ─── Legends ──────────────────────────────────────────────────────────────────

pub unsafe fn sl_pc_getlegend(
    sd: &mut MapSessionData, name: *const i8,
) -> *const i8 {
    if name.is_null() { return std::ptr::null(); }
    for x in 0..MAX_LEGENDS {
        if libc::strcasecmp(sd.status.legends[x].name.as_ptr(), name) == 0 {
            return sd.status.legends[x].text.as_ptr() as *const i8;
        }
    }
    std::ptr::null()
}

// ─── Active spell check ───────────────────────────────────────────────────────

pub unsafe fn sl_pc_activespells(sd: &mut MapSessionData, name: *const i8) -> i32 {
    if name.is_null() { return 0; }
    let id = magicdb_id(name);
    for x in 0..MAX_MAGIC_TIMERS {
        let da = &sd.status.dura_aether[x];
        if da.id as i32 == id && da.duration > 0 { return 1; }
    }
    0
}

// ─── Give XP ─────────────────────────────────────────────────────────────────

pub unsafe fn sl_pc_givexp(sd: &mut MapSessionData, amount: u32) {
    crate::game::pc::rust_pc_givexp(sd, amount, crate::config_globals::XP_RATE.load(std::sync::atomic::Ordering::Relaxed) as u32);
}

// ─── Clan bank reads ──────────────────────────────────────────────────────────

pub unsafe fn sl_pc_getclanitems(sd: &mut MapSessionData, slot: i32) -> i32 {
    let clan = crate::database::clan_db::rust_clandb_search(sd.status.clan as i32);
    if clan.is_null() || (*clan).clanbanks.is_null() { return 0; }
    if slot < 0 || slot >= 255 { return 0; }
    (*(*clan).clanbanks.add(slot as usize)).item_id as i32
}

pub unsafe fn sl_pc_getclanamounts(sd: &mut MapSessionData, slot: i32) -> i32 {
    let clan = crate::database::clan_db::rust_clandb_search(sd.status.clan as i32);
    if clan.is_null() || (*clan).clanbanks.is_null() { return 0; }
    if slot < 0 || slot >= 255 { return 0; }
    (*(*clan).clanbanks.add(slot as usize)).amount as i32
}

pub unsafe fn sl_pc_checkclankitemamounts(
    sd: &mut MapSessionData, item: i32, _amount: i32,
) -> i32 {
    let clan = crate::database::clan_db::rust_clandb_search(sd.status.clan as i32);
    if clan.is_null() || (*clan).clanbanks.is_null() { return 0; }
    let mut total: u32 = 0;
    for x in 0..255usize {
        let b = &*(*clan).clanbanks.add(x);
        if b.item_id as i32 == item { total = total.wrapping_add(b.amount); }
    }
    total as i32
}

// ─── Creation packet reads ────────────────────────────────────────────────────

pub unsafe fn sl_pc_getcreationitems(
    sd: &mut MapSessionData, len: i32, out: *mut u32,
) -> i32 {
    if out.is_null() { return 0; }
    let curitem = rfifob(sd.fd, len as usize) as i32 - 1;
    let maxinv = sd.status.maxinv as i32;
    if curitem >= 0 && curitem < maxinv && sd.status.inventory[curitem as usize].id != 0 {
        *out = sd.status.inventory[curitem as usize].id;
        return 1;
    }
    0
}

pub unsafe fn sl_pc_getcreationamounts(
    sd: &mut MapSessionData, len: i32, item_id: u32,
) -> i32 {
    let t = crate::database::item_db::search(item_id).typ as i32;
    if t < 3 || t > 17 {
        rfifob(sd.fd, len as usize) as i32
    } else {
        1
    }
}

// ─── Dialog send helpers ──────────────────────────────────────────────────────

use crate::game::map_parse::dialogs::{
    clif_input as clif_input_pc, clif_scriptmes as clif_scriptmes_pc,
    clif_scriptmenuseq as clif_scriptmenuseq_pc,
    clif_buydialog as clif_buydialog_pc, clif_selldialog as clif_selldialog_pc,
};

pub unsafe fn sl_pc_input_send(sd: &mut MapSessionData, msg: *const i8) {
    clif_input_pc(sd, sd.last_click as i32, msg, c"".as_ptr());
}

pub unsafe fn sl_pc_dialog_send(
    sd: &mut MapSessionData, msg: *const i8, prev: i32, next: i32,
) {
    clif_scriptmes_pc(sd, sd.last_click as i32, msg, prev, next);
}

pub unsafe fn sl_pc_dialogseq_send(
    sd: &mut MapSessionData, entries: *const *const i8, n: i32, can_continue: i32,
) {
    // Concatenate all text entries into a single dialog message separated by newlines.
    let mut combined = String::new();
    for i in 0..n as usize {
        if entries.is_null() { break; }
        let p = *entries.add(i);
        if !p.is_null() {
            let s = std::ffi::CStr::from_ptr(p).to_string_lossy();
            if !combined.is_empty() { combined.push('\n'); }
            combined.push_str(&s);
        }
    }
    let cmsg = std::ffi::CString::new(combined).unwrap_or_default();
    clif_scriptmes_pc(sd, sd.last_click as i32, cmsg.as_ptr(), 0, can_continue);
}

/// Build 1-indexed option array (buf[0]=NULL, buf[1..n]=options[0..n-1]) and call clif.
unsafe fn menu_send_1idx(
    sd: &mut MapSessionData, msg: *const i8,
    options: *const *const i8, n: i32,
) {
    let nu = n as usize;
    let mut buf: Vec<*const i8> = Vec::with_capacity(nu + 1);
    buf.push(std::ptr::null());
    for i in 0..nu { buf.push(if options.is_null() { std::ptr::null() } else { *options.add(i) }); }
    clif_scriptmenuseq_pc(sd, sd.last_click as i32, msg, buf.as_mut_ptr(), n, 0, 0);
}

pub unsafe fn sl_pc_menu_send(
    sd: &mut MapSessionData, msg: *const i8, options: *const *const i8, n: i32,
) {
    menu_send_1idx(sd, msg, options, n);
}

pub unsafe fn sl_pc_menuseq_send(
    sd: &mut MapSessionData, msg: *const i8, options: *const *const i8, n: i32,
) {
    menu_send_1idx(sd, msg, options, n);
}

pub unsafe fn sl_pc_menustring_send(
    sd: &mut MapSessionData, msg: *const i8, options: *const *const i8, n: i32,
) {
    menu_send_1idx(sd, msg, options, n);
}

pub fn sl_pc_menustring2_send(
    _sd: &mut MapSessionData, _msg: *const i8, _options: *const *const i8, _n: i32,
) {} // no matching clif_ packet

pub unsafe fn sl_pc_buy_send(
    sd: &mut MapSessionData, msg: *const i8,
    items: *const i32, values: *const i32,
    displaynames: *const *const i8, buytext: *const *const i8,
    n: i32,
) {
    if n <= 0 { return; }
    let nu = n as usize;
    let mut ilist: Vec<crate::servers::char::charstatus::Item> = vec![std::mem::zeroed(); nu];
    for i in 0..nu {
        ilist[i].id = *items.add(i) as u32;
        if !displaynames.is_null() && !(*displaynames.add(i)).is_null() {
            libc::strncpy(ilist[i].real_name.as_mut_ptr(), *displaynames.add(i),
                          ilist[i].real_name.len() - 1);
        }
        if !buytext.is_null() && !(*buytext.add(i)).is_null() {
            libc::strncpy(ilist[i].buytext.as_mut_ptr() as *mut i8,
                          *buytext.add(i), ilist[i].buytext.len() - 1);
        }
    }
    clif_buydialog_pc(sd, sd.last_click, msg, ilist.as_mut_ptr(), values as *mut i32, n);
}

pub unsafe fn sl_pc_buydialog_send(
    sd: &mut MapSessionData, msg: *const i8, items: *const i32, n: i32,
) {
    if n <= 0 { return; }
    let nu = n as usize;
    let mut ilist: Vec<crate::servers::char::charstatus::Item> = vec![std::mem::zeroed(); nu];
    for i in 0..nu { ilist[i].id = *items.add(i) as u32; }
    clif_buydialog_pc(sd, sd.last_click, msg, ilist.as_mut_ptr(), std::ptr::null_mut(), n);
}

pub unsafe fn sl_pc_buyextend_send(
    sd: &mut MapSessionData, msg: *const i8,
    items: *const i32, prices: *const i32,
    _maxamounts: *const i32, n: i32,
) {
    if n <= 0 { return; }
    let nu = n as usize;
    let mut ilist: Vec<crate::servers::char::charstatus::Item> = vec![std::mem::zeroed(); nu];
    for i in 0..nu { ilist[i].id = *items.add(i) as u32; }
    clif_buydialog_pc(sd, sd.last_click, msg, ilist.as_mut_ptr(), prices as *mut i32, n);
}

unsafe fn sell_send_inner(sd: &mut MapSessionData, msg: *const i8, items: *const i32, n: i32) {
    let nu = n as usize;
    let maxinv = sd.status.maxinv as usize;
    let mut slots: Vec<i32> = Vec::with_capacity(nu * 4);
    for j in 0..nu {
        let item_id = *items.add(j) as u32;
        for x in 0..maxinv {
            if sd.status.inventory[x].id == item_id { slots.push(x as i32); }
        }
    }
    clif_selldialog_pc(sd, sd.last_click, msg, slots.as_ptr() as *const i32, slots.len() as i32);
}

pub unsafe fn sl_pc_sell_send(
    sd: &mut MapSessionData, msg: *const i8, items: *const i32, n: i32,
) {
    if n <= 0 { return; }
    sell_send_inner(sd, msg, items, n);
}

pub unsafe fn sl_pc_sell2_send(
    sd: &mut MapSessionData, msg: *const i8, items: *const i32, n: i32,
) {
    sl_pc_sell_send(sd, msg, items, n);
}

pub unsafe fn sl_pc_sellextend_send(
    sd: &mut MapSessionData, msg: *const i8, items: *const i32, n: i32,
) {
    sl_pc_sell_send(sd, msg, items, n);
}

// Bank/clan bank/repair UI — no clif_ packet exists; all are no-ops.
pub fn sl_pc_showbank_send(_sd: &mut MapSessionData, _msg: *const i8) {}
pub fn sl_pc_showbankadd_send(_sd: &mut MapSessionData) {}
pub fn sl_pc_bankaddmoney_send(_sd: &mut MapSessionData) {}
pub fn sl_pc_bankwithdrawmoney_send(_sd: &mut MapSessionData) {}
pub fn sl_pc_clanshowbank_send(_sd: &mut MapSessionData, _msg: *const i8) {}
pub fn sl_pc_clanshowbankadd_send(_sd: &mut MapSessionData) {}
pub fn sl_pc_clanbankaddmoney_send(_sd: &mut MapSessionData) {}
pub fn sl_pc_clanbankwithdrawmoney_send(_sd: &mut MapSessionData) {}
pub fn sl_pc_clanviewbank_send(_sd: &mut MapSessionData) {}
pub fn sl_pc_repairextend_send(_sd: &mut MapSessionData) {}
pub fn sl_pc_repairall_send(_sd: &mut MapSessionData, _npc_bl: *mut std::ffi::c_void) {}

// ─── Extra extern declarations for later-ported functions ────────────────────

use crate::game::map_parse::player_state::clif_getchararea;
use crate::database::item_db;
use crate::game::pc::rust_pc_unequip as pc_unequip_slot;

// ─── Parcel removal ───────────────────────────────────────────────────────────

pub unsafe fn sl_pc_removeparcel(
    sd: &mut MapSessionData,
    _sender: i32, _item: u32, _amount: u32,
    pos: i32, _owner: u32,
    _engrave: *const i8, _npcflag: i32,
) {
    let char_id = sd.status.id;
    let _ = crate::database::blocking_run_async(async move {
        sqlx::query(
            "DELETE FROM `Parcels` WHERE `ParChaIdDestination`=? AND `ParPosition`=?"
        )
        .bind(char_id)
        .bind(pos)
        .execute(crate::database::get_pool()).await
    });
}

// ─── PvP / combat helpers ─────────────────────────────────────────────────────

/// Record `id` in the player's PvP kill list.
///
pub unsafe fn sl_pc_setpk(sd: &mut MapSessionData, id: i32) {
    let mut exist = -1i32;
    for x in 0..20usize {
        if sd.pvp[x][0] as i32 == id { exist = x as i32; break; }
    }
    if exist != -1 {
        sd.pvp[exist as usize][1] = libc::time(std::ptr::null_mut()) as u32;
    } else {
        for x in 0..20usize {
            if sd.pvp[x][0] == 0 {
                sd.pvp[x][0] = id as u32;
                sd.pvp[x][1] = libc::time(std::ptr::null_mut()) as u32;
                clif_getchararea(sd);
                break;
            }
        }
    }
}

/// Reduce HP without displaying a damage number.
///
pub unsafe fn sl_pc_removehealth_nodmgnum(sd: &mut MapSessionData, damage: i32, type_: i32) {
    use crate::game::pc::PC_DIE;
    if (sd.status.state as i32) != PC_DIE {
        clif_send_pc_health(&mut *sd, damage, type_);
    }
}

/// Expire timed items in inventory and equipped slots.
///
pub unsafe fn sl_pc_expireitem(sd: &mut MapSessionData) {
    let t = libc::time(std::ptr::null_mut()) as u32;

    for x in 0..sd.status.maxinv as usize {
        let id = sd.status.inventory[x].id;
        if id == 0 { continue; }
        let db_item = item_db::search(id);
        let item_t = db_item.time;
        let slot_t = sd.status.inventory[x].time;
        if (slot_t > 0 && slot_t < t) || (item_t > 0 && item_t < t) {
            let name = crate::game::scripting::types::item::fixed_str(&db_item.name);
            let msg = format!("Your {} has expired! Please visit the cash shop to purchase another.", name);
            if let Ok(cmsg) = std::ffi::CString::new(msg) {
                pc_delitem(sd, x as i32, 1, 8);
                clif_sendminitext(sd, cmsg.as_ptr());
            }
        }
    }

    // Find first empty inventory slot (receives the item moved by pc_unequip)
    let mut eqdel = -1i32;
    for x in 0..sd.status.maxinv as usize {
        if sd.status.inventory[x].id == 0 { eqdel = x as i32; break; }
    }

    for x in 0..MAX_EQUIP {
        let id = sd.status.equip[x].id;
        if id == 0 { continue; }
        let db_item = item_db::search(id);
        let item_t = db_item.time;
        let slot_t = sd.status.equip[x].time;
        if (slot_t > 0 && slot_t < t) || (item_t > 0 && item_t < t) {
            let name = crate::game::scripting::types::item::fixed_str(&db_item.name);
            let msg = format!("Your {} has expired! Please visit the cash shop to purchase another.", name);
            if let Ok(cmsg) = std::ffi::CString::new(msg) {
                pc_unequip_slot(sd, x as i32);
                if eqdel >= 0 { pc_delitem(sd, eqdel, 1, 8); }
                clif_sendminitext(sd, cmsg.as_ptr());
            }
        }
    }
}

/// Heal sd by `amount`; triggers `on_healed` Lua hook if attacker is set.
///
pub unsafe fn sl_pc_addhealth2(sd: &mut MapSessionData, amount: i32, _type: i32) {
    let bl_ptr = map_id2bl_acc(sd.attacker) as *mut crate::database::map_db::BlockList;
    if !bl_ptr.is_null() && amount > 0 {
        crate::game::scripting::doscript_blargs(
            c"player_combat".as_ptr(), c"on_healed".as_ptr(),
            &[&mut sd.bl as *mut _ as *mut _, bl_ptr as *mut _],
        );
    } else if amount > 0 {
        crate::game::scripting::doscript_blargs(
            c"player_combat".as_ptr(), c"on_healed".as_ptr(),
            &[&mut sd.bl as *mut _ as *mut _],
        );
    }
    clif_send_pc_healthscript(&mut *sd, -amount, 0);
    clif_sendstatus(sd, SFLAG_HPMP);
}
