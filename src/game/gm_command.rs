//! GM command dispatch -- replaces `c_src/gm_command.c`.

#![allow(non_snake_case, dead_code, unused_variables, unused_mut)]

use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong};
use std::os::raw::c_void;

use crate::database::map_db::BlockList;
use crate::game::mob::{MobSpawnData, BL_MOB, BL_PC, MOB_DEAD};
use crate::game::pc::{MapSessionData, PC_DIE, SFLAG_FULLSTATS, SFLAG_HPMP};

// Module globals (mirrors C file-scope vars)
static mut SPELLGFX:     c_int = 0;
static mut MUSICFX:      c_int = 0;
static mut SOUNDFX:      c_int = 0;
static mut DOWNTIMER:    c_int = 0;
static mut COMMAND_CODE: c_char = b'/' as c_char;

// Flag constants (from map_server.h enums) — imported from pc.rs where possible,
// redefined here for local clarity.
const OPT_STEALTH:    c_ulong = 32;
const OPT_GHOSTS:     c_ulong = 256;
const UFLAG_SILENCED: c_ulong = 1;
const UFLAG_IMMORTAL: c_ulong = 8;
const UFLAG_UNPHYS:   c_ulong = 16;

// AREA constant (from map_parse.h enum)
const AREA: c_int = 4;

// MAX_MAP_PER_SERVER (from mmo.h)
const MAX_MAP_PER_SERVER: c_int = 65535;

// MAX_KILLREG (from mmo.h)
const MAX_KILLREG: usize = 5000;

// External globals (from map_parse.c / core.c)
extern "C" {
    static fd_max:      c_int;
    static mut xp_rate: c_int;
    static mut d_rate:  c_int;
    static map_n:           c_int;
    static MOB_SPAWN_START:   c_uint;
    static MOB_SPAWN_MAX:     c_uint;
    static MOB_ONETIME_START: c_uint;
    static MOB_ONETIME_MAX:   c_uint;
}

// char_fd, sql_handle, and userlist are now Rust #[no_mangle] statics in src/game/map_server.rs.
use crate::game::map_server::{char_fd, sql_handle as SQL_HANDLE, userlist};

/// Helper: cast the Rust sql_handle (*mut Sql) to *mut c_void for C FFI calls in this file.
#[inline(always)]
unsafe fn sql_handle_void() -> *mut c_void { SQL_HANDLE as *mut c_void }

type LuaState = c_void; // opaque

extern "C" {
    fn printf(fmt: *const c_char, ...) -> c_int;

    // map
    fn map_name2sd(name: *const c_char) -> *mut MapSessionData;
    fn map_respawn(f: unsafe extern "C" fn(*mut BlockList, ...) -> c_int, m: c_int, bl_type: c_int);
    fn map_reload();
    fn map_foreachinarea(
        f: unsafe extern "C" fn(*mut BlockList, ...) -> c_int,
        m: c_int, x: c_int, y: c_int, range: c_int, bl_type: c_int, ...
    ) -> c_int;

    // clif
    fn clif_sendminitext(sd: *mut MapSessionData, msg: *const c_char);
    fn clif_sendmsg(sd: *mut MapSessionData, msg_type: c_int, msg: *const c_char);
    fn clif_broadcast(msg: *const c_char, color: c_int);
    fn clif_sendchararea(sd: *mut MapSessionData);
    fn clif_getchararea(sd: *mut MapSessionData);
    fn clif_sendstatus(sd: *mut MapSessionData, flags: c_int);
    fn clif_mystaytus(sd: *mut MapSessionData);
    fn broadcast_update_state(sd: *mut MapSessionData);
    fn clif_sendanimation(bl: *mut BlockList, ...) -> c_int;
    fn clif_lookgone(bl: *mut BlockList);
    fn clif_playsound(bl: *mut BlockList, sound: c_int);
    fn clif_refresh(sd: *mut MapSessionData);
    fn clif_sendweather(sd: *mut MapSessionData);
    fn clif_sendurl(sd: *mut MapSessionData, url_type: c_int, url: *const c_char);
    fn clif_transfer_test(sd: *mut MapSessionData, a: c_int, b: c_int, c: c_int);

    // pc — all ported to Rust (src/game/pc.rs), real symbols are rust_pc_*
    #[link_name = "rust_pc_warp"]
    fn pc_warp(sd: *mut MapSessionData, m: c_int, x: c_int, y: c_int) -> c_int;
    #[link_name = "rust_pc_additem"]
    fn pc_additem(sd: *mut MapSessionData, it: *mut c_void) -> c_int;
    #[link_name = "rust_pc_delitem"]
    fn pc_delitem(sd: *mut MapSessionData, idx: c_int, amount: c_int, flag: c_int) -> c_int;
    #[link_name = "rust_pc_res"]
    fn pc_res(sd: *mut MapSessionData) -> c_int;
    #[link_name = "rust_pc_loadmagic"]
    fn pc_loadmagic(sd: *mut MapSessionData) -> c_int;
    #[link_name = "rust_pc_readglobalreg"]
    fn pc_readglobalreg(sd: *mut MapSessionData, reg: *const c_char) -> c_int;

    // mob — now Rust-implemented (rust_mob_respawn in libyuri.a)
    #[link_name = "rust_mob_respawn"]
    fn mob_respawn(mob: *mut c_void) -> c_int;

    // scripting — all ported to Rust (ffi/scripting.rs), real symbols are rust_sl_*
    #[link_name = "rust_sl_reload"]
    fn sl_reload() -> c_int;
    #[link_name = "rust_sl_exec"]
    fn sl_exec(sd: *mut c_void, line: *mut c_char);
    #[link_name = "rust_sl_fixmem"]
    fn sl_fixmem();
    #[link_name = "rust_sl_luasize"]
    fn sl_luasize(sd: *mut c_void) -> c_int;

    // db reloads — Rust implementations under rust_* names
    #[link_name = "rust_itemdb_init"]
    fn itemdb_read() -> c_int;
    #[link_name = "rust_itemdb_id"]
    fn itemdb_id(name: *const c_char) -> c_uint;
    #[link_name = "rust_itemdb_dura"]
    fn itemdb_dura(id: c_uint) -> c_int;
    #[link_name = "rust_magicdb_id"]
    fn magicdb_id(name: *const c_char) -> c_int;
    #[link_name = "rust_boarddb_term"]
    fn boarddb_term();
    #[link_name = "rust_boarddb_init"]
    fn boarddb_init() -> c_int;
    #[link_name = "rust_clandb_init"]
    fn clandb_init() -> c_int;
    fn npc_init();
    fn warp_init();
    fn rust_mobdb_term();
    fn rust_mobdb_init();
    // mobspawn_read is an inline in mob.h wrapping rust_mobspawn_read
    #[link_name = "rust_mobspawn_read"]
    fn mobspawn_read() -> c_int;

    // SQL (sql_handle is now a Rust static; access via crate::game::map_server::sql_handle cast to *mut c_void)
    fn Sql_Query(handle: *mut c_void, fmt: *const c_char, ...) -> c_int;
    fn Sql_EscapeString(handle: *mut c_void, out: *mut c_char, src: *const c_char);
    fn Sql_FreeResult(handle: *mut c_void);
    // Sql_ShowDebug has a trailing underscore in libdeps.a
    #[link_name = "Sql_ShowDebug_"]
    fn Sql_ShowDebug(handle: *mut c_void);
    fn SqlStmt_Malloc(handle: *mut c_void) -> *mut c_void;
    #[link_name = "SqlStmt_ShowDebug_"]
    fn SqlStmt_ShowDebug(stmt: *mut c_void, file: *const c_char, line: c_ulong);

    // session helpers
    fn rust_session_exists(fd: c_int) -> c_int;
    fn rust_session_get_data(fd: c_int) -> *mut MapSessionData;
    fn rust_session_get_eof(fd: c_int) -> c_int;
    fn rust_session_set_eof(fd: c_int, val: c_int);

    // encrypt (for command_debug raw packet) — C symbol is `encrypt` in net_crypt.c
    #[link_name = "encrypt"]
    fn encrypt_fd(fd: c_int) -> c_int;

    // timer (for command_shutdown)
    fn timer_insert(
        delay: c_int, interval: c_int,
        f: unsafe extern "C" fn(c_int, c_int) -> c_int,
        id: c_uint, extra: c_int,
    ) -> c_int;
    fn timer_remove(tid: c_int);

    // shutdown timer callback (from map_server.c)
    fn map_reset_timer(v1: c_int, v2: c_int) -> c_int;
}

/// Dispatch a Lua event with a single block_list argument.
#[cfg(not(test))]
#[allow(dead_code)]
unsafe fn sl_doscript_simple(root: *const std::ffi::c_char, method: *const std::ffi::c_char, bl: *mut crate::database::map_db::BlockList) -> std::ffi::c_int {
    crate::game::scripting::doscript_blargs(root, method, &[bl as *mut _])
}


// No-ops: these were inlines returning 0 in C headers (class_db.h, magic_db.h).
// magicdb_read: magic_db.h says callers must use magicdb_init() instead.
// classdb_read / leveldb_read: class_db.h stubs both as returning 0.
#[allow(clippy::inline_always)]
#[inline(always)]
unsafe fn magicdb_read() {}
#[allow(clippy::inline_always)]
#[inline(always)]
unsafe fn classdb_read() {}
#[allow(clippy::inline_always)]
#[inline(always)]
unsafe fn leveldb_read() {}

const SQL_ERROR: c_int = -1;

type CmdFn = unsafe fn(*mut MapSessionData, *mut c_char, *mut LuaState) -> c_int;

struct CommandEntry {
    func:  CmdFn,
    name:  &'static str,
    level: c_int,
}

static COMMANDS: &[CommandEntry] = &[
    CommandEntry { func: command_debug,           name: "debug",           level: 99 },
    CommandEntry { func: command_item,            name: "item",            level: 50 },
    CommandEntry { func: command_res,             name: "res",             level: 99 },
    CommandEntry { func: command_hair,            name: "hair",            level: 99 },
    CommandEntry { func: command_checkdupes,      name: "checkdupes",      level: 99 },
    CommandEntry { func: command_checkwpe,        name: "checkwpe",        level: 99 },
    CommandEntry { func: command_kill,            name: "kill",            level: 99 },
    CommandEntry { func: command_killall,         name: "killall",         level: 99 },
    CommandEntry { func: command_deletespell,     name: "deletespell",     level: 99 },
    CommandEntry { func: command_xprate,          name: "xprate",          level: 99 },
    CommandEntry { func: command_heal,            name: "heal",            level: 99 },
    CommandEntry { func: command_level,           name: "level",           level: 99 },
    CommandEntry { func: command_randomspawn,     name: "randomspawn17",   level: 99 },
    CommandEntry { func: command_drate,           name: "droprate",        level: 99 },
    CommandEntry { func: command_spell,           name: "spell",           level: 99 },
    CommandEntry { func: command_val,             name: "val",             level: 99 },
    CommandEntry { func: command_disguise,        name: "disguise",        level: 99 },
    CommandEntry { func: command_warp,            name: "warp",            level: 10 },
    CommandEntry { func: command_givespell,       name: "givespell",       level: 50 },
    CommandEntry { func: command_side,            name: "side",            level: 99 },
    CommandEntry { func: command_state,           name: "state",           level: 20 },
    CommandEntry { func: command_armorcolor,      name: "armorc",          level: 99 },
    CommandEntry { func: command_makegm,          name: "makegm",          level: 99 },
    CommandEntry { func: command_who,             name: "who",             level: 99 },
    CommandEntry { func: command_legend,          name: "legend",          level: 99 },
    CommandEntry { func: command_luareload,       name: "reloadlua",       level: 99 },
    CommandEntry { func: command_luareload,       name: "rl",              level: 99 },
    CommandEntry { func: command_magicreload,     name: "reloadmagic",     level: 99 },
    CommandEntry { func: command_lua,             name: "lua",             level: 0  },
    CommandEntry { func: command_speed,           name: "speed",           level: 10 },
    CommandEntry { func: command_reloaditem,      name: "reloaditem",      level: 99 },
    CommandEntry { func: command_reloadcreations, name: "reloadcreations", level: 99 },
    CommandEntry { func: command_reloadmob,       name: "reloadmob",       level: 99 },
    CommandEntry { func: command_reloadspawn,     name: "reloadspawn",     level: 99 },
    CommandEntry { func: command_pvp,             name: "pvp",             level: 20 },
    CommandEntry { func: command_spellwork,       name: "spellwork",       level: 99 },
    CommandEntry { func: command_broadcast,       name: "bc",              level: 50 },
    CommandEntry { func: command_luasize,         name: "luasize",         level: 99 },
    CommandEntry { func: command_luafix,          name: "luafix",          level: 99 },
    CommandEntry { func: command_respawn,         name: "respawn",         level: 99 },
    CommandEntry { func: command_ban,             name: "ban",             level: 99 },
    CommandEntry { func: command_unban,           name: "unban",           level: 99 },
    CommandEntry { func: command_kc,              name: "kc",              level: 99 },
    CommandEntry { func: command_blockcount,      name: "blockc",          level: 99 },
    CommandEntry { func: command_stealth,         name: "stealth",         level: 1  },
    CommandEntry { func: command_ghosts,          name: "ghosts",          level: 1  },
    CommandEntry { func: command_unphysical,      name: "unphysical",      level: 99 },
    CommandEntry { func: command_immortality,     name: "immortality",     level: 99 },
    CommandEntry { func: command_silence,         name: "silence",         level: 99 },
    CommandEntry { func: command_shutdowncancel,  name: "shutdown_cancel", level: 99 },
    CommandEntry { func: command_shutdown,        name: "shutdown",        level: 99 },
    CommandEntry { func: command_weap,            name: "weap",            level: 99 },
    CommandEntry { func: command_shield,          name: "shield",          level: 99 },
    CommandEntry { func: command_armor,           name: "armor",           level: 99 },
    CommandEntry { func: command_boots,           name: "boots",           level: 99 },
    CommandEntry { func: command_mantle,          name: "mantle",          level: 99 },
    CommandEntry { func: command_necklace,        name: "necklace",        level: 99 },
    CommandEntry { func: command_faceacc,         name: "faceacc",         level: 99 },
    CommandEntry { func: command_crown,           name: "crown",           level: 99 },
    CommandEntry { func: command_helm,            name: "helm",            level: 99 },
    CommandEntry { func: command_gfxtoggle,       name: "gfxtoggle",       level: 99 },
    CommandEntry { func: command_weather,         name: "weather",         level: 50 },
    CommandEntry { func: command_light,           name: "light",           level: 50 },
    CommandEntry { func: command_gm,              name: "gm",              level: 20 },
    CommandEntry { func: command_report,          name: "report",          level: 0  },
    CommandEntry { func: command_url,             name: "url",             level: 99 },
    CommandEntry { func: command_cinv,            name: "cinv",            level: 50 },
    CommandEntry { func: command_cfloor,          name: "cfloor",          level: 50 },
    CommandEntry { func: command_cspells,         name: "cspells",         level: 50 },
    CommandEntry { func: command_job,             name: "job",             level: 20 },
    CommandEntry { func: command_music,           name: "music",           level: 50 },
    CommandEntry { func: command_musicn,          name: "musicn",          level: 99 },
    CommandEntry { func: command_musicp,          name: "musicp",          level: 99 },
    CommandEntry { func: command_musicq,          name: "musicq",          level: 99 },
    CommandEntry { func: command_sound,           name: "sound",           level: 50 },
    CommandEntry { func: command_nsound,          name: "nsound",          level: 99 },
    CommandEntry { func: command_psound,          name: "psound",          level: 99 },
    CommandEntry { func: command_soundq,          name: "soundq",          level: 99 },
    CommandEntry { func: command_nspell,          name: "nspell",          level: 99 },
    CommandEntry { func: command_pspell,          name: "pspell",          level: 99 },
    CommandEntry { func: command_spellq,          name: "spellq",          level: 99 },
    CommandEntry { func: command_reloadboard,     name: "reloadboard",     level: 99 },
    CommandEntry { func: command_reloadclan,      name: "reloadclan",      level: 99 },
    CommandEntry { func: command_item,            name: "i",               level: 50 },
    CommandEntry { func: command_reloadnpc,       name: "reloadnpc",       level: 99 },
    CommandEntry { func: command_reloadmaps,      name: "reloadmaps",      level: 99 },
    CommandEntry { func: command_reloadclass,     name: "reloadclass",     level: 99 },
    CommandEntry { func: command_reloadlevels,    name: "reloadlevels",    level: 99 },
    CommandEntry { func: command_reloadwarps,     name: "reloadwarps",     level: 99 },
    CommandEntry { func: command_transfer,        name: "transfer",        level: 99 },
];

// ─── Stub implementations (replaced batch-by-batch below) ────────────────────

unsafe fn command_debug(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    use crate::ffi::session::{rust_session_wdata_ptr, rust_session_commit, rust_session_wfifohead};
    if sd.is_null() || line.is_null() { return 0; }
    let s = std::ffi::CStr::from_ptr(line).to_str().unwrap_or("");
    let mut iter = s.splitn(2, char::is_whitespace);
    let packnum: u8 = iter.next().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
    let rest = iter.next().unwrap_or("");
    let vals: Vec<u8> = rest.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    let strnum = vals.len();
    let pktlen = strnum + 2;
    let fd = (*sd).fd;
    rust_session_wfifohead(fd, pktlen + 3);
    *rust_session_wdata_ptr(fd, 0) = 0xAA;
    let len_bytes = (pktlen as u16).to_be_bytes();
    *rust_session_wdata_ptr(fd, 1) = len_bytes[0];
    *rust_session_wdata_ptr(fd, 2) = len_bytes[1];
    *rust_session_wdata_ptr(fd, 3) = packnum;
    *rust_session_wdata_ptr(fd, 4) = 0x03;
    for (i, &v) in vals.iter().enumerate() {
        *rust_session_wdata_ptr(fd, 5 + i) = v;
    }
    let n = encrypt_fd(fd) as usize;
    rust_session_commit(fd, n);
    0
}
unsafe fn command_item(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    use crate::servers::char::charstatus::Item;
    if sd.is_null() || line.is_null() { return 0; }
    let line_str = std::ffi::CStr::from_ptr(line).to_str().unwrap_or("");
    let mut itemnum: c_uint = 0;
    let mut itemid: c_uint = 0;

    if !line_str.is_empty() && line_str.as_bytes()[0].is_ascii_digit() {
        // numeric id path
        let mut parts = line_str.trim().splitn(3, char::is_whitespace).filter(|s| !s.is_empty());
        itemid = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        itemnum = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    } else {
        // name path
        let mut parts = line_str.trim().splitn(3, char::is_whitespace).filter(|s| !s.is_empty());
        if let Some(name) = parts.next() {
            itemnum = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let mut namebuf = [0i8; 32];
            for (i, b) in name.bytes().take(31).enumerate() { namebuf[i] = b as i8; }
            itemid = itemdb_id(namebuf.as_ptr());
        }
    }
    if itemid == 0 { return -1; }
    if itemnum == 0 { itemnum = 1; }

    let mut it: Item = std::mem::zeroed();
    it.id = itemid;
    it.dura = itemdb_dura(itemid);
    it.amount = itemnum as i32;
    it.owner = 0;
    pc_additem(sd, &mut it as *mut Item as *mut c_void);
    0
}
unsafe fn command_res(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    if (*sd).status.state == PC_DIE as i8 { pc_res(sd); }
    0
}
unsafe fn command_hair(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let (hair, hair_color) = match parse_two_ints(line) { Some(v) => v, None => return -1 };
    (*sd).status.hair = hair as u16;
    (*sd).status.hair_color = hair_color as u16;
    clif_sendchararea(sd);
    clif_getchararea(sd);
    0
}
unsafe fn command_checkdupes(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    for x in 1..fd_max {
        if rust_session_exists(x) != 0 {
            let tsd = rust_session_get_data(x);
            if !tsd.is_null() && rust_session_get_eof(x) == 0 {
                let n = pc_readglobalreg(tsd, b"goldbardupe\0".as_ptr() as *const c_char);
                if n != 0 {
                    let name_str = std::ffi::CStr::from_ptr((*tsd).status.name.as_ptr()).to_str().unwrap_or("");
                    let mut buf = [0i8; 64];
                    let msg = format!("{} gold bar {} times\0", name_str, n);
                    for (i, b) in msg.bytes().take(63).enumerate() { buf[i] = b as i8; }
                    clif_sendminitext(sd, buf.as_ptr());
                }
            }
        }
    }
    0
}
unsafe fn command_checkwpe(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    for x in 1..fd_max {
        if rust_session_exists(x) != 0 {
            let tsd = rust_session_get_data(x);
            if !tsd.is_null() && rust_session_get_eof(x) == 0 {
                let n = pc_readglobalreg(tsd, b"WPEtimes\0".as_ptr() as *const c_char);
                if n != 0 {
                    let name_str = std::ffi::CStr::from_ptr((*tsd).status.name.as_ptr()).to_str().unwrap_or("");
                    let mut buf = [0i8; 64];
                    let msg = format!("{} WPE attempt {} times\0", name_str, n);
                    for (i, b) in msg.bytes().take(63).enumerate() { buf[i] = b as i8; }
                    clif_sendminitext(sd, buf.as_ptr());
                }
            }
        }
    }
    0
}
unsafe fn command_kill(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let tsd = map_name2sd(line);
    if !tsd.is_null() {
        if rust_session_exists((*tsd).fd) != 0 { rust_session_set_eof((*tsd).fd, 1); }
        clif_sendminitext(sd, b"Done.\0".as_ptr() as *const c_char);
    } else {
        clif_sendminitext(sd, b"User not found.\0".as_ptr() as *const c_char);
    }
    0
}
unsafe fn command_killall(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    for x in 1..fd_max {
        if rust_session_exists(x) != 0 {
            let tsd = rust_session_get_data(x);
            if !tsd.is_null() && rust_session_get_eof(x) == 0 && (*tsd).status.gm_level == 0 {
                rust_session_set_eof(x, 1);
            }
        }
    }
    if rust_session_get_eof((*sd).fd) == 0 {
        clif_sendminitext(sd, b"All but GMs have been mass booted.\0".as_ptr() as *const c_char);
    }
    0
}
unsafe fn command_deletespell(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    // Replicates C bug exactly: `spell` is used before it's set from name.
    // In C: `if (spell >= 0 && spell < 52)` where spell is uninitialized (0).
    // So it always clears skill[0].
    if sd.is_null() { return 0; }
    let _name = match parse_str32(line) { Some(v) => v, None => return -1 };
    let spell: c_int = 0; // C bug: spell used before assignment
    if spell >= 0 && spell < 52 {
        (*sd).status.skill[spell as usize] = 0;
        pc_loadmagic(sd);
    }
    0
}
unsafe fn command_xprate(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let rate = match parse_int(line) { Some(v) => v, None => return -1 };
    xp_rate = rate;
    let mut buf = [0i8; 256];
    let msg = format!("Experience rate: {}x\0", rate);
    for (i, b) in msg.bytes().take(255).enumerate() { buf[i] = b as i8; }
    clif_sendminitext(sd, buf.as_ptr());
    0
}
unsafe fn command_heal(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    (*sd).status.hp = (*sd).max_hp;
    (*sd).status.mp = (*sd).max_mp;
    clif_sendstatus(sd, SFLAG_HPMP);
    0
}
unsafe fn command_level(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let level = match parse_int(line) { Some(v) => v, None => return -1 };
    (*sd).status.level = level as u8;
    clif_sendstatus(sd, SFLAG_FULLSTATS);
    0
}
unsafe fn command_randomspawn   (_sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int { 0 }
unsafe fn command_drate(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let rate = match parse_int(line) { Some(v) => v, None => return -1 };
    d_rate = rate;
    let mut buf = [0i8; 256];
    let msg = format!("Drop rate: {} x\0", rate);
    for (i, b) in msg.bytes().take(255).enumerate() { buf[i] = b as i8; }
    clif_sendminitext(sd, buf.as_ptr());
    0
}
unsafe fn command_spell(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    if let Some((spell, sound)) = parse_two_ints(line) {
        SPELLGFX = spell;
        SOUNDFX = sound;
        clif_playsound(&mut (*sd).bl, sound);
    }
    map_foreachinarea(clif_sendanimation, (*sd).bl.m as c_int, (*sd).bl.x as c_int,
                      (*sd).bl.y as c_int, AREA, BL_PC, SPELLGFX, &mut (*sd).bl, SOUNDFX);
    0
}
unsafe fn command_val(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let count = (MOB_SPAWN_MAX - MOB_SPAWN_START) + (MOB_ONETIME_MAX - MOB_ONETIME_START);
    let mut buf = [0i8; 255];
    let msg = format!("Mob spawn count: {}\0", count);
    for (i, b) in msg.bytes().take(254).enumerate() { buf[i] = b as i8; }
    clif_sendminitext(sd, buf.as_ptr());
    0
}
unsafe fn command_disguise(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let (d, e) = match parse_two_ints(line) { Some(v) => v, None => return -1 };
    let os = (*sd).status.state;
    (*sd).status.state = 0;
    broadcast_update_state(sd);
    (*sd).status.state = os;
    (*sd).disguise = d as u16;
    (*sd).disguise_color = e as u16;
    broadcast_update_state(sd);
    0
}
unsafe fn command_warp(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let (m, x, y) = match parse_three_ints(line) { Some(v) => v, None => return -1 };
    pc_warp(sd, m, x, y);
    0
}
unsafe fn command_givespell(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let name = match parse_str32(line) { Some(v) => v, None => return -1 };
    let spell = magicdb_id(name.as_ptr());
    for x in 0..52usize {
        if (*sd).status.skill[x] == 0 {
            (*sd).status.skill[x] = spell as u16;
            pc_loadmagic(sd);
            break;
        }
        if (*sd).status.skill[x] == spell as u16 { break; }
    }
    0
}
unsafe fn command_side(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let side = match parse_int(line) { Some(v) => v, None => return -1 };
    (*sd).status.side = side as i8;
    clif_sendchararea(sd);
    clif_getchararea(sd);
    0
}
unsafe fn command_state(sd: *mut MapSessionData, line: *mut c_char, _lua_state: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let state_val = match parse_int(line) { Some(v) => v, None => return -1 };
    if (*sd).status.state == 1 && state_val != 1 {
        pc_res(sd);
    } else {
        (*sd).status.state = (state_val % 5) as i8;
        broadcast_update_state(sd);
    }
    0
}
unsafe fn command_armorcolor(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let ac = match parse_int(line) { Some(v) => v, None => return -1 };
    (*sd).status.armor_color = ac as u16;
    clif_sendchararea(sd);
    clif_getchararea(sd);
    0
}
unsafe fn command_makegm(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let name = match parse_str32(line) { Some(v) => v, None => return -1 };
    let tsd = map_name2sd(name.as_ptr());
    if !tsd.is_null() {
        (*tsd).status.gm_level = 99;
    }
    0
}
unsafe fn command_who(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let mut buf = [0i8; 256];
    let msg = format!("There are {} users online.\0", userlist.user_count);
    for (i, b) in msg.bytes().take(255).enumerate() { buf[i] = b as i8; }
    clif_sendminitext(sd, buf.as_ptr());
    0
}
unsafe fn command_legend(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    (*sd).status.legends[0].icon = 12;
    (*sd).status.legends[0].color = 128;
    let text = b"Blessed by a GM\0";
    for (i, &b) in text.iter().enumerate() {
        (*sd).status.legends[0].text[i] = b as i8;
    }
    0
}
unsafe fn command_luareload(sd: *mut MapSessionData, _line: *mut c_char, s: *mut LuaState) -> c_int {
    let errors = sl_reload();
    if sd.is_null() { return errors; }
    clif_sendminitext(sd, b"LUA Scripts reloaded!\0".as_ptr() as *const c_char);
    errors
}
unsafe fn command_magicreload(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    magicdb_read();
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"Magic DB reloaded!\0".as_ptr() as *const c_char);
    0
}
unsafe fn command_lua(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() || line.is_null() { return 0; }
    (*sd).luaexec = 0;
    sl_doscript_simple(b"canRunLuaTalk\0".as_ptr() as *const c_char, std::ptr::null(), &mut (*sd).bl as *mut BlockList);
    if (*sd).luaexec != 0 {
        sl_exec(sd as *mut c_void, line);
    }
    0
}
unsafe fn command_speed(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let d = match parse_int(line) { Some(v) => v, None => return -1 };
    (*sd).speed = d;
    clif_sendchararea(sd);
    clif_getchararea(sd);
    0
}
unsafe fn command_reloaditem(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    itemdb_read();
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"Item DB Reloaded!\0".as_ptr() as *const c_char);
    0
}
unsafe fn command_reloadcreations(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    // Creation system is Lua-script-driven; no DB layer to reload.
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"Creations DB reloaded!\0".as_ptr() as *const c_char);
    0
}
unsafe fn command_reloadmob(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    rust_mobdb_term();
    rust_mobdb_init();
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"Mob DB Reloaded\0".as_ptr() as *const c_char);
    0
}
unsafe fn command_reloadspawn(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    mobspawn_read();
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"Spawn DB Reloaded\0".as_ptr() as *const c_char);
    0
}
unsafe fn command_pvp(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let pvp = match parse_int(line) { Some(v) => v, None => return -1 };
    let mp = crate::ffi::map_db::get_map_ptr((*sd).bl.m);
    if !mp.is_null() { (*mp).pvp = pvp as u8; }
    let mut buf = [0i8; 64];
    let msg = format!("PvP set to: {}\0", pvp);
    for (i, b) in msg.bytes().take(63).enumerate() { buf[i] = b as i8; }
    clif_sendminitext(sd, buf.as_ptr());
    0
}
unsafe fn command_spellwork(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let mp = crate::ffi::map_db::get_map_ptr((*sd).bl.m);
    if !mp.is_null() { (*mp).spell ^= 1; }
    0
}
unsafe fn command_broadcast(_sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    clif_broadcast(line, -1);
    0
}
unsafe fn command_luasize(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if !sd.is_null() { sl_luasize(sd as *mut c_void); }
    0
}
unsafe fn command_luafix(_sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    sl_fixmem();
    0
}
// FFI-compatible callback for map_respawn: respawn dead non-onetime mobs.
unsafe extern "C" fn command_handle_mob_ffi(bl: *mut BlockList, _ap: ...) -> c_int {
    if bl.is_null() { return 0; }
    let mob = bl as *mut MobSpawnData;
    if (*mob).state == MOB_DEAD && (*mob).onetime == 0 {
        mob_respawn(mob as *mut c_void);
    }
    0
}
unsafe fn command_respawn(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    map_respawn(command_handle_mob_ffi, (*sd).bl.m as c_int, BL_MOB);
    0
}
unsafe fn command_ban(_sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    let name = match parse_str32(line) { Some(v) => v, None => return -1 };
    let tsd = map_name2sd(name.as_ptr());
    if !tsd.is_null() {
        printf(b"Banning %s\n\0".as_ptr() as *const c_char, name.as_ptr());
        let mut esc = [0i8; 65];
        Sql_EscapeString(sql_handle_void(), esc.as_mut_ptr(), name.as_ptr());
        if SQL_ERROR == Sql_Query(sql_handle_void(),
            b"UPDATE `Character` SET ChaBanned = '1' WHERE `ChaName` = '%s'\0".as_ptr() as *const c_char,
            esc.as_ptr()) {
            Sql_ShowDebug(sql_handle_void());
        }
        rust_session_set_eof((*tsd).fd, 1);
    }
    0
}
unsafe fn command_unban(_sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    let name = match parse_str32(line) { Some(v) => v, None => return -1 };
    printf(b"Unbanning %s\n\0".as_ptr() as *const c_char, name.as_ptr());
    let mut esc = [0i8; 65];
    Sql_EscapeString(sql_handle_void(), esc.as_mut_ptr(), name.as_ptr());
    if SQL_ERROR == Sql_Query(sql_handle_void(),
        b"UPDATE `Character` SET ChaBanned = '0' WHERE `ChaName` = '%s'\0".as_ptr() as *const c_char,
        esc.as_ptr()) {
        Sql_ShowDebug(sql_handle_void());
    }
    0
}
unsafe fn command_kc(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    for x in 0..MAX_KILLREG {
        let mut buf = [0i8; 255];
        let mob_id = (*sd).status.killreg[x].mob_id;
        let amount = (*sd).status.killreg[x].amount;
        let msg = format!("{} ({})\0", mob_id, amount);
        for (i, b) in msg.bytes().take(254).enumerate() { buf[i] = b as i8; }
        clif_sendminitext(sd, buf.as_ptr());
    }
    0
}
unsafe fn command_blockcount    (_sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int { 0 }
unsafe fn command_stealth(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    if (*sd).optFlags & OPT_STEALTH != 0 {
        (*sd).optFlags ^= OPT_STEALTH;
        clif_refresh(sd);
        clif_sendminitext(sd, b"Stealth :OFF\0".as_ptr() as *const c_char);
    } else {
        clif_lookgone(&mut (*sd).bl);
        (*sd).optFlags ^= OPT_STEALTH;
        clif_refresh(sd);
        clif_sendminitext(sd, b"Stealth :ON\0".as_ptr() as *const c_char);
    }
    0
}
unsafe fn command_ghosts(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    (*sd).optFlags ^= OPT_GHOSTS;
    clif_refresh(sd);
    if (*sd).optFlags & OPT_GHOSTS != 0 {
        clif_sendminitext(sd, b"Ghosts :ON\0".as_ptr() as *const c_char);
    } else {
        clif_sendminitext(sd, b"Ghosts :OFF\0".as_ptr() as *const c_char);
    }
    0
}
unsafe fn command_unphysical(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    (*sd).uFlags ^= UFLAG_UNPHYS;
    if (*sd).uFlags & UFLAG_UNPHYS != 0 {
        clif_sendminitext(sd, b"Unphysical :ON\0".as_ptr() as *const c_char);
    } else {
        clif_sendminitext(sd, b"Unphysical :OFF\0".as_ptr() as *const c_char);
    }
    0
}
unsafe fn command_immortality(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    (*sd).uFlags ^= UFLAG_IMMORTAL;
    if (*sd).uFlags & UFLAG_IMMORTAL != 0 {
        clif_sendminitext(sd, b"Immortality :ON\0".as_ptr() as *const c_char);
    } else {
        clif_sendminitext(sd, b"Immortality :OFF\0".as_ptr() as *const c_char);
    }
    0
}
unsafe fn command_silence(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let name = match parse_str32(line) { Some(v) => v, None => return -1 };
    let tsd = map_name2sd(name.as_ptr());
    if !tsd.is_null() {
        (*tsd).uFlags ^= UFLAG_SILENCED;
        if (*tsd).uFlags & UFLAG_SILENCED != 0 {
            clif_sendminitext(sd, b"Silenced.\0".as_ptr() as *const c_char);
            clif_sendminitext(tsd, b"You have been silenced.\0".as_ptr() as *const c_char);
        } else {
            clif_sendminitext(sd, b"Unsilenced.\0".as_ptr() as *const c_char);
            clif_sendminitext(tsd, b"Silence lifted.\0".as_ptr() as *const c_char);
        }
    } else {
        clif_sendminitext(sd, b"User not on.\0".as_ptr() as *const c_char);
    }
    0
}
unsafe fn command_shutdowncancel(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if DOWNTIMER != 0 {
        clif_broadcast(b"---------------------------------------------------\0".as_ptr() as *const c_char, -1);
        clif_broadcast(b"Server shutdown cancelled.\0".as_ptr() as *const c_char, -1);
        clif_broadcast(b"---------------------------------------------------\0".as_ptr() as *const c_char, -1);
        timer_remove(DOWNTIMER);
        DOWNTIMER = 0;
    } else if !sd.is_null() {
        clif_sendminitext(sd, b"Server is not shutting down.\0".as_ptr() as *const c_char);
    }
    0
}
unsafe fn command_shutdown(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() || line.is_null() { return 0; }
    if DOWNTIMER != 0 {
        clif_sendminitext(sd, b"Server is already shutting down.\0".as_ptr() as *const c_char);
        return 0;
    }
    let s = std::ffi::CStr::from_ptr(line).to_str().unwrap_or("");
    let mut parts = s.trim().splitn(3, char::is_whitespace).filter(|p| !p.is_empty());
    let t_num: i32 = match parts.next().and_then(|v| v.parse().ok()) {
        Some(v) => v,
        None => return -1,
    };
    let unit = parts.next().unwrap_or("").to_ascii_lowercase();
    let mut t_time: i32 = t_num;
    if unit == "s" || unit == "sec" {
        t_time *= 1000;
    } else if unit == "m" || unit == "min" {
        t_time *= 60000;
    } else if unit == "h" || unit == "hr" {
        t_time *= 3600000;
    }
    let mut msg_buf = [0i8; 255];
    if t_time >= 60000 {
        let d = t_time / 60000;
        t_time = d * 60000;
        let msg = format!("Reset in {} minutes.\0", d);
        for (i, b) in msg.bytes().take(254).enumerate() { msg_buf[i] = b as i8; }
    } else {
        let d = t_time / 1000;
        t_time = d * 1000;
        let msg = format!("Reset in {} seconds.\0", d);
        for (i, b) in msg.bytes().take(254).enumerate() { msg_buf[i] = b as i8; }
    }
    clif_broadcast(b"---------------------------------------------------\0".as_ptr() as *const c_char, -1);
    clif_broadcast(msg_buf.as_ptr(), -1);
    clif_broadcast(b"---------------------------------------------------\0".as_ptr() as *const c_char, -1);
    DOWNTIMER = timer_insert(250, 250, map_reset_timer, t_time as c_uint, 250);
    0
}
unsafe fn command_weap(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let (id, color) = match parse_two_ints(line) { Some(v) => v, None => return -1 };
    (*sd).gfx.weapon = id as u16;
    (*sd).gfx.cweapon = color as u8;
    clif_getchararea(sd);
    clif_sendchararea(sd);
    0
}
unsafe fn command_shield(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let (id, color) = match parse_two_ints(line) { Some(v) => v, None => return -1 };
    (*sd).gfx.shield = id as u16;
    (*sd).gfx.cshield = color as u8;
    clif_getchararea(sd);
    clif_sendchararea(sd);
    0
}
unsafe fn command_armor(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let (id, color) = match parse_two_ints(line) { Some(v) => v, None => return -1 };
    (*sd).gfx.armor = id as u16;
    (*sd).gfx.carmor = color as u8;
    clif_getchararea(sd);
    clif_sendchararea(sd);
    0
}
unsafe fn command_boots(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let (id, color) = match parse_two_ints(line) { Some(v) => v, None => return -1 };
    (*sd).gfx.boots = id as u16;
    (*sd).gfx.cboots = color as u8;
    clif_getchararea(sd);
    clif_sendchararea(sd);
    0
}
unsafe fn command_mantle(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let (id, color) = match parse_two_ints(line) { Some(v) => v, None => return -1 };
    (*sd).gfx.mantle = id as u16;
    (*sd).gfx.cmantle = color as u8;
    clif_getchararea(sd);
    clif_sendchararea(sd);
    0
}
unsafe fn command_necklace(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let (id, color) = match parse_two_ints(line) { Some(v) => v, None => return -1 };
    (*sd).gfx.necklace = id as u16;
    (*sd).gfx.cnecklace = color as u8;
    clif_getchararea(sd);
    clif_sendchararea(sd);
    0
}
unsafe fn command_faceacc(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let (id, color) = match parse_two_ints(line) { Some(v) => v, None => return -1 };
    (*sd).gfx.face_acc = id as u16;
    (*sd).gfx.cface_acc = color as u8;
    clif_getchararea(sd);
    clif_sendchararea(sd);
    0
}
unsafe fn command_crown(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let (id, color) = match parse_two_ints(line) { Some(v) => v, None => return -1 };
    (*sd).gfx.crown = id as u16;
    (*sd).gfx.ccrown = color as u8;
    clif_getchararea(sd);
    clif_sendchararea(sd);
    0
}
unsafe fn command_helm(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let (id, color) = match parse_two_ints(line) { Some(v) => v, None => return -1 };
    (*sd).gfx.helm = id as u16;
    (*sd).gfx.chelm = color as u8;
    clif_getchararea(sd);
    clif_sendchararea(sd);
    0
}
unsafe fn command_gfxtoggle(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    (*sd).gfx.toggle ^= 1;
    0
}
unsafe fn command_weather(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let weather = parse_int(line).unwrap_or(5);
    let mp = crate::ffi::map_db::get_map_ptr((*sd).bl.m);
    if !mp.is_null() { (*mp).weather = weather as u8; }
    for x in 1..fd_max {
        if rust_session_exists(x) != 0 {
            let tmpsd = rust_session_get_data(x) as *mut MapSessionData;
            if !tmpsd.is_null() && rust_session_get_eof(x) == 0 && (*tmpsd).bl.m == (*sd).bl.m {
                clif_sendweather(tmpsd);
            }
        }
    }
    0
}
unsafe fn command_light(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let light = parse_int(line).unwrap_or(232);
    let mp = crate::ffi::map_db::get_map_ptr((*sd).bl.m);
    if !mp.is_null() { (*mp).light = light as u8; }
    for x in 0..fd_max {
        if rust_session_exists(x) != 0 {
            let tmpsd = rust_session_get_data(x) as *mut MapSessionData;
            if !tmpsd.is_null() && rust_session_get_eof(x) == 0 && (*tmpsd).bl.m == (*sd).bl.m {
                pc_warp(tmpsd, (*tmpsd).bl.m as c_int, (*tmpsd).bl.x as c_int, (*tmpsd).bl.y as c_int);
            }
        }
    }
    0
}
unsafe fn command_gm(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let name_ptr = (*sd).status.name.as_ptr();
    let line_str = std::ffi::CStr::from_ptr(line).to_str().unwrap_or("");
    let name_str = std::ffi::CStr::from_ptr(name_ptr).to_str().unwrap_or("");
    let mut buf = [0i8; 65535];
    let msg = format!("<GM>{}: {}\0", name_str, line_str);
    for (i, b) in msg.bytes().take(65534).enumerate() { buf[i] = b as i8; }
    for x in 1..fd_max {
        if rust_session_exists(x) != 0 {
            let tsd = rust_session_get_data(x);
            if !tsd.is_null() && rust_session_get_eof(x) == 0 && (*tsd).status.gm_level != 0 {
                clif_sendmsg(tsd, 11, buf.as_ptr());
            }
        }
    }
    0
}
unsafe fn command_report(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let name_ptr = (*sd).status.name.as_ptr();
    let line_str = std::ffi::CStr::from_ptr(line).to_str().unwrap_or("");
    let name_str = std::ffi::CStr::from_ptr(name_ptr).to_str().unwrap_or("");
    let mut buf = [0i8; 65535];
    let msg = format!("<REPORT>{}: {}\0", name_str, line_str);
    for (i, b) in msg.bytes().take(65534).enumerate() { buf[i] = b as i8; }
    for x in 1..fd_max {
        if rust_session_exists(x) != 0 {
            let tsd = rust_session_get_data(x);
            if !tsd.is_null() && rust_session_get_eof(x) == 0 && (*tsd).status.gm_level > 0 {
                clif_sendmsg(tsd, 12, buf.as_ptr());
            }
        }
    }
    0
}
unsafe fn command_url(_sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if line.is_null() { return 0; }
    let line_str = std::ffi::CStr::from_ptr(line).to_str().unwrap_or("");
    let mut parts = line_str.trim().splitn(4, char::is_whitespace).filter(|s| !s.is_empty());
    let name_s = match parts.next() { Some(v) => v, None => return -1 };
    let url_type: c_int = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let url_s = parts.next().unwrap_or("");

    let mut namebuf = [0i8; 32];
    for (i, b) in name_s.bytes().take(31).enumerate() { namebuf[i] = b as i8; }
    let mut urlbuf = [0i8; 128];
    for (i, b) in url_s.bytes().take(127).enumerate() { urlbuf[i] = b as i8; }

    let tsd = map_name2sd(namebuf.as_ptr());
    if tsd.is_null() { return -1; }
    clif_sendurl(tsd, url_type, urlbuf.as_ptr());
    0
}
unsafe fn command_cinv(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let range = parse_two_ints(line);
    let (start, end) = match range {
        Some((s, e)) => (s, e),
        None => (0, 51),
    };
    for x in start..=end {
        let x = x as usize;
        if x < 52 && (*sd).status.inventory[x].id > 0 && (*sd).status.inventory[x].amount > 0 {
            pc_delitem(sd, x as c_int, (*sd).status.inventory[x].amount, 0);
        }
    }
    0
}
unsafe fn command_cfloor        (_sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int { 0 }
unsafe fn command_cspells(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let (start, end) = match parse_two_ints(line) {
        Some((s, e)) => (s as usize, e as usize),
        None => (0, 51),
    };
    for x in start..=end {
        if x < 52 && (*sd).status.skill[x] > 0 {
            (*sd).status.skill[x] = 0;
            pc_loadmagic(sd);
        }
    }
    0
}
unsafe fn command_job(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let (mut job, mut subjob) = parse_two_ints(line).unwrap_or((0, 0));
    if job < 0 { job = 5; }
    if subjob < 0 || subjob > 16 { subjob = 0; }
    (*sd).status.class = job as u8;
    (*sd).status.mark = subjob as u8;
    if SQL_ERROR == Sql_Query(sql_handle_void(),
        b"UPDATE `Character` SET `ChaPthId` = '%u', `ChaMark` = '%u' WHERE `ChaId` = '%u'\0".as_ptr() as *const c_char,
        (*sd).status.class as c_uint, (*sd).status.mark as c_uint, (*sd).status.id) {
        Sql_ShowDebug(sql_handle_void());
        Sql_FreeResult(sql_handle_void());
        return 0;
    }
    clif_mystaytus(sd);
    0
}
unsafe fn command_music(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    if let Some(music) = parse_int(line) { MUSICFX = music; }
    let oldm = (*sd).bl.m as c_int;
    let oldx = (*sd).bl.x as c_int;
    let oldy = (*sd).bl.y as c_int;
    let mp = crate::ffi::map_db::get_map_ptr((*sd).bl.m);
    if !mp.is_null() { (*mp).bgm = MUSICFX as u16; }
    pc_warp(sd, 10002, 0, 0);
    pc_warp(sd, oldm, oldx, oldy);
    0
}
unsafe fn command_musicn(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    MUSICFX += 1;
    let oldm = (*sd).bl.m as c_int;
    let oldx = (*sd).bl.x as c_int;
    let oldy = (*sd).bl.y as c_int;
    let mp = crate::ffi::map_db::get_map_ptr((*sd).bl.m);
    if !mp.is_null() { (*mp).bgm = MUSICFX as u16; }
    pc_warp(sd, 10002, 0, 0);
    pc_warp(sd, oldm, oldx, oldy);
    0
}
unsafe fn command_musicp(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    MUSICFX -= 1;
    let oldm = (*sd).bl.m as c_int;
    let oldx = (*sd).bl.x as c_int;
    let oldy = (*sd).bl.y as c_int;
    let mp = crate::ffi::map_db::get_map_ptr((*sd).bl.m);
    if !mp.is_null() { (*mp).bgm = MUSICFX as u16; }
    pc_warp(sd, 10002, 0, 0);
    pc_warp(sd, oldm, oldx, oldy);
    0
}
unsafe fn command_musicq(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let mut buf = [0i8; 25];
    let msg = format!("Current music is: {}\0", MUSICFX);
    for (i, b) in msg.bytes().take(24).enumerate() { buf[i] = b as i8; }
    clif_sendminitext(sd, buf.as_ptr());
    0
}
unsafe fn command_sound(sd: *mut MapSessionData, line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    if let Some(sound) = parse_int(line) { SOUNDFX = sound; }
    clif_playsound(&mut (*sd).bl, SOUNDFX);
    0
}
unsafe fn command_nsound(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    SOUNDFX += 1;
    if SOUNDFX > 125 { SOUNDFX = 125; }
    clif_playsound(&mut (*sd).bl, SOUNDFX);
    0
}
unsafe fn command_psound(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    SOUNDFX -= 1;
    if SOUNDFX < 0 { SOUNDFX = 0; }
    clif_playsound(&mut (*sd).bl, SOUNDFX);
    0
}
unsafe fn command_soundq(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let mut buf = [0i8; 25];
    let msg = format!("Current sound is: {}\0", SOUNDFX);
    for (i, b) in msg.bytes().take(24).enumerate() { buf[i] = b as i8; }
    clif_sendminitext(sd, buf.as_ptr());
    0
}
unsafe fn command_nspell(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    SPELLGFX += 1;
    if SPELLGFX > 427 { SPELLGFX = 427; }
    map_foreachinarea(clif_sendanimation, (*sd).bl.m as c_int, (*sd).bl.x as c_int,
                      (*sd).bl.y as c_int, AREA, BL_PC, SPELLGFX, &mut (*sd).bl, SOUNDFX);
    0
}
unsafe fn command_pspell(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    SPELLGFX -= 1;
    if SPELLGFX < 0 { SPELLGFX = 0; }
    map_foreachinarea(clif_sendanimation, (*sd).bl.m as c_int, (*sd).bl.x as c_int,
                      (*sd).bl.y as c_int, AREA, BL_PC, SPELLGFX, &mut (*sd).bl, SOUNDFX);
    0
}
unsafe fn command_spellq(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    let mut buf = [0i8; 25];
    let msg = format!("Current Spell is: {}\0", SPELLGFX);
    for (i, b) in msg.bytes().take(24).enumerate() { buf[i] = b as i8; }
    clif_sendminitext(sd, buf.as_ptr());
    0
}
unsafe fn command_reloadboard(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    boarddb_term();
    boarddb_init();
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"Board DB reloaded!\0".as_ptr() as *const c_char);
    0
}
unsafe fn command_reloadclan(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    clandb_init();
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"Clan DB reloaded!\0".as_ptr() as *const c_char);
    0
}
unsafe fn command_reloadnpc(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    npc_init();
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"NPC DB reloaded!\0".as_ptr() as *const c_char);
    0
}
unsafe fn command_reloadmaps(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    map_reload();
    if char_fd > 0 && rust_session_exists(char_fd) != 0 {
        use crate::ffi::session::{rust_session_wdata_ptr, rust_session_commit, rust_session_wfifohead};
        let pkt_len = (map_n * 2 + 8) as usize;
        rust_session_wfifohead(char_fd, pkt_len);
        (rust_session_wdata_ptr(char_fd, 0) as *mut u16).write_unaligned(0x3001u16.to_le());
        (rust_session_wdata_ptr(char_fd, 2) as *mut u32).write_unaligned(pkt_len as u32);
        (rust_session_wdata_ptr(char_fd, 6) as *mut u16).write_unaligned(map_n as u16);
        let mut j: usize = 0;
        for i in 0..MAX_MAP_PER_SERVER {
            let mp = crate::ffi::map_db::get_map_ptr(i as u16);
            if !mp.is_null() && !(*mp).tile.is_null() {
                (rust_session_wdata_ptr(char_fd, j * 2 + 8) as *mut u16).write_unaligned(i as u16);
                j += 1;
            }
            if j >= map_n as usize { break; }
        }
        rust_session_commit(char_fd, pkt_len);
    }
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"Maps reloaded!\0".as_ptr() as *const c_char);
    0
}
unsafe fn command_reloadclass(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    classdb_read();
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"Classes reloaded!\0".as_ptr() as *const c_char);
    0
}
unsafe fn command_reloadlevels(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    leveldb_read();
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"Levels reloaded!\0".as_ptr() as *const c_char);
    0
}
unsafe fn command_reloadwarps(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    warp_init();
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"Warps reloaded!\0".as_ptr() as *const c_char);
    0
}
unsafe fn command_transfer(sd: *mut MapSessionData, _line: *mut c_char, _s: *mut LuaState) -> c_int {
    if sd.is_null() { return 0; }
    clif_transfer_test(sd, 1, 10, 10);
    0
}

// ─── rust_command_reload: exported entry point for full mini-reset ────────────

#[no_mangle]
pub unsafe extern "C" fn rust_command_reload(
    sd: *mut MapSessionData, line: *mut c_char, state: *mut LuaState,
) -> c_int {
    let errors = command_luareload(sd, line, state);
    command_magicreload(sd, line, state);
    command_reloadmob(sd, line, state);
    command_reloadspawn(sd, line, state);
    command_reloaditem(sd, line, state);
    command_reloadnpc(sd, line, state);
    command_reloadboard(sd, line, state);
    command_reloadmaps(sd, line, state);
    command_reloadclass(sd, line, state);
    command_reloadwarps(sd, line, state);
    if !sd.is_null() {
        clif_sendminitext(sd, b"Mini reset complete!\0".as_ptr() as *const c_char);
    }
    errors
}

// ─── Parse helpers (replaces sscanf) ─────────────────────────────────────────

unsafe fn parse_int(line: *mut c_char) -> Option<c_int> {
    if line.is_null() { return None; }
    let s = std::ffi::CStr::from_ptr(line).to_str().ok()?;
    s.trim().splitn(2, char::is_whitespace).next()?.parse().ok()
}

unsafe fn parse_two_ints(line: *mut c_char) -> Option<(c_int, c_int)> {
    if line.is_null() { return None; }
    let s = std::ffi::CStr::from_ptr(line).to_str().ok()?;
    let mut p = s.trim().splitn(3, char::is_whitespace).filter(|p| !p.is_empty());
    let a = p.next()?.parse().ok()?;
    let b = p.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    Some((a, b))
}

unsafe fn parse_three_ints(line: *mut c_char) -> Option<(c_int, c_int, c_int)> {
    if line.is_null() { return None; }
    let s = std::ffi::CStr::from_ptr(line).to_str().ok()?;
    let mut p = s.trim().splitn(4, char::is_whitespace).filter(|x| !x.is_empty());
    Some((p.next()?.parse().ok()?, p.next()?.parse().ok()?, p.next()?.parse().ok()?))
}

unsafe fn parse_str32(line: *mut c_char) -> Option<[c_char; 32]> {
    if line.is_null() { return None; }
    let s = std::ffi::CStr::from_ptr(line).to_str().ok()?;
    let word = s.trim().splitn(2, char::is_whitespace).next()?;
    let mut buf = [0i8; 32];
    for (i, b) in word.bytes().take(31).enumerate() { buf[i] = b as i8; }
    Some(buf)
}

// ─── Command dispatcher ───────────────────────────────────────────────────────

unsafe fn dispatch(sd: *mut MapSessionData, p: *const c_char, len: c_int, log: bool) -> c_int {
    if *p != COMMAND_CODE { return 0; }
    let p = p.add(1);

    let mut cmd_line = [0u8; 257];
    let copy_len = (len as usize).min(256);
    std::ptr::copy_nonoverlapping(p as *const u8, cmd_line.as_mut_ptr(), copy_len);
    cmd_line[copy_len] = 0;

    let mut end = 0usize;
    while end < copy_len && cmd_line[end] != 0 && cmd_line[end] != b' ' && cmd_line[end] != b'\t' {
        end += 1;
    }
    cmd_line[end] = 0;

    let cmd_name = match std::str::from_utf8(&cmd_line[..end]) {
        Ok(s) => s,
        Err(_) => return 0,
    };

    let entry = match COMMANDS.iter().find(|e| e.name.eq_ignore_ascii_case(cmd_name)) {
        Some(e) => e,
        None => return 0,
    };

    if ((*sd).status.gm_level as c_int) < entry.level { return 0; }

    // Skip past the null byte we inserted, then past whitespace.
    // Clamp to copy_len so we never step past the buffer when end == copy_len.
    let args_offset = (end + 1).min(copy_len);
    let mut args_ptr = p.add(args_offset);
    while *args_ptr == b' ' as c_char || *args_ptr == b'\t' as c_char {
        args_ptr = args_ptr.add(1);
    }

    if log {
        printf(b"[command] gm command used cmd=%s\n\0".as_ptr() as *const c_char,
               cmd_line.as_ptr());
    }

    (entry.func)(sd, args_ptr as *mut c_char, std::ptr::null_mut());
    1 // command matched and executed — caller checks bool, not handler result
}

#[no_mangle]
pub unsafe extern "C" fn rust_is_command(sd: *mut MapSessionData, p: *const c_char, len: c_int) -> c_int {
    dispatch(sd, p, len, true)
}

#[no_mangle]
pub unsafe extern "C" fn rust_at_command(sd: *mut MapSessionData, p: *const c_char, len: c_int) -> c_int {
    dispatch(sd, p, len, false)
}
