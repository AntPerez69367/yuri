//! extern "C" stubs for C functions called by scripting method bodies.
//! Replace each group as the corresponding Rust module is ported.

use std::ffi::{c_char, c_int, c_uint, c_ulong, c_uchar};
use std::os::raw::c_void;
use crate::database::mob_db::MobDbData;

pub const BL_PC:  c_int = 0x01;
pub const BL_MOB: c_int = 0x02;
pub const BL_NPC: c_int = 0x04;
pub const BL_ALL: c_int = 0x0F;

extern "C" {
    // --- Map id/name lookups used by constructors ---
    pub fn map_id2sd(id: c_uint) -> *mut c_void;
    pub fn map_name2sd(name: *const c_char) -> *mut c_void;
    pub fn map_id2mob(id: c_uint) -> *mut c_void;

    // --- Phase 2: registry types ---

    // Player (USER*) integer registries
    pub fn pc_readglobalreg(sd: *mut c_void, attrname: *const c_char) -> c_int;
    pub fn pc_setglobalreg(sd: *mut c_void, attrname: *const c_char, val: c_ulong) -> c_int;
    pub fn pc_readnpcintreg(sd: *mut c_void, attrname: *const c_char) -> c_int;
    pub fn pc_setnpcintreg(sd: *mut c_void, attrname: *const c_char, val: c_int) -> c_int;
    pub fn pc_readquestreg(sd: *mut c_void, attrname: *const c_char) -> c_int;
    pub fn pc_setquestreg(sd: *mut c_void, attrname: *const c_char, val: c_int) -> c_int;

    // Player string registry
    pub fn pc_readglobalregstring(sd: *mut c_void, attrname: *const c_char) -> *const c_char;
    pub fn pc_setglobalregstring(sd: *mut c_void, attrname: *const c_char, val: *const c_char) -> c_int;

    // NPC integer registry (via static-inline wrapper in npc.h → npc_*_ffi symbols)
    pub fn npc_readglobalreg_ffi(nd: *mut c_void, attrname: *const c_char) -> c_int;
    pub fn npc_setglobalreg_ffi(nd: *mut c_void, attrname: *const c_char, val: c_int) -> c_int;

    // Mob registries — already #[no_mangle] Rust functions in ffi/mob.rs
    pub fn rust_mob_readglobalreg(mob: *mut c_void, attrname: *const c_char) -> c_int;
    pub fn rust_mob_setglobalreg(mob: *mut c_void, attrname: *const c_char, val: c_int) -> c_int;

    // Map registries — helpers in sl_compat.c extract bl.m from USER*
    pub fn map_readglobalreg_sd(sd: *mut c_void, attrname: *const c_char) -> c_int;
    pub fn map_setglobalreg_sd(sd: *mut c_void, attrname: *const c_char, val: c_int) -> c_int;

    // Map-indexed registries (direct map slot, not from USER*)
    pub fn map_readglobalreg(m: c_int, attrname: *const c_char) -> c_int;
    pub fn map_setglobalreg(m: c_int, attrname: *const c_char, val: c_int);

    // Game-global registries (no self pointer)
    pub fn map_readglobalgamereg(attrname: *const c_char) -> c_int;
    pub fn map_setglobalgamereg(attrname: *const c_char, val: c_int) -> c_int;

    // --- Phase 3: globals ---

    // C game globals (extern int in map_server.h)
    pub static serverid:   c_int;
    pub static cur_year:   c_int;
    pub static cur_season: c_int;
    pub static cur_day:    c_int;
    pub static cur_time:   c_int;

    // Broadcast
    pub fn clif_broadcast(msg: *const c_char, m: c_int) -> c_int;
    pub fn clif_gmbroadcast(msg: *const c_char, m: c_int) -> c_int;

    // Map helpers
    pub fn map_changepostcolor(board: c_int, post: c_int, color: c_int);
    /// Returns a pointer into the C map[] id-database for floor items.
    pub fn map_id2fl(id: c_uint) -> *mut c_void;

    // Magic/mob DB (Rust #[no_mangle] symbols)
    pub fn rust_magicdb_level(s: *const c_char) -> c_int;
    pub fn rust_mobdb_search(id: c_uint) -> *mut MobDbData;
    pub fn rust_mobdb_id(s: *const c_char) -> c_int;
    pub fn rust_mobspawn_onetime(
        id: c_uint, m: c_int, x: c_int, y: c_int,
        times: c_int, start: c_int, end: c_int,
        replace: c_uint, owner: c_uint,
    ) -> *mut c_uint;
    pub fn map_id2bl(id: c_uint) -> *mut c_void;

    // sl_globals — typed wrappers in sl_compat.c
    pub fn sl_g_realtime(day: *mut c_int, hour: *mut c_int, minute: *mut c_int, second: *mut c_int);
    pub fn sl_g_getwarp(m: c_int, x: c_int, y: c_int) -> c_int;
    pub fn sl_g_setwarps(mm: c_int, mx: c_int, my: c_int, tm: c_int, tx: c_int, ty: c_int) -> c_int;
    pub fn sl_g_getweather(region: c_uchar, indoor: c_uchar) -> c_int;
    pub fn sl_g_setweather(region: c_uchar, indoor: c_uchar, weather: c_uchar);
    pub fn sl_g_setweatherm(m: c_int, weather: c_uchar);
    pub fn sl_g_setlight(region: c_uchar, indoor: c_uchar, light: c_uchar);
    pub fn sl_g_savemap(m: c_int, path: *const c_char) -> c_int;
    pub fn sl_g_setmap(
        m: c_int, mapfile: *const c_char, title: *const c_char,
        bgm: c_int, bgmtype: c_int, pvp: c_int, spell: c_int,
        light: c_uchar, weather: c_int,
        sweeptime: c_int, cantalk: c_int, show_ghosts: c_int,
        region: c_int, indoor: c_int, warpout: c_int,
        bind: c_int, reqlvl: c_int, reqvita: c_int, reqmana: c_int,
    ) -> c_int;
    pub fn sl_g_throw(
        id: c_int, m: c_int, x: c_int, y: c_int, x2: c_int, y2: c_int,
        icon: c_int, color: c_int, action: c_int,
    );
    pub fn sl_g_sendmeta();
    pub fn sl_g_addmob(m: c_int, x: c_int, y: c_int, mobid: c_int) -> c_int;
    pub fn sl_g_checkonline_id(id: c_int) -> c_int;
    pub fn sl_g_checkonline_name(name: *const c_char) -> c_int;
    pub fn sl_g_getofflineid(name: *const c_char) -> c_int;
    pub fn sl_g_addmapmodifier(mapid: c_uint, modifier: *const c_char, value: c_int) -> c_int;
    pub fn sl_g_removemapmodifier(mapid: c_uint, modifier: *const c_char) -> c_int;
    pub fn sl_g_removemapmodifierid(mapid: c_uint) -> c_int;
    pub fn sl_g_getfreemapmodifierid() -> c_int;
    pub fn sl_g_getwisdomstarmultiplier() -> f32;
    pub fn sl_g_setwisdomstarmultiplier(mult: f32, value: c_int);
    pub fn sl_g_getkandonationpoints() -> c_int;
    pub fn sl_g_setkandonationpoints(val: c_int);
    pub fn sl_g_addkandonationpoints(val: c_int);
    pub fn sl_g_getclantribute(clan: c_int) -> c_uint;
    pub fn sl_g_setclantribute(clan: c_int, val: c_uint);
    pub fn sl_g_addclantribute(clan: c_int, val: c_uint);
    pub fn sl_g_getclanname(clan: c_int, buf: *mut i8, buflen: c_int) -> c_int;
    pub fn sl_g_setclanname(clan: c_int, name: *const c_char);
    pub fn sl_g_getclanbankslots(clan: c_int) -> c_int;
    pub fn sl_g_setclanbankslots(clan: c_int, val: c_int);
    pub fn sl_g_removeclanmember(id: c_int) -> c_int;
    pub fn sl_g_addclanmember(id: c_int, clan: c_int) -> c_int;
    pub fn sl_g_updateclanmemberrank(id: c_int, rank: c_int) -> c_int;
    pub fn sl_g_updateclanmembertitle(id: c_int, title: *const c_char) -> c_int;
    pub fn sl_g_removepathember(id: c_int) -> c_int;
    pub fn sl_g_addpathember(id: c_int, cls: c_int) -> c_int;
    pub fn sl_g_getxpforlevel(path: c_int, level: c_int) -> c_uint;

    // pc_* stubs added in Phase 6 as method bodies are written.
    // clif_* stubs added as method bodies are written.
    // mob_* stubs added in Phase 5 as method bodies are written.

    // NPC scripting helpers — sl_compat.c
    pub fn sl_g_getobjectscell(
        m: c_int, x: c_int, y: c_int, bl_type: c_int,
        out_ptrs: *mut *mut c_void, max_count: c_int,
    ) -> c_int;
    pub fn sl_g_getobjectscellwithtraps(
        m: c_int, x: c_int, y: c_int, bl_type: c_int,
        out_ptrs: *mut *mut c_void, max_count: c_int,
    ) -> c_int;
    pub fn sl_g_getaliveobjectscell(
        m: c_int, x: c_int, y: c_int, bl_type: c_int,
        out_ptrs: *mut *mut c_void, max_count: c_int,
    ) -> c_int;
    pub fn sl_g_getobjectsarea(
        bl: *mut c_void, bl_type: c_int,
        out_ptrs: *mut *mut c_void, max_count: c_int,
    ) -> c_int;
    pub fn sl_g_getaliveobjectsarea(
        bl: *mut c_void, bl_type: c_int,
        out_ptrs: *mut *mut c_void, max_count: c_int,
    ) -> c_int;
    pub fn sl_g_getobjectssamemap(
        bl: *mut c_void, bl_type: c_int,
        out_ptrs: *mut *mut c_void, max_count: c_int,
    ) -> c_int;
    pub fn sl_g_getaliveobjectssamemap(
        bl: *mut c_void, bl_type: c_int,
        out_ptrs: *mut *mut c_void, max_count: c_int,
    ) -> c_int;
    pub fn sl_g_getmappvp(m: c_int) -> c_int;
    pub fn sl_g_getmaptitle(m: c_int, buf: *mut c_char, buflen: c_int) -> c_int;
    pub fn sl_pc_getpk(sd: *mut c_void, id: c_int) -> c_int;
    pub fn sl_g_getobjectsinmap(
        m: c_int, bl_type: c_int,
        out_ptrs: *mut *mut c_void, max_count: c_int,
    ) -> c_int;
    pub fn sl_g_sendside(bl: *mut c_void);
    pub fn sl_g_sendanimxy(bl: *mut c_void, anim: c_int, x: c_int, y: c_int, times: c_int);
    pub fn sl_g_delete_bl(bl: *mut c_void);
    pub fn sl_g_talk(bl: *mut c_void, talk_type: c_int, msg: *const c_char);
    pub fn sl_g_getusers(out_ptrs: *mut *mut c_void, max_count: c_int) -> c_int;
    pub fn sl_g_addnpc(
        name: *const c_char, m: c_int, x: c_int, y: c_int, subtype: c_int,
        timer: c_int, duration: c_int, owner: c_int, movetime: c_int,
        npc_yname: *const c_char,
    );

    // PC attribute getters/setters — Task 4
    pub fn sl_pc_vregenoverflow(sd: *mut c_void) -> c_int;
    pub fn sl_pc_set_vregenoverflow(sd: *mut c_void, v: c_int);
    pub fn sl_pc_mregenoverflow(sd: *mut c_void) -> c_int;
    pub fn sl_pc_set_mregenoverflow(sd: *mut c_void, v: c_int);
    pub fn sl_pc_group_count(sd: *mut c_void) -> c_int;
    pub fn sl_pc_set_group_count(sd: *mut c_void, v: c_int);
    pub fn sl_pc_group_on(sd: *mut c_void) -> c_int;
    pub fn sl_pc_set_group_on(sd: *mut c_void, v: c_int);
    pub fn sl_pc_group_leader(sd: *mut c_void) -> c_int;
    pub fn sl_pc_set_group_leader(sd: *mut c_void, v: c_int);
    pub fn sl_pc_getgroup(sd: *mut c_void, out: *mut c_uint, max: c_int) -> c_int;

    // Shared block-object helpers — Task 6
    pub fn sl_g_sendanimation(bl: *mut c_void, anim: c_int, times: c_int);
    pub fn sl_g_playsound(bl: *mut c_void, sound: c_int);
    pub fn sl_g_sendaction(bl: *mut c_void, action: c_int, speed: c_int);
    pub fn sl_g_msg(bl: *mut c_void, color: c_int, msg: *const c_char, target: c_int);
    pub fn sl_g_dropitem(bl: *mut c_void, item_id: c_int, amount: c_int, owner: c_int);
    pub fn sl_g_dropitemxy(bl: *mut c_void, item_id: c_int, amount: c_int,
                           m: c_int, x: c_int, y: c_int, owner: c_int);
    pub fn sl_g_objectcanmove(bl: *mut c_void, x: c_int, y: c_int, side: c_int) -> c_int;
    pub fn sl_g_objectcanmovefrom(bl: *mut c_void, x: c_int, y: c_int, side: c_int) -> c_int;
    pub fn sl_g_repeatanimation(bl: *mut c_void, anim: c_int, duration: c_int);
    pub fn sl_g_selfanimation(bl: *mut c_void, target_id: c_int, anim: c_int, times: c_int);
    pub fn sl_g_selfanimationxy(bl: *mut c_void, target_id: c_int,
                                anim: c_int, x: c_int, y: c_int, times: c_int);
    pub fn sl_g_sendparcel(bl: *mut c_void, receiver: c_int, sender: c_int,
                           item: c_int, amount: c_int, owner: c_int,
                           engrave: *const c_char, npcflag: c_int);
    pub fn sl_g_throwblock(bl: *mut c_void, x: c_int, y: c_int,
                           icon: c_int, color: c_int, action: c_int);
    pub fn sl_g_deliddb(bl: *mut c_void);
    pub fn sl_g_addpermanentspawn(bl: *mut c_void);
    pub fn sl_fl_delete(bl: *mut c_void);

    // Async dialog send helpers — Task 10
    pub fn sl_pc_input_send(sd: *mut c_void, msg: *const c_char);
    pub fn sl_pc_dialog_send(sd: *mut c_void, msg: *const c_char, graphics: *const c_int, ngraphics: c_int);
    pub fn sl_pc_dialogseq_send(sd: *mut c_void, entries: *const *const c_char, n: c_int, can_continue: c_int);
    pub fn sl_pc_menu_send(sd: *mut c_void, msg: *const c_char, options: *const *const c_char, n: c_int);
    pub fn sl_pc_menuseq_send(sd: *mut c_void, msg: *const c_char, options: *const *const c_char, n: c_int);
    pub fn sl_pc_menustring_send(sd: *mut c_void, msg: *const c_char, options: *const *const c_char, n: c_int);
    pub fn sl_pc_menustring2_send(sd: *mut c_void, msg: *const c_char, options: *const *const c_char, n: c_int);
    pub fn sl_pc_buy_send(sd: *mut c_void, msg: *const c_char, items: *const c_int, values: *const c_int,
                          displaynames: *const *const c_char, buytext: *const *const c_char, n: c_int);
    pub fn sl_pc_buydialog_send(sd: *mut c_void, msg: *const c_char, items: *const c_int, n: c_int);
    pub fn sl_pc_buyextend_send(sd: *mut c_void, msg: *const c_char, items: *const c_int,
                                prices: *const c_int, maxamounts: *const c_int, n: c_int);
    pub fn sl_pc_sell_send(sd: *mut c_void, msg: *const c_char, items: *const c_int, n: c_int);
    pub fn sl_pc_sell2_send(sd: *mut c_void, msg: *const c_char, items: *const c_int, n: c_int);
    pub fn sl_pc_sellextend_send(sd: *mut c_void, msg: *const c_char, items: *const c_int, n: c_int);
    pub fn sl_pc_showbank_send(sd: *mut c_void, msg: *const c_char);
    pub fn sl_pc_showbankadd_send(sd: *mut c_void);
    pub fn sl_pc_bankaddmoney_send(sd: *mut c_void);
    pub fn sl_pc_bankwithdrawmoney_send(sd: *mut c_void);
    pub fn sl_pc_clanshowbank_send(sd: *mut c_void, msg: *const c_char);
    pub fn sl_pc_clanshowbankadd_send(sd: *mut c_void);
    pub fn sl_pc_clanbankaddmoney_send(sd: *mut c_void);
    pub fn sl_pc_clanbankwithdrawmoney_send(sd: *mut c_void);
    pub fn sl_pc_clanviewbank_send(sd: *mut c_void);
    pub fn sl_pc_repairextend_send(sd: *mut c_void);
    pub fn sl_pc_repairall_send(sd: *mut c_void, npc_bl: *mut c_void);
}
