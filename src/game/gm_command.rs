//! GM command dispatch.

#![allow(non_snake_case, dead_code, unused_variables, unused_mut)]

use std::sync::atomic::{AtomicI32, AtomicI8};

use crate::database::map_db::BlockList;
use crate::game::mob::{MobSpawnData, BL_MOB, BL_PC, MOB_DEAD};
use crate::game::pc::{MapSessionData, PC_DIE, SFLAG_FULLSTATS, SFLAG_HPMP};

// Module globals
static SPELLGFX:     AtomicI32 = AtomicI32::new(0);
static MUSICFX:      AtomicI32 = AtomicI32::new(0);
static SOUNDFX:      AtomicI32 = AtomicI32::new(0);
static DOWNTIMER:    AtomicI32 = AtomicI32::new(0);
static COMMAND_CODE: AtomicI8  = AtomicI8::new(b'/' as i8);

// Flag constants — imported from pc.rs where possible,
// redefined here for local clarity.
const OPT_STEALTH:    u64 = 32;
const OPT_GHOSTS:     u64 = 256;
const UFLAG_SILENCED: u64 = 1;
const UFLAG_IMMORTAL: u64 = 8;
const UFLAG_UNPHYS:   u64 = 16;

// AREA constant (from map_parse.h enum)
const AREA: i32 = 4;

// MAX_MAP_PER_SERVER (from mmo.h)
const MAX_MAP_PER_SERVER: i32 = 65535;

// MAX_KILLREG (from mmo.h)
const MAX_KILLREG: usize = 5000;

// All other formerly-C globals are now Rust statics accessible via direct paths.
use crate::config_globals::{xp_rate, d_rate};
use crate::database::map_db::map_n;
use crate::game::mob::{MOB_SPAWN_START, MOB_SPAWN_MAX, MOB_ONETIME_START, MOB_ONETIME_MAX};
use std::sync::atomic::Ordering;

// char_fd, sql_handle, and userlist are now Rust statics in src/game/map_server.rs.
use crate::game::map_server::{char_fd, userlist};

use crate::database::get_pool;

type LuaState = std::ffi::c_void; // opaque

// ── map functions ──────────────────────────────────────────────────────────────
use crate::game::map_server::{map_name2sd, map_reload, map_reset_timer};
use crate::game::block::{map_respawnmobs, foreach_in_area, AreaType};

// ── clif functions ─────────────────────────────────────────────────────────────
use crate::game::map_parse::chat::{clif_sendminitext, clif_sendmsg, clif_broadcast, clif_playsound};
use crate::game::map_parse::movement::clif_sendchararea;
use crate::game::map_parse::player_state::{clif_getchararea, clif_sendstatus, clif_mystaytus_by_addr, clif_refresh};
use crate::game::client::visual::{broadcast_update_state, clif_sendweather, clif_sendurl};
use crate::game::map_parse::combat::clif_sendanimation_inner;
use crate::game::map_parse::visual::clif_lookgone;
use crate::game::client::handlers::clif_transfer_test;

// ── pc functions (rust_pc_* names) ────────────────────────────────────────────
use crate::game::pc::{
    rust_pc_warp_sync as pc_warp,
    rust_pc_additem as pc_additem,
    rust_pc_delitem as pc_delitem,
    rust_pc_res as pc_res,
    rust_pc_loadmagic as pc_loadmagic,
    rust_pc_readglobalreg as pc_readglobalreg,
};

// ── mob functions ──────────────────────────────────────────────────────────────
use crate::game::mob::rust_mob_respawn as mob_respawn;

// ── scripting functions ────────────────────────────────────────────────────────
use crate::game::scripting::{
    rust_sl_reload as sl_reload,
    rust_sl_exec as sl_exec,
    rust_sl_fixmem as sl_fixmem,
    rust_sl_luasize as sl_luasize,
};

// ── database init functions ────────────────────────────────────────────────────
use crate::database::item_db::{rust_itemdb_init as itemdb_read, rust_itemdb_id as itemdb_id, rust_itemdb_dura as itemdb_dura};
use crate::database::magic_db::rust_magicdb_id as magicdb_id;
use crate::database::board_db::{rust_boarddb_term as boarddb_term, rust_boarddb_init as boarddb_init};
use crate::database::clan_db::rust_clandb_init as clandb_init;
use crate::game::npc::{npc_init, warp_init};
use crate::database::mob_db::{rust_mobdb_term, rust_mobdb_init};
use crate::game::mob::rust_mobspawn_read as mobspawn_read;

// ── session helpers ────────────────────────────────────────────────────────────
use crate::session::{
    rust_session_exists, rust_session_get_data, rust_session_get_eof, rust_session_set_eof,
};

// ── encrypt ────────────────────────────────────────────────────────────────────
use crate::network::crypt::encrypt as encrypt_fd;

// ── timer ──────────────────────────────────────────────────────────────────────
use crate::game::time_util::{timer_insert, timer_remove};

// ── libc printf (used for debug logging) ──────────────────────────────────────
use libc::printf;

/// Dispatch a Lua event with a single block_list argument.
#[cfg(not(test))]
#[allow(dead_code)]
unsafe fn sl_doscript_simple(root: *const i8, method: *const i8, bl: *mut crate::database::map_db::BlockList) -> i32 {
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

type CmdFn = unsafe fn(*mut MapSessionData, *mut i8, *mut LuaState) -> i32;

struct CommandEntry {
    func:  CmdFn,
    name:  &'static str,
    level: i32,
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

unsafe fn command_debug(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    use crate::session::{rust_session_wdata_ptr, rust_session_commit, rust_session_wfifohead};
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
unsafe fn command_item(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    use crate::servers::char::charstatus::Item;
    if sd.is_null() || line.is_null() { return 0; }
    let line_str = std::ffi::CStr::from_ptr(line).to_str().unwrap_or("");
    let mut itemnum: u32 = 0;
    let mut itemid: u32 = 0;

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
    pc_additem(sd, &mut it);
    0
}
unsafe fn command_res(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    if (*sd).status.state == PC_DIE as i8 { pc_res(sd); }
    0
}
unsafe fn command_hair(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let (hair, hair_color) = match parse_two_ints(line) { Some(v) => v, None => return -1 };
    (*sd).status.hair = hair as u16;
    (*sd).status.hair_color = hair_color as u16;
    clif_sendchararea(sd);
    clif_getchararea(sd);
    0
}
unsafe fn command_checkdupes(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    for x in 1..crate::session::get_fd_max() {
        if rust_session_exists(x) != 0 {
            let tsd = rust_session_get_data(x) as *mut MapSessionData;
            if !tsd.is_null() && rust_session_get_eof(x) == 0 {
                let n = pc_readglobalreg(tsd, b"goldbardupe\0".as_ptr() as *const i8);
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
unsafe fn command_checkwpe(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    for x in 1..crate::session::get_fd_max() {
        if rust_session_exists(x) != 0 {
            let tsd = rust_session_get_data(x) as *mut MapSessionData;
            if !tsd.is_null() && rust_session_get_eof(x) == 0 {
                let n = pc_readglobalreg(tsd, b"WPEtimes\0".as_ptr() as *const i8);
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
unsafe fn command_kill(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let tsd = map_name2sd(line);
    if !tsd.is_null() {
        if rust_session_exists((*tsd).fd) != 0 { rust_session_set_eof((*tsd).fd, 1); }
        clif_sendminitext(sd, b"Done.\0".as_ptr() as *const i8);
    } else {
        clif_sendminitext(sd, b"User not found.\0".as_ptr() as *const i8);
    }
    0
}
unsafe fn command_killall(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    for x in 1..crate::session::get_fd_max() {
        if rust_session_exists(x) != 0 {
            let tsd = rust_session_get_data(x) as *mut MapSessionData;
            if !tsd.is_null() && rust_session_get_eof(x) == 0 && (*tsd).status.gm_level == 0 {
                rust_session_set_eof(x, 1);
            }
        }
    }
    if rust_session_get_eof((*sd).fd) == 0 {
        clif_sendminitext(sd, b"All but GMs have been mass booted.\0".as_ptr() as *const i8);
    }
    0
}
unsafe fn command_deletespell(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    // Replicates C bug exactly: `spell` is used before it's set from name.
    // In C: `if (spell >= 0 && spell < 52)` where spell is uninitialized (0).
    // So it always clears skill[0].
    if sd.is_null() { return 0; }
    let _name = match parse_str32(line) { Some(v) => v, None => return -1 };
    let spell: i32 = 0; // C bug: spell used before assignment
    if spell >= 0 && spell < 52 {
        (*sd).status.skill[spell as usize] = 0;
        pc_loadmagic(sd);
    }
    0
}
unsafe fn command_xprate(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let rate = match parse_int(line) { Some(v) => v, None => return -1 };
    xp_rate = rate;
    let mut buf = [0i8; 256];
    let msg = format!("Experience rate: {}x\0", rate);
    for (i, b) in msg.bytes().take(255).enumerate() { buf[i] = b as i8; }
    clif_sendminitext(sd, buf.as_ptr());
    0
}
unsafe fn command_heal(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    (*sd).status.hp = (*sd).max_hp;
    (*sd).status.mp = (*sd).max_mp;
    clif_sendstatus(sd, SFLAG_HPMP);
    0
}
unsafe fn command_level(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let level = match parse_int(line) { Some(v) => v, None => return -1 };
    (*sd).status.level = level as u8;
    clif_sendstatus(sd, SFLAG_FULLSTATS);
    0
}
unsafe fn command_randomspawn   (_sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 { 0 }
unsafe fn command_drate(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let rate = match parse_int(line) { Some(v) => v, None => return -1 };
    d_rate = rate;
    let mut buf = [0i8; 256];
    let msg = format!("Drop rate: {} x\0", rate);
    for (i, b) in msg.bytes().take(255).enumerate() { buf[i] = b as i8; }
    clif_sendminitext(sd, buf.as_ptr());
    0
}
unsafe fn command_spell(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    if let Some((spell, sound)) = parse_two_ints(line) {
        SPELLGFX.store(spell, Ordering::Relaxed);
        SOUNDFX.store(sound, Ordering::Relaxed);
        clif_playsound(&mut (*sd).bl, sound);
    }
    let sd_bl = &mut (*sd).bl as *mut BlockList;
    let anim = SPELLGFX.load(Ordering::Relaxed);
    let times = SOUNDFX.load(Ordering::Relaxed);
    foreach_in_area(
        (*sd).bl.m as i32, (*sd).bl.x as i32, (*sd).bl.y as i32,
        AreaType::Area, BL_PC,
        |target_bl| clif_sendanimation_inner(target_bl, anim, sd_bl, times),
    );
    0
}
unsafe fn command_val(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let count = (MOB_SPAWN_MAX.load(Ordering::Relaxed) - MOB_SPAWN_START.load(Ordering::Relaxed))
              + (MOB_ONETIME_MAX.load(Ordering::Relaxed) - MOB_ONETIME_START.load(Ordering::Relaxed));
    let mut buf = [0i8; 255];
    let msg = format!("Mob spawn count: {}\0", count);
    for (i, b) in msg.bytes().take(254).enumerate() { buf[i] = b as i8; }
    clif_sendminitext(sd, buf.as_ptr());
    0
}
unsafe fn command_disguise(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
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
unsafe fn command_warp(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let (m, x, y) = match parse_three_ints(line) { Some(v) => v, None => return -1 };
    pc_warp(sd, m, x, y);
    0
}
unsafe fn command_givespell(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
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
unsafe fn command_side(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let side = match parse_int(line) { Some(v) => v, None => return -1 };
    (*sd).status.side = side as i8;
    clif_sendchararea(sd);
    clif_getchararea(sd);
    0
}
unsafe fn command_state(sd: *mut MapSessionData, line: *mut i8, _lua_state: *mut LuaState) -> i32 {
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
unsafe fn command_armorcolor(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let ac = match parse_int(line) { Some(v) => v, None => return -1 };
    (*sd).status.armor_color = ac as u16;
    clif_sendchararea(sd);
    clif_getchararea(sd);
    0
}
unsafe fn command_makegm(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let name = match parse_str32(line) { Some(v) => v, None => return -1 };
    let tsd = map_name2sd(name.as_ptr());
    if !tsd.is_null() {
        (*tsd).status.gm_level = 99;
    }
    0
}
unsafe fn command_who(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let mut buf = [0i8; 256];
    let msg = format!("There are {} users online.\0", userlist.user_count);
    for (i, b) in msg.bytes().take(255).enumerate() { buf[i] = b as i8; }
    clif_sendminitext(sd, buf.as_ptr());
    0
}
unsafe fn command_legend(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    (*sd).status.legends[0].icon = 12;
    (*sd).status.legends[0].color = 128;
    let text = b"Blessed by a GM\0";
    for (i, &b) in text.iter().enumerate() {
        (*sd).status.legends[0].text[i] = b as i8;
    }
    0
}
unsafe fn command_luareload(sd: *mut MapSessionData, _line: *mut i8, s: *mut LuaState) -> i32 {
    let errors = sl_reload();
    if sd.is_null() { return errors; }
    clif_sendminitext(sd, b"LUA Scripts reloaded!\0".as_ptr() as *const i8);
    errors
}
unsafe fn command_magicreload(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    magicdb_read();
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"Magic DB reloaded!\0".as_ptr() as *const i8);
    0
}
unsafe fn command_lua(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() || line.is_null() { return 0; }
    (*sd).luaexec = 0;
    sl_doscript_simple(b"canRunLuaTalk\0".as_ptr() as *const i8, std::ptr::null(), &mut (*sd).bl as *mut BlockList);
    if (*sd).luaexec != 0 {
        sl_exec(sd as *mut std::ffi::c_void, line);
    }
    0
}
unsafe fn command_speed(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let d = match parse_int(line) { Some(v) => v, None => return -1 };
    (*sd).speed = d;
    clif_sendchararea(sd);
    clif_getchararea(sd);
    0
}
unsafe fn command_reloaditem(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    itemdb_read();
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"Item DB Reloaded!\0".as_ptr() as *const i8);
    0
}
unsafe fn command_reloadcreations(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    // Creation system is Lua-script-driven; no DB layer to reload.
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"Creations DB reloaded!\0".as_ptr() as *const i8);
    0
}
unsafe fn command_reloadmob(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    rust_mobdb_term();
    rust_mobdb_init();
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"Mob DB Reloaded\0".as_ptr() as *const i8);
    0
}
unsafe fn command_reloadspawn(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    // mobspawn_read is now async; fire-and-forget from LocalSet.
    // The reload message is sent immediately; the actual reload completes shortly after.
    tokio::task::spawn_local(async move { mobspawn_read().await });
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"Spawn DB Reloaded\0".as_ptr() as *const i8);
    0
}
unsafe fn command_pvp(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let pvp = match parse_int(line) { Some(v) => v, None => return -1 };
    let mp = crate::database::map_db::get_map_ptr((*sd).bl.m);
    if !mp.is_null() { (*mp).pvp = pvp as u8; }
    let mut buf = [0i8; 64];
    let msg = format!("PvP set to: {}\0", pvp);
    for (i, b) in msg.bytes().take(63).enumerate() { buf[i] = b as i8; }
    clif_sendminitext(sd, buf.as_ptr());
    0
}
unsafe fn command_spellwork(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let mp = crate::database::map_db::get_map_ptr((*sd).bl.m);
    if !mp.is_null() { (*mp).spell ^= 1; }
    0
}
unsafe fn command_broadcast(_sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    clif_broadcast(line, -1);
    0
}
unsafe fn command_luasize(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if !sd.is_null() { sl_luasize(sd as *mut std::ffi::c_void); }
    0
}
unsafe fn command_luafix(_sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    sl_fixmem();
    0
}
unsafe fn command_respawn(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    map_respawnmobs(|bl| {
        if bl.is_null() { return 0; }
        let mob = bl as *mut MobSpawnData;
        if (*mob).state == MOB_DEAD && (*mob).onetime == 0 {
            mob_respawn(mob);
        }
        0
    }, (*sd).bl.m as i32, BL_MOB);
    0
}
async fn ban_character(name_str: String) {
    sqlx::query("UPDATE `Character` SET `ChaBanned` = '1' WHERE `ChaName` = ?")
        .bind(name_str)
        .execute(get_pool())
        .await
        .ok();
}
async fn unban_character(name_str: String) {
    sqlx::query("UPDATE `Character` SET `ChaBanned` = '0' WHERE `ChaName` = ?")
        .bind(name_str)
        .execute(get_pool())
        .await
        .ok();
}
unsafe fn command_ban(_sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    let name = match parse_str32(line) { Some(v) => v, None => return -1 };
    let tsd = map_name2sd(name.as_ptr());
    if !tsd.is_null() {
        printf(b"Banning %s\n\0".as_ptr() as *const i8, name.as_ptr());
        let name_str = std::ffi::CStr::from_ptr(name.as_ptr())
            .to_str().unwrap_or("").to_owned();
        // Fire-and-forget: DB write is independent of the disconnect below.
        tokio::task::spawn_local(ban_character(name_str));
        rust_session_set_eof((*tsd).fd, 1);
    }
    0
}
unsafe fn command_unban(_sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    let name = match parse_str32(line) { Some(v) => v, None => return -1 };
    printf(b"Unbanning %s\n\0".as_ptr() as *const i8, name.as_ptr());
    let name_str = std::ffi::CStr::from_ptr(name.as_ptr())
        .to_str().unwrap_or("").to_owned();
    // Fire-and-forget: DB write does not affect this command's return value.
    tokio::task::spawn_local(unban_character(name_str));
    0
}
unsafe fn command_kc(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
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
unsafe fn command_blockcount    (_sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 { 0 }
unsafe fn command_stealth(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    if (*sd).optFlags & OPT_STEALTH != 0 {
        (*sd).optFlags ^= OPT_STEALTH;
        clif_refresh(sd);
        clif_sendminitext(sd, b"Stealth :OFF\0".as_ptr() as *const i8);
    } else {
        clif_lookgone(&mut (*sd).bl);
        (*sd).optFlags ^= OPT_STEALTH;
        clif_refresh(sd);
        clif_sendminitext(sd, b"Stealth :ON\0".as_ptr() as *const i8);
    }
    0
}
unsafe fn command_ghosts(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    (*sd).optFlags ^= OPT_GHOSTS;
    clif_refresh(sd);
    if (*sd).optFlags & OPT_GHOSTS != 0 {
        clif_sendminitext(sd, b"Ghosts :ON\0".as_ptr() as *const i8);
    } else {
        clif_sendminitext(sd, b"Ghosts :OFF\0".as_ptr() as *const i8);
    }
    0
}
unsafe fn command_unphysical(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    (*sd).uFlags ^= UFLAG_UNPHYS;
    if (*sd).uFlags & UFLAG_UNPHYS != 0 {
        clif_sendminitext(sd, b"Unphysical :ON\0".as_ptr() as *const i8);
    } else {
        clif_sendminitext(sd, b"Unphysical :OFF\0".as_ptr() as *const i8);
    }
    0
}
unsafe fn command_immortality(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    (*sd).uFlags ^= UFLAG_IMMORTAL;
    if (*sd).uFlags & UFLAG_IMMORTAL != 0 {
        clif_sendminitext(sd, b"Immortality :ON\0".as_ptr() as *const i8);
    } else {
        clif_sendminitext(sd, b"Immortality :OFF\0".as_ptr() as *const i8);
    }
    0
}
unsafe fn command_silence(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let name = match parse_str32(line) { Some(v) => v, None => return -1 };
    let tsd = map_name2sd(name.as_ptr());
    if !tsd.is_null() {
        (*tsd).uFlags ^= UFLAG_SILENCED;
        if (*tsd).uFlags & UFLAG_SILENCED != 0 {
            clif_sendminitext(sd, b"Silenced.\0".as_ptr() as *const i8);
            clif_sendminitext(tsd, b"You have been silenced.\0".as_ptr() as *const i8);
        } else {
            clif_sendminitext(sd, b"Unsilenced.\0".as_ptr() as *const i8);
            clif_sendminitext(tsd, b"Silence lifted.\0".as_ptr() as *const i8);
        }
    } else {
        clif_sendminitext(sd, b"User not on.\0".as_ptr() as *const i8);
    }
    0
}
unsafe fn command_shutdowncancel(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    let dt = DOWNTIMER.load(Ordering::Relaxed);
    if dt != 0 {
        clif_broadcast(b"---------------------------------------------------\0".as_ptr() as *const i8, -1);
        clif_broadcast(b"Server shutdown cancelled.\0".as_ptr() as *const i8, -1);
        clif_broadcast(b"---------------------------------------------------\0".as_ptr() as *const i8, -1);
        timer_remove(dt);
        DOWNTIMER.store(0, Ordering::Relaxed);
    } else if !sd.is_null() {
        clif_sendminitext(sd, b"Server is not shutting down.\0".as_ptr() as *const i8);
    }
    0
}
unsafe fn command_shutdown(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() || line.is_null() { return 0; }
    if DOWNTIMER.load(Ordering::Relaxed) != 0 {
        clif_sendminitext(sd, b"Server is already shutting down.\0".as_ptr() as *const i8);
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
    clif_broadcast(b"---------------------------------------------------\0".as_ptr() as *const i8, -1);
    clif_broadcast(msg_buf.as_ptr(), -1);
    clif_broadcast(b"---------------------------------------------------\0".as_ptr() as *const i8, -1);
    DOWNTIMER.store(timer_insert(250, 250, Some(map_reset_timer), t_time as i32, 250), Ordering::Relaxed);
    0
}
unsafe fn command_weap(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let (id, color) = match parse_two_ints(line) { Some(v) => v, None => return -1 };
    (*sd).gfx.weapon = id as u16;
    (*sd).gfx.cweapon = color as u8;
    clif_getchararea(sd);
    clif_sendchararea(sd);
    0
}
unsafe fn command_shield(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let (id, color) = match parse_two_ints(line) { Some(v) => v, None => return -1 };
    (*sd).gfx.shield = id as u16;
    (*sd).gfx.cshield = color as u8;
    clif_getchararea(sd);
    clif_sendchararea(sd);
    0
}
unsafe fn command_armor(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let (id, color) = match parse_two_ints(line) { Some(v) => v, None => return -1 };
    (*sd).gfx.armor = id as u16;
    (*sd).gfx.carmor = color as u8;
    clif_getchararea(sd);
    clif_sendchararea(sd);
    0
}
unsafe fn command_boots(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let (id, color) = match parse_two_ints(line) { Some(v) => v, None => return -1 };
    (*sd).gfx.boots = id as u16;
    (*sd).gfx.cboots = color as u8;
    clif_getchararea(sd);
    clif_sendchararea(sd);
    0
}
unsafe fn command_mantle(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let (id, color) = match parse_two_ints(line) { Some(v) => v, None => return -1 };
    (*sd).gfx.mantle = id as u16;
    (*sd).gfx.cmantle = color as u8;
    clif_getchararea(sd);
    clif_sendchararea(sd);
    0
}
unsafe fn command_necklace(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let (id, color) = match parse_two_ints(line) { Some(v) => v, None => return -1 };
    (*sd).gfx.necklace = id as u16;
    (*sd).gfx.cnecklace = color as u8;
    clif_getchararea(sd);
    clif_sendchararea(sd);
    0
}
unsafe fn command_faceacc(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let (id, color) = match parse_two_ints(line) { Some(v) => v, None => return -1 };
    (*sd).gfx.face_acc = id as u16;
    (*sd).gfx.cface_acc = color as u8;
    clif_getchararea(sd);
    clif_sendchararea(sd);
    0
}
unsafe fn command_crown(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let (id, color) = match parse_two_ints(line) { Some(v) => v, None => return -1 };
    (*sd).gfx.crown = id as u16;
    (*sd).gfx.ccrown = color as u8;
    clif_getchararea(sd);
    clif_sendchararea(sd);
    0
}
unsafe fn command_helm(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let (id, color) = match parse_two_ints(line) { Some(v) => v, None => return -1 };
    (*sd).gfx.helm = id as u16;
    (*sd).gfx.chelm = color as u8;
    clif_getchararea(sd);
    clif_sendchararea(sd);
    0
}
unsafe fn command_gfxtoggle(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    (*sd).gfx.toggle ^= 1;
    0
}
unsafe fn command_weather(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let weather = parse_int(line).unwrap_or(5);
    let mp = crate::database::map_db::get_map_ptr((*sd).bl.m);
    if !mp.is_null() { (*mp).weather = weather as u8; }
    for x in 1..crate::session::get_fd_max() {
        if rust_session_exists(x) != 0 {
            let tmpsd = rust_session_get_data(x) as *mut MapSessionData;
            if !tmpsd.is_null() && rust_session_get_eof(x) == 0 && (*tmpsd).bl.m == (*sd).bl.m {
                clif_sendweather(tmpsd);
            }
        }
    }
    0
}
unsafe fn command_light(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let light = parse_int(line).unwrap_or(232);
    let mp = crate::database::map_db::get_map_ptr((*sd).bl.m);
    if !mp.is_null() { (*mp).light = light as u8; }
    for x in 0..crate::session::get_fd_max() {
        if rust_session_exists(x) != 0 {
            let tmpsd = rust_session_get_data(x) as *mut MapSessionData;
            if !tmpsd.is_null() && rust_session_get_eof(x) == 0 && (*tmpsd).bl.m == (*sd).bl.m {
                pc_warp(tmpsd, (*tmpsd).bl.m as i32, (*tmpsd).bl.x as i32, (*tmpsd).bl.y as i32);
            }
        }
    }
    0
}
unsafe fn command_gm(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let name_ptr = (*sd).status.name.as_ptr();
    let line_str = std::ffi::CStr::from_ptr(line).to_str().unwrap_or("");
    let name_str = std::ffi::CStr::from_ptr(name_ptr).to_str().unwrap_or("");
    let mut buf = [0i8; 65535];
    let msg = format!("<GM>{}: {}\0", name_str, line_str);
    for (i, b) in msg.bytes().take(65534).enumerate() { buf[i] = b as i8; }
    for x in 1..crate::session::get_fd_max() {
        if rust_session_exists(x) != 0 {
            let tsd = rust_session_get_data(x) as *mut MapSessionData;
            if !tsd.is_null() && rust_session_get_eof(x) == 0 && (*tsd).status.gm_level != 0 {
                clif_sendmsg(tsd, 11, buf.as_ptr());
            }
        }
    }
    0
}
unsafe fn command_report(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let name_ptr = (*sd).status.name.as_ptr();
    let line_str = std::ffi::CStr::from_ptr(line).to_str().unwrap_or("");
    let name_str = std::ffi::CStr::from_ptr(name_ptr).to_str().unwrap_or("");
    let mut buf = [0i8; 65535];
    let msg = format!("<REPORT>{}: {}\0", name_str, line_str);
    for (i, b) in msg.bytes().take(65534).enumerate() { buf[i] = b as i8; }
    for x in 1..crate::session::get_fd_max() {
        if rust_session_exists(x) != 0 {
            let tsd = rust_session_get_data(x) as *mut MapSessionData;
            if !tsd.is_null() && rust_session_get_eof(x) == 0 && (*tsd).status.gm_level > 0 {
                clif_sendmsg(tsd, 12, buf.as_ptr());
            }
        }
    }
    0
}
unsafe fn command_url(_sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if line.is_null() { return 0; }
    let line_str = std::ffi::CStr::from_ptr(line).to_str().unwrap_or("");
    let mut parts = line_str.trim().splitn(4, char::is_whitespace).filter(|s| !s.is_empty());
    let name_s = match parts.next() { Some(v) => v, None => return -1 };
    let url_type: i32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
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
unsafe fn command_cinv(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let range = parse_two_ints(line);
    let (start, end) = match range {
        Some((s, e)) => (s, e),
        None => (0, 51),
    };
    for x in start..=end {
        let x = x as usize;
        if x < 52 && (*sd).status.inventory[x].id > 0 && (*sd).status.inventory[x].amount > 0 {
            pc_delitem(sd, x as i32, (*sd).status.inventory[x].amount, 0);
        }
    }
    0
}
unsafe fn command_cfloor        (_sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 { 0 }
unsafe fn command_cspells(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
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
async fn set_job_class(class_val: u32, mark_val: u32, char_id: u32) {
    sqlx::query(
        "UPDATE `Character` SET `ChaPthId` = ?, `ChaMark` = ? WHERE `ChaId` = ?"
    )
    .bind(class_val)
    .bind(mark_val)
    .bind(char_id)
    .execute(get_pool())
    .await
    .ok();
}
unsafe fn command_job(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let (mut job, mut subjob) = parse_two_ints(line).unwrap_or((0, 0));
    if job < 0 { job = 5; }
    if subjob < 0 || subjob > 16 { subjob = 0; }
    (*sd).status.class = job as u8;
    (*sd).status.mark = subjob as u8;
    let class_val = (*sd).status.class as u32;
    let mark_val = (*sd).status.mark as u32;
    let char_id = (*sd).status.id;
    // Block until the DB write completes, then send the status update.
    // `blocking_run_async` joins the OS thread before returning, so `sd` cannot
    // be freed while `clif_mystaytus_by_addr` holds its `usize` pointer.
    // Do NOT use `spawn_local` here — that is fire-and-forget and allows the
    // session to be freed before the future runs (dangling pointer UB).
    crate::database::blocking_run_async(set_job_class(class_val, mark_val, char_id));
    let sd_usize = sd as usize;
    crate::database::blocking_run_async(clif_mystaytus_by_addr(sd_usize));
    0
}
unsafe fn command_music(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    if let Some(music) = parse_int(line) { MUSICFX.store(music, Ordering::Relaxed); }
    let oldm = (*sd).bl.m as i32;
    let oldx = (*sd).bl.x as i32;
    let oldy = (*sd).bl.y as i32;
    let mp = crate::database::map_db::get_map_ptr((*sd).bl.m);
    if !mp.is_null() { (*mp).bgm = MUSICFX.load(Ordering::Relaxed) as u16; }
    pc_warp(sd, 10002, 0, 0);
    pc_warp(sd, oldm, oldx, oldy);
    0
}
unsafe fn command_musicn(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    MUSICFX.fetch_add(1, Ordering::Relaxed);
    let oldm = (*sd).bl.m as i32;
    let oldx = (*sd).bl.x as i32;
    let oldy = (*sd).bl.y as i32;
    let mp = crate::database::map_db::get_map_ptr((*sd).bl.m);
    if !mp.is_null() { (*mp).bgm = MUSICFX.load(Ordering::Relaxed) as u16; }
    pc_warp(sd, 10002, 0, 0);
    pc_warp(sd, oldm, oldx, oldy);
    0
}
unsafe fn command_musicp(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    MUSICFX.fetch_sub(1, Ordering::Relaxed);
    let oldm = (*sd).bl.m as i32;
    let oldx = (*sd).bl.x as i32;
    let oldy = (*sd).bl.y as i32;
    let mp = crate::database::map_db::get_map_ptr((*sd).bl.m);
    if !mp.is_null() { (*mp).bgm = MUSICFX.load(Ordering::Relaxed) as u16; }
    pc_warp(sd, 10002, 0, 0);
    pc_warp(sd, oldm, oldx, oldy);
    0
}
unsafe fn command_musicq(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let mut buf = [0i8; 25];
    let msg = format!("Current music is: {}\0", MUSICFX.load(Ordering::Relaxed));
    for (i, b) in msg.bytes().take(24).enumerate() { buf[i] = b as i8; }
    clif_sendminitext(sd, buf.as_ptr());
    0
}
unsafe fn command_sound(sd: *mut MapSessionData, line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    if let Some(sound) = parse_int(line) { SOUNDFX.store(sound, Ordering::Relaxed); }
    clif_playsound(&mut (*sd).bl, SOUNDFX.load(Ordering::Relaxed));
    0
}
unsafe fn command_nsound(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let s = (SOUNDFX.fetch_add(1, Ordering::Relaxed) + 1).min(125);
    SOUNDFX.store(s, Ordering::Relaxed);
    clif_playsound(&mut (*sd).bl, s);
    0
}
unsafe fn command_psound(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let s = (SOUNDFX.fetch_sub(1, Ordering::Relaxed) - 1).max(0);
    SOUNDFX.store(s, Ordering::Relaxed);
    clif_playsound(&mut (*sd).bl, s);
    0
}
unsafe fn command_soundq(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let mut buf = [0i8; 25];
    let msg = format!("Current sound is: {}\0", SOUNDFX.load(Ordering::Relaxed));
    for (i, b) in msg.bytes().take(24).enumerate() { buf[i] = b as i8; }
    clif_sendminitext(sd, buf.as_ptr());
    0
}
unsafe fn command_nspell(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let g = (SPELLGFX.fetch_add(1, Ordering::Relaxed) + 1).min(427);
    SPELLGFX.store(g, Ordering::Relaxed);
    let sd_bl = &mut (*sd).bl as *mut BlockList;
    let anim = g;
    let times = SOUNDFX.load(Ordering::Relaxed);
    foreach_in_area(
        (*sd).bl.m as i32, (*sd).bl.x as i32, (*sd).bl.y as i32,
        AreaType::Area, BL_PC,
        |target_bl| clif_sendanimation_inner(target_bl, anim, sd_bl, times),
    );
    0
}
unsafe fn command_pspell(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let g = (SPELLGFX.fetch_sub(1, Ordering::Relaxed) - 1).max(0);
    SPELLGFX.store(g, Ordering::Relaxed);
    let sd_bl = &mut (*sd).bl as *mut BlockList;
    let anim = g;
    let times = SOUNDFX.load(Ordering::Relaxed);
    foreach_in_area(
        (*sd).bl.m as i32, (*sd).bl.x as i32, (*sd).bl.y as i32,
        AreaType::Area, BL_PC,
        |target_bl| clif_sendanimation_inner(target_bl, anim, sd_bl, times),
    );
    0
}
unsafe fn command_spellq(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    let mut buf = [0i8; 25];
    let msg = format!("Current Spell is: {}\0", SPELLGFX.load(Ordering::Relaxed));
    for (i, b) in msg.bytes().take(24).enumerate() { buf[i] = b as i8; }
    clif_sendminitext(sd, buf.as_ptr());
    0
}
unsafe fn command_reloadboard(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    boarddb_term();
    boarddb_init();
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"Board DB reloaded!\0".as_ptr() as *const i8);
    0
}
unsafe fn command_reloadclan(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    clandb_init();
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"Clan DB reloaded!\0".as_ptr() as *const i8);
    0
}
unsafe fn command_reloadnpc(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    npc_init();
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"NPC DB reloaded!\0".as_ptr() as *const i8);
    0
}
unsafe fn command_reloadmaps(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    map_reload();
    let cfd = char_fd.load(Ordering::Relaxed);
    if cfd > 0 && rust_session_exists(cfd) != 0 {
        use crate::session::{rust_session_wdata_ptr, rust_session_commit, rust_session_wfifohead};
        let map_n_val = map_n.load(Ordering::Relaxed);
        let pkt_len = (map_n_val * 2 + 8) as usize;
        rust_session_wfifohead(cfd, pkt_len);
        (rust_session_wdata_ptr(cfd, 0) as *mut u16).write_unaligned(0x3001u16.to_le());
        (rust_session_wdata_ptr(cfd, 2) as *mut u32).write_unaligned(pkt_len as u32);
        (rust_session_wdata_ptr(cfd, 6) as *mut u16).write_unaligned(map_n_val as u16);
        let mut j: usize = 0;
        for i in 0..MAX_MAP_PER_SERVER {
            let mp = crate::database::map_db::get_map_ptr(i as u16);
            if !mp.is_null() && !(*mp).tile.is_null() {
                (rust_session_wdata_ptr(cfd, j * 2 + 8) as *mut u16).write_unaligned(i as u16);
                j += 1;
            }
            if j >= map_n_val as usize { break; }
        }
        rust_session_commit(cfd, pkt_len);
    }
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"Maps reloaded!\0".as_ptr() as *const i8);
    0
}
unsafe fn command_reloadclass(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    classdb_read();
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"Classes reloaded!\0".as_ptr() as *const i8);
    0
}
unsafe fn command_reloadlevels(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    leveldb_read();
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"Levels reloaded!\0".as_ptr() as *const i8);
    0
}
unsafe fn command_reloadwarps(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    warp_init();
    if sd.is_null() { return 0; }
    clif_sendminitext(sd, b"Warps reloaded!\0".as_ptr() as *const i8);
    0
}
unsafe fn command_transfer(sd: *mut MapSessionData, _line: *mut i8, _s: *mut LuaState) -> i32 {
    if sd.is_null() { return 0; }
    clif_transfer_test(sd, 1, 10, 10);
    0
}

// ─── rust_command_reload: exported entry point for full mini-reset ────────────

pub unsafe fn rust_command_reload(
    sd: *mut MapSessionData, line: *mut i8, state: *mut LuaState,
) -> i32 {
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
        clif_sendminitext(sd, b"Mini reset complete!\0".as_ptr() as *const i8);
    }
    errors
}

// ─── Parse helpers (replaces sscanf) ─────────────────────────────────────────

unsafe fn parse_int(line: *mut i8) -> Option<i32> {
    if line.is_null() { return None; }
    let s = std::ffi::CStr::from_ptr(line).to_str().ok()?;
    s.trim().splitn(2, char::is_whitespace).next()?.parse().ok()
}

unsafe fn parse_two_ints(line: *mut i8) -> Option<(i32, i32)> {
    if line.is_null() { return None; }
    let s = std::ffi::CStr::from_ptr(line).to_str().ok()?;
    let mut p = s.trim().splitn(3, char::is_whitespace).filter(|p| !p.is_empty());
    let a = p.next()?.parse().ok()?;
    let b = p.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    Some((a, b))
}

unsafe fn parse_three_ints(line: *mut i8) -> Option<(i32, i32, i32)> {
    if line.is_null() { return None; }
    let s = std::ffi::CStr::from_ptr(line).to_str().ok()?;
    let mut p = s.trim().splitn(4, char::is_whitespace).filter(|x| !x.is_empty());
    Some((p.next()?.parse().ok()?, p.next()?.parse().ok()?, p.next()?.parse().ok()?))
}

unsafe fn parse_str32(line: *mut i8) -> Option<[i8; 32]> {
    if line.is_null() { return None; }
    let s = std::ffi::CStr::from_ptr(line).to_str().ok()?;
    let word = s.trim().splitn(2, char::is_whitespace).next()?;
    let mut buf = [0i8; 32];
    for (i, b) in word.bytes().take(31).enumerate() { buf[i] = b as i8; }
    Some(buf)
}

// ─── Command dispatcher ───────────────────────────────────────────────────────

unsafe fn dispatch(sd: *mut MapSessionData, p: *const i8, len: i32, log: bool) -> i32 {
    if *p != COMMAND_CODE.load(Ordering::Relaxed) { return 0; }
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

    if ((*sd).status.gm_level as i32) < entry.level { return 0; }

    // Skip past the null byte we inserted, then past whitespace.
    // Clamp to copy_len so we never step past the buffer when end == copy_len.
    let args_offset = (end + 1).min(copy_len);
    let mut args_ptr = p.add(args_offset);
    while *args_ptr == b' ' as i8 || *args_ptr == b'\t' as i8 {
        args_ptr = args_ptr.add(1);
    }

    if log {
        printf(b"[command] gm command used cmd=%s\n\0".as_ptr() as *const i8,
               cmd_line.as_ptr());
    }

    (entry.func)(sd, args_ptr as *mut i8, std::ptr::null_mut());
    1 // command matched and executed — caller checks bool, not handler result
}

pub unsafe fn rust_is_command(sd: *mut MapSessionData, p: *const i8, len: i32) -> i32 {
    dispatch(sd, p, len, true)
}

pub unsafe fn rust_at_command(sd: *mut MapSessionData, p: *const i8, len: i32) -> i32 {
    dispatch(sd, p, len, false)
}
