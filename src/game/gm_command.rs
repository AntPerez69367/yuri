//! GM command dispatch.
use std::ffi::CString;
use std::sync::atomic::{AtomicI32, AtomicI8, Ordering};

use crate::common::traits::LegacyEntity;

use crate::game::mob::{MobSpawnData, BL_PC, MOB_DEAD};
use crate::game::pc::{MapSessionData, PC_DIE, SFLAG_FULLSTATS, SFLAG_HPMP};

// Module globals
static SPELLGFX: AtomicI32 = AtomicI32::new(0);
static MUSICFX: AtomicI32 = AtomicI32::new(0);
static SOUNDFX: AtomicI32 = AtomicI32::new(0);
static DOWNTIMER: AtomicI32 = AtomicI32::new(0);
static COMMAND_CODE: AtomicI8 = AtomicI8::new(b'/' as i8);

use crate::common::constants::entity::player::{
    OPT_FLAG_GHOSTS, OPT_FLAG_STEALTH, U_FLAG_IMMORTAL, U_FLAG_SILENCED, U_FLAG_UNPHYSICAL,
};

use crate::config_globals::{D_RATE, XP_RATE};
use crate::game::mob::{MOB_ONETIME_MAX, MOB_ONETIME_START, MOB_SPAWN_MAX, MOB_SPAWN_START};

use crate::database::get_pool;
use crate::game::map_server::{map_id2sd_pc, userlist};
use crate::game::pc::PlayerEntity;

// ── map functions ──────────────────────────────────────────────────────────────
use crate::game::block::AreaType;
use crate::game::block_grid;
use crate::game::map_server::{map_name2sd, map_reload, map_reset_timer};

// ── clif functions ─────────────────────────────────────────────────────────────
use crate::game::client::handlers::clif_transfer_test;
use crate::game::client::visual::{broadcast_update_state, clif_sendurl, clif_sendweather};
use crate::game::map_parse::chat::{
    clif_broadcast, clif_playsound_entity, clif_sendminitext, clif_sendmsg,
};
use crate::game::map_parse::combat::clif_sendanimation_inner;
use crate::game::map_parse::player_state::{clif_mystatus, clif_refresh, clif_sendstatus};
use crate::game::map_parse::visual::{clif_lookgone_by_id, refresh_appearance};

// ── pc functions ───────────────────────────────────────────────────────────────
use crate::game::pc::{pc_additem, pc_delitem, pc_loadmagic, pc_readglobalreg, pc_res, pc_warp};

// ── mob functions ──────────────────────────────────────────────────────────────
use crate::game::mob::mob_respawn;

// ── scripting functions ────────────────────────────────────────────────────────
use crate::game::scripting::{sl_exec, sl_fixmem, sl_luasize, sl_reload};

// ── database init functions ────────────────────────────────────────────────────
use crate::database::item_db;
use crate::database::{board_db, clan_db, magic_db, mob_db};
use crate::game::mob::mobspawn_read;
use crate::game::npc::{npc_init, warp_init};

// ── session helpers ────────────────────────────────────────────────────────────
use crate::session::session_set_eof;

// ── encrypt ────────────────────────────────────────────────────────────────────
use crate::network::crypt::encrypt as encrypt_fd;

// ── timer ──────────────────────────────────────────────────────────────────────
use crate::game::time_util::{timer_insert, timer_remove};

// ── Helpers ────────────────────────────────────────────────────────────────────

#[inline]
fn as_ptr(sd: &mut MapSessionData) -> *mut MapSessionData {
    sd as *mut MapSessionData
}

/// Look up the `PlayerEntity` for a `MapSessionData` by ID.
#[inline]
fn pe_of(sd: &MapSessionData) -> Option<std::sync::Arc<PlayerEntity>> {
    map_id2sd_pc(sd.id)
}

fn str_to_cname(s: &str) -> [i8; 32] {
    let mut buf = [0i8; 32];
    for (i, b) in s.bytes().take(31).enumerate() {
        buf[i] = b as i8;
    }
    buf
}

fn carray_to_str(arr: &[i8]) -> &str {
    let bytes = unsafe { &*(arr as *const [i8] as *const [u8]) };
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end]).unwrap_or("")
}

fn send_minitext(sd: &mut MapSessionData, msg: &str) {
    if let Some(pe) = pe_of(sd) {
        if let Ok(cs) = CString::new(msg) {
            unsafe {
                clif_sendminitext(&pe, cs.as_ptr());
            }
        }
    }
}

// ── Parse helpers ──────────────────────────────────────────────────────────────

fn parse_int(line: &str) -> Option<i32> {
    line.split_whitespace().next()?.parse().ok()
}

fn parse_two_ints(line: &str) -> Option<(i32, i32)> {
    let mut p = line.split_whitespace();
    let a = p.next()?.parse().ok()?;
    let b = p.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    Some((a, b))
}

fn parse_three_ints(line: &str) -> Option<(i32, i32, i32)> {
    let mut p = line.split_whitespace();
    Some((
        p.next()?.parse().ok()?,
        p.next()?.parse().ok()?,
        p.next()?.parse().ok()?,
    ))
}

fn parse_first_word(line: &str) -> &str {
    line.split_whitespace().next().unwrap_or("")
}

// ── Command table ──────────────────────────────────────────────────────────────

type CmdFn = fn(&mut MapSessionData, &str) -> i32;

struct CommandEntry {
    func: CmdFn,
    name: &'static str,
    level: i32,
}

static COMMANDS: &[CommandEntry] = &[
    CommandEntry {
        func: command_debug,
        name: "debug",
        level: 99,
    },
    CommandEntry {
        func: command_item,
        name: "item",
        level: 50,
    },
    CommandEntry {
        func: command_res,
        name: "res",
        level: 99,
    },
    CommandEntry {
        func: command_hair,
        name: "hair",
        level: 99,
    },
    CommandEntry {
        func: command_checkdupes,
        name: "checkdupes",
        level: 99,
    },
    CommandEntry {
        func: command_checkwpe,
        name: "checkwpe",
        level: 99,
    },
    CommandEntry {
        func: command_kill,
        name: "kill",
        level: 99,
    },
    CommandEntry {
        func: command_killall,
        name: "killall",
        level: 99,
    },
    CommandEntry {
        func: command_deletespell,
        name: "deletespell",
        level: 99,
    },
    CommandEntry {
        func: command_xprate,
        name: "xprate",
        level: 99,
    },
    CommandEntry {
        func: command_heal,
        name: "heal",
        level: 99,
    },
    CommandEntry {
        func: command_level,
        name: "level",
        level: 99,
    },
    CommandEntry {
        func: command_randomspawn,
        name: "randomspawn17",
        level: 99,
    },
    CommandEntry {
        func: command_drate,
        name: "droprate",
        level: 99,
    },
    CommandEntry {
        func: command_spell,
        name: "spell",
        level: 99,
    },
    CommandEntry {
        func: command_val,
        name: "val",
        level: 99,
    },
    CommandEntry {
        func: command_disguise,
        name: "disguise",
        level: 99,
    },
    CommandEntry {
        func: command_warp,
        name: "warp",
        level: 10,
    },
    CommandEntry {
        func: command_givespell,
        name: "givespell",
        level: 50,
    },
    CommandEntry {
        func: command_side,
        name: "side",
        level: 99,
    },
    CommandEntry {
        func: command_state,
        name: "state",
        level: 20,
    },
    CommandEntry {
        func: command_armorcolor,
        name: "armorc",
        level: 99,
    },
    CommandEntry {
        func: command_makegm,
        name: "makegm",
        level: 99,
    },
    CommandEntry {
        func: command_who,
        name: "who",
        level: 99,
    },
    CommandEntry {
        func: command_legend,
        name: "legend",
        level: 99,
    },
    CommandEntry {
        func: command_luareload,
        name: "reloadlua",
        level: 99,
    },
    CommandEntry {
        func: command_luareload,
        name: "rl",
        level: 99,
    },
    CommandEntry {
        func: command_magicreload,
        name: "reloadmagic",
        level: 99,
    },
    CommandEntry {
        func: command_lua,
        name: "lua",
        level: 0,
    },
    CommandEntry {
        func: command_speed,
        name: "speed",
        level: 10,
    },
    CommandEntry {
        func: command_reloaditem,
        name: "reloaditem",
        level: 99,
    },
    CommandEntry {
        func: command_reloadcreations,
        name: "reloadcreations",
        level: 99,
    },
    CommandEntry {
        func: command_reloadmob,
        name: "reloadmob",
        level: 99,
    },
    CommandEntry {
        func: command_reloadspawn,
        name: "reloadspawn",
        level: 99,
    },
    CommandEntry {
        func: command_pvp,
        name: "pvp",
        level: 20,
    },
    CommandEntry {
        func: command_spellwork,
        name: "spellwork",
        level: 99,
    },
    CommandEntry {
        func: command_broadcast,
        name: "bc",
        level: 50,
    },
    CommandEntry {
        func: command_luasize,
        name: "luasize",
        level: 99,
    },
    CommandEntry {
        func: command_luafix,
        name: "luafix",
        level: 99,
    },
    CommandEntry {
        func: command_respawn,
        name: "respawn",
        level: 99,
    },
    CommandEntry {
        func: command_ban,
        name: "ban",
        level: 99,
    },
    CommandEntry {
        func: command_unban,
        name: "unban",
        level: 99,
    },
    CommandEntry {
        func: command_kc,
        name: "kc",
        level: 99,
    },
    CommandEntry {
        func: command_blockcount,
        name: "blockc",
        level: 99,
    },
    CommandEntry {
        func: command_stealth,
        name: "stealth",
        level: 1,
    },
    CommandEntry {
        func: command_ghosts,
        name: "ghosts",
        level: 1,
    },
    CommandEntry {
        func: command_unphysical,
        name: "unphysical",
        level: 99,
    },
    CommandEntry {
        func: command_immortality,
        name: "immortality",
        level: 99,
    },
    CommandEntry {
        func: command_silence,
        name: "silence",
        level: 99,
    },
    CommandEntry {
        func: command_shutdowncancel,
        name: "shutdown_cancel",
        level: 99,
    },
    CommandEntry {
        func: command_shutdown,
        name: "shutdown",
        level: 99,
    },
    CommandEntry {
        func: command_weap,
        name: "weap",
        level: 99,
    },
    CommandEntry {
        func: command_shield,
        name: "shield",
        level: 99,
    },
    CommandEntry {
        func: command_armor,
        name: "armor",
        level: 99,
    },
    CommandEntry {
        func: command_boots,
        name: "boots",
        level: 99,
    },
    CommandEntry {
        func: command_mantle,
        name: "mantle",
        level: 99,
    },
    CommandEntry {
        func: command_necklace,
        name: "necklace",
        level: 99,
    },
    CommandEntry {
        func: command_faceacc,
        name: "faceacc",
        level: 99,
    },
    CommandEntry {
        func: command_crown,
        name: "crown",
        level: 99,
    },
    CommandEntry {
        func: command_helm,
        name: "helm",
        level: 99,
    },
    CommandEntry {
        func: command_gfxtoggle,
        name: "gfxtoggle",
        level: 99,
    },
    CommandEntry {
        func: command_weather,
        name: "weather",
        level: 50,
    },
    CommandEntry {
        func: command_light,
        name: "light",
        level: 50,
    },
    CommandEntry {
        func: command_gm,
        name: "gm",
        level: 20,
    },
    CommandEntry {
        func: command_report,
        name: "report",
        level: 0,
    },
    CommandEntry {
        func: command_url,
        name: "url",
        level: 99,
    },
    CommandEntry {
        func: command_cinv,
        name: "cinv",
        level: 50,
    },
    CommandEntry {
        func: command_cfloor,
        name: "cfloor",
        level: 50,
    },
    CommandEntry {
        func: command_cspells,
        name: "cspells",
        level: 50,
    },
    CommandEntry {
        func: command_job,
        name: "job",
        level: 20,
    },
    CommandEntry {
        func: command_music,
        name: "music",
        level: 50,
    },
    CommandEntry {
        func: command_musicn,
        name: "musicn",
        level: 99,
    },
    CommandEntry {
        func: command_musicp,
        name: "musicp",
        level: 99,
    },
    CommandEntry {
        func: command_musicq,
        name: "musicq",
        level: 99,
    },
    CommandEntry {
        func: command_sound,
        name: "sound",
        level: 50,
    },
    CommandEntry {
        func: command_nsound,
        name: "nsound",
        level: 99,
    },
    CommandEntry {
        func: command_psound,
        name: "psound",
        level: 99,
    },
    CommandEntry {
        func: command_soundq,
        name: "soundq",
        level: 99,
    },
    CommandEntry {
        func: command_nspell,
        name: "nspell",
        level: 99,
    },
    CommandEntry {
        func: command_pspell,
        name: "pspell",
        level: 99,
    },
    CommandEntry {
        func: command_spellq,
        name: "spellq",
        level: 99,
    },
    CommandEntry {
        func: command_reloadboard,
        name: "reloadboard",
        level: 99,
    },
    CommandEntry {
        func: command_reloadclan,
        name: "reloadclan",
        level: 99,
    },
    CommandEntry {
        func: command_item,
        name: "i",
        level: 50,
    },
    CommandEntry {
        func: command_reloadnpc,
        name: "reloadnpc",
        level: 99,
    },
    CommandEntry {
        func: command_reloadmaps,
        name: "reloadmaps",
        level: 99,
    },
    CommandEntry {
        func: command_reloadclass,
        name: "reloadclass",
        level: 99,
    },
    CommandEntry {
        func: command_reloadlevels,
        name: "reloadlevels",
        level: 99,
    },
    CommandEntry {
        func: command_reloadwarps,
        name: "reloadwarps",
        level: 99,
    },
    CommandEntry {
        func: command_transfer,
        name: "transfer",
        level: 99,
    },
];

// ── Command implementations ────────────────────────────────────────────────────

fn command_debug(sd: &mut MapSessionData, line: &str) -> i32 {
    use crate::game::map_parse::packet::{wfifohead, wfifop, wfifoset};
    let mut iter = line.splitn(2, char::is_whitespace);
    let packnum: u8 = iter.next().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
    let rest = iter.next().unwrap_or("");
    let vals: Vec<u8> = rest
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let strnum = vals.len();
    let pktlen = strnum + 2;
    let fd = sd.fd;
    unsafe {
        wfifohead(fd, pktlen + 3);
        *wfifop(fd, 0) = 0xAA;
        let len_bytes = (pktlen as u16).to_be_bytes();
        *wfifop(fd, 1) = len_bytes[0];
        *wfifop(fd, 2) = len_bytes[1];
        *wfifop(fd, 3) = packnum;
        *wfifop(fd, 4) = 0x03;
        for (i, &v) in vals.iter().enumerate() {
            *wfifop(fd, 5 + i) = v;
        }
        let n = encrypt_fd(fd) as usize;
        wfifoset(fd, n);
    }
    0
}

fn command_item(sd: &mut MapSessionData, line: &str) -> i32 {
    use crate::common::types::Item;
    let mut itemnum: u32 = 0;
    let mut itemid: u32 = 0;

    if !line.is_empty() && line.as_bytes()[0].is_ascii_digit() {
        let mut parts = line.split_whitespace();
        itemid = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        itemnum = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    } else {
        let mut parts = line.split_whitespace();
        if let Some(name) = parts.next() {
            itemnum = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let namebuf = str_to_cname(name);
            itemid = item_db::id_by_name(carray_to_str(&namebuf));
        }
    }
    if itemid == 0 {
        return -1;
    }
    if itemnum == 0 {
        itemnum = 1;
    }

    unsafe {
        let mut it: Item = std::mem::zeroed();
        it.id = itemid;
        it.dura = item_db::search(itemid).dura;
        it.amount = itemnum as i32;
        it.owner = 0;
        if let Some(pe) = pe_of(sd) {
            pc_additem(&pe, &mut it);
        }
    }
    0
}

fn command_res(sd: &mut MapSessionData, _line: &str) -> i32 {
    if sd.player.combat.state == PC_DIE as i8 {
        unsafe {
            pc_res(as_ptr(sd));
        }
    }
    0
}

fn command_hair(sd: &mut MapSessionData, line: &str) -> i32 {
    let (hair, hair_color) = match parse_two_ints(line) {
        Some(v) => v,
        None => return -1,
    };
    sd.player.appearance.hair = hair as u16;
    sd.player.appearance.hair_color = hair_color as u16;
    if let Some(pe) = pe_of(sd) {
        refresh_appearance(&pe);
    }
    0
}

fn command_checkdupes(sd: &mut MapSessionData, _line: &str) -> i32 {
    let reporter_id = sd.id;
    crate::game::map_server::for_each_player(|tsd| {
        let n = unsafe { pc_readglobalreg(tsd as *mut MapSessionData, c"goldbardupe".as_ptr()) };
        if n != 0 {
            let name_str = tsd.player.identity.name.as_str();
            let msg = format!("{} gold bar {} times", name_str, n);
            if let Some(pe) = map_id2sd_pc(reporter_id) {
                if let Ok(cs) = CString::new(msg) {
                    unsafe {
                        clif_sendminitext(&pe, cs.as_ptr());
                    }
                }
            }
        }
    });
    0
}

fn command_checkwpe(sd: &mut MapSessionData, _line: &str) -> i32 {
    let reporter_id = sd.id;
    crate::game::map_server::for_each_player(|tsd| {
        let n = unsafe { pc_readglobalreg(tsd as *mut MapSessionData, c"WPEtimes".as_ptr()) };
        if n != 0 {
            let name_str = tsd.player.identity.name.as_str();
            let msg = format!("{} WPE attempt {} times", name_str, n);
            if let Some(pe) = map_id2sd_pc(reporter_id) {
                if let Ok(cs) = CString::new(msg) {
                    unsafe {
                        clif_sendminitext(&pe, cs.as_ptr());
                    }
                }
            }
        }
    });
    0
}

fn command_kill(sd: &mut MapSessionData, line: &str) -> i32 {
    let name = str_to_cname(parse_first_word(line));
    let tsd = unsafe { map_name2sd(name.as_ptr()) };
    if !tsd.is_null() {
        session_set_eof(unsafe { (*tsd).fd }, 1);
        send_minitext(sd, "Done.");
    } else {
        send_minitext(sd, "User not found.");
    }
    0
}

fn command_killall(sd: &mut MapSessionData, _line: &str) -> i32 {
    let manager = crate::session::get_session_manager();
    crate::game::map_server::for_each_player(|tsd| {
        if tsd.player.identity.gm_level == 0 && tsd.fd.raw() > 0 {
            if let Some(arc) = manager.get_session(tsd.fd) {
                if let Ok(mut guard) = arc.try_lock() {
                    guard.eof = 1;
                }
            }
        }
    });
    send_minitext(sd, "All but GMs have been mass booted.");
    0
}

fn command_deletespell(sd: &mut MapSessionData, line: &str) -> i32 {
    let spell_name = parse_first_word(line);
    if spell_name.is_empty() {
        return -1;
    }
    let spell = magic_db::id_by_name(spell_name);
    if (0..52).contains(&spell) {
        sd.player.spells.skills[spell as usize] = 0;
        unsafe {
            pc_loadmagic(as_ptr(sd));
        }
    }
    0
}

fn command_xprate(sd: &mut MapSessionData, line: &str) -> i32 {
    let rate = match parse_int(line) {
        Some(v) => v,
        None => return -1,
    };
    XP_RATE.store(rate, Ordering::Relaxed);
    send_minitext(sd, &format!("Experience rate: {}x", rate));
    0
}

fn command_heal(sd: &mut MapSessionData, _line: &str) -> i32 {
    sd.player.combat.hp = sd.max_hp;
    sd.player.combat.mp = sd.max_mp;
    if let Some(pe) = pe_of(sd) {
        unsafe {
            clif_sendstatus(&pe, SFLAG_HPMP);
        }
    }
    0
}

fn command_level(sd: &mut MapSessionData, line: &str) -> i32 {
    let level = match parse_int(line) {
        Some(v) => v,
        None => return -1,
    };
    sd.player.progression.level = level as u8;
    if let Some(pe) = pe_of(sd) {
        pe.set_class_level(sd.player.progression.class, level as u8);
        unsafe {
            clif_sendstatus(&pe, SFLAG_FULLSTATS);
        }
    }
    0
}

fn command_randomspawn(_sd: &mut MapSessionData, _line: &str) -> i32 {
    0
}

fn command_drate(sd: &mut MapSessionData, line: &str) -> i32 {
    let rate = match parse_int(line) {
        Some(v) => v,
        None => return -1,
    };
    D_RATE.store(rate, Ordering::Relaxed);
    send_minitext(sd, &format!("Drop rate: {} x", rate));
    0
}

fn command_spell(sd: &mut MapSessionData, line: &str) -> i32 {
    if let Some((spell, sound)) = parse_two_ints(line) {
        SPELLGFX.store(spell, Ordering::Relaxed);
        SOUNDFX.store(sound, Ordering::Relaxed);
        unsafe {
            clif_playsound_entity(sd.id, sd.m, sd.x, sd.y, BL_PC as u8, sound);
        }
    }

    let anim = SPELLGFX.load(Ordering::Relaxed);
    let times = SOUNDFX.load(Ordering::Relaxed);
    if let Some(grid) = block_grid::get_grid(sd.m as usize) {
        let slot = unsafe { &*crate::database::map_db::raw_map_ptr().add(sd.m as usize) };
        let ids = block_grid::ids_in_area(
            grid,
            sd.x as i32,
            sd.y as i32,
            AreaType::Area,
            slot.xs as i32,
            slot.ys as i32,
        );
        for id in ids {
            if let Some(arc) = crate::game::map_server::map_id2sd_pc(id) {
                let pc = arc.read();
                clif_sendanimation_inner(
                    pc.fd,
                    pc.player.appearance.setting_flags,
                    anim,
                    sd.id,
                    times,
                );
            }
        }
    }
    0
}

fn command_val(sd: &mut MapSessionData, _line: &str) -> i32 {
    let count = (MOB_SPAWN_MAX.load(Ordering::Relaxed) - MOB_SPAWN_START.load(Ordering::Relaxed))
        + (MOB_ONETIME_MAX.load(Ordering::Relaxed) - MOB_ONETIME_START.load(Ordering::Relaxed));
    send_minitext(sd, &format!("Mob spawn count: {}", count));
    0
}

fn command_disguise(sd: &mut MapSessionData, line: &str) -> i32 {
    let (d, e) = match parse_two_ints(line) {
        Some(v) => v,
        None => return -1,
    };
    let os = sd.player.combat.state;
    sd.player.combat.state = 0;
    if let Some(pe) = pe_of(sd) {
        unsafe {
            broadcast_update_state(&pe);
        }
    }
    sd.player.combat.state = os;
    sd.disguise = d as u16;
    sd.disguise_color = e as u16;
    if let Some(pe) = pe_of(sd) {
        unsafe {
            broadcast_update_state(&pe);
        }
    }
    0
}

fn command_warp(sd: &mut MapSessionData, line: &str) -> i32 {
    let (m, x, y) = match parse_three_ints(line) {
        Some(v) => v,
        None => return -1,
    };
    unsafe {
        pc_warp(as_ptr(sd), m, x, y);
    }
    0
}

fn command_givespell(sd: &mut MapSessionData, line: &str) -> i32 {
    let word = parse_first_word(line);
    if word.is_empty() {
        return -1;
    }
    let spell = magic_db::id_by_name(word);
    for x in 0..52usize {
        if sd.player.spells.skills[x] == 0 {
            sd.player.spells.skills[x] = spell as u16;
            unsafe {
                pc_loadmagic(as_ptr(sd));
            }
            break;
        }
        if sd.player.spells.skills[x] == spell as u16 {
            break;
        }
    }
    0
}

fn command_side(sd: &mut MapSessionData, line: &str) -> i32 {
    let side = match parse_int(line) {
        Some(v) => v,
        None => return -1,
    };
    sd.player.combat.side = side as i8;
    if let Some(pe) = pe_of(sd) {
        refresh_appearance(&pe);
    }
    0
}

fn command_state(sd: &mut MapSessionData, line: &str) -> i32 {
    let state_val = match parse_int(line) {
        Some(v) => v,
        None => return -1,
    };
    if sd.player.combat.state == 1 && state_val != 1 {
        unsafe {
            pc_res(as_ptr(sd));
        }
    } else {
        sd.player.combat.state = (state_val % 5) as i8;
        if let Some(pe) = pe_of(sd) {
            unsafe {
                broadcast_update_state(&pe);
            }
        }
    }
    0
}

fn command_armorcolor(sd: &mut MapSessionData, line: &str) -> i32 {
    let ac = match parse_int(line) {
        Some(v) => v,
        None => return -1,
    };
    sd.player.appearance.armor_color = ac as u16;
    if let Some(pe) = pe_of(sd) {
        refresh_appearance(&pe);
    }
    0
}

fn command_makegm(_sd: &mut MapSessionData, line: &str) -> i32 {
    let word = parse_first_word(line);
    if word.is_empty() {
        return -1;
    }
    let name = str_to_cname(word);
    let tsd = unsafe { map_name2sd(name.as_ptr()) };
    if !tsd.is_null() {
        unsafe {
            (*tsd).player.identity.gm_level = 99;
        }
    }
    0
}

fn command_who(sd: &mut MapSessionData, _line: &str) -> i32 {
    send_minitext(
        sd,
        &format!("There are {} users online.", userlist().user_count),
    );
    0
}

fn command_legend(sd: &mut MapSessionData, _line: &str) -> i32 {
    sd.player.legends.legends[0].icon = 12;
    sd.player.legends.legends[0].color = 128;
    let text = b"Blessed by a GM\0";
    for (i, &b) in text.iter().enumerate() {
        sd.player.legends.legends[0].text[i] = b as i8;
    }
    0
}

fn command_luareload(sd: &mut MapSessionData, _line: &str) -> i32 {
    let errors = sl_reload();
    send_minitext(sd, "LUA Scripts reloaded!");
    errors
}

fn command_magicreload(sd: &mut MapSessionData, _line: &str) -> i32 {
    // magicdb_read was a no-op in C headers; nothing to reload.
    send_minitext(sd, "Magic DB reloaded!");
    0
}

fn command_lua(sd: &mut MapSessionData, line: &str) -> i32 {
    sd.luaexec = 0;
    crate::game::scripting::doscript_blargs_id("canRunLuaTalk", None, &[sd.id]);
    if sd.luaexec != 0 {
        if let Ok(cs) = CString::new(line) {
            unsafe {
                sl_exec(as_ptr(sd), cs.as_ptr() as *mut i8);
            }
        }
    }
    0
}

fn command_speed(sd: &mut MapSessionData, line: &str) -> i32 {
    let d = match parse_int(line) {
        Some(v) => v,
        None => return -1,
    };
    sd.speed = d;
    if let Some(pe) = pe_of(sd) {
        refresh_appearance(&pe);
    }
    0
}

fn command_reloaditem(sd: &mut MapSessionData, _line: &str) -> i32 {
    item_db::init();
    send_minitext(sd, "Item DB Reloaded!");
    0
}

fn command_reloadcreations(sd: &mut MapSessionData, _line: &str) -> i32 {
    send_minitext(sd, "Creations DB reloaded!");
    0
}

fn command_reloadmob(sd: &mut MapSessionData, _line: &str) -> i32 {
    mob_db::term();
    mob_db::init();
    send_minitext(sd, "Mob DB Reloaded");
    0
}

fn command_reloadspawn(sd: &mut MapSessionData, _line: &str) -> i32 {
    tokio::task::spawn_local(async move { unsafe { mobspawn_read().await } });
    send_minitext(sd, "Spawn DB Reloaded");
    0
}

fn command_pvp(sd: &mut MapSessionData, line: &str) -> i32 {
    let pvp = match parse_int(line) {
        Some(v) => v,
        None => return -1,
    };
    unsafe {
        let mp = crate::database::map_db::get_map_ptr(sd.m);
        if !mp.is_null() {
            (*mp).pvp = pvp as u8;
        }
    }
    send_minitext(sd, &format!("PvP set to: {}", pvp));
    0
}

fn command_spellwork(sd: &mut MapSessionData, _line: &str) -> i32 {
    unsafe {
        let mp = crate::database::map_db::get_map_ptr(sd.m);
        if !mp.is_null() {
            (*mp).spell ^= 1;
        }
    }
    0
}

fn command_broadcast(_sd: &mut MapSessionData, line: &str) -> i32 {
    if let Ok(cs) = CString::new(line) {
        unsafe {
            clif_broadcast(cs.as_ptr(), -1);
        }
    }
    0
}

fn command_luasize(_sd: &mut MapSessionData, _line: &str) -> i32 {
    unsafe {
        sl_luasize();
    }
    0
}

fn command_luafix(_sd: &mut MapSessionData, _line: &str) -> i32 {
    unsafe {
        sl_fixmem();
    }
    0
}

fn command_respawn(sd: &mut MapSessionData, _line: &str) -> i32 {
    if let Some(grid) = block_grid::get_grid(sd.m as usize) {
        let all_ids: Vec<u32> = grid.all_ids().collect();
        for id in all_ids {
            if let Some(arc) = crate::game::map_server::map_id2mob_ref(id) {
                let mob = arc.write();
                if mob.state == MOB_DEAD && mob.onetime == 0 {
                    unsafe {
                        mob_respawn(&*mob as *const MobSpawnData as *mut MobSpawnData);
                    }
                }
            }
        }
    }
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

fn command_ban(_sd: &mut MapSessionData, line: &str) -> i32 {
    let word = parse_first_word(line);
    if word.is_empty() {
        return -1;
    }
    let name = str_to_cname(word);
    let tsd = unsafe { map_name2sd(name.as_ptr()) };
    if !tsd.is_null() {
        tracing::info!("[command] Banning {}", word);
        tokio::task::spawn_local(ban_character(word.to_owned()));
        session_set_eof(unsafe { (*tsd).fd }, 1);
    }
    0
}

fn command_unban(_sd: &mut MapSessionData, line: &str) -> i32 {
    let word = parse_first_word(line);
    if word.is_empty() {
        return -1;
    }
    tracing::info!("[command] Unbanning {}", word);
    tokio::task::spawn_local(unban_character(word.to_owned()));
    0
}

fn command_kc(sd: &mut MapSessionData, _line: &str) -> i32 {
    let entries: Vec<(u32, u32)> = sd
        .player
        .registries
        .kill_reg
        .iter()
        .map(|(&mob_id, &amount)| (mob_id, amount))
        .collect();
    for (mob_id, amount) in entries {
        send_minitext(sd, &format!("{} ({})", mob_id, amount));
    }
    0
}

fn command_blockcount(_sd: &mut MapSessionData, _line: &str) -> i32 {
    0
}

fn command_stealth(sd: &mut MapSessionData, _line: &str) -> i32 {
    if sd.optFlags & OPT_FLAG_STEALTH != 0 {
        sd.optFlags ^= OPT_FLAG_STEALTH;
        if let Some(pe) = pe_of(sd) {
            unsafe {
                clif_refresh(&pe);
            }
        }
        send_minitext(sd, "Stealth :OFF");
    } else {
        unsafe {
            clif_lookgone_by_id(sd.id);
        }
        sd.optFlags ^= OPT_FLAG_STEALTH;
        if let Some(pe) = pe_of(sd) {
            unsafe {
                clif_refresh(&pe);
            }
        }
        send_minitext(sd, "Stealth :ON");
    }
    0
}

fn command_ghosts(sd: &mut MapSessionData, _line: &str) -> i32 {
    sd.optFlags ^= OPT_FLAG_GHOSTS;
    if let Some(pe) = pe_of(sd) {
        unsafe {
            clif_refresh(&pe);
        }
    }
    if sd.optFlags & OPT_FLAG_GHOSTS != 0 {
        send_minitext(sd, "Ghosts :ON");
    } else {
        send_minitext(sd, "Ghosts :OFF");
    }
    0
}

fn command_unphysical(sd: &mut MapSessionData, _line: &str) -> i32 {
    sd.uFlags ^= U_FLAG_UNPHYSICAL;
    if sd.uFlags & U_FLAG_UNPHYSICAL != 0 {
        send_minitext(sd, "Unphysical :ON");
    } else {
        send_minitext(sd, "Unphysical :OFF");
    }
    0
}

fn command_immortality(sd: &mut MapSessionData, _line: &str) -> i32 {
    sd.uFlags ^= U_FLAG_IMMORTAL;
    if sd.uFlags & U_FLAG_IMMORTAL != 0 {
        send_minitext(sd, "Immortality :ON");
    } else {
        send_minitext(sd, "Immortality :OFF");
    }
    0
}

fn command_silence(sd: &mut MapSessionData, line: &str) -> i32 {
    let word = parse_first_word(line);
    if word.is_empty() {
        return -1;
    }
    let name = str_to_cname(word);
    let tsd = unsafe { map_name2sd(name.as_ptr()) };
    if !tsd.is_null() {
        unsafe {
            (*tsd).uFlags ^= U_FLAG_SILENCED;
            if (*tsd).uFlags & U_FLAG_SILENCED != 0 {
                send_minitext(sd, "Silenced.");
                if let Some(pe) = map_id2sd_pc((*tsd).id) {
                    clif_sendminitext(&pe, c"You have been silenced.".as_ptr());
                }
            } else {
                send_minitext(sd, "Unsilenced.");
                if let Some(pe) = map_id2sd_pc((*tsd).id) {
                    clif_sendminitext(&pe, c"Silence lifted.".as_ptr());
                }
            }
        }
    } else {
        send_minitext(sd, "User not on.");
    }
    0
}

fn command_shutdowncancel(sd: &mut MapSessionData, _line: &str) -> i32 {
    let dt = DOWNTIMER.load(Ordering::Relaxed);
    if dt != 0 {
        unsafe {
            clif_broadcast(
                c"---------------------------------------------------".as_ptr(),
                -1,
            );
            clif_broadcast(c"Server shutdown cancelled.".as_ptr(), -1);
            clif_broadcast(
                c"---------------------------------------------------".as_ptr(),
                -1,
            );
        }
        timer_remove(dt);
        DOWNTIMER.store(0, Ordering::Relaxed);
    } else {
        send_minitext(sd, "Server is not shutting down.");
    }
    0
}

fn command_shutdown(sd: &mut MapSessionData, line: &str) -> i32 {
    if DOWNTIMER.load(Ordering::Relaxed) != 0 {
        send_minitext(sd, "Server is already shutting down.");
        return 0;
    }
    let mut parts = line.split_whitespace();
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
    let msg = if t_time >= 60000 {
        let d = t_time / 60000;
        t_time = d * 60000;
        format!("Reset in {} minutes.", d)
    } else {
        let d = t_time / 1000;
        t_time = d * 1000;
        format!("Reset in {} seconds.", d)
    };
    if let Ok(cs) = CString::new(msg) {
        unsafe {
            clif_broadcast(
                c"---------------------------------------------------".as_ptr(),
                -1,
            );
            clif_broadcast(cs.as_ptr(), -1);
            clif_broadcast(
                c"---------------------------------------------------".as_ptr(),
                -1,
            );
        }
    }
    DOWNTIMER.store(
        timer_insert(250, 250, Some(map_reset_timer), t_time, 250),
        Ordering::Relaxed,
    );
    0
}

fn command_weap(sd: &mut MapSessionData, line: &str) -> i32 {
    let (id, color) = match parse_two_ints(line) {
        Some(v) => v,
        None => return -1,
    };
    sd.gfx.weapon = id as u16;
    sd.gfx.cweapon = color as u8;
    if let Some(pe) = pe_of(sd) {
        refresh_appearance(&pe);
    }
    0
}

fn command_shield(sd: &mut MapSessionData, line: &str) -> i32 {
    let (id, color) = match parse_two_ints(line) {
        Some(v) => v,
        None => return -1,
    };
    sd.gfx.shield = id as u16;
    sd.gfx.cshield = color as u8;
    if let Some(pe) = pe_of(sd) {
        refresh_appearance(&pe);
    }
    0
}

fn command_armor(sd: &mut MapSessionData, line: &str) -> i32 {
    let (id, color) = match parse_two_ints(line) {
        Some(v) => v,
        None => return -1,
    };
    sd.gfx.armor = id as u16;
    sd.gfx.carmor = color as u8;
    if let Some(pe) = pe_of(sd) {
        refresh_appearance(&pe);
    }
    0
}

fn command_boots(sd: &mut MapSessionData, line: &str) -> i32 {
    let (id, color) = match parse_two_ints(line) {
        Some(v) => v,
        None => return -1,
    };
    sd.gfx.boots = id as u16;
    sd.gfx.cboots = color as u8;
    if let Some(pe) = pe_of(sd) {
        refresh_appearance(&pe);
    }
    0
}

fn command_mantle(sd: &mut MapSessionData, line: &str) -> i32 {
    let (id, color) = match parse_two_ints(line) {
        Some(v) => v,
        None => return -1,
    };
    sd.gfx.mantle = id as u16;
    sd.gfx.cmantle = color as u8;
    if let Some(pe) = pe_of(sd) {
        refresh_appearance(&pe);
    }
    0
}

fn command_necklace(sd: &mut MapSessionData, line: &str) -> i32 {
    let (id, color) = match parse_two_ints(line) {
        Some(v) => v,
        None => return -1,
    };
    sd.gfx.necklace = id as u16;
    sd.gfx.cnecklace = color as u8;
    if let Some(pe) = pe_of(sd) {
        refresh_appearance(&pe);
    }
    0
}

fn command_faceacc(sd: &mut MapSessionData, line: &str) -> i32 {
    let (id, color) = match parse_two_ints(line) {
        Some(v) => v,
        None => return -1,
    };
    sd.gfx.face_acc = id as u16;
    sd.gfx.cface_acc = color as u8;
    if let Some(pe) = pe_of(sd) {
        refresh_appearance(&pe);
    }
    0
}

fn command_crown(sd: &mut MapSessionData, line: &str) -> i32 {
    let (id, color) = match parse_two_ints(line) {
        Some(v) => v,
        None => return -1,
    };
    sd.gfx.crown = id as u16;
    sd.gfx.ccrown = color as u8;
    if let Some(pe) = pe_of(sd) {
        refresh_appearance(&pe);
    }
    0
}

fn command_helm(sd: &mut MapSessionData, line: &str) -> i32 {
    let (id, color) = match parse_two_ints(line) {
        Some(v) => v,
        None => return -1,
    };
    sd.gfx.helm = id as u16;
    sd.gfx.chelm = color as u8;
    if let Some(pe) = pe_of(sd) {
        refresh_appearance(&pe);
    }
    0
}

fn command_gfxtoggle(sd: &mut MapSessionData, _line: &str) -> i32 {
    sd.gfx.toggle ^= 1;
    0
}

fn command_weather(sd: &mut MapSessionData, line: &str) -> i32 {
    let weather = parse_int(line).unwrap_or(5);
    unsafe {
        let mp = crate::database::map_db::get_map_ptr(sd.m);
        if !mp.is_null() {
            (*mp).weather = weather as u8;
        }
    }
    let m = sd.m;
    crate::game::map_server::for_each_player(|tsd| {
        if tsd.m == m {
            if let Some(pe) = map_id2sd_pc(tsd.id) {
                unsafe {
                    clif_sendweather(&pe);
                }
            }
        }
    });
    0
}

fn command_light(sd: &mut MapSessionData, line: &str) -> i32 {
    let light = parse_int(line).unwrap_or(232);
    unsafe {
        let mp = crate::database::map_db::get_map_ptr(sd.m);
        if !mp.is_null() {
            (*mp).light = light as u8;
        }
    }
    let m = sd.m;
    crate::game::map_server::for_each_player(|tsd| {
        if tsd.m == m {
            unsafe {
                pc_warp(
                    tsd as *mut MapSessionData,
                    tsd.m as i32,
                    tsd.x as i32,
                    tsd.y as i32,
                );
            }
        }
    });
    0
}

fn command_gm(sd: &mut MapSessionData, line: &str) -> i32 {
    let name_str = sd.player.identity.name.as_str();
    let msg = format!("<GM>{}: {}", name_str, line);
    crate::game::map_server::for_each_player(|tsd| {
        if tsd.player.identity.gm_level != 0 {
            if let Some(pe) = map_id2sd_pc(tsd.id) {
                if let Ok(cs) = CString::new(msg.as_str()) {
                    unsafe {
                        clif_sendmsg(&pe, 11, cs.as_ptr());
                    }
                }
            }
        }
    });
    0
}

fn command_report(sd: &mut MapSessionData, line: &str) -> i32 {
    let name_str = sd.player.identity.name.as_str();
    let msg = format!("<REPORT>{}: {}", name_str, line);
    crate::game::map_server::for_each_player(|tsd| {
        if tsd.player.identity.gm_level > 0 {
            if let Some(pe) = map_id2sd_pc(tsd.id) {
                if let Ok(cs) = CString::new(msg.as_str()) {
                    unsafe {
                        clif_sendmsg(&pe, 12, cs.as_ptr());
                    }
                }
            }
        }
    });
    0
}

fn command_url(_sd: &mut MapSessionData, line: &str) -> i32 {
    let mut parts = line.split_whitespace();
    let name_s = match parts.next() {
        Some(v) => v,
        None => return -1,
    };
    let url_type: i32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let url_s = parts.next().unwrap_or("");

    let namebuf = str_to_cname(name_s);
    let mut urlbuf = [0i8; 128];
    for (i, b) in url_s.bytes().take(127).enumerate() {
        urlbuf[i] = b as i8;
    }

    let tsd = unsafe { map_name2sd(namebuf.as_ptr()) };
    if tsd.is_null() {
        return -1;
    }
    unsafe {
        if let Some(pe) = map_id2sd_pc((*tsd).id) {
            clif_sendurl(&pe, url_type, urlbuf.as_ptr());
        }
    }
    0
}

fn command_cinv(sd: &mut MapSessionData, line: &str) -> i32 {
    let (start, end) = parse_two_ints(line).unwrap_or((0, 51));
    for x in start..=end {
        let x = x as usize;
        if x < 52
            && sd.player.inventory.inventory[x].id > 0
            && sd.player.inventory.inventory[x].amount > 0
        {
            let amount = sd.player.inventory.inventory[x].amount;
            if let Some(pe) = pe_of(sd) {
                unsafe {
                    pc_delitem(&pe, x as i32, amount, 0);
                }
            }
        }
    }
    0
}

fn command_cfloor(_sd: &mut MapSessionData, _line: &str) -> i32 {
    0
}

fn command_cspells(sd: &mut MapSessionData, line: &str) -> i32 {
    let (start, end) = match parse_two_ints(line) {
        Some((s, e)) => (s as usize, e as usize),
        None => (0, 51),
    };
    for x in start..=end {
        if x < 52 && sd.player.spells.skills[x] > 0 {
            sd.player.spells.skills[x] = 0;
            unsafe {
                pc_loadmagic(as_ptr(sd));
            }
        }
    }
    0
}

async fn set_job_class(class_val: u32, mark_val: u32, char_id: u32) {
    sqlx::query("UPDATE `Character` SET `ChaPthId` = ?, `ChaMark` = ? WHERE `ChaId` = ?")
        .bind(class_val)
        .bind(mark_val)
        .bind(char_id)
        .execute(get_pool())
        .await
        .ok();
}

fn command_job(sd: &mut MapSessionData, line: &str) -> i32 {
    let (mut job, mut subjob) = parse_two_ints(line).unwrap_or((0, 0));
    if job < 0 {
        job = 5;
    }
    if !(0..=16).contains(&subjob) {
        subjob = 0;
    }
    sd.player.progression.class = job as u8;
    sd.player.progression.mark = subjob as u8;
    if let Some(pe) = pe_of(sd) {
        pe.set_class_level(job as u8, sd.player.progression.level);
    }
    let class_val = sd.player.progression.class as u32;
    let mark_val = sd.player.progression.mark as u32;
    let char_id = sd.player.identity.id;
    crate::database::blocking_run_async(set_job_class(class_val, mark_val, char_id));
    if let Some(pe) = pe_of(sd) {
        unsafe {
            clif_mystatus(&pe);
        }
    }
    0
}

fn command_music(sd: &mut MapSessionData, line: &str) -> i32 {
    if let Some(music) = parse_int(line) {
        MUSICFX.store(music, Ordering::Relaxed);
    }
    let oldm = sd.m as i32;
    let oldx = sd.x as i32;
    let oldy = sd.y as i32;
    unsafe {
        let mp = crate::database::map_db::get_map_ptr(sd.m);
        if !mp.is_null() {
            (*mp).bgm = MUSICFX.load(Ordering::Relaxed) as u16;
        }
        pc_warp(as_ptr(sd), 10002, 0, 0);
        pc_warp(as_ptr(sd), oldm, oldx, oldy);
    }
    0
}

fn command_musicn(sd: &mut MapSessionData, _line: &str) -> i32 {
    MUSICFX.fetch_add(1, Ordering::Relaxed);
    let oldm = sd.m as i32;
    let oldx = sd.x as i32;
    let oldy = sd.y as i32;
    unsafe {
        let mp = crate::database::map_db::get_map_ptr(sd.m);
        if !mp.is_null() {
            (*mp).bgm = MUSICFX.load(Ordering::Relaxed) as u16;
        }
        pc_warp(as_ptr(sd), 10002, 0, 0);
        pc_warp(as_ptr(sd), oldm, oldx, oldy);
    }
    0
}

fn command_musicp(sd: &mut MapSessionData, _line: &str) -> i32 {
    MUSICFX.fetch_sub(1, Ordering::Relaxed);
    let oldm = sd.m as i32;
    let oldx = sd.x as i32;
    let oldy = sd.y as i32;
    unsafe {
        let mp = crate::database::map_db::get_map_ptr(sd.m);
        if !mp.is_null() {
            (*mp).bgm = MUSICFX.load(Ordering::Relaxed) as u16;
        }
        pc_warp(as_ptr(sd), 10002, 0, 0);
        pc_warp(as_ptr(sd), oldm, oldx, oldy);
    }
    0
}

fn command_musicq(sd: &mut MapSessionData, _line: &str) -> i32 {
    send_minitext(
        sd,
        &format!("Current music is: {}", MUSICFX.load(Ordering::Relaxed)),
    );
    0
}

fn command_sound(sd: &mut MapSessionData, line: &str) -> i32 {
    if let Some(sound) = parse_int(line) {
        SOUNDFX.store(sound, Ordering::Relaxed);
    }
    unsafe {
        clif_playsound_entity(
            sd.id,
            sd.m,
            sd.x,
            sd.y,
            BL_PC as u8,
            SOUNDFX.load(Ordering::Relaxed),
        );
    }
    0
}

fn command_nsound(sd: &mut MapSessionData, _line: &str) -> i32 {
    let s = (SOUNDFX.fetch_add(1, Ordering::Relaxed) + 1).min(125);
    SOUNDFX.store(s, Ordering::Relaxed);
    unsafe {
        clif_playsound_entity(sd.id, sd.m, sd.x, sd.y, BL_PC as u8, s);
    }
    0
}

fn command_psound(sd: &mut MapSessionData, _line: &str) -> i32 {
    let s = (SOUNDFX.fetch_sub(1, Ordering::Relaxed) - 1).max(0);
    SOUNDFX.store(s, Ordering::Relaxed);
    unsafe {
        clif_playsound_entity(sd.id, sd.m, sd.x, sd.y, BL_PC as u8, s);
    }
    0
}

fn command_soundq(sd: &mut MapSessionData, _line: &str) -> i32 {
    send_minitext(
        sd,
        &format!("Current sound is: {}", SOUNDFX.load(Ordering::Relaxed)),
    );
    0
}

fn command_nspell(sd: &mut MapSessionData, _line: &str) -> i32 {
    let g = (SPELLGFX.fetch_add(1, Ordering::Relaxed) + 1).min(427);
    SPELLGFX.store(g, Ordering::Relaxed);

    let anim = g;
    let times = SOUNDFX.load(Ordering::Relaxed);
    if let Some(grid) = block_grid::get_grid(sd.m as usize) {
        let slot = unsafe { &*crate::database::map_db::raw_map_ptr().add(sd.m as usize) };
        let ids = block_grid::ids_in_area(
            grid,
            sd.x as i32,
            sd.y as i32,
            AreaType::Area,
            slot.xs as i32,
            slot.ys as i32,
        );
        for id in ids {
            if let Some(arc) = crate::game::map_server::map_id2sd_pc(id) {
                let pc = arc.read();
                clif_sendanimation_inner(
                    pc.fd,
                    pc.player.appearance.setting_flags,
                    anim,
                    sd.id,
                    times,
                );
            }
        }
    }
    0
}

fn command_pspell(sd: &mut MapSessionData, _line: &str) -> i32 {
    let g = (SPELLGFX.fetch_sub(1, Ordering::Relaxed) - 1).max(0);
    SPELLGFX.store(g, Ordering::Relaxed);

    let anim = g;
    let times = SOUNDFX.load(Ordering::Relaxed);
    if let Some(grid) = block_grid::get_grid(sd.m as usize) {
        let slot = unsafe { &*crate::database::map_db::raw_map_ptr().add(sd.m as usize) };
        let ids = block_grid::ids_in_area(
            grid,
            sd.x as i32,
            sd.y as i32,
            AreaType::Area,
            slot.xs as i32,
            slot.ys as i32,
        );
        for id in ids {
            if let Some(arc) = crate::game::map_server::map_id2sd_pc(id) {
                let pc = arc.read();
                clif_sendanimation_inner(
                    pc.fd,
                    pc.player.appearance.setting_flags,
                    anim,
                    sd.id,
                    times,
                );
            }
        }
    }
    0
}

fn command_spellq(sd: &mut MapSessionData, _line: &str) -> i32 {
    send_minitext(
        sd,
        &format!("Current Spell is: {}", SPELLGFX.load(Ordering::Relaxed)),
    );
    0
}

fn command_reloadboard(sd: &mut MapSessionData, _line: &str) -> i32 {
    board_db::term();
    board_db::init();
    send_minitext(sd, "Board DB reloaded!");
    0
}

fn command_reloadclan(sd: &mut MapSessionData, _line: &str) -> i32 {
    clan_db::init();
    send_minitext(sd, "Clan DB reloaded!");
    0
}

fn command_reloadnpc(sd: &mut MapSessionData, _line: &str) -> i32 {
    unsafe {
        npc_init();
    }
    send_minitext(sd, "NPC DB reloaded!");
    0
}

fn command_reloadmaps(sd: &mut MapSessionData, _line: &str) -> i32 {
    unsafe {
        map_reload();
    }
    send_minitext(sd, "Maps reloaded!");
    0
}

fn command_reloadclass(sd: &mut MapSessionData, _line: &str) -> i32 {
    // classdb_read was a no-op in C headers.
    send_minitext(sd, "Classes reloaded!");
    0
}

fn command_reloadlevels(sd: &mut MapSessionData, _line: &str) -> i32 {
    // leveldb_read was a no-op in C headers.
    send_minitext(sd, "Levels reloaded!");
    0
}

fn command_reloadwarps(sd: &mut MapSessionData, _line: &str) -> i32 {
    unsafe {
        warp_init();
    }
    send_minitext(sd, "Warps reloaded!");
    0
}

fn command_transfer(sd: &mut MapSessionData, _line: &str) -> i32 {
    if let Some(pe) = pe_of(sd) {
        unsafe {
            clif_transfer_test(&pe, 1, 10, 10);
        }
    }
    0
}

// ── Command dispatcher ───────────────────────────────────────────────────────

unsafe fn dispatch(sd: *mut MapSessionData, p: *const i8, len: i32, log: bool) -> i32 {
    if sd.is_null() {
        return 0;
    }
    if *p != COMMAND_CODE.load(Ordering::Relaxed) {
        return 0;
    }

    let byte_len = ((len as usize).min(256)).saturating_sub(1);
    let bytes = std::slice::from_raw_parts(p.add(1) as *const u8, byte_len);
    let text = std::str::from_utf8(bytes).unwrap_or("");
    let text = text.trim_end_matches('\0');

    let (cmd_name, args) = match text.split_once(|c: char| c.is_whitespace()) {
        Some((name, rest)) => (name, rest.trim_start()),
        None => (text, ""),
    };

    let entry = match COMMANDS
        .iter()
        .find(|e| e.name.eq_ignore_ascii_case(cmd_name))
    {
        Some(e) => e,
        None => return 0,
    };

    if ((*sd).player.identity.gm_level as i32) < entry.level {
        return 0;
    }

    if log {
        tracing::info!("[command] gm command used cmd={}", cmd_name);
    }

    let args = args.trim_end_matches('\0');
    (entry.func)(&mut *sd, args);
    1
}

/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn is_command(sd: *mut MapSessionData, p: *const i8, len: i32) -> i32 {
    dispatch(sd, p, len, true)
}
