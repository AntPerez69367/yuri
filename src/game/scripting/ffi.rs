//! Scripting FFI compatibility shim — all C is gone, these are now Rust-to-Rust wrappers.
//! The sffi:: namespace used throughout the scripting module is preserved so callers
//! do not need to be changed.


pub const BL_PC:   i32 = 0x01;
pub const BL_MOB:  i32 = 0x02;
pub const BL_NPC:  i32 = 0x04;
pub const BL_ITEM: i32 = 0x08;
pub const BL_ALL:  i32 = 0x0F;

// ─── Globals ──────────────────────────────────────────────────────────────────
pub use crate::config_globals::serverid;
// Time globals are now AtomicI32; expose them directly (callers use .load()).
pub use crate::game::map_server::{cur_year, cur_season, cur_day, cur_time};

// ─── Map id/name lookups ──────────────────────────────────────────────────────
pub use crate::game::map_server::{
    map_id2bl, map_id2fl, map_changepostcolor,
    map_setglobalreg, map_readglobalgamereg, map_setglobalgamereg,
};

pub unsafe fn map_id2sd(id: u32) -> *mut std::ffi::c_void {
    crate::game::map_server::map_id2sd(id)
}
pub unsafe fn map_name2sd(name: *const i8) -> *mut std::ffi::c_void {
    crate::game::map_server::map_name2sd(name) as *mut std::ffi::c_void
}
pub unsafe fn map_name2npc(name: *const i8) -> *mut std::ffi::c_void {
    crate::game::map_server::map_name2npc(name)
}
pub unsafe fn map_id2mob(id: u32) -> *mut std::ffi::c_void {
    crate::game::map_server::map_id2mob(id) as *mut std::ffi::c_void
}

// ─── PC registries ────────────────────────────────────────────────────────────
pub unsafe fn rust_pc_readglobalreg(sd: *mut std::ffi::c_void, attrname: *const i8) -> i32 {
    crate::game::pc::rust_pc_readglobalreg(sd as *mut _, attrname as *const i8)
}
pub unsafe fn rust_pc_setglobalreg(sd: *mut std::ffi::c_void, attrname: *const i8, val: u64) -> i32 {
    crate::game::pc::rust_pc_setglobalreg(sd as *mut _, attrname as *const i8, val)
}
pub unsafe fn rust_pc_readglobalregstring(sd: *mut std::ffi::c_void, attrname: *const i8) -> *const i8 {
    crate::game::pc::rust_pc_readglobalregstring(sd as *mut _, attrname as *const i8) as *const i8
}
pub unsafe fn rust_pc_setglobalregstring(sd: *mut std::ffi::c_void, attrname: *const i8, val: *const i8) -> i32 {
    crate::game::pc::rust_pc_setglobalregstring(sd as *mut _, attrname as *const i8, val as *const i8)
}
pub unsafe fn rust_pc_readquestreg(sd: *mut std::ffi::c_void, attrname: *const i8) -> i32 {
    crate::game::pc::rust_pc_readquestreg(sd as *mut _, attrname)
}
pub unsafe fn rust_pc_setquestreg(sd: *mut std::ffi::c_void, attrname: *const i8, val: i32) -> i32 {
    crate::game::pc::rust_pc_setquestreg(sd as *mut _, attrname, val)
}

// ─── NPC registries ───────────────────────────────────────────────────────────
pub unsafe fn npc_readglobalreg_ffi(nd: *mut std::ffi::c_void, attrname: *const i8) -> i32 {
    crate::game::npc::npc_readglobalreg_ffi(nd as *mut _, attrname)
}
pub unsafe fn npc_setglobalreg_ffi(nd: *mut std::ffi::c_void, attrname: *const i8, val: i32) -> i32 {
    crate::game::npc::npc_setglobalreg_ffi(nd as *mut _, attrname, val)
}

// ─── Mob registries ───────────────────────────────────────────────────────────
pub unsafe fn rust_mob_readglobalreg(mob: *mut std::ffi::c_void, attrname: *const i8) -> i32 {
    crate::game::mob::rust_mob_readglobalreg(mob as *mut _, attrname)
}
pub unsafe fn rust_mob_setglobalreg(mob: *mut std::ffi::c_void, attrname: *const i8, val: i32) -> i32 {
    crate::game::mob::rust_mob_setglobalreg(mob as *mut _, attrname, val)
}

// ─── Broadcast / player state ─────────────────────────────────────────────────
pub use crate::game::map_parse::chat::{clif_broadcast, clif_gmbroadcast};

pub unsafe fn clif_mystaytus(sd: *mut std::ffi::c_void) {
    crate::game::map_parse::player_state::clif_mystaytus(sd as *mut _);
}

// ─── Magic / mob DB ───────────────────────────────────────────────────────────
pub use crate::database::magic_db::rust_magicdb_level;
pub use crate::database::mob_db::{rust_mobdb_search, rust_mobdb_id};
pub use crate::game::mob::rust_mobspawn_onetime;

// ─── sl_g_* map globals ───────────────────────────────────────────────────────
pub use crate::game::scripting::map_globals::{
    sl_g_setmap, sl_g_throw, sl_g_sendmeta,
    sl_g_sendside, sl_g_sendanimxy, sl_g_delete_bl, sl_g_talk,
    sl_g_getusers, sl_g_addnpc,
    sl_g_sendanimation, sl_g_playsound, sl_g_sendaction, sl_g_msg,
    sl_g_dropitem, sl_g_dropitemxy,
    sl_g_objectcanmove, sl_g_objectcanmovefrom,
    sl_g_repeatanimation, sl_g_selfanimation, sl_g_selfanimationxy,
    sl_g_sendparcel, sl_g_throwblock,
    sl_g_deliddb, sl_g_addpermanentspawn, sl_fl_delete,
};

// ─── sl_g_get* object collect ────────────────────────────────────────────────
pub use crate::game::scripting::object_collect::{
    sl_g_getobjectscell, sl_g_getobjectscellwithtraps, sl_g_getaliveobjectscell,
    sl_g_getobjectsarea, sl_g_getaliveobjectsarea,
    sl_g_getobjectssamemap, sl_g_getaliveobjectssamemap,
    sl_g_getobjectsinmap,
};

// These are accessed via sffi:: namespace in types/pc.rs.
// They are defined in pc_accessors.rs with , so we can pub use them directly.
pub use crate::game::scripting::pc_accessors::{
    sl_pc_getpk,
    sl_pc_vregenoverflow, sl_pc_set_vregenoverflow,
    sl_pc_mregenoverflow, sl_pc_set_mregenoverflow,
    sl_pc_group_count, sl_pc_set_group_count,
    sl_pc_group_on, sl_pc_set_group_on,
    sl_pc_group_leader, sl_pc_set_group_leader,
    sl_pc_getgroup,
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
