//! Player-character game logic.

#![allow(non_snake_case, dead_code, unused_variables)]


use crate::database::map_db::BlockList;
// MobSpawnData is used by future porting tasks (Tasks 6+); import it when needed.
use crate::game::types::GfxViewer;
use crate::servers::char::charstatus::MmoCharStatus;

// ─── Helper structs (from map_server.h) ───────────────────────────────────────

/// `struct n_post` — linked-list node for parcels/NPC posts.
#[repr(C)]
pub struct NPost {
    pub prev: *mut NPost,
    pub pos:  u32,
}

/// `struct script_reg` — integer registry slot.
#[repr(C)]
pub struct ScriptReg {
    pub index: i32,
    pub data:  i32,
}

/// `struct script_regstr` — string registry slot.
/// Note: C uses `char data[256]` — `i8` maps correctly to C `char`.
#[repr(C)]
pub struct ScriptRegStr {
    pub index: i32,
    pub data:  [i8; 256],
}

/// `struct sd_ignorelist` — linked-list node for the player ignore list.
/// Note: C field is `Next` (capital N), preserved here to match C layout exactly.
#[repr(C)]
pub struct SdIgnoreList {
    pub name: [i8; 100],
    pub Next: *mut SdIgnoreList,
}

// SAFETY: These structs are only accessed from the single game thread
// while holding appropriate locks. No concurrent access occurs.
unsafe impl Send for NPost {}
unsafe impl Send for SdIgnoreList {}

// ─── Nested sub-structs for MapSessionData ────────────────────────────────────

use crate::servers::char::charstatus::Item;

/// Anonymous `exchange` sub-struct inside `map_sessiondata`.
#[repr(C)]
pub struct PcExchange {
    pub item:          [Item; 52],
    pub item_count:    i32,
    pub exchange_done: i32,
    pub list_count:    i32,
    pub gold:          u32,
    pub target:        u32,
}

/// Anonymous `state` sub-struct inside `map_sessiondata`.
#[repr(C)]
pub struct PcState {
    pub menu_or_input: i32,
}

/// Anonymous `boditems` sub-struct inside `map_sessiondata`.
#[repr(C)]
pub struct PcBodItems {
    pub item:      [Item; 52],
    pub bod_count: i32,
}

// ─── MapSessionData ────────────────────────────────────────────────────────────

/// Player character session data — live in-memory state for a connected player.
///
/// Field order matches the C definition exactly. Every field has been verified
/// against the C source. Do NOT reorder fields — `#[repr(C)]` layout depends on it.
#[repr(C)]
pub struct MapSessionData {
    // Intrusive block-list header (must be first — cast to *mut BlockList for grid operations).
    pub bl:                BlockList,
    pub fd:                SessionId,

    // mmo
    pub status:            MmoCharStatus,

    // status timers
    pub equiptimer:        u64,
    pub ambushtimer:       u64,

    // unsigned int group (multi-field C declarations, split individually)
    pub max_hp:            u32,
    pub max_mp:            u32,
    pub tempmax_hp:        u32,
    pub attacker:          u32,
    pub rangeTarget:       u32,
    pub equipid:           u32,
    pub breakid:           u32,
    pub pvp:               [[u32; 2]; 20],
    pub killspvp:          u32,
    pub timevalues:        [u32; 5],
    pub lastvita:          u32,
    pub groupid:           u32,
    pub disptimer:         u32,
    pub disptimertick:     u32,
    pub basemight:         u32,
    pub basewill:          u32,
    pub basegrace:         u32,
    pub basearmor:         u32,
    pub intpercentage:     u32,
    pub profileStatus:     u32,

    // int combat stats (first C declaration line)
    pub might:             i32,
    pub will:              i32,
    pub grace:             i32,
    pub armor:             i32,
    pub minSdam:           i32,
    pub maxSdam:           i32,
    pub minLdam:           i32,
    pub maxLdam:           i32,
    pub hit:               i32,
    pub dam:               i32,
    pub healing:           i32,
    pub healingtimer:      i32,
    pub pongtimer:         i32,
    pub backstab:          i32,

    pub heartbeat:         i32,

    // int status flags (second C declaration line)
    pub flank:             i32,
    pub polearm:           i32,
    pub tooclose:          i32,
    pub canmove:           i32,
    pub iswalking:         i32,
    pub paralyzed:         i32,
    pub blind:             i32,
    pub drunk:             i32,
    pub snare:             i32,
    pub silence:           i32,
    pub critchance:        i32,
    pub afk:               i32,
    pub afktime:           i32,
    pub totalafktime:      i32,
    pub afktimer:          i32,
    pub extendhit:         i32,
    pub speed:             i32,

    // int timers/misc (third C declaration line)
    pub crit:              i32,
    pub duratimer:         i32,
    pub scripttimer:       i32,
    pub scripttick:        i32,
    pub secondduratimer:   i32,
    pub thirdduratimer:    i32,
    pub fourthduratimer:   i32,
    pub fifthduratimer:    i32,
    pub wisdom:            i32,
    pub bindx:             i32,
    pub bindy:             i32,
    pub hunter:            i32,

    // short stats
    pub protection:        i16,
    pub miss:              i16,
    pub attack_speed:      i16,
    pub con:               i16,

    // float stats
    pub rage:              f32,
    pub enchanted:         f32,
    pub sleep:             f32,
    pub deduction:         f32,
    pub damage:            f32,
    pub invis:             f32,
    pub fury:              f32,
    pub critmult:          f32,
    pub dmgshield:         f32,
    pub vregenoverflow:    f32,
    pub mregenoverflow:    f32,

    // double stats
    pub dmgdealt:          f64,
    pub dmgtaken:          f64,

    // char arrays / single chars
    pub afkmessage:        [i8; 80],
    pub mail:              [i8; 4000],
    pub ipaddress:         [i8; 255],

    pub takeoffid:         i8,
    pub attacked:          i8,
    pub boardshow:         i8,
    pub clone:             i8,
    pub action:            i8,
    pub luaexec:           i8,
    pub deathflag:         i8,
    pub selfbar:           i8,
    pub groupbars:         i8,
    pub mobbars:           i8,
    pub disptimertype:     i8,
    pub sendstatus_tick:   i8,

    pub underLevelFlag:    i8,
    pub dialogtype:        i8,
    pub alignment:         i8,
    pub boardnameval:      i8,

    // unsigned short flags
    pub disguise:          u16,
    pub disguise_color:    u16,

    pub cursed:            u8,
    pub castusetimer:      i32,
    pub fakeDrop:          u8,

    // unsigned char status bytes
    pub confused:          u8,
    pub talktype:          u8,
    pub pickuptype:        u8,
    pub invslot:           u8,
    pub equipslot:         u8,
    pub spottraps:         u8,

    // unsigned short coords
    pub throwx:            u16,
    pub throwy:            u16,
    pub viewx:             u16,
    pub viewy:             u16,
    pub bindmap:           u16,

    // encryption hash buffer (0x401 = 1025 bytes)
    pub EncHash:           [i8; 0x401],

    // npc
    pub npc_id:            i32,
    pub npc_pos:           i32,
    pub npc_lastpos:       i32,
    pub npc_menu:          i32,
    pub npc_amount:        i32,
    pub npc_g:             i32,
    pub npc_gc:            i32,
    pub target:            i32,
    pub time:              i32,
    pub time2:             i32,
    pub lasttime:          i32,
    pub timer:             i32,
    pub npc_stack:         i32,
    pub npc_stackmax:      i32,

    pub npc_script:        *mut i8,
    pub npc_scriptroot:    *mut i8,

    // registry
    pub reg:               *mut ScriptReg,
    pub regstr:            *mut ScriptRegStr,
    pub npcp:              NPost,
    pub reg_num:           i32,
    pub regstr_num:        i32,

    // group
    pub bcount:            i32,
    pub group_count:       i32,
    pub group_on:          i32,
    pub group_leader:      u32,

    // exchange
    pub exchange_on:       i32,
    pub exchange:          PcExchange,
    pub state:             PcState,
    pub boditems:          PcBodItems,

    // lua
    pub coref:             u32,
    pub coref_container:   u32,

    // creation system
    pub creation_works:    i32,
    pub creation_item:     i32,
    pub creation_itemamount: i32,

    // boards
    pub board_candel:      i32,
    pub board_canwrite:    i32,
    pub board:             i32,
    pub board_popup:       i32,
    pub co_timer:          i32,

    pub question:          [i8; 64],
    pub speech:            [i8; 255],
    pub profilepic_data:   [i8; 65535],
    pub profile_data:      [i8; 255],

    pub profilepic_size:   u16,
    pub profile_size:      u8,

    // mob
    pub mob_len:           i32,
    pub mob_count:         i32,
    pub mob_item:          i32,

    pub msPing:            i32,
    pub pbColor:           i32,

    pub time_check:        u32,
    pub time_hash:         u32,
    pub last_click:        u32,

    pub chat_timer:        i32,
    pub savetimer:         i32,

    pub gfx:               GfxViewer,
    pub IgnoreList:        *mut SdIgnoreList,

    pub optFlags:          u64,
    pub uFlags:            u64,
    pub LastPongStamp:     u64,
    pub LastPingTick:      u64,
    pub flags:             u64,
    pub LastWalkTick:      u64,

    pub PrevSeed:          u8,
    pub NextSeed:          u8,
    pub LastWalk:          u8,
    pub loaded:            u8,
}

// SAFETY: MapSessionData is only accessed from the single game thread while
// holding appropriate locks. Raw pointers are to C-managed memory.
// Sync is required because Arc<RwLock<MapSessionData>> needs T: Send + Sync.
unsafe impl Send for MapSessionData {}
unsafe impl Sync for MapSessionData {}

#[cfg(test)]
mod layout_tests {
    use super::*;
    // Verified with: printf("%zu\n", sizeof(struct map_sessiondata))
    const EXPECTED_SIZE: usize = 3335344;
    #[test]
    fn map_session_data_size() {
        assert_eq!(std::mem::size_of::<MapSessionData>(), EXPECTED_SIZE);
    }
}

// ─── Constants ────────────────────────────────────────────────────────────────

// Registry size constants (from mmo.h)
pub const MAX_GLOBALREG:       usize = 5000;
pub const MAX_GLOBALPLAYERREG: usize = 500;
pub const MAX_GLOBALQUESTREG:  usize = 250;
pub const MAX_GLOBALNPCREG:    usize = 100;

// BL_* type flags (from map_server.h `enum { BL_PC=1, BL_MOB=2, BL_NPC=4, BL_ITEM=8 }`)
pub const BL_PC:   i32 = 0x01;
pub const BL_MOB:  i32 = 0x02;
pub const BL_NPC:  i32 = 0x04;
pub const BL_ITEM: i32 = 0x08;

// PC state values — `enum { PC_ALIVE, PC_DIE, PC_INVIS, PC_MOUNTED, PC_DISGUISE }`
pub const PC_ALIVE:    i32 = 0;
pub const PC_DIE:      i32 = 1;
pub const PC_INVIS:    i32 = 2;
pub const PC_MOUNTED:  i32 = 3;
pub const PC_DISGUISE: i32 = 4;

// optFlags enum values (from map_server.h)
pub const OPT_FLAG_STEALTH:     u64 = 32;
pub const OPT_FLAG_NOCLICK:     u64 = 64;
pub const OPT_FLAG_WALKTHROUGH: u64 = 128;
pub const OPT_FLAG_GHOSTS:      u64 = 256;

// uFlags enum values (from map_server.h)
pub const U_FLAG_NONE:       u64 = 0;
pub const U_FLAG_SILENCED:   u64 = 1;
pub const U_FLAG_CANPK:      u64 = 2;
pub const U_FLAG_CANBEPK:    u64 = 3;
pub const U_FLAG_IMMORTAL:   u64 = 8;
pub const U_FLAG_UNPHYSICAL: u64 = 16;
pub const U_FLAG_EVENTHOST:  u64 = 32;
pub const U_FLAG_CONSTABLE:  u64 = 64;
pub const U_FLAG_ARCHON:     u64 = 128;
pub const U_FLAG_GM:         u64 = 256;

// SFLAG values for clif_sendstatus (from map_server.h)
pub const SFLAG_UNKNOWN1:   i32 = 0x01;
pub const SFLAG_UNKNOWN2:   i32 = 0x02;
pub const SFLAG_UNKNOWN3:   i32 = 0x04;
pub const SFLAG_ALWAYSON:   i32 = 0x08;
pub const SFLAG_XPMONEY:    i32 = 0x10;
pub const SFLAG_HPMP:       i32 = 0x20;
pub const SFLAG_FULLSTATS:  i32 = 0x40;
pub const SFLAG_GMON:       i32 = 0x80;

// settingFlags values for sd->status.settingFlags (from mmo.h)
pub const FLAG_WHISPER:   u32 = 1;
pub const FLAG_GROUP:     u32 = 2;
pub const FLAG_SHOUT:     u32 = 4;
pub const FLAG_ADVICE:    u32 = 8;
pub const FLAG_MAGIC:     u32 = 16;
pub const FLAG_WEATHER:   u32 = 32;
pub const FLAG_REALM:     u32 = 64;
pub const FLAG_EXCHANGE:  u32 = 128;
pub const FLAG_FASTMOVE:  u32 = 256;
pub const FLAG_SOUND:     u32 = 4096;
pub const FLAG_HELM:      u32 = 8192;
pub const FLAG_NECKLACE:  u32 = 16384;

// normalFlags (from mmo.h `enum normalFlags`)
pub const FLAG_PARCEL: u64 = 1;
pub const FLAG_MAIL:   u64 = 16;

// MAX_MAP_PER_SERVER (from mmo.h)
pub const MAX_MAP_PER_SERVER: i32 = 65535;

// SP_* parameter type constants (from map_server.h)
pub const SP_HP:  i32 = 0;
pub const SP_MP:  i32 = 1;
pub const SP_MHP: i32 = 2;
pub const SP_MMP: i32 = 3;

// AREA constant: enum value 4 in map_parse.h `{ ALL_CLIENT, SAMESRV, SAMEMAP,
//   SAMEMAP_WOS, AREA, ... }`
pub const AREA: i32 = 4;

// LOOK_SEND (enum { LOOK_GET=0, LOOK_SEND=1 } in map_parse.h)
pub const LOOK_SEND: i32 = 1;

// FLOOR subtype constant (enum { SCRIPT=0, FLOOR=1 } in map_server.h)
pub const FLOOR: u8 = 1;

// BLOCK_SIZE (from c_deps/yuri.h: `#define BLOCK_SIZE 8`)
pub const BLOCK_SIZE_PC: i32 = 8;

// MAX_GROUP_MEMBERS (from map_server.h `#define MAX_GROUP_MEMBERS 256`)
pub const MAX_GROUP_MEMBERS: usize = 256;

// ITM_* item type constants
pub const ITM_EAT:       i32 = 0;
pub const ITM_USE:       i32 = 1;
pub const ITM_SMOKE:     i32 = 2;
pub const ITM_WEAP:      i32 = 3;
pub const ITM_ARMOR:     i32 = 4;
pub const ITM_SHIELD:    i32 = 5;
pub const ITM_HELM:      i32 = 6;
pub const ITM_LEFT:      i32 = 7;
pub const ITM_RIGHT:     i32 = 8;
pub const ITM_SUBLEFT:   i32 = 9;
pub const ITM_SUBRIGHT:  i32 = 10;
pub const ITM_FACEACC:   i32 = 11;
pub const ITM_CROWN:     i32 = 12;
pub const ITM_MANTLE:    i32 = 13;
pub const ITM_NECKLACE:  i32 = 14;
pub const ITM_BOOTS:     i32 = 15;
pub const ITM_COAT:      i32 = 16;
pub const ITM_HAND:      i32 = 17;
pub const ITM_ETC:       i32 = 18;
pub const ITM_USESPC:    i32 = 19;
pub const ITM_TRAPS:     i32 = 20;
pub const ITM_BAG:       i32 = 21;
pub const ITM_MAP:       i32 = 22;
pub const ITM_QUIVER:    i32 = 23;
pub const ITM_MOUNT:     i32 = 24;
pub const ITM_FACE:      i32 = 25;
pub const ITM_SET:       i32 = 26;
pub const ITM_SKIN:      i32 = 27;
pub const ITM_HAIR_DYE:  i32 = 28;
pub const ITM_FACEACCTWO: i32 = 29;

// EQ_* equip slot constants
pub const EQ_WEAP:      i32 = 0;
pub const EQ_ARMOR:     i32 = 1;
pub const EQ_SHIELD:    i32 = 2;
pub const EQ_HELM:      i32 = 3;
pub const EQ_LEFT:      i32 = 4;
pub const EQ_RIGHT:     i32 = 5;
pub const EQ_SUBLEFT:   i32 = 6;
pub const EQ_SUBRIGHT:  i32 = 7;
pub const EQ_FACEACC:   i32 = 8;
pub const EQ_CROWN:     i32 = 9;
pub const EQ_MANTLE:    i32 = 10;
pub const EQ_NECKLACE:  i32 = 11;
pub const EQ_BOOTS:     i32 = 12;
pub const EQ_COAT:      i32 = 13;
pub const EQ_FACEACCTWO: i32 = 14;

// MAP_ERR* message indices (0-based, MAP_WHISPFAIL=0)
pub const MAP_WHISPFAIL:    usize = 0;
pub const MAP_ERRGHOST:     usize = 1;
pub const MAP_ERRITMLEVEL:  usize = 2;
pub const MAP_ERRITMMIGHT:  usize = 3;
pub const MAP_ERRITMGRACE:  usize = 4;
pub const MAP_ERRITMWILL:   usize = 5;
pub const MAP_ERRITMSEX:    usize = 6;
pub const MAP_ERRITMFULL:   usize = 7;
pub const MAP_ERRITMMAX:    usize = 8;
pub const MAP_ERRITMPATH:   usize = 9;
pub const MAP_ERRITMMARK:   usize = 10;
pub const MAP_ERRITM2H:     usize = 11;
pub const MAP_ERRMOUNT:     usize = 12;

// MapMsgData and map_msg are defined authoritatively in map_server.rs.
// Re-exported here so callers importing from `crate::game::pc` still find them.
pub use crate::game::map_server::{MapMsgData, map_msg};



use crate::game::map_server::{
    map_id2fl, map_delitem,
    map_additem, map_readglobalreg,
};
use crate::game::block_grid;
/// Legacy raw-pointer player lookup for deeply unsafe code paths.
fn map_id2sd_pc(id: u32) -> *mut MapSessionData {
    match crate::game::map_server::map_id2sd_pc(id) {
        Some(arc) => {
            let ptr = &mut *arc.write() as *mut MapSessionData;
            ptr
        }
        None => std::ptr::null_mut(),
    }
}
fn map_id2bl_pc(id: u32) -> *mut BlockList {
    crate::game::map_server::map_id2bl_ref(id)
}
use crate::game::map_parse::player_state::{
    clif_sendstatus, clif_getchararea, clif_refresh, clif_sendtime,
};
use crate::game::client::visual::clif_sendupdatestatus;
use crate::game::map_parse::movement::clif_sendchararea;
use crate::game::map_parse::chat::clif_sendminitext;
use crate::game::map_parse::items::{
    clif_sendadditem, clif_senddelitem, clif_sendequip,
};
use crate::game::map_parse::combat::{
    clif_sendmagic, clif_sendaction, clif_send_selfbar, clif_send_groupbars,
    clif_send_duration, clif_send_aether,
};
use crate::game::map_parse::visual::clif_spawn;
use crate::game::map_parse::groups::clif_grouphealth_update;
use crate::game::client::visual::{
    broadcast_update_state, clif_sendupdatestatus_onequip,
};
use crate::game::client::handlers::{clif_quit, clif_transfer};
use crate::game::time_util::{timer_insert, timer_remove};
use crate::game::scripting::sl_async_freeco;
use crate::database::item_db;
use crate::database::magic_db;
use crate::database::class_db::{path as classdb_path, level as classdb_level};
use crate::session::{session_exists, SessionId};
use crate::game::map_parse::packet::{wfifop, wfifohead, wfifoset};
use crate::network::crypt::encrypt;
unsafe fn encrypt_fd(fd: SessionId) -> i32 { encrypt(fd) }
use crate::game::scripting::pc_accessors::sl_pc_forcesave;
use crate::game::time_util::gettick;
unsafe fn gettick_pc() -> u32 { gettick() }
use crate::game::map_parse::visual::clif_lookgone;
unsafe fn clif_lookgone_pc(bl: *mut BlockList) { clif_lookgone(bl); }



// Re-export groups from map_server so that callers can import via crate::game::pc::groups.
pub use crate::game::map_server::groups;



use crate::game::block::AreaType;
use crate::game::map_parse::combat::{clif_send_mobbars_inner, clif_sendanimation_inner};
use crate::game::map_parse::visual::clif_object_look_sub2_inner;

// ─── Lua dispatch helpers ─────────────────────────────────────────────────────

/// Dispatch a Lua event with a single block_list argument.
unsafe fn sl_doscript_simple_pc(root: *const i8, method: *const i8, bl: *mut BlockList) -> i32 {
    crate::game::scripting::doscript_blargs(root, method, &[bl as *mut _])
}

/// Dispatch a Lua event with two block_list arguments.
unsafe fn sl_doscript_2_pc(root: *const i8, method: *const i8, bl1: *mut BlockList, bl2: *mut BlockList) -> i32 {
    crate::game::scripting::doscript_blargs(root, method, &[bl1 as *mut _, bl2 as *mut _])
}

// ─── Timer functions ─────────────────────────────────────────────────────────
//
// Naming: `pc_<name>` for pc_* functions.

// Each function is `` so C can call it back as a timer callback.

/// `int pc_item_timer(int id, int none)` — removes a floor item when its timer expires.
/// Calls `clif_lookgone` to hide it from clients, then `map_delitem` to remove it.
pub unsafe fn pc_item_timer(id: i32, _none: i32) -> i32 {
    let fl = map_id2bl_pc(id as u32);
    if fl.is_null() { return 1; }
    clif_lookgone_pc(fl);
    map_delitem(id as u32);
    1
}

/// `int pc_savetimer(int id, int none)` — periodically saves a player's character data.
pub unsafe fn pc_savetimer(id: i32, _none: i32) -> i32 {
    let sd = map_id2sd_pc(id as u32);
    if !sd.is_null() {
        sl_pc_forcesave(&mut *sd);
    }
    0
}

/// `int pc_castusetimer(int id, int none)` — resets `castusetimer` field to 0 each tick.
pub unsafe fn pc_castusetimer(id: i32, _none: i32) -> i32 {
    let sd = map_id2sd_pc(id as u32);
    if !sd.is_null() {
        (*sd).castusetimer = 0;
    }
    0
}

/// `int pc_afktimer(int id, int none)` — tracks AFK time and plays idle animations.
pub unsafe fn pc_afktimer(id: i32, _none: i32) -> i32 {
    let sd = map_id2sd_pc(id as u32);
    if sd.is_null() { return 0; }

    (*sd).afktime += 1;

    if (*sd).afk == 1 && (*sd).status.state == 0 {
        (*sd).totalafktime += 10;
        clif_sendaction(&mut (*sd).bl, 0x10, 0x4E, 0);
        return 0;
    }

    if (*sd).afk == 1 && (*sd).status.state == 3 {
        (*sd).totalafktime += 10;
        let sd_bl = &mut (*sd).bl as *mut BlockList;
        if let Some(grid) = block_grid::get_grid((*sd).bl.m as usize) {
            let slot = &*crate::database::map_db::get_map_ptr((*sd).bl.m as u16);
            let ids = block_grid::ids_in_area(grid, (*sd).bl.x as i32, (*sd).bl.y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
            for id in ids {
                if let Some(tsd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                    clif_sendanimation_inner(&mut tsd_arc.write().bl, 324, sd_bl, 0);
                }
            }
        }
        return 0;
    }

    if (*sd).afk == 1 && (*sd).status.state == PC_DIE as i8 {
        (*sd).totalafktime += 10;
        return 0;
    }

    if (*sd).afktime >= 30 {
        if (*sd).status.state == 0 {
            (*sd).totalafktime += 300;
            clif_sendaction(&mut (*sd).bl, 0x10, 0x4E, 0);
        } else if (*sd).status.state == 3 {
            (*sd).totalafktime += 300;
            let sd_bl = &mut (*sd).bl as *mut BlockList;
            if let Some(grid) = block_grid::get_grid((*sd).bl.m as usize) {
                let slot = &*crate::database::map_db::get_map_ptr((*sd).bl.m as u16);
                let ids = block_grid::ids_in_area(grid, (*sd).bl.x as i32, (*sd).bl.y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
                for id in ids {
                    if let Some(tsd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                        clif_sendanimation_inner(&mut tsd_arc.write().bl, 324, sd_bl, 0);
                    }
                }
            }
        }
        (*sd).afk = 1;
    }

    0
}

/// `int pc_starttimer(USER* sd)` — registers all periodic timers for a logged-in player.
pub unsafe fn pc_starttimer(sd: *mut MapSessionData) -> i32 {
    (*sd).timer = timer_insert(1000, 1000,
        Some(pc_timer as unsafe fn(i32, i32) -> i32),
        (*sd).bl.id as i32, 0);
    (*sd).pongtimer = timer_insert(30000, 30000,
        Some(pc_sendpong as unsafe fn(i32, i32) -> i32),
        (*sd).bl.id as i32, 0);
    (*sd).savetimer = timer_insert(60000, 60000,
        Some(pc_savetimer as unsafe fn(i32, i32) -> i32),
        (*sd).bl.id as i32, 0);
    if (*sd).status.gm_level < 50 {
        (*sd).afktimer = timer_insert(10000, 10000,
            Some(pc_afktimer as unsafe fn(i32, i32) -> i32),
            (*sd).bl.id as i32, 0);
    }
    (*sd).duratimer = timer_insert(1000, 1000,
        Some(bl_duratimer as unsafe fn(i32, i32) -> i32),
        (*sd).bl.id as i32, 0);
    (*sd).secondduratimer = timer_insert(250, 250,
        Some(bl_secondduratimer as unsafe fn(i32, i32) -> i32),
        (*sd).bl.id as i32, 0);
    (*sd).thirdduratimer = timer_insert(500, 500,
        Some(bl_thirdduratimer as unsafe fn(i32, i32) -> i32),
        (*sd).bl.id as i32, 0);
    (*sd).fourthduratimer = timer_insert(1500, 1500,
        Some(bl_fourthduratimer as unsafe fn(i32, i32) -> i32),
        (*sd).bl.id as i32, 0);
    (*sd).fifthduratimer = timer_insert(3000, 3000,
        Some(bl_fifthduratimer as unsafe fn(i32, i32) -> i32),
        (*sd).bl.id as i32, 0);
    (*sd).scripttimer = timer_insert(500, 500,
        Some(pc_scripttimer as unsafe fn(i32, i32) -> i32),
        (*sd).bl.id as i32, 0);
    (*sd).castusetimer = timer_insert(250, 250,
        Some(pc_castusetimer as unsafe fn(i32, i32) -> i32),
        (*sd).bl.id as i32, 0);
    0
}

/// `int pc_stoptimer(USER* sd)` — removes all periodic timers for a player.
pub unsafe fn pc_stoptimer(sd: *mut MapSessionData) -> i32 {
    if (*sd).timer != 0         { timer_remove((*sd).timer);         (*sd).timer = 0; }
    if (*sd).healingtimer != 0  { timer_remove((*sd).healingtimer);  (*sd).healingtimer = 0; }
    if (*sd).pongtimer != 0     { timer_remove((*sd).pongtimer);     (*sd).pongtimer = 0; }
    if (*sd).afktimer != 0      { timer_remove((*sd).afktimer);      (*sd).afktimer = 0; }
    if (*sd).duratimer != 0     { timer_remove((*sd).duratimer);     (*sd).duratimer = 0; }
    if (*sd).savetimer != 0     { timer_remove((*sd).savetimer);     (*sd).savetimer = 0; }
    if (*sd).secondduratimer != 0 { timer_remove((*sd).secondduratimer); (*sd).secondduratimer = 0; }
    if (*sd).thirdduratimer != 0  { timer_remove((*sd).thirdduratimer);  (*sd).thirdduratimer = 0; }
    if (*sd).fourthduratimer != 0 { timer_remove((*sd).fourthduratimer); (*sd).fourthduratimer = 0; }
    if (*sd).fifthduratimer != 0  { timer_remove((*sd).fifthduratimer);  (*sd).fifthduratimer = 0; }
    if (*sd).scripttimer != 0   { timer_remove((*sd).scripttimer);   (*sd).scripttimer = 0; }
    0
}

/// `int bl_duratimer(int id, int none)` — 1000ms tick: processes skill passive/equip
/// while-effects and decrements duration/aether for active magic on a player.
pub unsafe fn bl_duratimer(id: i32, _none: i32) -> i32 {
    use crate::game::mob::MAX_MAGIC_TIMERS;
    let sd = map_id2sd_pc(id as u32);
    if sd.is_null() { return 0; }

    // while_passive: each learned spell fires once per second
    for x in 0..52usize {
        if (*sd).status.skill[x] > 0 {
            sl_doscript_simple_pc(magic_db::search((*sd).status.skill[x] as i32).yname.as_ptr(), c"while_passive".as_ptr(), &mut (*sd).bl as *mut BlockList);
        }
    }

    // while_equipped: each worn item fires once per second
    for x in 0..14usize {
        if (*sd).status.equip[x].id > 0 {
            sl_doscript_simple_pc(item_db::search((*sd).status.equip[x].id).yname.as_ptr(), c"while_equipped".as_ptr(), &mut (*sd).bl as *mut BlockList);
        }
    }

    // duration / aether tick for each active magic timer slot
    for x in 0..MAX_MAGIC_TIMERS {
        let mid = (*sd).status.dura_aether[x].id as i32;
        if (*sd).status.dura_aether[x].id > 0 {
            let tbl: *mut BlockList = if (*sd).status.dura_aether[x].caster_id > 0 {
                map_id2bl_pc((*sd).status.dura_aether[x].caster_id)
            } else {
                std::ptr::null_mut()
            };

            if (*sd).status.dura_aether[x].duration > 0 {
                (*sd).status.dura_aether[x].duration -= 1000;

                if !tbl.is_null() {
                    // C initialises `health` as uninitialised long — translate as 0.
                    let health: i64 = if (*tbl).bl_type as i32 == BL_MOB {
                        let tmob = tbl as *mut crate::game::mob::MobSpawnData;
                        (*tmob).current_vita as i64
                    } else {
                        0
                    };
                    if health > 0 || (*tbl).bl_type as i32 == BL_PC {
                        sl_doscript_2_pc(magic_db::search(mid).yname.as_ptr(), c"while_cast".as_ptr(), &mut (*sd).bl as *mut BlockList, tbl);
                    }
                } else {
                    sl_doscript_simple_pc(magic_db::search(mid).yname.as_ptr(), c"while_cast".as_ptr(), &mut (*sd).bl as *mut BlockList);
                }

                if (*sd).status.dura_aether[x].duration <= 0 {
                    (*sd).status.dura_aether[x].duration = 0;
                    clif_send_duration(
                        &mut *sd,
                        (*sd).status.dura_aether[x].id as i32,
                        0i32,
                        map_id2sd_pc((*sd).status.dura_aether[x].caster_id),
                    );
                    (*sd).status.dura_aether[x].caster_id = 0;
                    {
                        let anim = (*sd).status.dura_aether[x].animation as i32;
                        let sd_bl = &mut (*sd).bl as *mut BlockList;
                        if let Some(grid) = block_grid::get_grid((*sd).bl.m as usize) {
                            let slot = &*crate::database::map_db::get_map_ptr((*sd).bl.m as u16);
                            let ids = block_grid::ids_in_area(grid, (*sd).bl.x as i32, (*sd).bl.y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
                            for id in ids {
                                if let Some(tsd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                                    clif_sendanimation_inner(&mut tsd_arc.write().bl, anim, sd_bl, -1);
                                }
                            }
                        }
                    }
                    (*sd).status.dura_aether[x].animation = 0;

                    if (*sd).status.dura_aether[x].aether == 0 {
                        (*sd).status.dura_aether[x].id = 0;
                    }

                    if !tbl.is_null() {
                        sl_doscript_2_pc(magic_db::search(mid).yname.as_ptr(), c"uncast".as_ptr(), &mut (*sd).bl as *mut BlockList, tbl);
                    } else {
                        sl_doscript_simple_pc(magic_db::search(mid).yname.as_ptr(), c"uncast".as_ptr(), &mut (*sd).bl as *mut BlockList);
                    }
                }
            }

            if (*sd).status.dura_aether[x].aether > 0 {
                (*sd).status.dura_aether[x].aether -= 1000;

                if (*sd).status.dura_aether[x].aether <= 0 {
                    clif_send_aether(&mut *sd, (*sd).status.dura_aether[x].id as i32, 0);

                    if (*sd).status.dura_aether[x].duration == 0 {
                        (*sd).status.dura_aether[x].id = 0;
                    }

                    (*sd).status.dura_aether[x].aether = 0;
                }
            }
        }
    }

    0
}

/// `int bl_secondduratimer(int id, int none)` — 250ms tick: fires `while_passive_250`
/// and `while_equipped_250` and `while_cast_250` events (no expire logic).
pub unsafe fn bl_secondduratimer(id: i32, _none: i32) -> i32 {
    use crate::game::mob::MAX_MAGIC_TIMERS;
    let sd = map_id2sd_pc(id as u32);
    if sd.is_null() { return 0; }

    for x in 0..52usize {
        if (*sd).status.skill[x] > 0 {
            sl_doscript_simple_pc(magic_db::search((*sd).status.skill[x] as i32).yname.as_ptr(), c"while_passive_250".as_ptr(), &mut (*sd).bl as *mut BlockList);
        }
    }

    for x in 0..14usize {
        if (*sd).status.equip[x].id > 0 {
            sl_doscript_simple_pc(item_db::search((*sd).status.equip[x].id).yname.as_ptr(), c"while_equipped_250".as_ptr(), &mut (*sd).bl as *mut BlockList);
        }
    }

    for x in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[x].id > 0 {
            let tbl: *mut BlockList = if (*sd).status.dura_aether[x].caster_id > 0 {
                map_id2bl_pc((*sd).status.dura_aether[x].caster_id)
            } else {
                std::ptr::null_mut()
            };

            if (*sd).status.dura_aether[x].duration > 0 {
                if !tbl.is_null() {
                    let health: i64 = if (*tbl).bl_type as i32 == BL_MOB {
                        let tmob = tbl as *mut crate::game::mob::MobSpawnData;
                        (*tmob).current_vita as i64
                    } else {
                        0
                    };
                    if health > 0 || (*tbl).bl_type as i32 == BL_PC {
                        sl_doscript_2_pc(magic_db::search((*sd).status.dura_aether[x].id as i32).yname.as_ptr(), c"while_cast_250".as_ptr(), &mut (*sd).bl as *mut BlockList, tbl);
                    }
                } else {
                    sl_doscript_simple_pc(magic_db::search((*sd).status.dura_aether[x].id as i32).yname.as_ptr(), c"while_cast_250".as_ptr(), &mut (*sd).bl as *mut BlockList);
                }
            }
        }
    }

    0
}

/// `int bl_thirdduratimer(int id, int none)` — 500ms tick: fires `while_passive_500`,
/// `while_equipped_500`, `while_cast_500` events (no expire logic).
pub unsafe fn bl_thirdduratimer(id: i32, _none: i32) -> i32 {
    use crate::game::mob::MAX_MAGIC_TIMERS;
    let sd = map_id2sd_pc(id as u32);
    if sd.is_null() { return 0; }

    for x in 0..52usize {
        if (*sd).status.skill[x] > 0 {
            sl_doscript_simple_pc(magic_db::search((*sd).status.skill[x] as i32).yname.as_ptr(), c"while_passive_500".as_ptr(), &mut (*sd).bl as *mut BlockList);
        }
    }

    for x in 0..14usize {
        if (*sd).status.equip[x].id > 0 {
            sl_doscript_simple_pc(item_db::search((*sd).status.equip[x].id).yname.as_ptr(), c"while_equipped_500".as_ptr(), &mut (*sd).bl as *mut BlockList);
        }
    }

    for x in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[x].id > 0 {
            let tbl: *mut BlockList = if (*sd).status.dura_aether[x].caster_id > 0 {
                map_id2bl_pc((*sd).status.dura_aether[x].caster_id)
            } else {
                std::ptr::null_mut()
            };

            if (*sd).status.dura_aether[x].duration > 0 {
                if !tbl.is_null() {
                    let health: i64 = if (*tbl).bl_type as i32 == BL_MOB {
                        let tmob = tbl as *mut crate::game::mob::MobSpawnData;
                        (*tmob).current_vita as i64
                    } else {
                        0
                    };
                    if health > 0 || (*tbl).bl_type as i32 == BL_PC {
                        sl_doscript_2_pc(magic_db::search((*sd).status.dura_aether[x].id as i32).yname.as_ptr(), c"while_cast_500".as_ptr(), &mut (*sd).bl as *mut BlockList, tbl);
                    }
                } else {
                    sl_doscript_simple_pc(magic_db::search((*sd).status.dura_aether[x].id as i32).yname.as_ptr(), c"while_cast_500".as_ptr(), &mut (*sd).bl as *mut BlockList);
                }
            }
        }
    }

    0
}

/// `int bl_fourthduratimer(int id, int none)` — 1500ms tick: fires `while_passive_1500`,
/// `while_equipped_1500`, `while_cast_1500` events (no expire logic).
pub unsafe fn bl_fourthduratimer(id: i32, _none: i32) -> i32 {
    use crate::game::mob::MAX_MAGIC_TIMERS;
    let sd = map_id2sd_pc(id as u32);
    if sd.is_null() { return 0; }

    for x in 0..52usize {
        if (*sd).status.skill[x] > 0 {
            sl_doscript_simple_pc(magic_db::search((*sd).status.skill[x] as i32).yname.as_ptr(), c"while_passive_1500".as_ptr(), &mut (*sd).bl as *mut BlockList);
        }
    }

    for x in 0..14usize {
        if (*sd).status.equip[x].id > 0 {
            sl_doscript_simple_pc(item_db::search((*sd).status.equip[x].id).yname.as_ptr(), c"while_equipped_1500".as_ptr(), &mut (*sd).bl as *mut BlockList);
        }
    }

    for x in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[x].id > 0 {
            let tbl: *mut BlockList = if (*sd).status.dura_aether[x].caster_id > 0 {
                map_id2bl_pc((*sd).status.dura_aether[x].caster_id)
            } else {
                std::ptr::null_mut()
            };

            if (*sd).status.dura_aether[x].duration > 0 {
                if !tbl.is_null() {
                    let health: i64 = if (*tbl).bl_type as i32 == BL_MOB {
                        let tmob = tbl as *mut crate::game::mob::MobSpawnData;
                        (*tmob).current_vita as i64
                    } else {
                        0
                    };
                    if health > 0 || (*tbl).bl_type as i32 == BL_PC {
                        sl_doscript_2_pc(magic_db::search((*sd).status.dura_aether[x].id as i32).yname.as_ptr(), c"while_cast_1500".as_ptr(), &mut (*sd).bl as *mut BlockList, tbl);
                    }
                } else {
                    sl_doscript_simple_pc(magic_db::search((*sd).status.dura_aether[x].id as i32).yname.as_ptr(), c"while_cast_1500".as_ptr(), &mut (*sd).bl as *mut BlockList);
                }
            }
        }
    }

    0
}

/// `int bl_fifthduratimer(int id, int none)` — 3000ms tick: fires `while_passive_3000`,
/// `while_equipped_3000`, `while_cast_3000` events (no expire logic).
pub unsafe fn bl_fifthduratimer(id: i32, _none: i32) -> i32 {
    use crate::game::mob::MAX_MAGIC_TIMERS;
    let sd = map_id2sd_pc(id as u32);
    if sd.is_null() { return 0; }

    for x in 0..52usize {
        if (*sd).status.skill[x] > 0 {
            sl_doscript_simple_pc(magic_db::search((*sd).status.skill[x] as i32).yname.as_ptr(), c"while_passive_3000".as_ptr(), &mut (*sd).bl as *mut BlockList);
        }
    }

    for x in 0..14usize {
        if (*sd).status.equip[x].id > 0 {
            sl_doscript_simple_pc(item_db::search((*sd).status.equip[x].id).yname.as_ptr(), c"while_equipped_3000".as_ptr(), &mut (*sd).bl as *mut BlockList);
        }
    }

    for x in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[x].id > 0 {
            let tbl: *mut BlockList = if (*sd).status.dura_aether[x].caster_id > 0 {
                map_id2bl_pc((*sd).status.dura_aether[x].caster_id)
            } else {
                std::ptr::null_mut()
            };

            if (*sd).status.dura_aether[x].duration > 0 {
                if !tbl.is_null() {
                    let health: i64 = if (*tbl).bl_type as i32 == BL_MOB {
                        let tmob = tbl as *mut crate::game::mob::MobSpawnData;
                        (*tmob).current_vita as i64
                    } else {
                        0
                    };
                    if health > 0 || (*tbl).bl_type as i32 == BL_PC {
                        sl_doscript_2_pc(magic_db::search((*sd).status.dura_aether[x].id as i32).yname.as_ptr(), c"while_cast_3000".as_ptr(), &mut (*sd).bl as *mut BlockList, tbl);
                    }
                } else {
                    sl_doscript_simple_pc(magic_db::search((*sd).status.dura_aether[x].id as i32).yname.as_ptr(), c"while_cast_3000".as_ptr(), &mut (*sd).bl as *mut BlockList);
                }
            }
        }
    }

    0
}

/// `int bl_aethertimer(int id, int none)` — decrements aether timers and clears
/// expired aether slots; called from NPC/scripting code via a one-shot timer.
pub unsafe fn bl_aethertimer(id: i32, _none: i32) -> i32 {
    use crate::game::mob::MAX_MAGIC_TIMERS;
    let sd = map_id2sd_pc(id as u32);
    if sd.is_null() { return 0; }

    for x in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[x].id > 0 {
            if (*sd).status.dura_aether[x].aether > 0 {
                (*sd).status.dura_aether[x].aether -= 1000;
            }

            if (*sd).status.dura_aether[x].aether <= 0 {
                clif_send_aether(&mut *sd, (*sd).status.dura_aether[x].id as i32, 0);

                if (*sd).status.dura_aether[x].duration == 0 {
                    (*sd).status.dura_aether[x].id = 0;
                }

                (*sd).status.dura_aether[x].aether = 0;
                return 0;
            }
        }
    }

    0
}

/// `int pc_timer(int id, int none)` — 1000ms main player tick: resets cooldowns,
/// expires PvP flags, decrements PK duration, and updates group health bars.
pub unsafe fn pc_timer(id: i32, _none: i32) -> i32 {
    let sd = map_id2sd_pc(id as u32);
    if sd.is_null() { return 1; }

    (*sd).time2 += 1000;
    (*sd).time = 0;
    (*sd).chat_timer = 0;

    if (*sd).time2 >= 60000 {
        pc_requestmp(sd);
        (*sd).time2 = 0;
    }

    let mut reset: i32 = 0;
    for x in 0..20usize {
        if (*sd).pvp[x][1] != 0 {
            if gettick_pc().wrapping_sub((*sd).pvp[x][1]) >= 60000 {
                (*sd).pvp[x][0] = 0;
                (*sd).pvp[x][1] = 0;
                reset = 1;
            }
        }
    }

    if (*sd).status.pk == 1 && (*sd).status.pkduration > 0 {
        (*sd).status.pkduration -= 1000;

        if (*sd).status.pkduration == 0 {
            (*sd).status.pk = 0;
            clif_sendchararea(sd);
        }
    }

    if (*sd).group_count > 0 {
        clif_grouphealth_update(sd);
    }

    if reset != 0 {
        clif_getchararea(sd);
    }

    0
}

/// `int pc_scripttimer(int id, int none)` — 500ms script tick: updates UI bars,
/// fires die script on death, fires Lua `pc_timer` tick/advice hooks.
pub unsafe fn pc_scripttimer(id: i32, _none: i32) -> i32 {
    let sd = map_id2sd_pc(id as u32);
    if sd.is_null() { return 1; }

    if (*sd).selfbar != 0 {
        clif_send_selfbar(&mut *sd);
    }

    if (*sd).groupbars != 0 && (*sd).group_count > 1 {
        let base = (*sd).groupid as usize * 256;
        let grp = groups();
        if base < grp.len() {
            for x in 0..(*sd).group_count as usize {
                if base + x >= grp.len() { break; }
                let tsd = map_id2sd_pc(grp[base + x]);
                if tsd.is_null() { continue; }
                if (*tsd).bl.m == (*sd).bl.m {
                    clif_send_groupbars(&mut *sd, &mut *tsd);
                    clif_grouphealth_update(sd);
                }
            }
        }
    }

    if (*sd).mobbars != 0 {
        if let Some(grid) = block_grid::get_grid((*sd).bl.m as usize) {
            let slot = &*crate::database::map_db::get_map_ptr((*sd).bl.m as u16);
            let ids = block_grid::ids_in_area(grid, (*sd).bl.x as i32, (*sd).bl.y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
            for id in ids {
                if let Some(mob_arc) = crate::game::map_server::map_id2mob_ref(id) {
                    let mut mob = mob_arc.write();
                    clif_send_mobbars_inner(&mut mob.bl, &mut *sd);
                }
            }
        }
    }

    if (*sd).status.hp == 0 && (*sd).deathflag != 0 {
        pc_diescript(sd);
        return 0;
    }

    if (*sd).dmgshield > 0.0 {
        clif_send_duration(&mut *sd, 0, (*sd).dmgshield as i32 + 1, std::ptr::null_mut());
    }

    (*sd).deathflag = 0;
    (*sd).scripttick += 1;

    sl_doscript_simple_pc(c"pc_timer".as_ptr(), c"tick".as_ptr(), &mut (*sd).bl as *mut BlockList);

    if (*sd).status.setting_flags & FLAG_ADVICE as u16 != 0 {
        sl_doscript_simple_pc(c"pc_timer".as_ptr(), c"advice".as_ptr(), &mut (*sd).bl as *mut BlockList);
    }

    0
}

/// `int pc_atkspeed(int id, int none)` — resets `attacked` flag; called by a one-shot timer.
pub unsafe fn pc_atkspeed(id: i32, _none: i32) -> i32 {
    let sd = map_id2sd_pc(id as u32);
    if sd.is_null() {
        tracing::warn!("[attack] pc_atkspeed: id={} sd=null, removing timer", id);
        return 1;
    }
    tracing::debug!("[attack] pc_atkspeed: id={} resetting attacked from {} to 0", id, (*sd).attacked);
    (*sd).attacked = 0;
    1
}

/// `int pc_disptimertick(int id, int none)` — counts down the display timer and fires
/// the Lua `display_timer` event when it reaches zero.
pub unsafe fn pc_disptimertick(id: i32, _none: i32) -> i32 {
    let sd = map_id2sd_pc(id as u32);
    if sd.is_null() { return 1; }

    if ((*sd).disptimertick as i64) - 1 < 0 {
        (*sd).disptimertick = 0;
    } else {
        (*sd).disptimertick -= 1;
    }

    if (*sd).disptimertick == 0 {
        sl_doscript_simple_pc(c"pc_timer".as_ptr(), c"display_timer".as_ptr(), &mut (*sd).bl as *mut BlockList);
        timer_remove((*sd).disptimer as i32);
        (*sd).disptimertype = 0;
        (*sd).disptimer = 0;
        return 1;
    }

    0
}

/// `int pc_sendpong(int id, int none)` — sends a keep-alive ping packet to the client
/// and sets EOF if the session has already closed.

pub unsafe fn pc_sendpong(id: i32, _none: i32) -> i32 {
    let sd = map_id2sd_pc(id as u32);
    if sd.is_null() { return 1; }

    if !session_exists((*sd).fd) {
        return 0;
    }

    // WFIFOHEAD(fd, 10)
    wfifohead((*sd).fd, 10);

    // WFIFOB(fd, 0) = 0xAA
    let p = wfifop((*sd).fd, 0);
    if !p.is_null() { *p = 0xAAu8; }

    // WFIFOW(fd, 1) = SWAP16(0x09)  — big-endian 16-bit (byte-swap of 0x0009 → 0x0900)
    let p = wfifop((*sd).fd, 1) as *mut u16;
    if !p.is_null() { p.write_unaligned(0x09u16.swap_bytes()); }

    // WFIFOB(fd, 3) = 0x68
    let p = wfifop((*sd).fd, 3);
    if !p.is_null() { *p = 0x68u8; }

    // WFIFOL(fd, 5) = SWAP32(gettick())  — big-endian 32-bit tick
    let tick = gettick_pc();
    let p = wfifop((*sd).fd, 5) as *mut u32;
    if !p.is_null() { p.write_unaligned(tick.swap_bytes()); }

    // WFIFOB(fd, 9) = 0x00
    let p = wfifop((*sd).fd, 9);
    if !p.is_null() { *p = 0x00u8; }

    // WFIFOSET(fd, encrypt(fd))
    let enc_len = encrypt_fd((*sd).fd);
    wfifoset((*sd).fd, enc_len as usize);

    (*sd).LastPingTick = gettick_pc() as u64;
    0
}

// ─── Stat-calculation functions ───────────────────────────────────────────────

/// `int pc_requestmp(USER *sd)` — checks mail and parcel tables via SQL and sets
/// FLAG_MAIL / FLAG_PARCEL bits on `sd->flags`.
///
async fn check_new_mail(char_name: String) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM `Mail` WHERE `MalNew` = 1 AND `MalChaNameDestination` = ?"
    )
    .bind(char_name)
    .fetch_one(crate::database::get_pool())
    .await
    .unwrap_or(0) > 0
}

async fn check_pending_parcels(char_id: u32) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM `Parcels` WHERE `ParChaIdDestination` = ?"
    )
    .bind(char_id)
    .fetch_one(crate::database::get_pool())
    .await
    .unwrap_or(0) > 0
}

pub unsafe fn pc_requestmp(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }

    (*sd).flags = 0;

    let char_name = std::ffi::CStr::from_ptr((*sd).status.name.as_ptr())
        .to_str().unwrap_or("").to_owned();
    let char_id = (*sd).status.id;

    // EXEMPT from async conversion: this function is called from sync contexts
    // (timer callback pc_timer, Lua sl_pc_sendstatus, and the login sequence
    // intif_mmo_tosd). The flags must be set before clif_sendstatus writes them
    // into the login packet, so fire-and-forget is not safe here. Converting to
    // native async would require cascading intif_mmo_tosd → async, which is a
    // large refactor deferred to a later task.
    if crate::database::blocking_run_async(check_new_mail(char_name)) {
        (*sd).flags |= FLAG_MAIL;
    }
    if crate::database::blocking_run_async(check_pending_parcels(char_id)) {
        (*sd).flags |= FLAG_PARCEL;
    }

    0
}

/// `int pc_checklevel(USER *sd)` — iterates from current level to 99, checks if
/// the player's XP meets the threshold, and fires the "onLevel" script for each
/// level they qualify for.
///
pub unsafe fn pc_checklevel(sd: *mut MapSessionData) -> i32 {
    let path_raw = (*sd).status.class as i32;
    let path = if path_raw > 5 { classdb_path(path_raw) } else { path_raw };

    for x in (*sd).status.level as i32..99 {
        let lvlxp = classdb_level(path, x);
        if (*sd).status.exp >= lvlxp {
            sl_doscript_simple_pc(c"onLevel".as_ptr(), std::ptr::null(), &mut (*sd).bl as *mut BlockList);
        }
    }

    0
}

/// `int pc_givexp(USER *sd, unsigned int exp, unsigned int xprate)` — awards XP to
/// the player, checking stack-on-player and AFK conditions first, then calls
/// `pc_checklevel` and sends status updates.
///
/// Note: the `if (exp < 0)` branch in C is dead code because `exp` is `unsigned int`
/// and can never be negative; it is preserved here for faithful translation.
pub unsafe fn pc_givexp(
    sd: *mut MapSessionData,
    exp: u32,
    xprate: u32,
) -> i32 {
    let mut xpstring = [0i8; 256];
    let mut stack: i32 = 0;

    // stack check — count non-GM PCs at the exact same tile
    let sx = (*sd).bl.x;
    let sy = (*sd).bl.y;
    if let Some(grid) = crate::game::block_grid::get_grid((*sd).bl.m as usize) {
        for id in grid.ids_at_tile(sx, sy) {
            if stack >= 32768 { break; }
            if let Some(tsd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                let tsd = tsd_arc.read();
                if tsd.bl.x == sx && tsd.bl.y == sy && tsd.status.gm_level == 0 {
                    stack += 1;
                }
            }
        }
    }

    if stack > 1 {
        let msg = b"You cannot gain experience while on top of other players.\0";
        libc::snprintf(
            xpstring.as_mut_ptr(),
            xpstring.len(),
            msg.as_ptr() as *const i8,
        );
        clif_sendminitext(sd, xpstring.as_ptr());
        return 0;
    }

    // AFK check
    if (*sd).afk == 1 {
        let msg = b"You cannot gain experience while AFK.\0";
        libc::snprintf(
            xpstring.as_mut_ptr(),
            xpstring.len(),
            msg.as_ptr() as *const i8,
        );
        clif_sendminitext(sd, xpstring.as_ptr());
        return 0;
    }

    if exp == 0 { return 0; }

    // cast to i64 makes this unreachable; preserved as dead code matching C original where exp is unsigned int
    if (exp as i64) < 0 {
        if ((*sd).status.exp as i64) < (exp as i64).abs() {
            (*sd).status.exp = 0;
        } else {
            (*sd).status.exp = (*sd).status.exp.wrapping_add(exp);
        }
        return 0;
    }

    let totalxp: i64 = (exp as i64).wrapping_mul(xprate as i64);
    let difxp: u32 = 4294967295u32.wrapping_sub((*sd).status.exp);

    let (tempxp, defaultxp): (u32, u32) = if (difxp as i64) > totalxp {
        (
            (*sd).status.exp.wrapping_add(totalxp as u32),
            totalxp as u32,
        )
    } else {
        (
            (*sd).status.exp.wrapping_add(difxp),
            difxp,
        )
    };

    (*sd).status.exp = tempxp;

    libc::snprintf(
        xpstring.as_mut_ptr(),
        xpstring.len(),
        c"%u experience!".as_ptr(),
        defaultxp,
    );

    pc_checklevel(sd);
    clif_sendminitext(sd, xpstring.as_ptr());
    clif_sendstatus(sd, SFLAG_XPMONEY);
    clif_sendupdatestatus_onequip(sd);

    0
}

/// `int pc_calcstat(USER *sd)` — recalculates all derived stats from base stats and
/// equipped items, applies active magic aether/passive skills, computes TNL percentage,
/// clamps all stats, then sends a full status update to the client.
///
pub unsafe fn pc_calcstat(sd: *mut MapSessionData) -> i32 {
    use crate::game::mob::MAX_MAGIC_TIMERS;

    if sd.is_null() { return 0; }

    // Reset combat modifiers
    (*sd).dam       = 0;
    (*sd).hit       = 0;
    (*sd).miss      = 0;
    (*sd).crit      = 0;
    (*sd).critmult  = 0.0f32;
    (*sd).deduction = 1.0f32;
    (*sd).snare     = 0;
    (*sd).sleep     = 1.0f32;
    (*sd).silence   = 0;
    (*sd).paralyzed = 0;
    (*sd).blind     = 0;
    (*sd).drunk     = 0;

    if (*sd).rage == 0.0f32      { (*sd).rage     = 1.0f32; }
    if (*sd).enchanted == 0.0f32 { (*sd).enchanted = 1.0f32; }

    // C: `if (sd->status.basehp <= 0)` — unsigned int, so equivalent to == 0.
    if (*sd).status.basehp == 0 { (*sd).status.basehp = 5; }
    if (*sd).status.basemp == 0 { (*sd).status.basemp = 5; }

    // Copy base stats
    (*sd).armor   = (*sd).status.basearmor  as i32;
    (*sd).max_hp  = (*sd).status.basehp;
    (*sd).max_mp  = (*sd).status.basemp;
    (*sd).might   = (*sd).status.basemight  as i32;
    (*sd).grace   = (*sd).status.basegrace  as i32;
    (*sd).will    = (*sd).status.basewill   as i32;

    (*sd).maxSdam = 0;
    (*sd).minSdam = 0;
    (*sd).minLdam = 0;
    (*sd).maxLdam = 0;

    (*sd).attack_speed = 20;
    (*sd).protection   = 0;
    (*sd).healing      = 0;
    (*sd).status.tnl   = 0;
    (*sd).status.realtnl = 0;

    // Accumulate stats from equipped items
    for x in 0..14usize {
        let id = (*sd).status.equip[x].id;
        if id > 0 {
            let db = item_db::search(id);
            (*sd).max_hp  = (*sd).max_hp.wrapping_add(db.vita  as u32);
            (*sd).max_mp  = (*sd).max_mp.wrapping_add(db.mana  as u32);
            (*sd).might   += db.might;
            (*sd).grace   += db.grace;
            (*sd).will    += db.will;
            (*sd).armor   += db.ac;
            (*sd).healing += db.healing;
            (*sd).dam     += db.dam;
            (*sd).hit     += db.hit;
            (*sd).minSdam += db.min_sdam as i32; // u32 field, i32 accumulator
            (*sd).maxSdam += db.max_sdam as i32;
            (*sd).minLdam += db.min_ldam as i32;
            (*sd).maxLdam += db.max_ldam as i32;
            (*sd).protection = ((*sd).protection as i32 + db.protection) as i16;
        }
    }

    // Mount state
    if (*sd).status.state == PC_MOUNTED as i8 {
        if (*sd).status.gm_level == 0 {
            if (*sd).speed < 40 { (*sd).speed = 40; }
        }
        sl_doscript_simple_pc(c"remount".as_ptr(), std::ptr::null(), &mut (*sd).bl as *mut BlockList);
    } else {
        (*sd).speed = 90;
    }

    // Fire recast and passive scripts (only when alive)
    if (*sd).status.state != PC_DIE as i8 {
        // Recast active magic aether slots
        for x in 0..MAX_MAGIC_TIMERS {
            let p = &(*sd).status.dura_aether[x];
            if p.id > 0 && p.duration > 0 {
                let tsd = map_id2sd_pc(p.caster_id);
                if !tsd.is_null() {
                    sl_doscript_2_pc(magic_db::search(p.id as i32).yname.as_ptr(), c"recast".as_ptr(), &mut (*sd).bl as *mut BlockList, &mut (*tsd).bl as *mut BlockList);
                } else {
                    // sl_doscript_simple(magicdb_yname(p->id), "recast", &sd->bl)
                    sl_doscript_simple_pc(magic_db::search(p.id as i32).yname.as_ptr(), c"recast".as_ptr(), &mut (*sd).bl as *mut BlockList);
                }
            }
        }

        // Passive skills
        for x in 0..52usize {
            if (*sd).status.skill[x] > 0 {
                sl_doscript_simple_pc(magic_db::search((*sd).status.skill[x] as i32).yname.as_ptr(), c"passive".as_ptr(), &mut (*sd).bl as *mut BlockList);
            }
        }

        // Re-equip scripts
        for x in 0..14usize {
            if (*sd).status.equip[x].id > 0 {
                sl_doscript_simple_pc(item_db::search((*sd).status.equip[x].id).yname.as_ptr(), c"re_equip".as_ptr(), &mut (*sd).bl as *mut BlockList);
            }
        }
    }

    // Compute TNL percentage for group status window (added 8-5-16)
    if (*sd).status.tnl == 0 {
        let path_raw = (*sd).status.class as i32;
        let path = if path_raw > 5 { classdb_path(path_raw) } else { path_raw };
        let level = (*sd).status.level as i32;

        if level < 99 {
            let helper = classdb_level(path, level).wrapping_sub(classdb_level(path, level - 1)) as i64;
            let tnl    = classdb_level(path, level) as i64 - (*sd).status.exp as i64;
            let mut percentage = (((helper - tnl) as f32) / (helper as f32)) * 100.0f32;
            // C bug preserved: tnl assigned before death-penalty correction; C never re-assigns it
            (*sd).status.tnl = percentage as i32 as u32;
            if tnl > helper {
                // XP went below previous level threshold (e.g. after a death penalty);
                // recomputes percentage for internal use only — status.tnl is NOT updated here (matches C)
                percentage = ((*sd).status.exp as f32 / helper as f32) * 100.0f32 + 0.5f32;
            }
            let _ = percentage; // suppress unused-variable warning; death-penalty path uses it in C for nothing further
        } else {
            (*sd).status.tnl = (((*sd).status.exp as f64 / 4294967295.0f64) * 100.0f64) as i32 as u32;
        }
    }

    // Compute real TNL for F1 menu (added 8-6-16)
    if (*sd).status.realtnl == 0 {
        let path_raw = (*sd).status.class as i32;
        let path = if path_raw > 5 { classdb_path(path_raw) } else { path_raw };
        let level = (*sd).status.level as i32;

        if level < 99 {
            let tnl = classdb_level(path, level) as i64 - (*sd).status.exp as i64;
            (*sd).status.realtnl = tnl as i32 as u32;
        } else {
            (*sd).status.realtnl = 0;
        }
    }

    // Clamp stat values
    if (*sd).might  > 255 { (*sd).might  = 255; }
    if (*sd).grace  > 255 { (*sd).grace  = 255; }
    if (*sd).will   > 255 { (*sd).will   = 255; }
    if (*sd).might  < 0   { (*sd).might  = 0;   }
    if (*sd).grace  < 0   { (*sd).grace  = 0;   }
    if (*sd).will   < 0   { (*sd).will   = 0;   }

    if (*sd).dam    < 0   { (*sd).dam    = 0;   }
    if (*sd).dam    > 255 { (*sd).dam    = 255; }
    if (*sd).armor  < -127 { (*sd).armor = -127; }
    if (*sd).armor  > 127  { (*sd).armor =  127; }
    if (*sd).dam    < 0   { (*sd).dam    = 0;   }   // duplicate clamp, preserved faithfully
    if (*sd).attack_speed < 3 { (*sd).attack_speed = 3; }

    // Global map health/magic overrides
    let max_health = map_readglobalreg((*sd).bl.m as i32, c"maxHealth".as_ptr());
    let max_magic  = map_readglobalreg((*sd).bl.m as i32, c"maxMagic".as_ptr());
    if max_health > 0 { (*sd).max_hp = max_health as u32; }
    if max_magic  > 0 { (*sd).max_mp = max_magic  as u32; }

    // Clamp current HP/MP
    if (*sd).status.hp > (*sd).max_hp { (*sd).status.hp = (*sd).max_hp; }
    if (*sd).status.mp > (*sd).max_mp { (*sd).status.mp = (*sd).max_mp; }

    clif_sendstatus(sd, SFLAG_FULLSTATS | SFLAG_HPMP | SFLAG_XPMONEY);

    0
}

/// `float pc_calcdamage(USER *sd)` — calculates the physical damage the player
/// can deal: base damage from might plus a random roll from equipped weapon range.
///
pub unsafe fn pc_calcdamage(sd: *mut MapSessionData) -> f32 {
    let mut damage: f32 = 6.0f32 + ((*sd).might as f32) / 8.0f32;

    if (*sd).minSdam > 0 && (*sd).maxSdam > 0 {
        let mut ran = (*sd).maxSdam - (*sd).minSdam;
        if ran <= 0 { ran = 1; }
        ran = ((rand::random::<u32>() & 0x00FF_FFFF) % (ran as u32)) as i32 + (*sd).minSdam;
        damage += (ran as f32) / 2.0f32;
    }

    damage
}

// ─── Registry functions ───────────────────────────────────────────────────────
//
// These functions manage player variable storage (local and global registries).
// Local registries (reg/regstr) are heap-allocated growable arrays on MapSessionData.
// Global registries (global_reg, global_regstring, acctreg, npcintreg, questreg)
// are fixed-size arrays in MmoCharStatus, found by scanning for matching key strings.
//
// All string comparisons use `libc::strcasecmp` (case-insensitive), matching C.
// String copies into fixed [i8; N] arrays use `libc::strcpy` (safe within bounds).

// ── Local integer registry (per-script, heap-allocated) ──────────────────────

/// `int pc_readreg(USER *sd, int reg)` — reads a local integer variable by index.
///
/// Scans `sd->reg[0..reg_num]` for a slot with `index == reg`.
pub unsafe fn pc_readreg(sd: *mut MapSessionData, reg: i32) -> i32 {
    if sd.is_null() { return 0; }
    let sd = &*sd;
    let reg_arr = std::slice::from_raw_parts(sd.reg, sd.reg_num as usize);
    for r in reg_arr {
        if r.index == reg { return r.data; }
    }
    0
}

/// `int pc_setreg(USER *sd, int reg, int val)` — sets a local integer variable by index.
///
/// Scans for an existing slot; if found, updates `data`. If not found, grows the
/// `reg` array, zeroes the new slot, then sets index and data.
pub unsafe fn pc_setreg(sd: *mut MapSessionData, reg: i32, val: i32) -> i32 {
    if sd.is_null() { return 0; }
    // Search for existing slot
    for i in 0..(*sd).reg_num as usize {
        if (*(*sd).reg.add(i)).index == reg {
            (*(*sd).reg.add(i)).data = val;
            return 0;
        }
    }
    // Not found — grow array
    let new_num = (*sd).reg_num + 1;
    let new_ptr = libc::realloc(
        (*sd).reg as *mut libc::c_void,
        new_num as usize * std::mem::size_of::<ScriptReg>(),
    ) as *mut ScriptReg;
    if new_ptr.is_null() { return 0; }
    (*sd).reg = new_ptr;
    let slot = (*sd).reg_num as usize;
    (*sd).reg_num = new_num;
    std::ptr::write_bytes((*sd).reg.add(slot), 0, 1);
    (*(*sd).reg.add(slot)).index = reg;
    (*(*sd).reg.add(slot)).data = val;
    0
}

// ── Local string registry (per-script, heap-allocated) ───────────────────────

/// `char *pc_readregstr(USER *sd, int reg)` — reads a local string variable by index.
///
/// Returns pointer to the stored C string, or NULL if not found.
pub unsafe fn pc_readregstr(sd: *mut MapSessionData, reg: i32) -> *mut i8 {
    if sd.is_null() { return std::ptr::null_mut(); }
    for i in 0..(*sd).regstr_num as usize {
        if (*(*sd).regstr.add(i)).index == reg {
            return (*(*sd).regstr.add(i)).data.as_mut_ptr();
        }
    }
    std::ptr::null_mut()
}

/// `int pc_setregstr(USER *sd, int reg, char *str)` — sets a local string variable by index.
///
/// Checks length, updates existing slot or grows the `regstr` array.
pub unsafe fn pc_setregstr(sd: *mut MapSessionData, reg: i32, str_: *mut i8) -> i32 {
    if sd.is_null() { return 0; }
    // Check string length — must fit in data[256] (including null terminator)
    let len = libc::strlen(str_ as *const i8);
    if len + 1 >= std::mem::size_of::<[i8; 256]>() {
        libc::printf(c"pc_setregstr: string too long !\n".as_ptr());
        return 0;
    }
    // Search for existing slot
    for i in 0..(*sd).regstr_num as usize {
        if (*(*sd).regstr.add(i)).index == reg {
            libc::strcpy((*(*sd).regstr.add(i)).data.as_mut_ptr() as *mut i8,
                         str_ as *const i8);
            return 0;
        }
    }
    // Not found — grow array
    let new_num = (*sd).regstr_num + 1;
    let new_ptr = libc::realloc(
        (*sd).regstr as *mut libc::c_void,
        new_num as usize * std::mem::size_of::<ScriptRegStr>(),
    ) as *mut ScriptRegStr;
    if new_ptr.is_null() { return 0; }
    (*sd).regstr = new_ptr;
    let slot = (*sd).regstr_num as usize;
    (*sd).regstr_num = new_num;
    std::ptr::write_bytes((*sd).regstr.add(slot), 0, 1);
    (*(*sd).regstr.add(slot)).index = reg;
    libc::strcpy((*(*sd).regstr.add(slot)).data.as_mut_ptr() as *mut i8,
                 str_ as *const i8);
    0
}

// ── Global string registry (persisted in MmoCharStatus) ──────────────────────

/// `char *pc_readglobalregstring(USER *sd, const char *reg)` — reads a global string variable.
///
/// Scans `sd->status.global_regstring[0..MAX_GLOBALPLAYERREG]` for a case-insensitive match.
/// Returns pointer to `val` if found, or pointer to static empty string.
pub unsafe fn pc_readglobalregstring(
    sd: *mut MapSessionData, reg: *const i8,
) -> *mut i8 {
    if sd.is_null() || reg.is_null() { return c"".as_ptr() as *mut i8; }
    let sd = &mut *sd;
    for i in 0..MAX_GLOBALPLAYERREG {
        if libc::strcasecmp(sd.status.global_regstring[i].str.as_ptr(), reg) == 0 {
            return sd.status.global_regstring[i].val.as_mut_ptr();
        }
    }
    c"".as_ptr() as *mut i8
}

/// `int pc_setglobalregstring(USER *sd, const char *reg, const char *val)` — sets a global string variable.
///
/// Finds an existing slot by case-insensitive key match, or claims the first empty slot.
/// Setting to `""` clears the key string (marks slot unused).
pub unsafe fn pc_setglobalregstring(
    sd: *mut MapSessionData, reg: *const i8, val: *const i8,
) -> i32 {
    if sd.is_null() || reg.is_null() { return 0; }
    let sd = &mut *sd;
    // Find existing slot
    let mut exist: i32 = -1;
    for i in 0..MAX_GLOBALPLAYERREG {
        if libc::strcasecmp(sd.status.global_regstring[i].str.as_ptr(), reg) == 0 {
            exist = i as i32;
            break;
        }
    }
    if exist != -1 {
        let idx = exist as usize;
        if libc::strcasecmp(val, c"".as_ptr()) == 0 {
            // Clear key (marks slot empty)
            libc::strcpy(sd.status.global_regstring[idx].str.as_mut_ptr() as *mut i8, c"".as_ptr());
        }
        libc::strcpy(sd.status.global_regstring[idx].val.as_mut_ptr() as *mut i8,
                     val as *const i8);
        return 0;
    }
    // Find empty slot
    for i in 0..MAX_GLOBALPLAYERREG {
        if libc::strcasecmp(sd.status.global_regstring[i].str.as_ptr(), c"".as_ptr()) == 0 {
            libc::strcpy(sd.status.global_regstring[i].str.as_mut_ptr() as *mut i8,
                         reg as *const i8);
            libc::strcpy(sd.status.global_regstring[i].val.as_mut_ptr() as *mut i8,
                         val as *const i8);
            return 0;
        }
    }
    libc::printf(c"pc_setglobalreg : couldn't set %s\n".as_ptr(), reg);
    1
}

// ── Global integer registry (persisted in MmoCharStatus) ─────────────────────

/// `int pc_readglobalreg(USER *sd, const char *reg)` — reads a global integer variable.
///
/// Scans `sd->status.global_reg[0..MAX_GLOBALPLAYERREG]` for a case-insensitive match.
pub unsafe fn pc_readglobalreg(
    sd: *mut MapSessionData, reg: *const i8,
) -> i32 {
    if sd.is_null() || reg.is_null() { return 0; }
    let sd = &*sd;
    for i in 0..MAX_GLOBALPLAYERREG {
        if libc::strcasecmp(sd.status.global_reg[i].str.as_ptr(), reg) == 0 {
            return sd.status.global_reg[i].val;
        }
    }
    0
}

/// `int pc_setglobalreg(USER *sd, const char *reg, unsigned long val)` — sets a global integer variable.
///
/// Finds an existing slot by case-insensitive key match (scanning all MAX_GLOBALREG slots),
/// or claims the first empty slot. Setting val to 0 also clears the key string.
pub unsafe fn pc_setglobalreg(
    sd: *mut MapSessionData, reg: *const i8, val: u64,
) -> i32 {
    if sd.is_null() || reg.is_null() { return 0; }
    let sd = &mut *sd;
    // Find existing slot (scan full array)
    let mut exist: i32 = -1;
    for i in 0..MAX_GLOBALREG {
        if libc::strcasecmp(sd.status.global_reg[i].str.as_ptr(), reg) == 0 {
            exist = i as i32;
            break;
        }
    }
    if exist != -1 {
        let idx = exist as usize;
        if val == 0 {
            libc::strcpy(sd.status.global_reg[idx].str.as_mut_ptr() as *mut i8, c"".as_ptr());
        }
        sd.status.global_reg[idx].val = val as i32;
        return 0;
    }
    // Find empty slot (scan full MAX_GLOBALREG array, matching C behavior)
    for i in 0..MAX_GLOBALREG {
        if libc::strcasecmp(sd.status.global_reg[i].str.as_ptr(), c"".as_ptr()) == 0 {
            libc::strcpy(sd.status.global_reg[i].str.as_mut_ptr() as *mut i8,
                         reg as *const i8);
            sd.status.global_reg[i].val = val as i32;
            return 0;
        }
    }
    libc::printf(c"pc_setglobalreg : couldn't set %s\n".as_ptr(), reg);
    1
}

// ── Parameter read/write (HP/MP/max) ─────────────────────────────────────────

/// `int pc_readparam(USER *sd, int type)` — reads a player parameter by SP_* constant.
///
pub unsafe fn pc_readparam(sd: *mut MapSessionData, type_: i32) -> i32 {
    if sd.is_null() { return 0; }
    let sd = &*sd;
    match type_ {
        SP_HP  => sd.status.hp as i32,
        SP_MP  => sd.status.mp as i32,
        SP_MHP => sd.max_hp as i32,
        SP_MMP => sd.max_mp as i32,
        _      => 0,
    }
}

/// `int pc_setparam(USER *sd, int type, int val)` — sets a player parameter by SP_* constant.
///
pub unsafe fn pc_setparam(sd: *mut MapSessionData, type_: i32, val: i32) -> i32 {
    if sd.is_null() { return 0; }
    match type_ {
        SP_HP  => (*sd).status.hp  = val as u32,
        SP_MP  => (*sd).status.mp  = val as u32,
        SP_MHP => (*sd).max_hp     = val as u32,
        SP_MMP => (*sd).max_mp     = val as u32,
        _      => {}
    }
    clif_sendupdatestatus(sd);
    0
}

// ── Account registry (persisted in MmoCharStatus.acctreg) ────────────────────

/// `int pc_readacctreg(USER *sd, const char *reg)` — reads an account-scoped integer variable.
///
/// Scans `sd->status.acctreg[0..MAX_GLOBALREG]` for a case-insensitive match.
/// Returns the integer value or 0. (Function declared in pc.h but never defined in pc.c;
/// implemented here for completeness.)
pub unsafe fn pc_readacctreg(
    sd: *mut MapSessionData, reg: *const i8,
) -> i32 {
    if sd.is_null() || reg.is_null() { return 0; }
    let sd = &*sd;
    for i in 0..MAX_GLOBALREG {
        if libc::strcasecmp(sd.status.acctreg[i].str.as_ptr(), reg) == 0 {
            return sd.status.acctreg[i].val;
        }
    }
    0
}

/// `int pc_setacctreg(USER *sd, const char *reg, int val)` — sets an account-scoped integer variable.
///
/// Finds an existing slot by case-insensitive key match, or claims the first empty slot.
/// Setting val to 0 clears the key string (marks slot unused).
/// (Function declared in pc.h but never defined in pc.c; implemented here.)
pub unsafe fn pc_setacctreg(
    sd: *mut MapSessionData, reg: *const i8, val: i32,
) -> i32 {
    if sd.is_null() || reg.is_null() { return 0; }
    let sd = &mut *sd;
    let mut exist: i32 = -1;
    for i in 0..MAX_GLOBALREG {
        if libc::strcasecmp(sd.status.acctreg[i].str.as_ptr(), reg) == 0 {
            exist = i as i32;
            break;
        }
    }
    if exist != -1 {
        let idx = exist as usize;
        if val == 0 {
            libc::strcpy(sd.status.acctreg[idx].str.as_mut_ptr() as *mut i8, c"".as_ptr());
        }
        sd.status.acctreg[idx].val = val;
        return 0;
    }
    for i in 0..MAX_GLOBALREG {
        if libc::strcasecmp(sd.status.acctreg[i].str.as_ptr(), c"".as_ptr()) == 0 {
            libc::strcpy(sd.status.acctreg[i].str.as_mut_ptr() as *mut i8,
                         reg as *const i8);
            sd.status.acctreg[i].val = val;
            return 0;
        }
    }
    0
}

// ── NPC integer registry (persisted in MmoCharStatus.npcintreg) ──────────────

/// `int pc_readnpcintreg(USER *sd, const char *reg)` — reads an NPC-scoped integer variable.
///
/// Scans `sd->status.npcintreg[0..MAX_GLOBALNPCREG]` for a case-insensitive match.
pub unsafe fn pc_readnpcintreg(
    sd: *mut MapSessionData, reg: *const i8,
) -> i32 {
    if sd.is_null() || reg.is_null() { return 0; }
    let sd = &*sd;
    for i in 0..MAX_GLOBALNPCREG {
        if libc::strcasecmp(sd.status.npcintreg[i].str.as_ptr(), reg) == 0 {
            return sd.status.npcintreg[i].val;
        }
    }
    0
}

/// `int pc_setnpcintreg(USER *sd, const char *reg, int val)` — sets an NPC-scoped integer variable.
///
/// Finds an existing slot by case-insensitive key match, or claims the first empty slot.
/// Setting val to 0 clears the key string (marks slot unused).
pub unsafe fn pc_setnpcintreg(
    sd: *mut MapSessionData, reg: *const i8, val: i32,
) -> i32 {
    if sd.is_null() || reg.is_null() { return 0; }
    let sd = &mut *sd;
    let mut exist: i32 = -1;
    for i in 0..MAX_GLOBALNPCREG {
        if libc::strcasecmp(sd.status.npcintreg[i].str.as_ptr(), reg) == 0 {
            exist = i as i32;
            break;
        }
    }
    if exist != -1 {
        let idx = exist as usize;
        if val == 0 {
            libc::strcpy(sd.status.npcintreg[idx].str.as_mut_ptr() as *mut i8, c"".as_ptr());
        }
        sd.status.npcintreg[idx].val = val;
        return 0;
    }
    for i in 0..MAX_GLOBALNPCREG {
        if libc::strcasecmp(sd.status.npcintreg[i].str.as_ptr(), c"".as_ptr()) == 0 {
            libc::strcpy(sd.status.npcintreg[i].str.as_mut_ptr() as *mut i8,
                         reg as *const i8);
            sd.status.npcintreg[i].val = val;
            return 0;
        }
    }
    0
}

// ── Quest registry (persisted in MmoCharStatus.questreg) ─────────────────────

/// `int pc_readquestreg(USER *sd, const char *reg)` — reads a quest integer variable.
///
/// Scans `sd->status.questreg[0..MAX_GLOBALQUESTREG]` for a case-insensitive match.
pub unsafe fn pc_readquestreg(
    sd: *mut MapSessionData, reg: *const i8,
) -> i32 {
    if sd.is_null() || reg.is_null() { return 0; }
    let sd = &*sd;
    for i in 0..MAX_GLOBALQUESTREG {
        if libc::strcasecmp(sd.status.questreg[i].str.as_ptr(), reg) == 0 {
            return sd.status.questreg[i].val;
        }
    }
    0
}

/// `int pc_setquestreg(USER *sd, const char *reg, int val)` — sets a quest integer variable.
///
/// Finds an existing slot by case-insensitive key match, or claims the first empty slot.
/// Setting val to 0 clears the key string (marks slot unused).
pub unsafe fn pc_setquestreg(
    sd: *mut MapSessionData, reg: *const i8, val: i32,
) -> i32 {
    if sd.is_null() || reg.is_null() { return 0; }
    let sd = &mut *sd;
    let mut exist: i32 = -1;
    for i in 0..MAX_GLOBALQUESTREG {
        if libc::strcasecmp(sd.status.questreg[i].str.as_ptr(), reg) == 0 {
            exist = i as i32;
            break;
        }
    }
    if exist != -1 {
        let idx = exist as usize;
        if val == 0 {
            libc::strcpy(sd.status.questreg[idx].str.as_mut_ptr() as *mut i8, c"".as_ptr());
        }
        sd.status.questreg[idx].val = val;
        return 0;
    }
    for i in 0..MAX_GLOBALQUESTREG {
        if libc::strcasecmp(sd.status.questreg[i].str.as_ptr(), c"".as_ptr()) == 0 {
            libc::strcpy(sd.status.questreg[i].str.as_mut_ptr() as *mut i8,
                         reg as *const i8);
            sd.status.questreg[i].val = val;
            return 0;
        }
    }
    0
}

// ─── Item management functions ────────────────────────────────────────────────

use crate::game::scripting::types::floor::FloorItemData;

// ─── pc_isinvenspace ─────────────────────────────────────────────────────────

/// `int pc_isinvenspace(USER* sd, int id, int owner, const char* engrave,
///     unsigned int customLook, unsigned int customLookColor,
///     unsigned int customIcon, unsigned int customIconColor)`
///
/// Returns the first inventory slot that can accept an item with the given
/// attributes, or `sd->status.maxinv` when no slot is available.
pub unsafe fn pc_isinvenspace(
    sd:               *mut MapSessionData,
    id:               i32,
    owner:            i32,
    engrave:          *const i8,
    custom_look:      u32,
    custom_look_color: u32,
    custom_icon:      u32,
    custom_icon_color: u32,
) -> i32 {
    if sd.is_null() { return 0; }
    let sd = &mut *sd;
    let maxinv = sd.status.maxinv as usize;
    let id_u  = id as u32;
    let own_u = owner as u32;

    if item_db::search(id_u).max_amount > 0 {
        // Count how many of this item the player already owns (inventory + equip).
        let mut maxamount: i32 = 0;
        for i in 0..maxinv {
            let inv = &sd.status.inventory[i];
            if inv.id == id_u && item_db::search(id_u).max_amount > 0
                && inv.owner == own_u
                && libc::strcasecmp(inv.real_name.as_ptr(), engrave) == 0
                && inv.custom_look       == custom_look
                && inv.custom_look_color == custom_look_color
                && inv.custom_icon       == custom_icon
                && inv.custom_icon_color == custom_icon_color
            {
                maxamount += inv.amount;
            }
        }
        for i in 0..14usize {
            let eq = &sd.status.equip[i];
            if eq.id == id_u && item_db::search(id_u).max_amount > 0
                && sd.status.inventory[i].owner == own_u
                && libc::strcasecmp(sd.status.inventory[i].real_name.as_ptr(), engrave) == 0
                && sd.status.inventory[i].custom_look       == custom_look
                && sd.status.inventory[i].custom_look_color == custom_look_color
                && sd.status.inventory[i].custom_icon       == custom_icon
                && sd.status.inventory[i].custom_icon_color == custom_icon_color
            {
                maxamount += 1;
            }
        }

        // Find a slot that already has the item but isn't full.
        for i in 0..maxinv {
            let inv = &sd.status.inventory[i];
            if inv.id == id_u
                && inv.amount < item_db::search(id_u).stack_amount
                && maxamount < item_db::search(id_u).max_amount
                && inv.owner == own_u
                && libc::strcasecmp(inv.real_name.as_ptr(), engrave) == 0
                && inv.custom_look       == custom_look
                && inv.custom_look_color == custom_look_color
                && inv.custom_icon       == custom_icon
                && inv.custom_icon_color == custom_icon_color
            {
                return i as i32;
            }
        }

        // Find an empty slot under the global cap.
        for i in 0..maxinv {
            if sd.status.inventory[i].id == 0
                && maxamount < item_db::search(id_u).max_amount
            {
                return i as i32;
            }
        }

        return sd.status.maxinv as i32;
    } else {
        // No per-player max — just stack or find empty.
        for i in 0..maxinv {
            let inv = &sd.status.inventory[i];
            if inv.id == id_u
                && inv.amount < item_db::search(id_u).stack_amount
                && inv.owner == own_u
                && libc::strcasecmp(inv.real_name.as_ptr(), engrave) == 0
                && inv.custom_look       == custom_look
                && inv.custom_look_color == custom_look_color
                && inv.custom_icon       == custom_icon
                && inv.custom_icon_color == custom_icon_color
            {
                return i as i32;
            }
        }
        for i in 0..maxinv {
            if sd.status.inventory[i].id == 0 {
                return i as i32;
            }
        }
        return sd.status.maxinv as i32;
    }
}

// ─── pc_isinvenitemspace ──────────────────────────────────────────────────────

/// `int pc_isinvenitemspace(USER* sd, int num, int id, int owner, char* engrave)`
///
/// Returns the number of additional units of `id` that can be placed in
/// inventory slot `num`.  Returns 0 when the slot is incompatible.
pub unsafe fn pc_isinvenitemspace(
    sd:      *mut MapSessionData,
    num:     i32,
    id:      i32,
    owner:   i32,
    engrave: *mut i8,
) -> i32 {
    if sd.is_null() { return 0; }
    let sd = &mut *sd;
    let id_u  = id as u32;
    let own_u = owner as u32;
    let num   = num as usize;

    if item_db::search(id_u).max_amount > 0 {
        let mut maxamount: i32 = 0;
        let maxinv = sd.status.maxinv as usize;
        for i in 0..maxinv {
            if sd.status.inventory[i].id == id_u && item_db::search(id_u).max_amount > 0 {
                maxamount += sd.status.inventory[i].amount;
            }
        }
        for i in 0..14usize {
            if sd.status.equip[i].id == id_u && item_db::search(id_u).max_amount > 0 {
                // C checks takeoffid: skip the slot being unequipped
                if sd.takeoffid == -1
                    || sd.status.equip[sd.takeoffid as usize].id != id_u
                {
                    maxamount += 1;
                }
            }
        }

        if sd.status.inventory[num].id == 0
            && item_db::search(id_u).max_amount - maxamount >= item_db::search(id_u).stack_amount
        {
            return item_db::search(id_u).stack_amount;
        } else if sd.status.inventory[num].id != id_u
            || sd.status.inventory[num].owner != own_u
            || libc::strcasecmp(sd.status.inventory[num].real_name.as_ptr(), engrave) != 0
        {
            return 0;
        } else {
            return item_db::search(id_u).max_amount - maxamount;
        }
    } else {
        if sd.status.inventory[num].id == 0 {
            return item_db::search(id_u).stack_amount;
        } else if sd.status.inventory[num].id != id_u
            || sd.status.inventory[num].owner != own_u
            || libc::strcasecmp(sd.status.inventory[num].real_name.as_ptr(), engrave) != 0
        {
            return 0;
        } else {
            return item_db::search(id_u).stack_amount - sd.status.inventory[num].amount;
        }
    }
}

// ─── pc_dropitemfull (helper) ─────────────────────────────────────────────────

/// Allocate a `FloorItemData` from `fl2`, attempt to stack it on an existing
/// floor item at the player's cell, and if no match exists add it to the map.
unsafe fn pc_dropitemfull_inner(sd: *mut MapSessionData, fl2: *const Item) -> i32 {
    use std::mem;

    let mut fl = Box::new(mem::zeroed::<FloorItemData>());

    (*fl).bl.m = (*sd).bl.m;
    (*fl).bl.x = (*sd).bl.x;
    (*fl).bl.y = (*sd).bl.y;
    // Copy the item into fl->data (BoundItem and Item share the same layout)
    libc::memcpy(
        &mut (*fl).data as *mut _ as *mut libc::c_void,
        fl2 as *const libc::c_void,
        mem::size_of::<Item>(),
    );
    // looters is already zeroed by mem::zeroed()

    let mut def = [0i32; 2];

    // Only attempt stacking if item is at full durability.
    if (*fl).data.dura == item_db::search((*fl).data.id as u32).dura {
        if let Some(grid) = block_grid::get_grid((*fl).bl.m as usize) {
            let cell_ids = grid.ids_at_tile((*fl).bl.x, (*fl).bl.y);
            for id in cell_ids {
                if let Some(fl_arc) = crate::game::map_server::map_id2fl_ref(id) {
                    let mut fl_existing = fl_arc.write();
                    pc_addtocurrent2_inner(&mut fl_existing.bl as *mut BlockList, def.as_mut_ptr(), fl.as_mut() as *mut FloorItemData);
                }
            }
        }
    }

    if def[0] == 0 {
        let fl_raw = Box::into_raw(fl);
        map_additem(&mut (*fl_raw).bl);
        let fl_bl = &mut (*fl_raw).bl as *mut BlockList;
        if let Some(grid) = block_grid::get_grid((*sd).bl.m as usize) {
            let slot = &*crate::database::map_db::get_map_ptr((*sd).bl.m as u16);
            let ids = block_grid::ids_in_area(grid, (*sd).bl.x as i32, (*sd).bl.y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
            for id in ids {
                if let Some(tsd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                    clif_object_look_sub2_inner(&mut tsd_arc.write().bl as *mut BlockList, LOOK_SEND, fl_bl);
                }
            }
        }
    } else {
        drop(fl);
    }
    0
}

/// `int pc_dropitemfull(USER* sd, struct item* fl2)` — public C-callable export.
pub unsafe fn pc_dropitemfull(
    sd:  *mut MapSessionData,
    fl2: *mut Item,
) -> i32 {
    if sd.is_null() || fl2.is_null() { return 0; }
    pc_dropitemfull_inner(sd, fl2)
}



/// Typed inner callback: attempt to stack `fl2` onto the existing floor item `bl`.
/// Sets `def[0] = 1` on a successful merge.
///
pub unsafe fn pc_addtocurrent2_inner(
    bl: *mut BlockList,
    def: *mut i32,
    fl2: *mut FloorItemData,
) -> i32 {
    if bl.is_null() { return 0; }
    let fl = bl as *mut FloorItemData;

    if def.is_null() || fl2.is_null() { return 0; }
    if *def != 0 { return 0; }

    // Items stack when all identity fields match exactly.
    if (*fl).data.id   == (*fl2).data.id
        && (*fl).data.owner == (*fl2).data.owner
        && libc::strcasecmp((*fl).data.real_name.as_ptr(), (*fl2).data.real_name.as_ptr()) == 0
        && (*fl).data.custom_icon       == (*fl2).data.custom_icon
        && (*fl).data.custom_icon_color == (*fl2).data.custom_icon_color
        && (*fl).data.custom_look       == (*fl2).data.custom_look
        && (*fl).data.custom_look_color == (*fl2).data.custom_look_color
        && libc::strcmp((*fl).data.note.as_ptr(), (*fl2).data.note.as_ptr()) == 0
        && (*fl).data.custom    == (*fl2).data.custom
        && (*fl).data.protected == (*fl2).data.protected
    {
        (*fl).data.amount += (*fl2).data.amount;
        *def = 1;
    }
    0
}



/// Typed inner callback: stack inventory slot `id` amount onto existing floor item `fl`.
/// Sets `def[0] = fl->bl.id` on successful merge.
///
pub unsafe fn pc_addtocurrent_inner(
    bl: *mut BlockList,
    def: *mut i32,
    id: i32,
    type_: i32,
    sd: *mut MapSessionData,
) -> i32 {
    if bl.is_null() { return 0; }
    let fl = bl as *mut FloorItemData;
    let id = id as usize;   // inventory slot index

    if def.is_null() || sd.is_null() { return 0; }
    if *def != 0 { return 0; }

    // Only stack items at full durability.
    if (*fl).data.dura < item_db::search((*fl).data.id as u32).dura { return 0; }
    libc::memset(
        (*fl).looters.as_mut_ptr() as *mut libc::c_void,
        0,
        std::mem::size_of::<u32>() * MAX_GROUP_MEMBERS,
    );

    let inv = &(*sd).status.inventory[id];
    if (*fl).data.id   == inv.id
        && (*fl).data.owner == inv.owner
        && libc::strcasecmp((*fl).data.real_name.as_ptr(), inv.real_name.as_ptr()) == 0
        && (*fl).data.custom_icon       == inv.custom_icon
        && (*fl).data.custom_icon_color == inv.custom_icon_color
        && (*fl).data.custom_look       == inv.custom_look
        && (*fl).data.custom_look_color == inv.custom_look_color
        && libc::strcmp((*fl).data.note.as_ptr(), inv.note.as_ptr()) == 0
        && (*fl).data.custom    == inv.custom
        && (*fl).data.protected == inv.protected
    {
        (*fl).lastamount = (*fl).data.amount as u32;
        if type_ != 0 {
            (*fl).data.amount += inv.amount;
        } else {
            (*fl).data.amount += 1;
        }
        sl_doscript_2_pc(c"characterLog".as_ptr(), c"dropWrite".as_ptr(), &mut (*sd).bl as *mut BlockList, &mut (*fl).bl as *mut BlockList);
        *def = (*fl).bl.id as i32;
    }
    0
}



// ─── pc_additem ───────────────────────────────────────────────────────────────

/// `int pc_additem(USER* sd, struct item* fl)` — add item to inventory with logging.
pub unsafe fn pc_additem(
    sd: *mut MapSessionData,
    fl: *mut Item,
) -> i32 {
    if sd.is_null() || fl.is_null() { return 0; }

    // Gold dupe guard: id==0 with amount is bogus.
    if (*fl).id == 0 && (*fl).amount != 0 { return 0; }

    let id_u = (*fl).id;
    let maxinv = (*sd).status.maxinv as i32;

    let mut num = pc_isinvenspace(
        sd, id_u as i32, (*fl).owner as i32,
        (*fl).real_name.as_ptr(),
        (*fl).custom_look, (*fl).custom_look_color,
        (*fl).custom_icon, (*fl).custom_icon_color,
    );

    if num >= maxinv {
        if item_db::search(id_u).max_amount > 0 {
            let mut errbuf = [0i8; 64];
            libc::snprintf(
                errbuf.as_mut_ptr(), 64,
                c"(%s). You can't have more than (%i).".as_ptr(),
                item_db::search(id_u).name.as_ptr(), item_db::search(id_u).max_amount,
            );
            pc_dropitemfull_inner(sd, fl);
            clif_sendminitext(sd, errbuf.as_ptr());
        } else {
            clif_sendminitext(sd, map_msg()[MAP_ERRITMFULL].message.as_ptr());
            pc_dropitemfull_inner(sd, fl);
        }
        return 0;
    }

    loop {
        let i = pc_isinvenitemspace(
            sd, num, id_u as i32, (*fl).owner as i32, (*fl).real_name.as_mut_ptr(),
        );

        if (*fl).amount > i {
            // Partial fill: put as much as fits.
            let inv = &mut (*sd).status.inventory[num as usize];
            inv.id         = id_u;
            inv.dura       = (*fl).dura;
            inv.protected  = (*fl).protected;
            inv.owner      = (*fl).owner;
            inv.time       = (*fl).time;
            libc::strcpy(inv.real_name.as_mut_ptr(), (*fl).real_name.as_ptr());
            libc::strcpy(inv.note.as_mut_ptr(), (*fl).note.as_ptr());
            inv.custom_look       = (*fl).custom_look;
            inv.custom_look_color = (*fl).custom_look_color;
            inv.custom_icon       = (*fl).custom_icon;
            inv.custom_icon_color = (*fl).custom_icon_color;
            inv.custom     = (*fl).custom;
            inv.amount    += i;
            (*fl).amount  -= i;
        } else {
            // Full fill: place the remaining amount.
            let inv = &mut (*sd).status.inventory[num as usize];
            inv.id         = id_u;
            inv.dura       = (*fl).dura;
            inv.protected  = (*fl).protected;
            inv.owner      = (*fl).owner;
            inv.time       = (*fl).time;
            libc::strcpy(inv.real_name.as_mut_ptr(), (*fl).real_name.as_ptr());
            libc::strcpy(inv.note.as_mut_ptr(), (*fl).note.as_ptr());
            inv.custom_look       = (*fl).custom_look;
            inv.custom_look_color = (*fl).custom_look_color;
            inv.custom_icon       = (*fl).custom_icon;
            inv.custom_icon_color = (*fl).custom_icon_color;
            inv.custom     = (*fl).custom;
            inv.amount    += (*fl).amount;
            (*fl).amount   = 0;
        }

        clif_sendadditem(sd, num);
        num = pc_isinvenspace(
            sd, id_u as i32, (*fl).owner as i32,
            (*fl).real_name.as_ptr(),
            (*fl).custom_look, (*fl).custom_look_color,
            (*fl).custom_icon, (*fl).custom_icon_color,
        );

        if !((*fl).amount != 0 && num < maxinv) { break; }
    }

    if num >= maxinv && (*fl).amount != 0 {
        if item_db::search(id_u).max_amount > 0 {
            let mut errbuf = [0i8; 64];
            libc::snprintf(
                errbuf.as_mut_ptr(), 64,
                c"(%s). You can't have more than (%i).".as_ptr(),
                item_db::search(id_u).name.as_ptr(), item_db::search(id_u).max_amount,
            );
            pc_dropitemfull_inner(sd, fl);
            clif_sendminitext(sd, errbuf.as_ptr());
        } else {
            pc_dropitemfull_inner(sd, fl);
            clif_sendminitext(sd, map_msg()[MAP_ERRITMFULL].message.as_ptr());
        }
    }
    0
}

// ─── pc_additemnolog ──────────────────────────────────────────────────────────

/// `int pc_additemnolog(USER* sd, struct item* fl)` — add item without SQL logging.
pub unsafe fn pc_additemnolog(
    sd: *mut MapSessionData,
    fl: *mut Item,
) -> i32 {
    if sd.is_null() || fl.is_null() { return 0; }

    if (*fl).id == 0 && (*fl).amount != 0 { return 0; }

    let id_u   = (*fl).id;
    let maxinv = (*sd).status.maxinv as i32;

    let mut num = pc_isinvenspace(
        sd, id_u as i32, (*fl).owner as i32,
        (*fl).real_name.as_ptr(),
        (*fl).custom_look, (*fl).custom_look_color,
        (*fl).custom_icon, (*fl).custom_icon_color,
    );

    if num >= maxinv {
        if item_db::search(id_u).max_amount > 0 {
            let mut errbuf = [0i8; 64];
            libc::snprintf(
                errbuf.as_mut_ptr(), 64,
                c"(%s). You can't have more than (%i).".as_ptr(),
                item_db::search(id_u).name.as_ptr(), item_db::search(id_u).max_amount,
            );
            pc_dropitemfull_inner(sd, fl);
            clif_sendminitext(sd, errbuf.as_ptr());
        } else {
            clif_sendminitext(sd, map_msg()[MAP_ERRITMFULL].message.as_ptr());
            pc_dropitemfull_inner(sd, fl);
        }
        return 0;
    }

    loop {
        let i = pc_isinvenitemspace(
            sd, num, id_u as i32, (*fl).owner as i32, (*fl).real_name.as_mut_ptr(),
        );

        if (*fl).amount > i {
            let inv = &mut (*sd).status.inventory[num as usize];
            inv.id         = id_u;
            inv.dura       = (*fl).dura;
            inv.protected  = (*fl).protected;
            inv.owner      = (*fl).owner;
            inv.time       = (*fl).time;
            libc::strcpy(inv.real_name.as_mut_ptr(), (*fl).real_name.as_ptr());
            inv.custom_look       = (*fl).custom_look;
            inv.custom_look_color = (*fl).custom_look_color;
            inv.custom_icon       = (*fl).custom_icon;
            inv.custom_icon_color = (*fl).custom_icon_color;
            inv.custom     = (*fl).custom;
            inv.amount    += i;
            (*fl).amount  -= i;
        } else {
            let inv = &mut (*sd).status.inventory[num as usize];
            inv.id         = id_u;
            inv.dura       = (*fl).dura;
            inv.protected  = (*fl).protected;
            inv.owner      = (*fl).owner;
            inv.time       = (*fl).time;
            libc::strcpy(inv.real_name.as_mut_ptr(), (*fl).real_name.as_ptr());
            inv.custom_look       = (*fl).custom_look;
            inv.custom_look_color = (*fl).custom_look_color;
            inv.custom_icon       = (*fl).custom_icon;
            inv.custom_icon_color = (*fl).custom_icon_color;
            inv.custom     = (*fl).custom;
            inv.amount    += (*fl).amount;
            (*fl).amount   = 0;
        }

        clif_sendadditem(sd, num);
        num = pc_isinvenspace(
            sd, id_u as i32, (*fl).owner as i32,
            (*fl).real_name.as_ptr(),
            (*fl).custom_look, (*fl).custom_look_color,
            (*fl).custom_icon, (*fl).custom_icon_color,
        );

        if !((*fl).amount != 0 && num < maxinv) { break; }
    }

    if num >= maxinv && (*fl).amount != 0 {
        if item_db::search(id_u).max_amount > 0 {
            let mut errbuf = [0i8; 64];
            libc::snprintf(
                errbuf.as_mut_ptr(), 64,
                c"(%s). You can't have more than (%i).".as_ptr(),
                item_db::search(id_u).name.as_ptr(), item_db::search(id_u).max_amount,
            );
            pc_dropitemfull_inner(sd, fl);
            clif_sendminitext(sd, errbuf.as_ptr());
        } else {
            pc_dropitemfull_inner(sd, fl);
            clif_sendminitext(sd, map_msg()[MAP_ERRITMFULL].message.as_ptr());
        }
    }
    0
}

// ─── pc_delitem ───────────────────────────────────────────────────────────────

/// `int pc_delitem(USER* sd, int id, int amount, int type)` — remove `amount`
/// units from inventory slot `id`.  If the slot becomes empty it is zeroed and
/// the client is notified with a delete-item packet; otherwise the client
/// receives an updated add-item count and a mini-text with the item name.
pub unsafe fn pc_delitem(
    sd:     *mut MapSessionData,
    id:     i32,
    amount: i32,
    type_:  i32,
) -> i32 {
    if sd.is_null() { return 0; }
    let maxinv = (*sd).status.maxinv as i32;
    if id < 0 || id >= maxinv { return 0; }
    let inv = &mut (*sd).status.inventory[id as usize];
    if inv.id == 0 { return 0; }
    if amount <= 0 { return 0; }

    if amount >= inv.amount {
        inv.amount = 0;
        libc::memset(inv as *mut Item as *mut libc::c_void, 0, std::mem::size_of::<Item>());
        clif_senddelitem(sd, id, type_);
        return 0;
    }

    inv.amount -= amount;

    if inv.amount <= 0 {
        libc::memset(inv as *mut Item as *mut libc::c_void, 0, std::mem::size_of::<Item>());
        clif_senddelitem(sd, id, type_);
    } else {
        let item_id = (*sd).status.inventory[id as usize].id;
        let mut buf = [0i8; 255];
        libc::snprintf(
            buf.as_mut_ptr(), 255,
            c"%s (%d)".as_ptr(),
            item_db::search(item_id).name.as_ptr(),
            amount,
        );
        clif_sendminitext(sd, buf.as_ptr());
        clif_sendadditem(sd, id);
    }
    0
}

// ─── pc_dropitemmap ───────────────────────────────────────────────────────────

/// `int pc_dropitemmap(USER* sd, int id, int type)` — drop one (or all) units
/// of inventory slot `id` onto the map floor.
pub unsafe fn pc_dropitemmap(
    sd:    *mut MapSessionData,
    id:    i32,
    type_: i32,
) -> i32 {
    if sd.is_null() { return 0; }
    let id_u = id as usize;

    if id > (*sd).status.maxinv as i32 { return 0; }
    if (*sd).status.inventory[id_u].id == 0 { return 0; }

    if (*sd).status.inventory[id_u].amount <= 0 {
        clif_senddelitem(sd, id, 1);
        return 0;
    }

    let mut def = [0i32; 2];

    let mut fl = Box::new(unsafe { std::mem::zeroed::<FloorItemData>() });

    (*fl).bl.m = (*sd).bl.m;
    (*fl).bl.x = (*sd).bl.x;
    (*fl).bl.y = (*sd).bl.y;
    libc::memcpy(
        &mut (*fl).data as *mut _ as *mut libc::c_void,
        &(*sd).status.inventory[id_u] as *const Item as *const libc::c_void,
        std::mem::size_of::<Item>(),
    );
    // looters is already zeroed by mem::zeroed()

    // Attempt to stack onto an existing floor item at full durability.
    if (*fl).data.dura == item_db::search((*fl).data.id as u32).dura {
        if let Some(grid) = block_grid::get_grid((*fl).bl.m as usize) {
            let cell_ids = grid.ids_at_tile((*fl).bl.x, (*fl).bl.y);
            for cell_id in cell_ids {
                if let Some(fl_arc) = crate::game::map_server::map_id2fl_ref(cell_id) {
                    let mut fl_existing = fl_arc.write();
                    pc_addtocurrent_inner(&mut fl_existing.bl as *mut BlockList, def.as_mut_ptr(), id, type_, sd);
                }
            }
        }
    }

    (*sd).status.inventory[id_u].amount -= 1;

    if type_ != 0 || (*sd).status.inventory[id_u].amount == 0 {
        // Full drop: clear the slot.
        libc::memset(
            &mut (*sd).status.inventory[id_u] as *mut Item as *mut libc::c_void,
            0,
            std::mem::size_of::<Item>(),
        );
        clif_senddelitem(sd, id, 1);
    } else {
        // Partial drop: update count.
        (*fl).data.amount = 1;
        clif_sendadditem(sd, id);
    }

    if def[0] == 0 {
        let fl_raw = Box::into_raw(fl);
        map_additem(&mut (*fl_raw).bl);
        sl_doscript_2_pc(c"characterLog".as_ptr(), c"dropWrite".as_ptr(), &mut (*sd).bl as *mut BlockList, &mut (*fl_raw).bl as *mut BlockList);
        let fl_bl = &mut (*fl_raw).bl as *mut BlockList;
        if let Some(grid) = block_grid::get_grid((*sd).bl.m as usize) {
            let slot = &*crate::database::map_db::get_map_ptr((*sd).bl.m as u16);
            let ids = block_grid::ids_in_area(grid, (*sd).bl.x as i32, (*sd).bl.y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
            for id in ids {
                if let Some(tsd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                    clif_object_look_sub2_inner(&mut tsd_arc.write().bl as *mut BlockList, LOOK_SEND, fl_bl);
                }
            }
        }
    } else {
        drop(fl);
    }
    0
}

// ─── pc_changeitem ────────────────────────────────────────────────────────────

/// `int pc_changeitem(USER* sd, int id1, int id2)` — swap inventory slots `id1`
/// and `id2`, sending the appropriate add/delete packets to the client.
pub unsafe fn pc_changeitem(
    sd:  *mut MapSessionData,
    id1: i32,
    id2: i32,
) -> i32 {
    if sd.is_null() { return 0; }
    let maxinv = (*sd).status.maxinv as i32;
    if id1 >= maxinv { return 0; }
    if id2 >= maxinv { return 0; }

    let i1 = id1 as usize;
    let i2 = id2 as usize;

    // Swap using a byte-level copy to preserve the full Item layout.
    let tmp: Item = (*sd).status.inventory[i2];
    (*sd).status.inventory[i2] = (*sd).status.inventory[i1];
    (*sd).status.inventory[i1] = tmp;

    if (*sd).status.inventory[i1].id != 0 {
        if (*sd).status.inventory[i2].id == 0 {
            clif_senddelitem(sd, id2, 0);
        }
        clif_sendadditem(sd, id1);
    }
    if (*sd).status.inventory[i2].id != 0 {
        if (*sd).status.inventory[i1].id == 0 {
            clif_senddelitem(sd, id1, 0);
        }
        clif_sendadditem(sd, id2);
    }
    0
}

// ─── pc_useitem ───────────────────────────────────────────────────────────────

/// `int pc_useitem(USER* sd, int id)` — use / equip the item in inventory slot `id`.
///
/// Handles all item types: food, usables, consumables, mounts, equipment, etc.
/// Delegates equip logic to `pc_equipitem`.
pub unsafe fn pc_useitem(
    sd: *mut MapSessionData,
    id: i32,
) -> i32 {
    if sd.is_null() { return 0; }
    let maxinv = (*sd).status.maxinv as i32;
    if id < 0 || id >= maxinv { return 0; }
    let id_u = id as usize;

    if (*sd).status.inventory[id_u].id == 0 { return 0; }

    // Ownership check.
    if (*sd).status.inventory[id_u].owner != 0
        && (*sd).status.inventory[id_u].owner != (*sd).status.id
    {
        clif_sendminitext(sd, c"You cannot use this, it does not belong to you!".as_ptr());
        return 0;
    }

    // Equipment type: check whether the current equip slot can be replaced.
    let equip_type = item_db::search((*sd).status.inventory[id_u].id).typ as i32 - 3;
    if equip_type >= 0 && (equip_type as usize) < (*sd).status.equip.len() {
        if (*sd).status.equip[equip_type as usize].id > 0 && (*sd).status.gm_level == 0 {
            if item_db::search((*sd).status.equip[equip_type as usize].id).unequip as i32 == 1 {
                clif_sendminitext(sd, c"You are unable to unequip that.".as_ptr());
                return 0;
            }
        }
    }

    // Class / path restriction check.
    if item_db::search((*sd).status.inventory[id_u].id).class as i32 != 0 {
        if classdb_path((*sd).status.class as i32) == 5 {
            // GM — no restriction
        } else if (item_db::search((*sd).status.inventory[id_u].id).class as i32) < 6 {
            if classdb_path((*sd).status.class as i32)
                != item_db::search((*sd).status.inventory[id_u].id).class as i32
            {
                clif_sendminitext(sd, map_msg()[MAP_ERRITMPATH].message.as_ptr());
                return 0;
            }
        } else {
            if (*sd).status.class as i32 != item_db::search((*sd).status.inventory[id_u].id).class as i32 {
                clif_sendminitext(sd, map_msg()[MAP_ERRITMPATH].message.as_ptr());
                return 0;
            }
        }
        if ((*sd).status.mark as i32) < item_db::search((*sd).status.inventory[id_u].id).rank {
            clif_sendminitext(sd, map_msg()[MAP_ERRITMMARK].message.as_ptr());
            return 0;
        }
    }

    // Ghost / mounted state restrictions.
    if (*sd).status.state == PC_DIE as i8 {
        clif_sendminitext(sd, map_msg()[MAP_ERRGHOST].message.as_ptr());
        return 0;
    }
    if (*sd).status.state == PC_MOUNTED as i8 {
        clif_sendminitext(sd, map_msg()[MAP_ERRMOUNT].message.as_ptr());
        return 0;
    }

    // Set a timed expiry if the item has one.
    if item_db::search((*sd).status.inventory[id_u].id).time as i32 != 0
        && (*sd).status.inventory[id_u].time == 0
    {
        (*sd).status.inventory[id_u].time =
            (libc::time(std::ptr::null_mut()) as u32)
                .wrapping_add(item_db::search((*sd).status.inventory[id_u].id).time as i32 as u32);
    }

    let map_ptr = crate::database::map_db::get_map_ptr((*sd).bl.m as u16);

    macro_rules! can_use {
        () => {
            !map_ptr.is_null() && (*map_ptr).can_use != 0 || (*sd).status.gm_level != 0
        };
    }
    macro_rules! can_eat {
        () => {
            !map_ptr.is_null() && (*map_ptr).can_eat != 0 || (*sd).status.gm_level != 0
        };
    }
    macro_rules! can_smoke {
        () => {
            !map_ptr.is_null() && (*map_ptr).can_smoke != 0 || (*sd).status.gm_level != 0
        };
    }
    macro_rules! can_equip {
        () => {
            !map_ptr.is_null() && (*map_ptr).can_equip != 0 || (*sd).status.gm_level != 0
        };
    }

    let item_type = item_db::search((*sd).status.inventory[id_u].id).typ as i32;

    match item_type {
        t if t == ITM_EAT => {
            if !can_eat!() {
                clif_sendminitext(sd, c"You cannot eat this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd);
            sl_doscript_simple_pc(item_db::search((*sd).status.inventory[id_u].id).yname.as_ptr(), c"use".as_ptr(), &mut (*sd).bl as *mut BlockList);
            sl_doscript_simple_pc(c"use".as_ptr(), std::ptr::null(), &mut (*sd).bl as *mut BlockList);
            pc_delitem(sd, id, 1, 2);
        }
        t if t == ITM_USE => {
            if !can_use!() {
                clif_sendminitext(sd, c"You cannot use this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd);
            sl_doscript_simple_pc(item_db::search((*sd).status.inventory[id_u].id).yname.as_ptr(), c"use".as_ptr(), &mut (*sd).bl as *mut BlockList);
            sl_doscript_simple_pc(c"use".as_ptr(), std::ptr::null(), &mut (*sd).bl as *mut BlockList);
            pc_delitem(sd, id, 1, 6);
        }
        t if t == ITM_USESPC => {
            if !can_use!() {
                clif_sendminitext(sd, c"You cannot use this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd);
            sl_doscript_simple_pc(item_db::search((*sd).status.inventory[id_u].id).yname.as_ptr(), c"use".as_ptr(), &mut (*sd).bl as *mut BlockList);
            sl_doscript_simple_pc(c"use".as_ptr(), std::ptr::null(), &mut (*sd).bl as *mut BlockList);
            // No auto-delete for USESPC — script decides.
        }
        t if t == ITM_BAG => {
            if !can_use!() {
                clif_sendminitext(sd, c"You cannot use this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd);
            sl_doscript_simple_pc(item_db::search((*sd).status.inventory[id_u].id).yname.as_ptr(), c"use".as_ptr(), &mut (*sd).bl as *mut BlockList);
            sl_doscript_simple_pc(c"use".as_ptr(), std::ptr::null(), &mut (*sd).bl as *mut BlockList);
        }
        t if t == ITM_MAP => {
            if !can_use!() {
                clif_sendminitext(sd, c"You cannot use this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd);
            sl_doscript_simple_pc(item_db::search((*sd).status.inventory[id_u].id).yname.as_ptr(), c"use".as_ptr(), &mut (*sd).bl as *mut BlockList);
            sl_doscript_simple_pc(c"maps".as_ptr(), c"use".as_ptr(), &mut (*sd).bl as *mut BlockList);
        }
        t if t == ITM_QUIVER => {
            if !can_use!() {
                clif_sendminitext(sd, c"You cannot use this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            clif_sendminitext(sd, c"This item is only usable with a bow.".as_ptr());
        }
        t if t == ITM_MOUNT => {
            if !can_use!() {
                clif_sendminitext(sd, c"You cannot use this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd);
            sl_doscript_simple_pc(item_db::search((*sd).status.inventory[id_u].id).yname.as_ptr(), c"use".as_ptr(), &mut (*sd).bl as *mut BlockList);
            sl_doscript_simple_pc(c"onMountItem".as_ptr(), std::ptr::null(), &mut (*sd).bl as *mut BlockList);
        }
        t if t == ITM_FACE => {
            if !can_use!() {
                clif_sendminitext(sd, c"You cannot use this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd);
            sl_doscript_simple_pc(item_db::search((*sd).status.inventory[id_u].id).yname.as_ptr(), c"use".as_ptr(), &mut (*sd).bl as *mut BlockList);
            sl_doscript_simple_pc(c"useFace".as_ptr(), std::ptr::null(), &mut (*sd).bl as *mut BlockList);
        }
        t if t == ITM_SET => {
            if !can_use!() {
                clif_sendminitext(sd, c"You cannot use this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd);
            sl_doscript_simple_pc(item_db::search((*sd).status.inventory[id_u].id).yname.as_ptr(), c"use".as_ptr(), &mut (*sd).bl as *mut BlockList);
            sl_doscript_simple_pc(c"useSetItem".as_ptr(), std::ptr::null(), &mut (*sd).bl as *mut BlockList);
        }
        t if t == ITM_SKIN => {
            if !can_use!() {
                clif_sendminitext(sd, c"You cannot use this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd);
            sl_doscript_simple_pc(item_db::search((*sd).status.inventory[id_u].id).yname.as_ptr(), c"use".as_ptr(), &mut (*sd).bl as *mut BlockList);
            sl_doscript_simple_pc(c"useSkinItem".as_ptr(), std::ptr::null(), &mut (*sd).bl as *mut BlockList);
        }
        t if t == ITM_HAIR_DYE => {
            if !can_use!() {
                clif_sendminitext(sd, c"You cannot use this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd);
            sl_doscript_simple_pc(item_db::search((*sd).status.inventory[id_u].id).yname.as_ptr(), c"use".as_ptr(), &mut (*sd).bl as *mut BlockList);
            sl_doscript_simple_pc(c"useHairDye".as_ptr(), std::ptr::null(), &mut (*sd).bl as *mut BlockList);
        }
        t if t == ITM_FACEACCTWO => {
            if !can_use!() {
                clif_sendminitext(sd, c"You cannot use this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd);
            sl_doscript_simple_pc(item_db::search((*sd).status.inventory[id_u].id).yname.as_ptr(), c"use".as_ptr(), &mut (*sd).bl as *mut BlockList);
            sl_doscript_simple_pc(c"useBeardItem".as_ptr(), std::ptr::null(), &mut (*sd).bl as *mut BlockList);
        }
        t if t == ITM_SMOKE => {
            if !can_smoke!() {
                clif_sendminitext(sd, c"You cannot smoke this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd);
            sl_doscript_simple_pc(item_db::search((*sd).status.inventory[id_u].id).yname.as_ptr(), c"use".as_ptr(), &mut (*sd).bl as *mut BlockList);
            sl_doscript_simple_pc(c"use".as_ptr(), std::ptr::null(), &mut (*sd).bl as *mut BlockList);
            (*sd).status.inventory[id_u].dura -= 1;
            if (*sd).status.inventory[id_u].dura == 0 {
                pc_delitem(sd, id, 1, 3);
            } else {
                clif_sendadditem(sd, id);
            }
        }
        // All equip types: ITM_WEAP(3) through ITM_HAND(17) inclusive.
        // This range covers: WEAP, ARMOR, SHIELD, HELM, LEFT, RIGHT, SUBLEFT,
        // SUBRIGHT, FACEACC, CROWN, MANTLE, NECKLACE, BOOTS, COAT, HAND.
        t if t >= ITM_WEAP && t <= ITM_HAND => {
            if !can_equip!() {
                clif_sendminitext(sd, c"You cannot equip/de-equip on this map.".as_ptr());
                return 0;
            }
            pc_equipitem(sd, id);
        }
        t if t == ITM_ETC => {
            if !can_use!() {
                clif_sendminitext(sd, c"You cannot use this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd);
            sl_doscript_simple_pc(item_db::search((*sd).status.inventory[id_u].id).yname.as_ptr(), c"use".as_ptr(), &mut (*sd).bl as *mut BlockList);
            sl_doscript_simple_pc(c"use".as_ptr(), std::ptr::null(), &mut (*sd).bl as *mut BlockList);
        }
        _ => {}
    }

    0
}

// ─── pc_runfloor_sub ──────────────────────────────────────────────────────────

/// `int pc_runfloor_sub(USER* sd)` — check if the player is standing on a FLOOR
/// or sub-2 NPC cell, and if so trigger its script.
pub unsafe fn pc_runfloor_sub(sd: *mut MapSessionData) -> i32 {
    use crate::game::npc::NpcData;
    if sd.is_null() { return 0; }

    let npc_id = match block_grid::first_in_cell((*sd).bl.m as usize, (*sd).bl.x, (*sd).bl.y, BL_NPC) {
        Some(id) => id,
        None => return 0,
    };
    let nd_arc = match crate::game::map_server::map_id2npc_ref(npc_id) {
        Some(n) => n,
        None => return 0,
    };
    let nd = &mut *nd_arc.write() as *mut NpcData;

    if (*nd).bl.subtype != FLOOR && (*nd).bl.subtype != 2 { return 0; }

    if (*nd).bl.subtype == 2 {
        sl_async_freeco(sd);
        sl_doscript_2_pc((*nd).name.as_ptr(), c"click".as_ptr(), &mut (*sd).bl as *mut BlockList, &mut (*nd).bl as *mut BlockList);
    }
    0
}

// ─── Equipment functions ──────────────────────────────────────────────────────
//

/// `int pc_isequip(USER* sd, int type)` — returns the item id in equip slot
/// `type`, or 0 if the slot is empty.
///
/// Bounds-checked: returns 0 for out-of-range `type`.
pub unsafe fn pc_isequip(
    sd:   *mut MapSessionData,
    type_: i32,
) -> i32 {
    if sd.is_null() { return 0; }
    if type_ < 0 || type_ >= 15 { return 0; }
    (*sd).status.equip[type_ as usize].id as i32
}

/// `int pc_loaditem(USER* sd)` — send all non-empty inventory slots to the
/// client via `clif_sendadditem`.
pub unsafe fn pc_loaditem(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }
    let maxinv = (*sd).status.maxinv as usize;
    for i in 0..maxinv {
        if (*sd).status.inventory[i].id != 0 {
            clif_sendadditem(sd, i as i32);
        }
    }
    0
}

/// `int pc_loadequip(USER* sd)` — send all non-empty equip slots to the client
/// via `clif_sendequip`.
///
/// Only slots 0..14 are active equipment positions.
pub unsafe fn pc_loadequip(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }
    for i in 0..14 {
        if (*sd).status.equip[i].id > 0 {
            clif_sendequip(sd, i as i32);
        }
    }
    0
}

/// `int pc_canequipitem(USER* sd, int id)` — check whether inventory slot `id`
/// can be equipped given the current state of the player.
///
/// Returns a `MAP_ERR*` index on failure, or 0 on success.
///
/// Checks:
/// - Two-handed weapon conflicts with an equipped shield and vice-versa.
/// - Item level requirement.
/// - Might (strength) requirement.
/// - Sex restriction.
///
/// `id` is a slot index into `sd->status.inventory`.
pub unsafe fn pc_canequipitem(
    sd: *mut MapSessionData,
    id: i32,
) -> i32 {
    if sd.is_null() { return 0; }
    let maxinv = (*sd).status.maxinv as i32;
    if id < 0 || id >= maxinv { return 0; }

    let itemid = (*sd).status.inventory[id as usize].id;

    // Two-handed weapon conflicts:
    // If a weapon with look 10000..29999 is equipped, a shield cannot be added.
    if pc_isequip(sd, EQ_WEAP) != 0 {
        let weap_look = item_db::search((*sd).status.equip[EQ_WEAP as usize].id).look;
        if item_db::search(itemid).typ as i32 == ITM_SHIELD
            && weap_look >= 10000
            && weap_look <= 29999
        {
            return MAP_ERRITM2H as i32;
        }
    }

    // If a shield is equipped, a two-handed weapon cannot be added.
    if pc_isequip(sd, EQ_SHIELD) != 0 {
        let itm_look = item_db::search(itemid).look;
        if item_db::search(itemid).typ as i32 == ITM_WEAP
            && itm_look >= 10000
            && itm_look <= 29999
        {
            return MAP_ERRITM2H as i32;
        }
    }

    if ((*sd).status.level as i32) < item_db::search(itemid).level as i32 {
        return MAP_ERRITMLEVEL as i32;
    }
    if (*sd).might < item_db::search(itemid).mightreq {
        return MAP_ERRITMMIGHT as i32;
    }
    let item_sex = item_db::search(itemid).sex as i32;
    if ((*sd).status.sex as i32) != item_sex && item_sex != 2 {
        return MAP_ERRITMSEX as i32;
    }

    0
}

/// `int pc_canequipstats(USER* sd, int id)` — check whether an item with item-id
/// `id` can be equipped given the player's current HP/MP totals.
///
/// Returns 1 if allowed, 0 if the vita/mana penalty would reduce hp/mp below 0.
pub unsafe fn pc_canequipstats(
    sd: *mut MapSessionData,
    id: u32,
) -> i32 {
    if sd.is_null() { return 0; }

    let vita = item_db::search(id).vita;
    if vita < 0 && vita.unsigned_abs() > (*sd).max_hp {
        return 0;
    }
    let mana = item_db::search(id).mana;
    if mana < 0 && mana.unsigned_abs() > (*sd).max_mp {
        return 0;
    }

    1
}

/// `int pc_equipitem(USER* sd, int id)` — begin the equip sequence for inventory
/// slot `id`.
///
/// Validates state, ownership, equip eligibility, and stat requirements before
/// firing the `onEquip` Lua event via `sl_doscript_blargs`.  The actual slot
/// assignment happens in `pc_equipscript` which runs from within the Lua hook.
pub unsafe fn pc_equipitem(
    sd: *mut MapSessionData,
    id: i32,
) -> i32 {
    if sd.is_null() { return 0; }
    let maxinv = (*sd).status.maxinv as i32;
    if id < 0 || id >= maxinv { return 0; }
    let id_u = id as usize;

    if (*sd).status.inventory[id_u].id == 0 { return 0; }

    // State restrictions (non-GMs only).
    if (*sd).status.state != 0 && (*sd).status.gm_level == 0 {
        if (*sd).status.state == 1 {
            clif_sendminitext(sd, c"Spirit's can't do that.".as_ptr());
        }
        if (*sd).status.state == 3 {
            clif_sendminitext(sd, c"You can't do that while riding a mount.".as_ptr());
        }
        if (*sd).status.state == 4 {
            clif_sendminitext(sd, c"You can't do that while transformed.".as_ptr());
        }
        return 0;
    }

    // Ownership check.
    if (*sd).status.inventory[id_u].owner != 0
        && (*sd).status.inventory[id_u].owner != (*sd).bl.id
    {
        clif_sendminitext(sd, c"This does not belong to you.".as_ptr());
        return 0;
    }

    // Equip eligibility (level, might, sex, 2h conflicts).
    let ret = pc_canequipitem(sd, id);
    if ret != 0 {
        clif_sendminitext(sd, map_msg()[ret as usize].message.as_ptr());
        return 0;
    }

    // Determine equip slot from item type.  Equip types start at ITM_WEAP=3,
    // so slot = type - 3.  Valid range: 0..=14.
    let slot = item_db::search((*sd).status.inventory[id_u].id).typ as i32 - 3;
    if slot < 0 || slot > 14 {
        // Not an equip item.
        return 0;
    }

    // Stat check.
    if pc_canequipstats(sd, (*sd).status.inventory[id_u].id) == 0 {
        clif_sendminitext(sd, c"Your stats are too low to equip that.".as_ptr());
        return 0;
    }

    // Store the item id and inventory slot so pc_equipscript can finish the job.
    (*sd).equipid = (*sd).status.inventory[id_u].id;
    (*sd).invslot = id as u8;

    // Fire the Lua equip hooks.
    sl_doscript_simple_pc(c"onEquip".as_ptr(), std::ptr::null(), &mut (*sd).bl as *mut BlockList);
    sl_doscript_simple_pc(item_db::search((*sd).status.inventory[id_u].id).yname.as_ptr(), c"onEquip".as_ptr(), &mut (*sd).bl as *mut BlockList);

    0
}

/// `int pc_equipscript(USER* sd)` — second phase of the equip sequence, called
/// from within the Lua `onEquip` hook.
///
/// Resolves the target slot (handling left/right ring swaps), removes any
/// previously-equipped item in that slot via an `onUnequip` hook, copies the
/// inventory item into the equip array, removes it from the inventory, and then
/// updates client state.
pub unsafe fn pc_equipscript(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }

    let mut ret = item_db::search((*sd).equipid).typ as i32 - 3;

    // Left/right ring slot arbitration: prefer the empty slot.
    if ret == EQ_LEFT {
        ret = if (*sd).status.equip[EQ_LEFT as usize].id != 0
                 && (*sd).status.equip[EQ_RIGHT as usize].id == 0
              { EQ_RIGHT } else { EQ_LEFT };
    }

    if ret == EQ_RIGHT {
        ret = if (*sd).status.equip[EQ_RIGHT as usize].id != 0
                 && (*sd).status.equip[EQ_LEFT as usize].id == 0
              { EQ_LEFT } else { EQ_RIGHT };
    }

    // Sub-ring slot arbitration.
    if ret == EQ_SUBLEFT {
        ret = if (*sd).status.equip[EQ_SUBLEFT as usize].id != 0
                 && (*sd).status.equip[EQ_SUBRIGHT as usize].id == 0
              { EQ_SUBLEFT } else { EQ_SUBRIGHT };
    }

    if ret == EQ_SUBRIGHT {
        ret = if (*sd).status.equip[EQ_SUBRIGHT as usize].id != 0
                 && (*sd).status.equip[EQ_SUBLEFT as usize].id == 0
              { EQ_SUBLEFT } else { EQ_SUBRIGHT };
    }

    // State restrictions (non-GMs only).
    if (*sd).status.state != 0 && (*sd).status.gm_level == 0 {
        if (*sd).status.state == 1 {
            clif_sendminitext(sd, c"Spirits can't do that.".as_ptr());
        }
        if (*sd).status.state == 2 {
            clif_sendminitext(sd, c"You can't do that while transformed.".as_ptr());
        }
        if (*sd).status.state == 3 {
            clif_sendminitext(sd, c"You can't do that while riding a mount.".as_ptr());
        }
        if (*sd).status.state == 4 {
            clif_sendminitext(sd, c"You can't do that while transformed.".as_ptr());
        }
        return 0;
    }

    if (*sd).status.equip[ret as usize].id != 0 {
        // A different item is already in this slot — trigger its unequip hook
        // instead of equipping immediately.
        (*sd).target   = (*sd).bl.id as i32;
        (*sd).attacker = (*sd).bl.id;
        (*sd).takeoffid = ret as i8;
        sl_doscript_simple_pc(c"onUnequip".as_ptr(), std::ptr::null(), &mut (*sd).bl as *mut BlockList);
        sl_doscript_simple_pc(item_db::search((*sd).equipid).yname.as_ptr(), c"equip".as_ptr(), &mut (*sd).bl as *mut BlockList);
        (*sd).equipid = 0;
        return 0;
    }

    // Slot is free: copy inventory item → equip slot, remove from inventory.
    let invslot = (*sd).invslot as usize;
    libc::memcpy(
        &mut (*sd).status.equip[ret as usize] as *mut _ as *mut libc::c_void,
        &(*sd).status.inventory[invslot] as *const _ as *const libc::c_void,
        std::mem::size_of::<crate::servers::char::charstatus::Item>(),
    );

    pc_delitem(sd, invslot as i32, 1, 6);
    sl_doscript_simple_pc(item_db::search((*sd).equipid).yname.as_ptr(), c"equip".as_ptr(), &mut (*sd).bl as *mut BlockList);
    (*sd).equipid = 0;

    // If a two-handed weapon was equipped, reset enchantment.
    if ret == EQ_WEAP && (*sd).enchanted > 1.0f32 {
        (*sd).enchanted = 1.0f32;
        (*sd).flank    = 0;
        (*sd).backstab = 0;
        clif_sendminitext(sd, c"Your weapon loses its enchantment.".as_ptr());
    }

    clif_sendequip(sd, ret);
    (*sd).status.equip[ret as usize].amount = 1;

    pc_calcstat(sd);
    clif_sendupdatestatus_onequip(sd);
    broadcast_update_state(sd);

    0
}

/// `int pc_unequip(USER* sd, int type)` — begin the unequip sequence for equip
/// slot `type`.
///
/// If the slot is empty, returns 1 immediately.  Otherwise stores `takeoffid`
/// and fires the `onUnequip` Lua hook so `pc_unequipscript` can finish.
pub unsafe fn pc_unequip(
    sd:    *mut MapSessionData,
    type_: i32,
) -> i32 {
    if sd.is_null() { return 1; }
    if type_ < 0 || type_ >= 15 { return 1; }

    if (*sd).status.equip[type_ as usize].id == 0 { return 1; }

    (*sd).takeoffid = type_ as i8;
    sl_doscript_simple_pc(c"onUnequip".as_ptr(), std::ptr::null(), &mut (*sd).bl as *mut BlockList);
    0
}

/// `int pc_unequipscript(USER* sd)` — second phase of the unequip sequence,
/// called from within the Lua `onUnequip` hook.
///
/// If `sd->equipid > 0`, the player is simultaneously equipping a new item
/// (swap): the old equip slot item is moved to inventory and the inventory item
/// occupies the slot.  Otherwise the equip slot is cleared and the item is
/// returned to inventory.
///
/// In both paths the client is updated and `pc_calcstat` recalculates stats.
pub unsafe fn pc_unequipscript(sd: *mut MapSessionData) -> i32 {
    if sd.is_null() { return 0; }

    let type_  = (*sd).takeoffid as usize;
    let takeoff = (*sd).status.equip[type_].id;

    if (*sd).equipid > 0 {
        // Swap: move old equip item to inventory, place new inventory item in slot.
        let mut it = std::mem::zeroed::<crate::servers::char::charstatus::Item>();
        let invslot = (*sd).invslot as usize;
        libc::memcpy(
            &mut it as *mut _ as *mut libc::c_void,
            &(*sd).status.equip[type_] as *const _ as *const libc::c_void,
            std::mem::size_of::<crate::servers::char::charstatus::Item>(),
        );
        libc::memcpy(
            &mut (*sd).status.equip[type_] as *mut _ as *mut libc::c_void,
            &(*sd).status.inventory[invslot] as *const _ as *const libc::c_void,
            std::mem::size_of::<crate::servers::char::charstatus::Item>(),
        );

        pc_delitem(sd, invslot as i32, 1, 6);
        pc_additem(sd, &mut it as *mut _);
        clif_sendequip(sd, type_ as i32);
        (*sd).status.equip[type_].amount = 1;
    } else {
        // Simple unequip: clear slot and return item to inventory.
        let mut it = std::mem::zeroed::<crate::servers::char::charstatus::Item>();
        libc::memcpy(
            &mut it as *mut _ as *mut libc::c_void,
            &(*sd).status.equip[type_] as *const _ as *const libc::c_void,
            std::mem::size_of::<crate::servers::char::charstatus::Item>(),
        );

        // Guard against a zeroed-out slot (C checks `&it.id <= 0` — bogus pointer
        // arithmetic, but effectively means id==0 due to struct layout).
        if it.id == 0 { return 1; }

        if pc_additem(sd, &mut it as *mut _) != 0 { return 1; }

        libc::memset(
            &mut (*sd).status.equip[type_] as *mut _ as *mut libc::c_void,
            0,
            std::mem::size_of::<crate::servers::char::charstatus::Item>(),
        );
        (*sd).target   = (*sd).bl.id as i32;
        (*sd).attacker = (*sd).bl.id;
    }

    // If a two-handed weapon was unequipped, reset enchantment.
    if type_ == EQ_WEAP as usize && (*sd).enchanted > 1.0f32 {
        (*sd).enchanted = 1.0f32;
        (*sd).flank    = 0;
        (*sd).backstab = 0;
        clif_sendminitext(sd, c"Your weapon loses its enchantment.".as_ptr());
    }

    // Fire the item's unequip Lua hook.
    sl_doscript_simple_pc(item_db::search(takeoff).yname.as_ptr(), c"unequip".as_ptr(), &mut (*sd).bl as *mut BlockList);

    (*sd).takeoffid = -1i8;
    pc_calcstat(sd);
    clif_sendupdatestatus_onequip(sd);
    broadcast_update_state(sd);

    0
}

/// `int pc_getitemscript(USER* sd, int id)` — pick up floor item with block-list
/// id `id` and add it to the player's inventory.
///
/// - Gold (item id 0): credited directly to `sd->status.money`.
/// - Non-droppable items (unless player is GM): rejected with a minitext.
/// - Stackable items with `pickuptype==0` and `stackamount==1`: picks up 1 at
///   a time (the floor item keeps the rest).
/// - All other cases: pick up the whole stack.
///
/// `clif_lookgone` + `map_delitem` are called when the floor item is exhausted.
pub unsafe fn pc_getitemscript(
    sd: *mut MapSessionData,
    id: i32,
) -> i32 {
    if sd.is_null() { return 0; }

    let fl_raw = map_id2fl(id as u32);
    if fl_raw.is_null() { return 0; }
    let fl = fl_raw as *mut FloorItemData;

    if (*fl).data.id == 0 {
        // It's gold — credit the amount and remove from map.
        (*sd).status.money += (*fl).data.amount as u32;
        clif_sendstatus(sd, SFLAG_XPMONEY);
        clif_lookgone_pc(&mut (*fl).bl as *mut BlockList);
        map_delitem((*fl).bl.id);

        return 0;
    }

    // Non-droppable items are blocked for regular players.
    if item_db::search((*fl).data.id).droppable != 0 && (*sd).status.gm_level == 0 {
        clif_sendminitext(sd, c"That item cannot be picked up.".as_ptr());
        return 0;
    }

    let mut it = std::mem::zeroed::<crate::servers::char::charstatus::Item>();
    let add: bool;

    if (*sd).pickuptype == 0
        && item_db::search((*fl).data.id).stack_amount == 1
        && (*fl).data.amount > 1
    {
        // Take only 1 from the stack.
        libc::memcpy(
            &mut it as *mut _ as *mut libc::c_void,
            &(*fl).data as *const _ as *const libc::c_void,
            std::mem::size_of::<crate::servers::char::charstatus::Item>(),
        );
        it.amount = 1;
        (*fl).data.amount -= 1;
        add = true;
    } else {
        // Take the whole stack.
        libc::memcpy(
            &mut it as *mut _ as *mut libc::c_void,
            &(*fl).data as *const _ as *const libc::c_void,
            std::mem::size_of::<crate::servers::char::charstatus::Item>(),
        );
        (*fl).data.amount = 0;
        add = true;
    }

    if (*fl).data.amount <= 0 {
        clif_lookgone_pc(&mut (*fl).bl as *mut BlockList);
        map_delitem((*fl).bl.id);
    }

    if add {
        pc_additem(sd, &mut it as *mut _);
    }

    if (*sd).pickuptype > 0 && (*fl).data.amount > 0 {
        return 0;
    }

    0
}

// ─── Position / warp / magic / state / combat functions ───────────────────────
//


/// `int pc_setpos(USER* sd, int m, int x, int y)` — sets the player's block-list
/// position without sending any client packets.
///
/// Guards against attempting to set position on a mob object (bl.id >= MOB_START_NUM).
/// Sets bl.m, bl.x, bl.y, and bl.type.
pub unsafe fn pc_setpos(
    sd: *mut MapSessionData,
    m: i32,
    x: i32,
    y: i32,
) -> i32 {
    use crate::game::mob::{MOB_START_NUM, BL_PC};
    if (*sd).bl.id >= MOB_START_NUM { return 0; }
    (*sd).bl.m  = m as u16;
    (*sd).bl.x  = x as u16;
    (*sd).bl.y  = y as u16;
    (*sd).bl.bl_type = BL_PC as u8;
    0
}

/// `int pc_warp(USER* sd, int m, int x, int y)` — full warp sequence.
///
/// If the target map is not loaded on this server, queries the `Maps` table for
/// the destination map server and calls `clif_transfer`. Otherwise, fires
/// pre-warp Lua hooks, calls `clif_quit` / `pc_setpos` / `clif_spawn` /
/// `clif_refresh`, then fires post-warp Lua hooks.
async fn lookup_map_server(map_id: i32) -> Option<u32> {
    sqlx::query_scalar::<_, Option<u32>>(
        "SELECT `MapServer` FROM `Maps` WHERE `MapId` = ?"
    )
    .bind(map_id)
    .fetch_optional(crate::database::get_pool())
    .await
    .ok()
    .flatten()
    .flatten()
}

pub async unsafe fn pc_warp(
    sd: *mut MapSessionData,
    mut m: i32,
    mut x: i32,
    mut y: i32,
) -> i32 {
    use crate::servers::char::charstatus::{MAX_SPELLS, MAX_MAGIC_TIMERS};
    use crate::database::map_db::map_is_loaded;

    if sd.is_null() { return 0; }

    let oldmap = (*sd).bl.m as i32;

    if m < 0 { m = 0; }
    if m >= MAX_MAP_PER_SERVER { m = MAX_MAP_PER_SERVER - 1; }

    // If the target map is not loaded on this server, hand off to the right server.
    if !map_is_loaded(m as u16) {
        if !session_exists((*sd).fd) {
            return 0;
        }

        let destsrv = lookup_map_server(m).await;

        let destsrv = match destsrv {
            Some(srv) => srv as i32,
            None => return 0,
        };

        if x < 0 || x > 255 { x = 1; }
        if y < 0 || y > 255 { y = 1; }

        (*sd).status.dest_pos.m = m as u16;
        (*sd).status.dest_pos.x = x as u16;
        (*sd).status.dest_pos.y = y as u16;

        clif_transfer(sd, destsrv, m, x, y);
        return 0;
    }

    // Map is loaded locally — clamp coordinates to map bounds.
    let map_ptr = crate::database::map_db::get_map_ptr(m as u16);
    if map_ptr.is_null() { return 0; }
    let xs = (*map_ptr).xs as i32;
    let ys = (*map_ptr).ys as i32;
    let can_mount = (*map_ptr).can_mount;

    if x == -1 {
        x = (xs / 2) + if xs % 2 != 0 { 1 } else { 0 };
        y = (ys / 2) + if ys % 2 != 0 { 1 } else { 0 };
    }

    if x < 0 { x = 0; }
    if y < 0 { y = 0; }
    if x >= xs { x = xs - 1; }
    if y >= ys { y = ys - 1; }

    // Fire map-leave hooks when changing maps.
    if m != oldmap {
        sl_doscript_simple_pc(c"mapLeave".as_ptr(), std::ptr::null(), &mut (*sd).bl as *mut BlockList);
        if can_mount == 0 {
            sl_doscript_simple_pc(c"onDismount".as_ptr(), std::ptr::null(), &mut (*sd).bl as *mut BlockList);
        }
    }

    // Fire passive_before_warp for each known spell.
    for i in 0..MAX_SPELLS {
        sl_doscript_simple_pc(magic_db::search((*sd).status.skill[i] as i32).yname.as_ptr(), c"passive_before_warp".as_ptr(), &mut (*sd).bl as *mut BlockList);
    }

    // Fire before_warp_while_cast for each active aether timer.
    for i in 0..MAX_MAGIC_TIMERS {
        sl_doscript_simple_pc(magic_db::search((*sd).status.dura_aether[i].id as i32).yname.as_ptr(), c"before_warp_while_cast".as_ptr(), &mut (*sd).bl as *mut BlockList);
    }

    // Perform the actual move.
    clif_quit(sd);
    pc_setpos(sd, m, x, y);
    clif_sendtime(sd);
    clif_spawn(sd);
    clif_refresh(sd);

    // Fire map-enter hooks when changing maps.
    if m != oldmap {
        sl_doscript_simple_pc(c"mapEnter".as_ptr(), std::ptr::null(), &mut (*sd).bl as *mut BlockList);
    }

    // Fire passive_on_warp for each known spell.
    for i in 0..MAX_SPELLS {
        sl_doscript_simple_pc(magic_db::search((*sd).status.skill[i] as i32).yname.as_ptr(), c"passive_on_warp".as_ptr(), &mut (*sd).bl as *mut BlockList);
    }

    // Fire on_warp_while_cast for each active aether timer.
    for i in 0..MAX_MAGIC_TIMERS {
        sl_doscript_simple_pc(magic_db::search((*sd).status.dura_aether[i].id as i32).yname.as_ptr(), c"on_warp_while_cast".as_ptr(), &mut (*sd).bl as *mut BlockList);
    }

    0
}

/// `int pc_loadmagic(USER* sd)` — sends each of the player's known spells to
/// the client via `clif_sendmagic`.
pub unsafe fn pc_loadmagic(sd: *mut MapSessionData) -> i32 {
    use crate::servers::char::charstatus::MAX_SPELLS;
    for i in 0..MAX_SPELLS {
        if (*sd).status.skill[i] > 0 {
            clif_sendmagic(&mut *sd, i as i32);
        }
    }
    0
}

/// `int pc_magic_startup(USER* sd)` — initialises spell durations at login.
///
/// For each active aether timer, sends the duration bar to the client and
/// calls the `recast` Lua hook on the spell.  Also sends any pending aether
/// (cooldown) values.
pub unsafe fn pc_magic_startup(sd: *mut MapSessionData) -> i32 {
    use crate::servers::char::charstatus::MAX_MAGIC_TIMERS;

    if sd.is_null() { return 0; }

    for x in 0..MAX_MAGIC_TIMERS {
        let p = &(*sd).status.dura_aether[x];

        if p.id > 0 {
            if p.duration > 0 {
                let tsd = map_id2sd_pc(p.caster_id);
                clif_send_duration(&mut *sd, p.id as i32, (p.duration / 1000) as i32, tsd);

                if !tsd.is_null() {
                    (*sd).target   = p.caster_id as i32;
                    (*sd).attacker = p.caster_id;
                    sl_doscript_2_pc(magic_db::search(p.id as i32).yname.as_ptr(), c"recast".as_ptr(), &mut (*sd).bl as *mut BlockList, &mut (*tsd).bl as *mut BlockList);
                } else {
                    (*sd).target   = (*sd).status.id as i32;
                    (*sd).attacker = (*sd).status.id;
                    sl_doscript_simple_pc(magic_db::search(p.id as i32).yname.as_ptr(), c"recast".as_ptr(), &mut (*sd).bl as *mut BlockList);
                }
            }

            if p.aether > 0 {
                clif_send_aether(&mut *sd, p.id as i32, p.aether / 1000);
            }
        }
    }

    0
}

/// `int pc_reload_aether(USER* sd)` — resends active aether (spell cooldown)
/// values to the client.  Called when the client reconnects.
pub unsafe fn pc_reload_aether(sd: *mut MapSessionData) -> i32 {
    use crate::servers::char::charstatus::MAX_MAGIC_TIMERS;
    for x in 0..MAX_MAGIC_TIMERS {
        let p = &(*sd).status.dura_aether[x];
        if p.id > 0 && p.aether > 0 {
            clif_send_aether(&mut *sd, p.id as i32, p.aether / 1000);
        }
    }
    0
}

/// `int pc_die(USER* sd)` — fires the `onDeathPlayer` Lua hook.
///
/// The actual stat/state changes are handled by `pc_diescript`; this function
/// just fires the hook so scripts can respond immediately.
pub unsafe fn pc_die(sd: *mut MapSessionData) -> i32 {
    sl_doscript_simple_pc(c"onDeathPlayer".as_ptr(), std::ptr::null(), &mut (*sd).bl as *mut BlockList);
    0
}

/// `int pc_diescript(USER* sd)` — full death processing.
///
/// - Clears `deathflag`, sets state to dead, zeroes HP.
/// - Clears all non-dispel-immune aether timers and fires their `uncast` hooks.
/// - Removes the dead player from all mob threat tables.
/// - Resets combat state (enchanted, flank, backstab, dmgshield).
/// - Recalculates stats and broadcasts updated state.
pub unsafe fn pc_diescript(sd: *mut MapSessionData) -> i32 {
    use crate::servers::char::charstatus::MAX_MAGIC_TIMERS;
    use crate::game::mob::{
        MAX_THREATCOUNT,
        MOB_SPAWN_START, MOB_SPAWN_MAX,
        MOB_ONETIME_START, MOB_ONETIME_MAX,
    };
    use std::sync::atomic::Ordering;

    if sd.is_null() { return 0; }

    let attacker_bl = map_id2bl_pc((*sd).attacker);

    (*sd).deathflag = 0;

    // Set the killer (use attacker's bl.id if we have it).
    if !attacker_bl.is_null() {
        (*sd).status.killedby = (*attacker_bl).id;
    }
    (*sd).status.state = 1; // PC_DIE
    (*sd).status.hp    = 0;

    // Clear active aether timers that are not dispel-immune.
    for i in 0..MAX_MAGIC_TIMERS {
        let id = (*sd).status.dura_aether[i].id;
        if id == 0 { continue; }

        if magic_db::search(id as i32).dispell as i32 > 0 { continue; }

        (*sd).status.dura_aether[i].duration = 0;
        clif_send_duration(
            &mut *sd,
            (*sd).status.dura_aether[i].id as i32,
            0,
            map_id2sd_pc((*sd).status.dura_aether[i].caster_id),
        );
        (*sd).status.dura_aether[i].caster_id = 0;

        {
            let anim = (*sd).status.dura_aether[i].animation as i32;
            let sd_bl = &mut (*sd).bl as *mut BlockList;
            if let Some(grid) = block_grid::get_grid((*sd).bl.m as usize) {
                let slot = &*crate::database::map_db::get_map_ptr((*sd).bl.m as u16);
                let ids = block_grid::ids_in_area(grid, (*sd).bl.x as i32, (*sd).bl.y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
                for id in ids {
                    if let Some(tsd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                        clif_sendanimation_inner(&mut tsd_arc.write().bl, anim, sd_bl, -1);
                    }
                }
            }
        }
        (*sd).status.dura_aether[i].animation = 0;

        if (*sd).status.dura_aether[i].aether == 0 {
            (*sd).status.dura_aether[i].id = 0;
        }

        // Fire uncast hook.
        let caster_bl = if (*sd).status.dura_aether[i].caster_id != (*sd).bl.id {
            map_id2bl_pc((*sd).status.dura_aether[i].caster_id)
        } else {
            std::ptr::null_mut()
        };

        if !caster_bl.is_null() {
            sl_doscript_2_pc(magic_db::search(id as i32).yname.as_ptr(), c"uncast".as_ptr(), &mut (*sd).bl as *mut BlockList, caster_bl);
        } else {
            sl_doscript_simple_pc(magic_db::search(id as i32).yname.as_ptr(), c"uncast".as_ptr(), &mut (*sd).bl as *mut BlockList);
        }
    }

    // Remove dead player from all spawn-mob threat tables.
    let spawn_start = MOB_SPAWN_START.load(Ordering::Relaxed);
    let spawn_max   = MOB_SPAWN_MAX.load(Ordering::Relaxed);
    if spawn_start != spawn_max {
        let mut x = spawn_start;
        while x < spawn_max {
            if let Some(tmob_arc) = crate::game::map_server::map_id2mob_ref(x) {
                let mut tmob = tmob_arc.write();
                for i in 0..MAX_THREATCOUNT {
                    if tmob.threat[i].user == (*sd).bl.id {
                        tmob.threat[i].user   = 0;
                        tmob.threat[i].amount = 0;
                    }
                }
            }
            x += 1;
        }
    }

    // Remove dead player from all one-time mob threat tables.
    let onetime_start = MOB_ONETIME_START.load(Ordering::Relaxed);
    let onetime_max   = MOB_ONETIME_MAX.load(Ordering::Relaxed);
    if onetime_start != onetime_max {
        let mut x = onetime_start;
        while x < onetime_max {
            if let Some(tmob_arc) = crate::game::map_server::map_id2mob_ref(x) {
                let mut tmob = tmob_arc.write();
                for i in 0..MAX_THREATCOUNT {
                    if tmob.threat[i].user == (*sd).bl.id {
                        tmob.threat[i].user   = 0;
                        tmob.threat[i].amount = 0;
                    }
                }
            }
            x += 1;
        }
    }

    // Reset combat modifiers.
    (*sd).enchanted  = 1.0_f32;
    (*sd).flank      = 0;
    (*sd).backstab   = 0;
    (*sd).dmgshield  = 0.0_f32;

    pc_calcstat(sd);
    broadcast_update_state(sd);

    0
}

/// Sync bridge for Lua/FFI callers that cannot `.await`.
/// SAFETY: MapSessionData: Send; blocking_run_async joins before returning.
pub unsafe fn pc_warp_sync(sd: *mut MapSessionData, m: i32, x: i32, y: i32) -> i32 {
    let sd_usize = sd as usize;
    crate::database::blocking_run_async(crate::database::assert_send(async move {
        let sd = sd_usize as *mut MapSessionData;
        pc_warp(sd, m, x, y).await
    }))
}

/// `int pc_res(USER* sd)` — resurrects the player in-place.
///
/// Sets state to alive, restores 100 HP, sends an HP/MP status update, and
/// warps the player to their current position (which re-spawns them for other
/// clients on the same map).
pub unsafe fn pc_res(sd: *mut MapSessionData) -> i32 {
    (*sd).status.state = PC_ALIVE as i8;
    (*sd).status.hp    = 100;
    clif_sendstatus(sd, SFLAG_HPMP);
    pc_warp_sync(sd, (*sd).bl.m as i32, (*sd).bl.x as i32, (*sd).bl.y as i32);
    0
}

// ─── Kill-registry helpers ────────────────────────────────────────────────────

/// Increment the kill-count for `mob` in `sd`'s kill registry, or add a new
/// entry if the mob is not yet present.
///
/// # Safety
/// `sd` must be a valid, non-null pointer to an initialised [`MapSessionData`].
pub unsafe fn addtokillreg(sd: *mut MapSessionData, mob: i32) -> i32 {
    use crate::servers::char::charstatus::MAX_KILLREG;
    if sd.is_null() { return 0; }
    let mob_u = mob as u32;
    // First pass: increment existing entry.
    for x in 0..MAX_KILLREG {
        if (*sd).status.killreg[x].mob_id == mob_u {
            (*sd).status.killreg[x].amount = (*sd).status.killreg[x].amount.wrapping_add(1);
            return 0;
        }
    }
    // Second pass: claim first empty slot.
    for x in 0..MAX_KILLREG {
        if (*sd).status.killreg[x].mob_id == 0 {
            (*sd).status.killreg[x].mob_id = mob_u;
            (*sd).status.killreg[x].amount = 1;
            return 0;
        }
    }
    0
}
