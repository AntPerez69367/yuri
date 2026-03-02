//! Player-character game logic — replaces `c_src/pc.c`.

#![allow(non_snake_case, dead_code, unused_variables)]

use std::ffi::{c_char, c_double, c_float, c_int, c_long, c_short, c_uchar, c_uint, c_ulong, c_ushort};
use std::os::raw::c_void;

use crate::database::map_db::BlockList;
// MobSpawnData is used by future porting tasks (Tasks 6+); import it when needed.
use crate::game::types::GfxViewer;
use crate::servers::char::charstatus::MmoCharStatus;

// ─── Helper structs (from map_server.h) ───────────────────────────────────────

/// `struct n_post` — linked-list node for parcels/NPC posts.
#[repr(C)]
pub struct NPost {
    pub prev: *mut NPost,
    pub pos:  c_uint,
}

/// `struct script_reg` — integer registry slot.
#[repr(C)]
pub struct ScriptReg {
    pub index: c_int,
    pub data:  c_int,
}

/// `struct script_regstr` — string registry slot.
/// Note: C uses `char data[256]` — `i8` maps correctly to C `char`.
#[repr(C)]
pub struct ScriptRegStr {
    pub index: c_int,
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
    pub item_count:    c_int,
    pub exchange_done: c_int,
    pub list_count:    c_int,
    pub gold:          c_uint,
    pub target:        c_uint,
}

/// Anonymous `state` sub-struct inside `map_sessiondata`.
#[repr(C)]
pub struct PcState {
    pub menu_or_input: c_int,
}

/// Anonymous `boditems` sub-struct inside `map_sessiondata`.
#[repr(C)]
pub struct PcBodItems {
    pub item:      [Item; 52],
    pub bod_count: c_int,
}

// ─── MapSessionData — mirrors `struct map_sessiondata` from map_server.h ──────

/// Mirrors `struct map_sessiondata` from `c_src/map_server.h`.
///
/// Field order matches the C definition exactly. Every field has been verified
/// against the C source. Do NOT reorder fields — `#[repr(C)]` layout depends on it.
#[repr(C)]
pub struct MapSessionData {
    // Intrusive block-list header (must be first — C code casts bl* ↔ sd*)
    pub bl:                BlockList,
    pub fd:                c_int,

    // mmo
    pub status:            MmoCharStatus,

    // status timers
    pub equiptimer:        c_ulong,
    pub ambushtimer:       c_ulong,

    // unsigned int group (multi-field C declarations, split individually)
    pub max_hp:            c_uint,
    pub max_mp:            c_uint,
    pub tempmax_hp:        c_uint,
    pub attacker:          c_uint,
    pub rangeTarget:       c_uint,
    pub equipid:           c_uint,
    pub breakid:           c_uint,
    pub pvp:               [[c_uint; 2]; 20],
    pub killspvp:          c_uint,
    pub timevalues:        [c_uint; 5],
    pub lastvita:          c_uint,
    pub groupid:           c_uint,
    pub disptimer:         c_uint,
    pub disptimertick:     c_uint,
    pub basemight:         c_uint,
    pub basewill:          c_uint,
    pub basegrace:         c_uint,
    pub basearmor:         c_uint,
    pub intpercentage:     c_uint,
    pub profileStatus:     c_uint,

    // int combat stats (first C declaration line)
    pub might:             c_int,
    pub will:              c_int,
    pub grace:             c_int,
    pub armor:             c_int,
    pub minSdam:           c_int,
    pub maxSdam:           c_int,
    pub minLdam:           c_int,
    pub maxLdam:           c_int,
    pub hit:               c_int,
    pub dam:               c_int,
    pub healing:           c_int,
    pub healingtimer:      c_int,
    pub pongtimer:         c_int,
    pub backstab:          c_int,

    pub heartbeat:         c_int,

    // int status flags (second C declaration line)
    pub flank:             c_int,
    pub polearm:           c_int,
    pub tooclose:          c_int,
    pub canmove:           c_int,
    pub iswalking:         c_int,
    pub paralyzed:         c_int,
    pub blind:             c_int,
    pub drunk:             c_int,
    pub snare:             c_int,
    pub silence:           c_int,
    pub critchance:        c_int,
    pub afk:               c_int,
    pub afktime:           c_int,
    pub totalafktime:      c_int,
    pub afktimer:          c_int,
    pub extendhit:         c_int,
    pub speed:             c_int,

    // int timers/misc (third C declaration line)
    pub crit:              c_int,
    pub duratimer:         c_int,
    pub scripttimer:       c_int,
    pub scripttick:        c_int,
    pub secondduratimer:   c_int,
    pub thirdduratimer:    c_int,
    pub fourthduratimer:   c_int,
    pub fifthduratimer:    c_int,
    pub wisdom:            c_int,
    pub bindx:             c_int,
    pub bindy:             c_int,
    pub hunter:            c_int,

    // short stats
    pub protection:        c_short,
    pub miss:              c_short,
    pub attack_speed:      c_short,
    pub con:               c_short,

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
    pub castusetimer:      c_int,
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
    pub npc_id:            c_int,
    pub npc_pos:           c_int,
    pub npc_lastpos:       c_int,
    pub npc_menu:          c_int,
    pub npc_amount:        c_int,
    pub npc_g:             c_int,
    pub npc_gc:            c_int,
    pub target:            c_int,
    pub time:              c_int,
    pub time2:             c_int,
    pub lasttime:          c_int,
    pub timer:             c_int,
    pub npc_stack:         c_int,
    pub npc_stackmax:      c_int,

    pub npc_script:        *mut i8,
    pub npc_scriptroot:    *mut i8,

    // registry
    pub reg:               *mut ScriptReg,
    pub regstr:            *mut ScriptRegStr,
    pub npcp:              NPost,
    pub reg_num:           c_int,
    pub regstr_num:        c_int,

    // group
    pub bcount:            c_int,
    pub group_count:       c_int,
    pub group_on:          c_int,
    pub group_leader:      c_uint,

    // exchange
    pub exchange_on:       c_int,
    pub exchange:          PcExchange,
    pub state:             PcState,
    pub boditems:          PcBodItems,

    // lua
    pub coref:             c_uint,
    pub coref_container:   c_uint,

    // creation system
    pub creation_works:    c_int,
    pub creation_item:     c_int,
    pub creation_itemamount: c_int,

    // boards
    pub board_candel:      c_int,
    pub board_canwrite:    c_int,
    pub board:             c_int,
    pub board_popup:       c_int,
    pub co_timer:          c_int,

    pub question:          [i8; 64],
    pub speech:            [i8; 255],
    pub profilepic_data:   [i8; 65535],
    pub profile_data:      [i8; 255],

    pub profilepic_size:   u16,
    pub profile_size:      u8,

    // mob
    pub mob_len:           c_int,
    pub mob_count:         c_int,
    pub mob_item:          c_int,

    pub msPing:            c_int,
    pub pbColor:           c_int,

    pub time_check:        c_uint,
    pub time_hash:         c_uint,
    pub last_click:        c_uint,

    pub chat_timer:        c_int,
    pub savetimer:         c_int,

    pub gfx:               GfxViewer,
    pub IgnoreList:        *mut SdIgnoreList,

    pub optFlags:          c_ulong,
    pub uFlags:            c_ulong,
    pub LastPongStamp:     c_ulong,
    pub LastPingTick:      c_ulong,
    pub flags:             c_ulong,
    pub LastWalkTick:      c_ulong,

    pub PrevSeed:          u8,
    pub NextSeed:          u8,
    pub LastWalk:          u8,
    pub loaded:            u8,
}

// SAFETY: MapSessionData is only accessed from the single game thread while
// holding appropriate locks. Raw pointers are to C-managed memory.
unsafe impl Send for MapSessionData {}

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

// ─── Constants (from c_src/map_server.h and c_src/mmo.h) ──────────────────────

// Registry size constants (from mmo.h)
pub const MAX_GLOBALREG:       usize = 5000;
pub const MAX_GLOBALPLAYERREG: usize = 500;
pub const MAX_GLOBALQUESTREG:  usize = 250;
pub const MAX_GLOBALNPCREG:    usize = 100;

// BL_* type flags (from map_server.h `enum { BL_PC=1, BL_MOB=2, BL_NPC=4, BL_ITEM=8 }`)
pub const BL_PC:   c_int = 0x01;
pub const BL_MOB:  c_int = 0x02;
pub const BL_NPC:  c_int = 0x04;
pub const BL_ITEM: c_int = 0x08;

// PC state values — `enum { PC_ALIVE, PC_DIE, PC_INVIS, PC_MOUNTED, PC_DISGUISE }`
pub const PC_ALIVE:    c_int = 0;
pub const PC_DIE:      c_int = 1;
pub const PC_INVIS:    c_int = 2;
pub const PC_MOUNTED:  c_int = 3;
pub const PC_DISGUISE: c_int = 4;

// optFlags enum values (from map_server.h)
pub const OPT_FLAG_STEALTH:     c_ulong = 32;
pub const OPT_FLAG_NOCLICK:     c_ulong = 64;
pub const OPT_FLAG_WALKTHROUGH: c_ulong = 128;
pub const OPT_FLAG_GHOSTS:      c_ulong = 256;

// uFlags enum values (from map_server.h)
pub const U_FLAG_NONE:       c_ulong = 0;
pub const U_FLAG_SILENCED:   c_ulong = 1;
pub const U_FLAG_CANPK:      c_ulong = 2;
pub const U_FLAG_CANBEPK:    c_ulong = 3;
pub const U_FLAG_IMMORTAL:   c_ulong = 8;
pub const U_FLAG_UNPHYSICAL: c_ulong = 16;
pub const U_FLAG_EVENTHOST:  c_ulong = 32;
pub const U_FLAG_CONSTABLE:  c_ulong = 64;
pub const U_FLAG_ARCHON:     c_ulong = 128;
pub const U_FLAG_GM:         c_ulong = 256;

// SFLAG values for clif_sendstatus (from map_server.h)
pub const SFLAG_UNKNOWN1:   c_int = 0x01;
pub const SFLAG_UNKNOWN2:   c_int = 0x02;
pub const SFLAG_UNKNOWN3:   c_int = 0x04;
pub const SFLAG_ALWAYSON:   c_int = 0x08;
pub const SFLAG_XPMONEY:    c_int = 0x10;
pub const SFLAG_HPMP:       c_int = 0x20;
pub const SFLAG_FULLSTATS:  c_int = 0x40;
pub const SFLAG_GMON:       c_int = 0x80;

// settingFlags values for sd->status.settingFlags (from mmo.h)
pub const FLAG_WHISPER:   c_uint = 1;
pub const FLAG_GROUP:     c_uint = 2;
pub const FLAG_SHOUT:     c_uint = 4;
pub const FLAG_ADVICE:    c_uint = 8;
pub const FLAG_MAGIC:     c_uint = 16;
pub const FLAG_WEATHER:   c_uint = 32;
pub const FLAG_REALM:     c_uint = 64;
pub const FLAG_EXCHANGE:  c_uint = 128;
pub const FLAG_FASTMOVE:  c_uint = 256;
pub const FLAG_SOUND:     c_uint = 4096;
pub const FLAG_HELM:      c_uint = 8192;
pub const FLAG_NECKLACE:  c_uint = 16384;

// normalFlags (from mmo.h `enum normalFlags`)
pub const FLAG_PARCEL: c_ulong = 1;
pub const FLAG_MAIL:   c_ulong = 16;

// MAX_MAP_PER_SERVER (from mmo.h)
pub const MAX_MAP_PER_SERVER: c_int = 65535;

// SP_* parameter type constants (from map_server.h)
pub const SP_HP:  c_int = 0;
pub const SP_MP:  c_int = 1;
pub const SP_MHP: c_int = 2;
pub const SP_MMP: c_int = 3;

// AREA constant: enum value 4 in map_parse.h `{ ALL_CLIENT, SAMESRV, SAMEMAP,
//   SAMEMAP_WOS, AREA, ... }`
pub const AREA: c_int = 4;

// LOOK_SEND (enum { LOOK_GET=0, LOOK_SEND=1 } in map_parse.h)
pub const LOOK_SEND: c_int = 1;

// FLOOR subtype constant (enum { SCRIPT=0, FLOOR=1 } in map_server.h)
pub const FLOOR: c_uchar = 1;

// BLOCK_SIZE (from c_deps/yuri.h: `#define BLOCK_SIZE 8`)
pub const BLOCK_SIZE_PC: c_int = 8;

// MAX_GROUP_MEMBERS (from map_server.h `#define MAX_GROUP_MEMBERS 256`)
pub const MAX_GROUP_MEMBERS: usize = 256;

// ITM_* item type constants (from c_src/item_db.h enum)
pub const ITM_EAT:       c_int = 0;
pub const ITM_USE:       c_int = 1;
pub const ITM_SMOKE:     c_int = 2;
pub const ITM_WEAP:      c_int = 3;
pub const ITM_ARMOR:     c_int = 4;
pub const ITM_SHIELD:    c_int = 5;
pub const ITM_HELM:      c_int = 6;
pub const ITM_LEFT:      c_int = 7;
pub const ITM_RIGHT:     c_int = 8;
pub const ITM_SUBLEFT:   c_int = 9;
pub const ITM_SUBRIGHT:  c_int = 10;
pub const ITM_FACEACC:   c_int = 11;
pub const ITM_CROWN:     c_int = 12;
pub const ITM_MANTLE:    c_int = 13;
pub const ITM_NECKLACE:  c_int = 14;
pub const ITM_BOOTS:     c_int = 15;
pub const ITM_COAT:      c_int = 16;
pub const ITM_HAND:      c_int = 17;
pub const ITM_ETC:       c_int = 18;
pub const ITM_USESPC:    c_int = 19;
pub const ITM_TRAPS:     c_int = 20;
pub const ITM_BAG:       c_int = 21;
pub const ITM_MAP:       c_int = 22;
pub const ITM_QUIVER:    c_int = 23;
pub const ITM_MOUNT:     c_int = 24;
pub const ITM_FACE:      c_int = 25;
pub const ITM_SET:       c_int = 26;
pub const ITM_SKIN:      c_int = 27;
pub const ITM_HAIR_DYE:  c_int = 28;
pub const ITM_FACEACCTWO: c_int = 29;

// EQ_* equip slot constants (from c_src/item_db.h enum)
pub const EQ_WEAP:      c_int = 0;
pub const EQ_ARMOR:     c_int = 1;
pub const EQ_SHIELD:    c_int = 2;
pub const EQ_HELM:      c_int = 3;
pub const EQ_LEFT:      c_int = 4;
pub const EQ_RIGHT:     c_int = 5;
pub const EQ_SUBLEFT:   c_int = 6;
pub const EQ_SUBRIGHT:  c_int = 7;
pub const EQ_FACEACC:   c_int = 8;
pub const EQ_CROWN:     c_int = 9;
pub const EQ_MANTLE:    c_int = 10;
pub const EQ_NECKLACE:  c_int = 11;
pub const EQ_BOOTS:     c_int = 12;
pub const EQ_COAT:      c_int = 13;
pub const EQ_FACEACCTWO: c_int = 14;

// MAP_ERR* message indices (from c_src/map_server.h enum starting at MAP_WHISPFAIL=0)
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

/// `struct map_msg_data` from `c_src/map_server.h`.
/// Layout: `char message[256]` at offset 0, `int len` at offset 256.
/// MSG_MAX = 38 entries (count from map_server.h enum).
#[repr(C)]
pub struct MapMsgData {
    pub message: [i8; 256],
    pub len:     c_int,
}

// Sql opaque handle (declared in db_mysql.h as `struct Sql`)
// Used for sql_handle global in pc.c
#[repr(C)]
pub struct Sql {
    _opaque: [u8; 0],
}

// SqlStmt opaque handle (declared in db_mysql.h as `struct SqlStmt`)
#[repr(C)]
pub struct SqlStmt {
    _opaque: [u8; 0],
}

// SqlDataType used by SqlStmt_BindColumn (from db_mysql.h enum)
#[repr(C)]
pub enum SqlDataType {
    SqlDtNull = 0,
    SqlDtUChar,
    SqlDtChar,
    SqlDtUShort,
    SqlDtShort,
    SqlDtUInt,
    SqlDtInt,
    SqlDtULong,
    SqlDtLong,
    SqlDtULongLong,
    SqlDtLongLong,
    SqlDtFloat,
    SqlDtDouble,
    SqlDtString,
    SqlDtEnum,
    SqlDtBlob,
    SqlDtLastType,
}

// SQL_ERROR / SQL_SUCCESS constants (from db_mysql.h)
pub const SQL_ERROR:   c_int = -1;
pub const SQL_SUCCESS: c_int =  0;

// ─── Extern "C" declarations (C functions called by pc.c) ─────────────────────

#[cfg(not(test))]
extern "C" {
    // ── map entity lookup ──────────────────────────────────────────────────────
    // NOTE: map_id2bl, map_id2mob, map_id2sd, map_addiddb, map_deliddb,
    //       map_addblock, map_delblock, map_moveblock, map_additem, map_canmove,
    //       map_foreachinarea, map_foreachincell, map_foreachinblock
    //   are already declared in mob.rs extern block — do not redeclare here.

    /// `struct flooritem_data* map_id2fl(unsigned int id)`
    pub fn map_id2fl(id: c_uint) -> *mut c_void;  // FLOORITEM* — opaque (full type in floor.rs)

    /// `struct npc_data* map_id2npc(unsigned int id)` — returns NPC* (opaque here)
    pub fn map_id2npc(id: c_uint) -> *mut c_void;

    /// `struct block_list* map_firstincell(int m, int x, int y, int type)`
    pub fn map_firstincell(m: c_int, x: c_int, y: c_int, bl_type: c_int) -> *mut BlockList;

    /// `int map_isloaded(m)` — actually a macro `(map[m].registry)` but linked via C wrapper
    /// In practice Rust code uses ffi::map_db::map_is_loaded; kept here for completeness.
    // Not declared here — use ffi::map_db::map_is_loaded instead.

    /// `unsigned int map_readglobalreg(int m, const char* reg)`
    pub fn map_readglobalreg(m: c_int, reg: *const c_char) -> c_uint;

    // ── map[] C global array — accessed as a pointer in Rust ──────────────────
    // Accessed via ffi::map_db::get_map_ptr / map_is_loaded; no static decl needed here.

    // ── groups[][] global array (from map_parse.c) ─────────────────────────────
    /// `extern unsigned int groups[MAX_GROUPS][MAX_GROUP_MEMBERS]`
    /// Flat 2-D array: groups[256][256]. Access as groups[groupid * 256 + slot].
    #[link_name = "groups"]
    pub static groups: [c_uint; 65536]; // 256 * 256 = 65536 elements

    // ── clif_* network helpers ─────────────────────────────────────────────────
    /// `int clif_sendstatus(USER* sd, int flags)`
    pub fn clif_sendstatus(sd: *mut MapSessionData, flags: c_int) -> c_int;

    /// `int clif_sendupdatestatus(USER* sd)`
    pub fn clif_sendupdatestatus(sd: *mut MapSessionData) -> c_int;

    /// `int clif_sendupdatestatus_onequip(USER* sd)`
    pub fn clif_sendupdatestatus_onequip(sd: *mut MapSessionData) -> c_int;

    /// `int clif_sendminitext(USER* sd, const char* text)`
    pub fn clif_sendminitext(sd: *mut MapSessionData, text: *const c_char) -> c_int;

    /// `int clif_sendadditem(USER* sd, int slot)`
    pub fn clif_sendadditem(sd: *mut MapSessionData, slot: c_int) -> c_int;

    /// `int clif_senddelitem(USER* sd, int slot, int type)`
    pub fn clif_senddelitem(sd: *mut MapSessionData, slot: c_int, type_: c_int) -> c_int;

    /// `int clif_sendequip(USER* sd, int slot)`
    pub fn clif_sendequip(sd: *mut MapSessionData, slot: c_int) -> c_int;

    /// `int clif_sendmagic(USER* sd, int slot)`
    pub fn clif_sendmagic(sd: *mut MapSessionData, slot: c_int) -> c_int;

    /// `int clif_sendtime(USER* sd)`
    pub fn clif_sendtime(sd: *mut MapSessionData) -> c_int;

    /// `int clif_spawn(USER* sd)`
    pub fn clif_spawn(sd: *mut MapSessionData) -> c_int;

    /// `int clif_quit(USER* sd)`
    pub fn clif_quit(sd: *mut MapSessionData) -> c_int;

    /// `int clif_refresh(USER* sd)`
    pub fn clif_refresh(sd: *mut MapSessionData) -> c_int;

    /// `int clif_getchararea(USER* sd)`
    pub fn clif_getchararea(sd: *mut MapSessionData) -> c_int;

    /// `int clif_sendchararea(USER* sd)`
    pub fn clif_sendchararea(sd: *mut MapSessionData) -> c_int;

    /// `int clif_sendaction(struct block_list* bl, int a, int b, int c)`
    pub fn clif_sendaction(bl: *mut BlockList, a: c_int, b: c_int, c: c_int) -> c_int;

    /// `int clif_transfer(USER* sd, int serverid, int m, int x, int y)`
    pub fn clif_transfer(sd: *mut MapSessionData, serverid: c_int, m: c_int, x: c_int, y: c_int) -> c_int;

    /// `int clif_grouphealth_update(USER* sd)`
    pub fn clif_grouphealth_update(sd: *mut MapSessionData) -> c_int;

    /// `void clif_send_selfbar(USER* sd)`
    pub fn clif_send_selfbar(sd: *mut MapSessionData);

    /// `void clif_send_groupbars(USER* sd, USER* tsd)`
    pub fn clif_send_groupbars(sd: *mut MapSessionData, tsd: *mut MapSessionData);

    /// `int clif_send_mobbars(struct block_list* bl, va_list ap)`
    pub fn clif_send_mobbars(bl: *mut BlockList, ...) -> c_int;

    /// `int clif_send_duration(USER* sd, int id, int time, USER* tsd)`
    pub fn clif_send_duration(sd: *mut MapSessionData, id: c_int, time: c_uint, tsd: *mut MapSessionData) -> c_int;

    /// `int clif_send_aether(USER* sd, int id, int val)`
    pub fn clif_send_aether(sd: *mut MapSessionData, id: c_int, val: c_int) -> c_int;

    /// `int clif_updatestate(struct block_list* bl, va_list ap)`
    pub fn clif_updatestate(bl: *mut BlockList, ...) -> c_int;

    /// `int clif_broadcast(const char* msg, int m)`
    pub fn clif_broadcast(msg: *const c_char, m: c_int) -> c_int;

    // clif_lookgone, clif_object_look_sub2, clif_sendanimation
    // already declared in mob.rs extern block.

    // ── timer functions ────────────────────────────────────────────────────────
    /// `int timer_insert(uint32_t tick, uint32_t interval, int (*func)(int,int), int id, int data)`
    pub fn timer_insert(
        tick: c_uint,
        interval: c_uint,
        func: unsafe extern "C" fn(c_int, c_int) -> c_int,
        id: c_int,
        data: c_int,
    ) -> c_int;

    /// `int timer_remove(int handle)`
    pub fn timer_remove(handle: c_int) -> c_int;

    // ── scripting ──────────────────────────────────────────────────────────────
    // sl_doscript_blargs already declared in mob.rs extern block.

    /// `void rust_sl_async_freeco(void* user)` — exposed via scripting.h macro
    #[link_name = "rust_sl_async_freeco"]
    pub fn sl_async_freeco(sd: *mut c_void);

    // ── item db lookups — redirect to rust_itemdb_* symbols ──────────────────
    #[link_name = "rust_itemdb_yname"]
    pub fn itemdb_yname(id: c_uint) -> *mut c_char;

    #[link_name = "rust_itemdb_name"]
    pub fn itemdb_name(id: c_uint) -> *mut c_char;

    #[link_name = "rust_itemdb_vita"]
    pub fn itemdb_vita(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_mana"]
    pub fn itemdb_mana(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_might"]
    pub fn itemdb_might(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_grace"]
    pub fn itemdb_grace(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_will"]
    pub fn itemdb_will(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_ac"]
    pub fn itemdb_ac(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_healing"]
    pub fn itemdb_healing(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_dam"]
    pub fn itemdb_dam(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_hit"]
    pub fn itemdb_hit(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_dura"]
    pub fn itemdb_dura(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_maxamount"]
    pub fn itemdb_maxamount(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_stackamount"]
    pub fn itemdb_stackamount(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_type"]
    pub fn itemdb_type(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_look"]
    pub fn itemdb_look(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_level"]
    pub fn itemdb_level(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_mightreq"]
    pub fn itemdb_mightreq(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_sex"]
    pub fn itemdb_sex(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_droppable"]
    pub fn itemdb_droppable(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_unequip"]
    pub fn itemdb_unequip(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_class"]
    pub fn itemdb_class(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_rank"]
    pub fn itemdb_rank(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_time"]
    pub fn itemdb_time(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_protection"]
    pub fn itemdb_protection(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_minSdam"]
    pub fn itemdb_minSdam(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_maxSdam"]
    pub fn itemdb_maxSdam(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_minLdam"]
    pub fn itemdb_minLdam(id: c_uint) -> c_int;

    #[link_name = "rust_itemdb_maxLdam"]
    pub fn itemdb_maxLdam(id: c_uint) -> c_int;

    // ── magic db lookups ───────────────────────────────────────────────────────
    // magicdb_yname and magicdb_name already declared in mob.rs extern block.

    #[link_name = "rust_magicdb_dispel"]
    pub fn magicdb_dispel(id: c_int) -> c_int;

    // ── class db lookups ───────────────────────────────────────────────────────
    #[link_name = "rust_classdb_path"]
    pub fn classdb_path(id: c_int) -> c_int;

    /// `unsigned int rust_classdb_level(int path, int lvl)`
    #[link_name = "rust_classdb_level"]
    pub fn classdb_level(path: c_int, lvl: c_int) -> c_uint;

    // ── SQL / db_mysql functions ───────────────────────────────────────────────
    /// The global MySQL handle (map_server.c / map_server.h)
    #[link_name = "sql_handle"]
    pub static sql_handle: *mut Sql;

    /// `int Sql_Query(Sql* self, const char* query, ...)` — variadic
    pub fn Sql_Query(self_: *mut Sql, query: *const c_char, ...) -> c_int;

    /// `uint64_t Sql_NumRows(Sql* self)` — returned as u64 (= C uint64_t)
    pub fn Sql_NumRows(self_: *mut Sql) -> u64;

    /// `void Sql_FreeResult(Sql* self)`
    pub fn Sql_FreeResult(self_: *mut Sql);

    /// `size_t Sql_EscapeString(Sql* self, char* out_to, const char* from)`
    pub fn Sql_EscapeString(self_: *mut Sql, out_to: *mut c_char, from: *const c_char) -> usize;

    /// `struct SqlStmt* SqlStmt_Malloc(Sql* sql)`
    pub fn SqlStmt_Malloc(sql: *mut Sql) -> *mut SqlStmt;

    /// `int SqlStmt_Prepare(SqlStmt* self, const char* query, ...)` — variadic
    pub fn SqlStmt_Prepare(self_: *mut SqlStmt, query: *const c_char, ...) -> c_int;

    /// `int SqlStmt_Execute(SqlStmt* self)`
    pub fn SqlStmt_Execute(self_: *mut SqlStmt) -> c_int;

    /// `int SqlStmt_BindColumn(SqlStmt* self, size_t idx, SqlDataType type, void* buf, size_t buf_len, unsigned long* out_len, int* is_null)`
    pub fn SqlStmt_BindColumn(
        self_: *mut SqlStmt,
        idx: usize,
        buffer_type: SqlDataType,
        buffer: *mut c_void,
        buffer_len: usize,
        out_len: *mut c_ulong,
        is_null: *mut c_int,
    ) -> c_int;

    /// `uint64_t SqlStmt_NumRows(SqlStmt* self)`
    pub fn SqlStmt_NumRows(self_: *mut SqlStmt) -> u64;

    /// `int SqlStmt_NextRow(SqlStmt* self)` — returns SQL_SUCCESS or SQL_ERROR
    pub fn SqlStmt_NextRow(self_: *mut SqlStmt) -> c_int;

    /// `void SqlStmt_Free(SqlStmt* self)`
    pub fn SqlStmt_Free(self_: *mut SqlStmt);

    // ── session helpers — Rust-exported to C (from yuri.h/session.h) ──────────
    #[link_name = "rust_session_exists"]
    pub fn rust_session_exists(fd: c_int) -> bool;

    #[link_name = "rust_session_set_eof"]
    pub fn rust_session_set_eof(fd: c_int, val: c_int);

    // ── rnd / tick / time ─────────────────────────────────────────────────────
    // rnd and gettick are already declared in mob.rs extern block.
    // cur_time is already declared in mob.rs extern block.

    // ── map_msg global array ──────────────────────────────────────────────────
    // `extern struct map_msg_data map_msg[MSG_MAX]`
    // MSG_MAX = 38 (count from map_server.h enum ending at MSG_MAX).
    // map_msg[idx].message is a char[256], offset 0; len is int at offset 256.
    #[link_name = "map_msg"]
    pub static map_msg: [MapMsgData; 38];

    // ── map_foreachincell — re-declared for pc.rs item callbacks ─────────────
    #[link_name = "map_foreachincell"]
    pub fn map_foreachincell_pc(
        f: unsafe extern "C" fn(*mut BlockList, ...) -> c_int,
        m: c_int, x: c_int, y: c_int, bl_type: c_int, ...
    ) -> c_int;

    // ── map_additem — re-declared for pc.rs item drops ────────────────────────
    #[link_name = "map_additem"]
    pub fn map_additem_pc(bl: *mut BlockList);

    // ── clif_object_look_sub2 — re-declared for pc.rs item drop broadcast ────
    #[link_name = "clif_object_look_sub2"]
    pub fn clif_object_look_sub2(bl: *mut BlockList, ...) -> c_int;

    // ── intif_save — C inline helper in map_char.h ────────────────────────────
    // intif_save is a static inline in map_char.h that calls rust_intif_save.
    // It cannot appear in a Rust extern block. Rust code that needs to save
    // should call rust_intif_save or implement the same serialization directly.
    // Declare rust_intif_save for those cases:
    pub fn rust_intif_save(data: *const u8, len: c_uint);

    // ── map entity lookup (typed for pc.rs — mob.rs uses opaque c_void) ───────
    /// `USER* map_id2sd(unsigned int id)` — typed as MapSessionData* for pc.rs use.
    #[link_name = "map_id2sd"]
    pub fn map_id2sd_pc(id: c_uint) -> *mut MapSessionData;

    /// `struct block_list* map_id2bl(unsigned int id)` — re-declared for pc.rs use.
    #[link_name = "map_id2bl"]
    pub fn map_id2bl_pc(id: c_uint) -> *mut BlockList;

    /// `void map_delitem(unsigned int id)` — remove a floor item from the map.
    pub fn map_delitem(id: c_uint);

    /// `int map_foreachinarea(...)` — re-declared for pc.rs use.
    #[link_name = "map_foreachinarea"]
    pub fn map_foreachinarea_pc(
        f: unsafe extern "C" fn(*mut BlockList, ...) -> c_int,
        m: c_int, x: c_int, y: c_int, range: c_int, bl_type: c_int, ...
    ) -> c_int;

    // ── clif callbacks shared with mob.rs ─────────────────────────────────────
    /// `int clif_sendanimation(struct block_list* bl, ...)` — re-declared for pc.rs.
    #[link_name = "clif_sendanimation"]
    pub fn clif_sendanimation_pc(bl: *mut BlockList, ...) -> c_int;

    /// `void clif_lookgone(struct block_list* bl)` — re-declared for pc.rs.
    #[link_name = "clif_lookgone"]
    pub fn clif_lookgone_pc(bl: *mut BlockList);

    // ── scripting — re-declared for pc.rs use (defined in mob.rs but not pub-use) ──
    /// `int sl_doscript_blargs(const char* root, const char* method, int nargs, ...)`
    #[link_name = "sl_doscript_blargs"]
    pub fn sl_doscript_blargs_pc(
        yname: *const c_char, event: *const c_char, nargs: c_int, ...
    ) -> c_int;

    // ── magic db — re-declared for pc.rs use ─────────────────────────────────
    /// `char* magicdb_yname(int id)` — redirects to rust_magicdb_yname.
    #[link_name = "rust_magicdb_yname"]
    pub fn magicdb_yname_pc(id: c_int) -> *mut c_char;

    // ── tick — re-declared for pc.rs use ─────────────────────────────────────
    /// `unsigned int gettick()`
    #[link_name = "gettick"]
    pub fn gettick_pc() -> c_uint;

    // ── intif_save proxy via sl_pc_forcesave (sl_compat.c) ───────────────────
    /// `int sl_pc_forcesave(void* sd)` — calls intif_save(sd) in C.
    pub fn sl_pc_forcesave(sd: *mut c_void) -> c_int;

    // ── SQL debug helper ──────────────────────────────────────────────────────
    /// `void Sql_ShowDebug_(Sql* self, const char* file, unsigned long line)`
    /// Called by the C `Sql_ShowDebug(self)` macro; we invoke it directly in Rust.
    pub fn Sql_ShowDebug_(self_: *mut Sql, file: *const c_char, line: c_ulong);

    // ── rnd — rnd(x) is a C macro over randomMT(); use randomMT() directly ──────
    /// `unsigned int randomMT(void)` — Mersenne Twister generator.
    /// Equivalent to: `(int)(randomMT() & 0xFFFFFF) % n`
    pub fn randomMT() -> c_uint;

    // ── network encryption (net_crypt.c) ──────────────────────────────────────
    /// `int encrypt(int fd)` — encrypts the WFIFO buffer and returns the encrypted length.
    #[link_name = "encrypt"]
    pub fn encrypt_fd(fd: c_int) -> c_int;
}

// ─── Timer functions (ported from c_src/pc.c and c_src/map_parse.c) ──────────
//
// Naming: `rust_pc_<name>` for pc_* functions, `rust_bl_<name>` for bl_* functions.
// All functions gated on #[cfg(not(test))] because they call C FFI.
// Each function is `#[no_mangle]` so C can call it back as a timer callback.

/// `int pc_item_timer(int id, int none)` — removes a floor item when its timer expires.
/// Calls `clif_lookgone` to hide it from clients, then `map_delitem` to remove it.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_item_timer(id: c_int, _none: c_int) -> c_int {
    use crate::game::mob::{map_id2bl, BL_ITEM};
    let fl = map_id2bl(id as c_uint);
    if fl.is_null() { return 1; }
    clif_lookgone_pc(fl);
    map_delitem(id as c_uint);
    1
}

/// `int pc_savetimer(int id, int none)` — periodically saves a player's character data.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_savetimer(id: c_int, _none: c_int) -> c_int {
    let sd = map_id2sd_pc(id as c_uint);
    if !sd.is_null() {
        sl_pc_forcesave(sd as *mut c_void);
    }
    0
}

/// `int pc_castusetimer(int id, int none)` — resets `castusetimer` field to 0 each tick.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_castusetimer(id: c_int, _none: c_int) -> c_int {
    let sd = map_id2sd_pc(id as c_uint);
    if !sd.is_null() {
        (*sd).castusetimer = 0;
    }
    0
}

/// `int pc_afktimer(int id, int none)` — tracks AFK time and plays idle animations.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_afktimer(id: c_int, _none: c_int) -> c_int {
    let sd = map_id2sd_pc(id as c_uint);
    if sd.is_null() { return 0; }

    (*sd).afktime += 1;

    if (*sd).afk == 1 && (*sd).status.state == 0 {
        (*sd).totalafktime += 10;
        clif_sendaction(&mut (*sd).bl as *mut BlockList, 0x10, 0x4E, 0);
        return 0;
    }

    if (*sd).afk == 1 && (*sd).status.state == 3 {
        (*sd).totalafktime += 10;
        map_foreachinarea_pc(
            clif_sendanimation_pc,
            (*sd).bl.m as c_int, (*sd).bl.x as c_int, (*sd).bl.y as c_int,
            AREA, BL_PC, 324i32, &mut (*sd).bl as *mut BlockList, 0i32,
        );
        return 0;
    }

    if (*sd).afk == 1 && (*sd).status.state == PC_DIE as i8 {
        (*sd).totalafktime += 10;
        return 0;
    }

    if (*sd).afktime >= 30 {
        if (*sd).status.state == 0 {
            (*sd).totalafktime += 300;
            clif_sendaction(&mut (*sd).bl as *mut BlockList, 0x10, 0x4E, 0);
        } else if (*sd).status.state == 3 {
            (*sd).totalafktime += 300;
            map_foreachinarea_pc(
                clif_sendanimation_pc,
                (*sd).bl.m as c_int, (*sd).bl.x as c_int, (*sd).bl.y as c_int,
                AREA, BL_PC, 324i32, &mut (*sd).bl as *mut BlockList, 0i32,
            );
        }
        (*sd).afk = 1;
    }

    0
}

/// `int pc_starttimer(USER* sd)` — registers all periodic timers for a logged-in player.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_starttimer(sd: *mut MapSessionData) -> c_int {
    (*sd).timer = timer_insert(1000, 1000,
        rust_pc_timer as unsafe extern "C" fn(c_int, c_int) -> c_int,
        (*sd).bl.id as c_int, 0);
    (*sd).pongtimer = timer_insert(30000, 30000,
        rust_pc_sendpong as unsafe extern "C" fn(c_int, c_int) -> c_int,
        (*sd).bl.id as c_int, 0);
    (*sd).savetimer = timer_insert(60000, 60000,
        rust_pc_savetimer as unsafe extern "C" fn(c_int, c_int) -> c_int,
        (*sd).bl.id as c_int, 0);
    if (*sd).status.gm_level < 50 {
        (*sd).afktimer = timer_insert(10000, 10000,
            rust_pc_afktimer as unsafe extern "C" fn(c_int, c_int) -> c_int,
            (*sd).bl.id as c_int, 0);
    }
    (*sd).duratimer = timer_insert(1000, 1000,
        rust_bl_duratimer as unsafe extern "C" fn(c_int, c_int) -> c_int,
        (*sd).bl.id as c_int, 0);
    (*sd).secondduratimer = timer_insert(250, 250,
        rust_bl_secondduratimer as unsafe extern "C" fn(c_int, c_int) -> c_int,
        (*sd).bl.id as c_int, 0);
    (*sd).thirdduratimer = timer_insert(500, 500,
        rust_bl_thirdduratimer as unsafe extern "C" fn(c_int, c_int) -> c_int,
        (*sd).bl.id as c_int, 0);
    (*sd).fourthduratimer = timer_insert(1500, 1500,
        rust_bl_fourthduratimer as unsafe extern "C" fn(c_int, c_int) -> c_int,
        (*sd).bl.id as c_int, 0);
    (*sd).fifthduratimer = timer_insert(3000, 3000,
        rust_bl_fifthduratimer as unsafe extern "C" fn(c_int, c_int) -> c_int,
        (*sd).bl.id as c_int, 0);
    (*sd).scripttimer = timer_insert(500, 500,
        rust_pc_scripttimer as unsafe extern "C" fn(c_int, c_int) -> c_int,
        (*sd).bl.id as c_int, 0);
    (*sd).castusetimer = timer_insert(250, 250,
        rust_pc_castusetimer as unsafe extern "C" fn(c_int, c_int) -> c_int,
        (*sd).bl.id as c_int, 0);
    0
}

/// `int pc_stoptimer(USER* sd)` — removes all periodic timers for a player.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_stoptimer(sd: *mut MapSessionData) -> c_int {
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
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_bl_duratimer(id: c_int, _none: c_int) -> c_int {
    use crate::game::mob::MAX_MAGIC_TIMERS;
    let sd = map_id2sd_pc(id as c_uint);
    if sd.is_null() { return 0; }

    // while_passive: each learned spell fires once per second
    for x in 0..52usize {
        if (*sd).status.skill[x] > 0 {
            sl_doscript_blargs_pc(
                magicdb_yname_pc((*sd).status.skill[x] as c_int),
                c"while_passive".as_ptr(),
                1i32, &mut (*sd).bl as *mut BlockList,
            );
        }
    }

    // while_equipped: each worn item fires once per second
    for x in 0..14usize {
        if (*sd).status.equip[x].id > 0 {
            sl_doscript_blargs_pc(
                itemdb_yname((*sd).status.equip[x].id),
                c"while_equipped".as_ptr(),
                1i32, &mut (*sd).bl as *mut BlockList,
            );
        }
    }

    // duration / aether tick for each active magic timer slot
    for x in 0..MAX_MAGIC_TIMERS {
        let mid = (*sd).status.dura_aether[x].id as c_int;
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
                    let health: i64 = if (*tbl).bl_type as c_int == BL_MOB {
                        let tmob = tbl as *mut crate::game::mob::MobSpawnData;
                        (*tmob).current_vita as i64
                    } else {
                        0
                    };
                    if health > 0 || (*tbl).bl_type as c_int == BL_PC {
                        sl_doscript_blargs_pc(
                            magicdb_yname_pc(mid), c"while_cast".as_ptr(),
                            2i32, &mut (*sd).bl as *mut BlockList, tbl,
                        );
                    }
                } else {
                    sl_doscript_blargs_pc(
                        magicdb_yname_pc(mid), c"while_cast".as_ptr(),
                        1i32, &mut (*sd).bl as *mut BlockList,
                    );
                }

                if (*sd).status.dura_aether[x].duration <= 0 {
                    (*sd).status.dura_aether[x].duration = 0;
                    clif_send_duration(
                        sd,
                        (*sd).status.dura_aether[x].id as c_int,
                        0u32,
                        map_id2sd_pc((*sd).status.dura_aether[x].caster_id),
                    );
                    (*sd).status.dura_aether[x].caster_id = 0;
                    map_foreachinarea_pc(
                        clif_sendanimation_pc,
                        (*sd).bl.m as c_int,
                        (*sd).bl.x as c_int,
                        (*sd).bl.y as c_int,
                        AREA, BL_PC,
                        (*sd).status.dura_aether[x].animation as c_int,
                        &mut (*sd).bl as *mut BlockList,
                        -1i32,
                    );
                    (*sd).status.dura_aether[x].animation = 0;

                    if (*sd).status.dura_aether[x].aether == 0 {
                        (*sd).status.dura_aether[x].id = 0;
                    }

                    if !tbl.is_null() {
                        sl_doscript_blargs_pc(
                            magicdb_yname_pc(mid), c"uncast".as_ptr(),
                            2i32, &mut (*sd).bl as *mut BlockList, tbl,
                        );
                    } else {
                        sl_doscript_blargs_pc(
                            magicdb_yname_pc(mid), c"uncast".as_ptr(),
                            1i32, &mut (*sd).bl as *mut BlockList,
                        );
                    }
                }
            }

            if (*sd).status.dura_aether[x].aether > 0 {
                (*sd).status.dura_aether[x].aether -= 1000;

                if (*sd).status.dura_aether[x].aether <= 0 {
                    clif_send_aether(sd, (*sd).status.dura_aether[x].id as c_int, 0);

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
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_bl_secondduratimer(id: c_int, _none: c_int) -> c_int {
    use crate::game::mob::MAX_MAGIC_TIMERS;
    let sd = map_id2sd_pc(id as c_uint);
    if sd.is_null() { return 0; }

    for x in 0..52usize {
        if (*sd).status.skill[x] > 0 {
            sl_doscript_blargs_pc(
                magicdb_yname_pc((*sd).status.skill[x] as c_int),
                c"while_passive_250".as_ptr(),
                1i32, &mut (*sd).bl as *mut BlockList,
            );
        }
    }

    for x in 0..14usize {
        if (*sd).status.equip[x].id > 0 {
            sl_doscript_blargs_pc(
                itemdb_yname((*sd).status.equip[x].id),
                c"while_equipped_250".as_ptr(),
                1i32, &mut (*sd).bl as *mut BlockList,
            );
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
                    let health: i64 = if (*tbl).bl_type as c_int == BL_MOB {
                        let tmob = tbl as *mut crate::game::mob::MobSpawnData;
                        (*tmob).current_vita as i64
                    } else {
                        0
                    };
                    if health > 0 || (*tbl).bl_type as c_int == BL_PC {
                        sl_doscript_blargs_pc(
                            magicdb_yname_pc((*sd).status.dura_aether[x].id as c_int),
                            c"while_cast_250".as_ptr(),
                            2i32, &mut (*sd).bl as *mut BlockList, tbl,
                        );
                    }
                } else {
                    sl_doscript_blargs_pc(
                        magicdb_yname_pc((*sd).status.dura_aether[x].id as c_int),
                        c"while_cast_250".as_ptr(),
                        1i32, &mut (*sd).bl as *mut BlockList,
                    );
                }
            }
        }
    }

    0
}

/// `int bl_thirdduratimer(int id, int none)` — 500ms tick: fires `while_passive_500`,
/// `while_equipped_500`, `while_cast_500` events (no expire logic).
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_bl_thirdduratimer(id: c_int, _none: c_int) -> c_int {
    use crate::game::mob::MAX_MAGIC_TIMERS;
    let sd = map_id2sd_pc(id as c_uint);
    if sd.is_null() { return 0; }

    for x in 0..52usize {
        if (*sd).status.skill[x] > 0 {
            sl_doscript_blargs_pc(
                magicdb_yname_pc((*sd).status.skill[x] as c_int),
                c"while_passive_500".as_ptr(),
                1i32, &mut (*sd).bl as *mut BlockList,
            );
        }
    }

    for x in 0..14usize {
        if (*sd).status.equip[x].id > 0 {
            sl_doscript_blargs_pc(
                itemdb_yname((*sd).status.equip[x].id),
                c"while_equipped_500".as_ptr(),
                1i32, &mut (*sd).bl as *mut BlockList,
            );
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
                    let health: i64 = if (*tbl).bl_type as c_int == BL_MOB {
                        let tmob = tbl as *mut crate::game::mob::MobSpawnData;
                        (*tmob).current_vita as i64
                    } else {
                        0
                    };
                    if health > 0 || (*tbl).bl_type as c_int == BL_PC {
                        sl_doscript_blargs_pc(
                            magicdb_yname_pc((*sd).status.dura_aether[x].id as c_int),
                            c"while_cast_500".as_ptr(),
                            2i32, &mut (*sd).bl as *mut BlockList, tbl,
                        );
                    }
                } else {
                    sl_doscript_blargs_pc(
                        magicdb_yname_pc((*sd).status.dura_aether[x].id as c_int),
                        c"while_cast_500".as_ptr(),
                        1i32, &mut (*sd).bl as *mut BlockList,
                    );
                }
            }
        }
    }

    0
}

/// `int bl_fourthduratimer(int id, int none)` — 1500ms tick: fires `while_passive_1500`,
/// `while_equipped_1500`, `while_cast_1500` events (no expire logic).
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_bl_fourthduratimer(id: c_int, _none: c_int) -> c_int {
    use crate::game::mob::MAX_MAGIC_TIMERS;
    let sd = map_id2sd_pc(id as c_uint);
    if sd.is_null() { return 0; }

    for x in 0..52usize {
        if (*sd).status.skill[x] > 0 {
            sl_doscript_blargs_pc(
                magicdb_yname_pc((*sd).status.skill[x] as c_int),
                c"while_passive_1500".as_ptr(),
                1i32, &mut (*sd).bl as *mut BlockList,
            );
        }
    }

    for x in 0..14usize {
        if (*sd).status.equip[x].id > 0 {
            sl_doscript_blargs_pc(
                itemdb_yname((*sd).status.equip[x].id),
                c"while_equipped_1500".as_ptr(),
                1i32, &mut (*sd).bl as *mut BlockList,
            );
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
                    let health: i64 = if (*tbl).bl_type as c_int == BL_MOB {
                        let tmob = tbl as *mut crate::game::mob::MobSpawnData;
                        (*tmob).current_vita as i64
                    } else {
                        0
                    };
                    if health > 0 || (*tbl).bl_type as c_int == BL_PC {
                        sl_doscript_blargs_pc(
                            magicdb_yname_pc((*sd).status.dura_aether[x].id as c_int),
                            c"while_cast_1500".as_ptr(),
                            2i32, &mut (*sd).bl as *mut BlockList, tbl,
                        );
                    }
                } else {
                    sl_doscript_blargs_pc(
                        magicdb_yname_pc((*sd).status.dura_aether[x].id as c_int),
                        c"while_cast_1500".as_ptr(),
                        1i32, &mut (*sd).bl as *mut BlockList,
                    );
                }
            }
        }
    }

    0
}

/// `int bl_fifthduratimer(int id, int none)` — 3000ms tick: fires `while_passive_3000`,
/// `while_equipped_3000`, `while_cast_3000` events (no expire logic).
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_bl_fifthduratimer(id: c_int, _none: c_int) -> c_int {
    use crate::game::mob::MAX_MAGIC_TIMERS;
    let sd = map_id2sd_pc(id as c_uint);
    if sd.is_null() { return 0; }

    for x in 0..52usize {
        if (*sd).status.skill[x] > 0 {
            sl_doscript_blargs_pc(
                magicdb_yname_pc((*sd).status.skill[x] as c_int),
                c"while_passive_3000".as_ptr(),
                1i32, &mut (*sd).bl as *mut BlockList,
            );
        }
    }

    for x in 0..14usize {
        if (*sd).status.equip[x].id > 0 {
            sl_doscript_blargs_pc(
                itemdb_yname((*sd).status.equip[x].id),
                c"while_equipped_3000".as_ptr(),
                1i32, &mut (*sd).bl as *mut BlockList,
            );
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
                    let health: i64 = if (*tbl).bl_type as c_int == BL_MOB {
                        let tmob = tbl as *mut crate::game::mob::MobSpawnData;
                        (*tmob).current_vita as i64
                    } else {
                        0
                    };
                    if health > 0 || (*tbl).bl_type as c_int == BL_PC {
                        sl_doscript_blargs_pc(
                            magicdb_yname_pc((*sd).status.dura_aether[x].id as c_int),
                            c"while_cast_3000".as_ptr(),
                            2i32, &mut (*sd).bl as *mut BlockList, tbl,
                        );
                    }
                } else {
                    sl_doscript_blargs_pc(
                        magicdb_yname_pc((*sd).status.dura_aether[x].id as c_int),
                        c"while_cast_3000".as_ptr(),
                        1i32, &mut (*sd).bl as *mut BlockList,
                    );
                }
            }
        }
    }

    0
}

/// `int bl_aethertimer(int id, int none)` — decrements aether timers and clears
/// expired aether slots; called from NPC/scripting code via a one-shot timer.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_bl_aethertimer(id: c_int, _none: c_int) -> c_int {
    use crate::game::mob::MAX_MAGIC_TIMERS;
    let sd = map_id2sd_pc(id as c_uint);
    if sd.is_null() { return 0; }

    for x in 0..MAX_MAGIC_TIMERS {
        if (*sd).status.dura_aether[x].id > 0 {
            if (*sd).status.dura_aether[x].aether > 0 {
                (*sd).status.dura_aether[x].aether -= 1000;
            }

            if (*sd).status.dura_aether[x].aether <= 0 {
                clif_send_aether(sd, (*sd).status.dura_aether[x].id as c_int, 0);

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
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_timer(id: c_int, _none: c_int) -> c_int {
    let sd = map_id2sd_pc(id as c_uint);
    if sd.is_null() { return 1; }

    (*sd).time2 += 1000;
    (*sd).time = 0;
    (*sd).chat_timer = 0;

    if (*sd).time2 >= 60000 {
        rust_pc_requestmp(sd);
        (*sd).time2 = 0;
    }

    let mut reset: c_int = 0;
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

        if (*sd).status.pkduration <= 0 {
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
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_scripttimer(id: c_int, _none: c_int) -> c_int {
    let sd = map_id2sd_pc(id as c_uint);
    if sd.is_null() { return 1; }

    if (*sd).selfbar != 0 {
        clif_send_selfbar(sd);
    }

    if (*sd).groupbars != 0 && (*sd).group_count > 1 {
        for x in 0..(*sd).group_count as usize {
            let tsd = map_id2sd_pc(groups[(*sd).groupid as usize * 256 + x]);
            if tsd.is_null() { continue; }
            if (*tsd).bl.m == (*sd).bl.m {
                clif_send_groupbars(sd, tsd);
                clif_grouphealth_update(sd);
            }
        }
    }

    if (*sd).mobbars != 0 {
        map_foreachinarea_pc(
            clif_send_mobbars,
            (*sd).bl.m as c_int, (*sd).bl.x as c_int, (*sd).bl.y as c_int,
            AREA, BL_MOB, sd,
        );
    }

    if (*sd).status.hp <= 0 && (*sd).deathflag != 0 {
        rust_pc_diescript(sd);
        return 0;
    }

    if (*sd).dmgshield > 0.0 {
        clif_send_duration(sd, 0, (*sd).dmgshield as c_uint + 1, std::ptr::null_mut());
    }

    (*sd).deathflag = 0;
    (*sd).scripttick += 1;

    sl_doscript_blargs_pc(
        c"pc_timer".as_ptr(), c"tick".as_ptr(),
        1i32, &mut (*sd).bl as *mut BlockList,
    );

    if (*sd).status.setting_flags & FLAG_ADVICE as u16 != 0 {
        sl_doscript_blargs_pc(
            c"pc_timer".as_ptr(), c"advice".as_ptr(),
            1i32, &mut (*sd).bl as *mut BlockList,
        );
    }

    0
}

/// `int pc_atkspeed(int id, int none)` — resets `attacked` flag; called by a one-shot timer.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_atkspeed(id: c_int, _none: c_int) -> c_int {
    let sd = map_id2sd_pc(id as c_uint);
    if sd.is_null() { return 1; }
    (*sd).attacked = 0;
    1
}

/// `int pc_disptimertick(int id, int none)` — counts down the display timer and fires
/// the Lua `display_timer` event when it reaches zero.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_disptimertick(id: c_int, _none: c_int) -> c_int {
    let sd = map_id2sd_pc(id as c_uint);
    if sd.is_null() { return 1; }

    if ((*sd).disptimertick as i64) - 1 < 0 {
        (*sd).disptimertick = 0;
    } else {
        (*sd).disptimertick -= 1;
    }

    if (*sd).disptimertick == 0 {
        sl_doscript_blargs_pc(
            c"pc_timer".as_ptr(), c"display_timer".as_ptr(),
            1i32, &mut (*sd).bl as *mut BlockList,
        );
        timer_remove((*sd).disptimer as c_int);
        (*sd).disptimertype = 0;
        (*sd).disptimer = 0;
        return 1;
    }

    0
}

/// `int pc_sendpong(int id, int none)` — sends a keep-alive ping packet to the client
/// and sets EOF if the session has already closed.
/// (Originally in c_src/map_parse.c; moved here as part of pc.rs timer block.)
///
/// The C WFIFO macros expand to `rust_session_*` calls, so they are invoked
/// directly here using the Rust session FFI layer (see c_src/session.h).
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_sendpong(id: c_int, _none: c_int) -> c_int {
    let sd = map_id2sd_pc(id as c_uint);
    if sd.is_null() { return 1; }

    if !rust_session_exists((*sd).fd) {
        rust_session_set_eof((*sd).fd, 8);
        return 0;
    }

    // WFIFOHEAD(fd, 10)
    crate::ffi::session::rust_session_wfifohead((*sd).fd, 10);

    // WFIFOB(fd, 0) = 0xAA
    let p = crate::ffi::session::rust_session_wdata_ptr((*sd).fd, 0);
    if !p.is_null() { *p = 0xAAu8; }

    // WFIFOW(fd, 1) = SWAP16(0x09)  — big-endian 16-bit (byte-swap of 0x0009 → 0x0900)
    let p = crate::ffi::session::rust_session_wdata_ptr((*sd).fd, 1) as *mut u16;
    if !p.is_null() { p.write_unaligned(0x09u16.swap_bytes()); }

    // WFIFOB(fd, 3) = 0x68
    let p = crate::ffi::session::rust_session_wdata_ptr((*sd).fd, 3);
    if !p.is_null() { *p = 0x68u8; }

    // WFIFOL(fd, 5) = SWAP32(gettick())  — big-endian 32-bit tick
    let tick = gettick_pc();
    let p = crate::ffi::session::rust_session_wdata_ptr((*sd).fd, 5) as *mut u32;
    if !p.is_null() { p.write_unaligned(tick.swap_bytes()); }

    // WFIFOB(fd, 9) = 0x00
    let p = crate::ffi::session::rust_session_wdata_ptr((*sd).fd, 9);
    if !p.is_null() { *p = 0x00u8; }

    // WFIFOSET(fd, encrypt(fd))
    let enc_len = encrypt_fd((*sd).fd);
    crate::ffi::session::rust_session_commit((*sd).fd, enc_len as usize);

    (*sd).LastPingTick = gettick_pc() as c_ulong;
    0
}

// ─── Stat-calculation functions (ported from c_src/pc.c) ──────────────────────

/// `int pc_requestmp(USER *sd)` — checks mail and parcel tables via SQL and sets
/// FLAG_MAIL / FLAG_PARCEL bits on `sd->flags`.
///
/// Faithfully translated from `pc.c:601`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_requestmp(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() { return 0; }

    (*sd).flags = 0;

    // Check for new mail
    let mut escaped_name = [0i8; 255];
    Sql_EscapeString(sql_handle, escaped_name.as_mut_ptr(), (*sd).status.name.as_ptr());
    let query_mail = c"SELECT `MalNew` FROM `Mail` WHERE `MalNew` = 1 AND `MalChaNameDestination` = '%s'";
    if SQL_ERROR == Sql_Query(sql_handle, query_mail.as_ptr(), escaped_name.as_ptr()) {
        Sql_ShowDebug_(sql_handle, c"pc.rs".as_ptr(), line!() as c_ulong);
        return 0;
    }
    if Sql_NumRows(sql_handle) > 0 {
        (*sd).flags |= FLAG_MAIL;
    }
    Sql_FreeResult(sql_handle);

    // Check for pending parcels
    let query_parcel = c"SELECT `ParItmId` FROM `Parcels` WHERE `ParChaIdDestination`='%u'";
    if SQL_ERROR == Sql_Query(sql_handle, query_parcel.as_ptr(), (*sd).status.id) {
        Sql_ShowDebug_(sql_handle, c"pc.rs".as_ptr(), line!() as c_ulong);
        return 0;
    }
    if Sql_NumRows(sql_handle) > 0 {
        (*sd).flags |= FLAG_PARCEL;
    }
    Sql_FreeResult(sql_handle);

    0
}

/// `int pc_checklevel(USER *sd)` — iterates from current level to 99, checks if
/// the player's XP meets the threshold, and fires the "onLevel" script for each
/// level they qualify for.
///
/// Faithfully translated from `pc.c:742`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_checklevel(sd: *mut MapSessionData) -> c_int {
    let path_raw = (*sd).status.class as c_int;
    let path = if path_raw > 5 { classdb_path(path_raw) } else { path_raw };

    for x in (*sd).status.level as c_int..99 {
        let lvlxp = classdb_level(path, x);
        if (*sd).status.exp >= lvlxp {
            sl_doscript_blargs_pc(
                c"onLevel".as_ptr(),
                std::ptr::null(),
                1i32,
                &mut (*sd).bl as *mut BlockList,
            );
        }
    }

    0
}

/// `int pc_givexp(USER *sd, unsigned int exp, unsigned int xprate)` — awards XP to
/// the player, checking stack-on-player and AFK conditions first, then calls
/// `pc_checklevel` and sends status updates.
///
/// Faithfully translated from `pc.c:763`.
/// Note: the `if (exp < 0)` branch in C is dead code because `exp` is `unsigned int`
/// and can never be negative; it is preserved here for faithful translation.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_givexp(
    sd: *mut MapSessionData,
    exp: c_uint,
    xprate: c_uint,
) -> c_int {
    use crate::database::map_db::BLOCK_SIZE;

    let mut xpstring = [0i8; 256];
    let mut stack: c_int = 0;

    let bx = ((*sd).bl.x as c_int) / BLOCK_SIZE as c_int;
    let by = ((*sd).bl.y as c_int) / BLOCK_SIZE as c_int;

    // stack check — count PCs at the exact same tile
    let map_ptr = crate::ffi::map_db::get_map_ptr((*sd).bl.m as u16);
    if !map_ptr.is_null() {
        let bxs = (*map_ptr).bxs as c_int;
        let block_slot = bx + by * bxs;
        // `block` is `*mut *mut BlockList`; use `.add()` to index into the array.
        let mut bl: *mut BlockList = *(*map_ptr).block.add(block_slot as usize);
        while !bl.is_null() && stack < 32768 {
            let tsd = map_id2sd_pc((*bl).id);
            if ((*bl).bl_type as c_int & BL_PC) != 0
                && (*bl).x == (*sd).bl.x
                && (*bl).y == (*sd).bl.y
                && stack < 32768
                && !tsd.is_null()
                && (*tsd).status.gm_level == 0
            {
                stack += 1;
            }
            bl = (*bl).next;
        }
    }

    if stack > 1 {
        let msg = b"You cannot gain experience while on top of other players.\0";
        libc::snprintf(
            xpstring.as_mut_ptr(),
            xpstring.len(),
            msg.as_ptr() as *const c_char,
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
            msg.as_ptr() as *const c_char,
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
    let difxp: c_uint = 4294967295u32.wrapping_sub((*sd).status.exp);

    let (tempxp, defaultxp): (c_uint, c_uint) = if (difxp as i64) > totalxp {
        (
            (*sd).status.exp.wrapping_add(totalxp as c_uint),
            totalxp as c_uint,
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

    rust_pc_checklevel(sd);
    clif_sendminitext(sd, xpstring.as_ptr());
    clif_sendstatus(sd, SFLAG_XPMONEY);
    clif_sendupdatestatus_onequip(sd);

    0
}

/// `int pc_calcstat(USER *sd)` — recalculates all derived stats from base stats and
/// equipped items, applies active magic aether/passive skills, computes TNL percentage,
/// clamps all stats, then sends a full status update to the client.
///
/// Faithfully translated from `pc.c:838`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_calcstat(sd: *mut MapSessionData) -> c_int {
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
    (*sd).armor   = (*sd).status.basearmor  as c_int;
    (*sd).max_hp  = (*sd).status.basehp;
    (*sd).max_mp  = (*sd).status.basemp;
    (*sd).might   = (*sd).status.basemight  as c_int;
    (*sd).grace   = (*sd).status.basegrace  as c_int;
    (*sd).will    = (*sd).status.basewill   as c_int;

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
            (*sd).max_hp  = (*sd).max_hp.wrapping_add(itemdb_vita(id)  as c_uint);
            (*sd).max_mp  = (*sd).max_mp.wrapping_add(itemdb_mana(id)  as c_uint);
            (*sd).might   += itemdb_might(id);
            (*sd).grace   += itemdb_grace(id);
            (*sd).will    += itemdb_will(id);
            (*sd).armor   += itemdb_ac(id);
            (*sd).healing += itemdb_healing(id);
            (*sd).dam     += itemdb_dam(id);
            (*sd).hit     += itemdb_hit(id);
            (*sd).minSdam += itemdb_minSdam(id);
            (*sd).maxSdam += itemdb_maxSdam(id);
            (*sd).minLdam += itemdb_minLdam(id);
            (*sd).maxLdam += itemdb_maxLdam(id);
            (*sd).protection = ((*sd).protection as c_int + itemdb_protection(id)) as c_short;
        }
    }

    // Mount state
    if (*sd).status.state == PC_MOUNTED as i8 {
        if (*sd).status.gm_level == 0 {
            if (*sd).speed < 40 { (*sd).speed = 40; }
        }
        sl_doscript_blargs_pc(
            c"remount".as_ptr(), std::ptr::null(),
            1i32, &mut (*sd).bl as *mut BlockList,
        );
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
                    sl_doscript_blargs_pc(
                        magicdb_yname_pc(p.id as c_int),
                        c"recast".as_ptr(),
                        2i32,
                        &mut (*sd).bl as *mut BlockList,
                        &mut (*tsd).bl as *mut BlockList,
                    );
                } else {
                    // sl_doscript_simple(magicdb_yname(p->id), "recast", &sd->bl)
                    sl_doscript_blargs_pc(
                        magicdb_yname_pc(p.id as c_int),
                        c"recast".as_ptr(),
                        1i32,
                        &mut (*sd).bl as *mut BlockList,
                    );
                }
            }
        }

        // Passive skills
        for x in 0..52usize {
            if (*sd).status.skill[x] > 0 {
                sl_doscript_blargs_pc(
                    magicdb_yname_pc((*sd).status.skill[x] as c_int),
                    c"passive".as_ptr(),
                    1i32,
                    &mut (*sd).bl as *mut BlockList,
                );
            }
        }

        // Re-equip scripts
        for x in 0..14usize {
            if (*sd).status.equip[x].id > 0 {
                sl_doscript_blargs_pc(
                    itemdb_yname((*sd).status.equip[x].id),
                    c"re_equip".as_ptr(),
                    1i32,
                    &mut (*sd).bl as *mut BlockList,
                );
            }
        }
    }

    // Compute TNL percentage for group status window (added 8-5-16)
    if (*sd).status.tnl == 0 {
        let path_raw = (*sd).status.class as c_int;
        let path = if path_raw > 5 { classdb_path(path_raw) } else { path_raw };
        let level = (*sd).status.level as c_int;

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
        let path_raw = (*sd).status.class as c_int;
        let path = if path_raw > 5 { classdb_path(path_raw) } else { path_raw };
        let level = (*sd).status.level as c_int;

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
    let max_health = map_readglobalreg((*sd).bl.m as c_int, c"maxHealth".as_ptr());
    let max_magic  = map_readglobalreg((*sd).bl.m as c_int, c"maxMagic".as_ptr());
    if max_health > 0 { (*sd).max_hp = max_health; }
    if max_magic  > 0 { (*sd).max_mp = max_magic;  }

    // Clamp current HP/MP
    if (*sd).status.hp > (*sd).max_hp { (*sd).status.hp = (*sd).max_hp; }
    if (*sd).status.mp > (*sd).max_mp { (*sd).status.mp = (*sd).max_mp; }

    clif_sendstatus(sd, SFLAG_FULLSTATS | SFLAG_HPMP | SFLAG_XPMONEY);

    0
}

/// `float pc_calcdamage(USER *sd)` — calculates the physical damage the player
/// can deal: base damage from might plus a random roll from equipped weapon range.
///
/// Faithfully translated from `pc.c:1019`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_calcdamage(sd: *mut MapSessionData) -> c_float {
    let mut damage: c_float = 6.0f32 + ((*sd).might as c_float) / 8.0f32;

    if (*sd).minSdam > 0 && (*sd).maxSdam > 0 {
        let mut ran = (*sd).maxSdam - (*sd).minSdam;
        if ran <= 0 { ran = 1; }
        ran = ((randomMT() & 0xFFFFFF) % (ran as c_uint)) as c_int + (*sd).minSdam;
        damage += (ran as c_float) / 2.0f32;
    }

    damage
}

/// `int pc_calcdam(USER *sd)` — minimal damage helper; always returns 1.
///
/// Faithfully translated from `pc.c:1034`. The body is intentionally trivial.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_calcdam(_sd: *mut MapSessionData) -> c_int {
    // C body: `int dam = 1; return dam;` — trivial stub.
    1
}

// ─── Registry functions (ported from c_src/pc.c) ─────────────────────────────
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
/// Returns 0 if not found. Translated from `pc.c:2445`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_readreg(sd: *mut MapSessionData, reg: c_int) -> c_int {
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
/// `reg` array with `libc::realloc`, zeroes the new slot, then sets index and data.
/// Translated from `pc.c:2456`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_setreg(sd: *mut MapSessionData, reg: c_int, val: c_int) -> c_int {
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
/// Translated from `pc.c:2476`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_readregstr(sd: *mut MapSessionData, reg: c_int) -> *mut i8 {
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
/// Translated from `pc.c:2487`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_setregstr(sd: *mut MapSessionData, reg: c_int, str_: *mut i8) -> c_int {
    if sd.is_null() { return 0; }
    // Check string length — must fit in data[256] (including null terminator)
    let len = libc::strlen(str_ as *const libc::c_char);
    if len + 1 >= std::mem::size_of::<[i8; 256]>() {
        libc::printf(c"pc_setregstr: string too long !\n".as_ptr());
        return 0;
    }
    // Search for existing slot
    for i in 0..(*sd).regstr_num as usize {
        if (*(*sd).regstr.add(i)).index == reg {
            libc::strcpy((*(*sd).regstr.add(i)).data.as_mut_ptr() as *mut libc::c_char,
                         str_ as *const libc::c_char);
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
    libc::strcpy((*(*sd).regstr.add(slot)).data.as_mut_ptr() as *mut libc::c_char,
                 str_ as *const libc::c_char);
    0
}

// ── Global string registry (persisted in MmoCharStatus) ──────────────────────

/// `char *pc_readglobalregstring(USER *sd, const char *reg)` — reads a global string variable.
///
/// Scans `sd->status.global_regstring[0..MAX_GLOBALPLAYERREG]` for a case-insensitive match.
/// Returns pointer to `val` if found, or pointer to static empty string.
/// Translated from `pc.c:2512`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_readglobalregstring(
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
/// Translated from `pc.c:2534`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_setglobalregstring(
    sd: *mut MapSessionData, reg: *const i8, val: *const i8,
) -> c_int {
    if sd.is_null() || reg.is_null() { return 0; }
    let sd = &mut *sd;
    // Find existing slot
    let mut exist: c_int = -1;
    for i in 0..MAX_GLOBALPLAYERREG {
        if libc::strcasecmp(sd.status.global_regstring[i].str.as_ptr(), reg) == 0 {
            exist = i as c_int;
            break;
        }
    }
    if exist != -1 {
        let idx = exist as usize;
        if libc::strcasecmp(val, c"".as_ptr()) == 0 {
            // Clear key (marks slot empty)
            libc::strcpy(sd.status.global_regstring[idx].str.as_mut_ptr() as *mut libc::c_char, c"".as_ptr());
        }
        libc::strcpy(sd.status.global_regstring[idx].val.as_mut_ptr() as *mut libc::c_char,
                     val as *const libc::c_char);
        return 0;
    }
    // Find empty slot
    for i in 0..MAX_GLOBALPLAYERREG {
        if libc::strcasecmp(sd.status.global_regstring[i].str.as_ptr(), c"".as_ptr()) == 0 {
            libc::strcpy(sd.status.global_regstring[i].str.as_mut_ptr() as *mut libc::c_char,
                         reg as *const libc::c_char);
            libc::strcpy(sd.status.global_regstring[i].val.as_mut_ptr() as *mut libc::c_char,
                         val as *const libc::c_char);
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
/// Returns the integer value or 0. Translated from `pc.c:2572`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_readglobalreg(
    sd: *mut MapSessionData, reg: *const i8,
) -> c_int {
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
/// Translated from `pc.c:2594`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_setglobalreg(
    sd: *mut MapSessionData, reg: *const i8, val: c_ulong,
) -> c_int {
    if sd.is_null() || reg.is_null() { return 0; }
    let sd = &mut *sd;
    // Find existing slot (scan full array)
    let mut exist: c_int = -1;
    for i in 0..MAX_GLOBALREG {
        if libc::strcasecmp(sd.status.global_reg[i].str.as_ptr(), reg) == 0 {
            exist = i as c_int;
            break;
        }
    }
    if exist != -1 {
        let idx = exist as usize;
        if val == 0 {
            libc::strcpy(sd.status.global_reg[idx].str.as_mut_ptr() as *mut libc::c_char, c"".as_ptr());
        }
        sd.status.global_reg[idx].val = val as i32;
        return 0;
    }
    // Find empty slot (scan full MAX_GLOBALREG array, matching C behavior)
    for i in 0..MAX_GLOBALREG {
        if libc::strcasecmp(sd.status.global_reg[i].str.as_ptr(), c"".as_ptr()) == 0 {
            libc::strcpy(sd.status.global_reg[i].str.as_mut_ptr() as *mut libc::c_char,
                         reg as *const libc::c_char);
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
/// Translated from `pc.c:2632`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_readparam(sd: *mut MapSessionData, type_: c_int) -> c_int {
    if sd.is_null() { return 0; }
    let sd = &*sd;
    match type_ {
        SP_HP  => sd.status.hp as c_int,
        SP_MP  => sd.status.mp as c_int,
        SP_MHP => sd.max_hp as c_int,
        SP_MMP => sd.max_mp as c_int,
        _      => 0,
    }
}

/// `int pc_setparam(USER *sd, int type, int val)` — sets a player parameter by SP_* constant.
///
/// Translated from `pc.c:2654`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_setparam(sd: *mut MapSessionData, type_: c_int, val: c_int) -> c_int {
    if sd.is_null() { return 0; }
    match type_ {
        SP_HP  => (*sd).status.hp  = val as u32,
        SP_MP  => (*sd).status.mp  = val as u32,
        SP_MHP => (*sd).max_hp     = val as c_uint,
        SP_MMP => (*sd).max_mp     = val as c_uint,
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
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_readacctreg(
    sd: *mut MapSessionData, reg: *const i8,
) -> c_int {
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
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_setacctreg(
    sd: *mut MapSessionData, reg: *const i8, val: c_int,
) -> c_int {
    if sd.is_null() || reg.is_null() { return 0; }
    let sd = &mut *sd;
    let mut exist: c_int = -1;
    for i in 0..MAX_GLOBALREG {
        if libc::strcasecmp(sd.status.acctreg[i].str.as_ptr(), reg) == 0 {
            exist = i as c_int;
            break;
        }
    }
    if exist != -1 {
        let idx = exist as usize;
        if val == 0 {
            libc::strcpy(sd.status.acctreg[idx].str.as_mut_ptr() as *mut libc::c_char, c"".as_ptr());
        }
        sd.status.acctreg[idx].val = val;
        return 0;
    }
    for i in 0..MAX_GLOBALREG {
        if libc::strcasecmp(sd.status.acctreg[i].str.as_ptr(), c"".as_ptr()) == 0 {
            libc::strcpy(sd.status.acctreg[i].str.as_mut_ptr() as *mut libc::c_char,
                         reg as *const libc::c_char);
            sd.status.acctreg[i].val = val;
            return 0;
        }
    }
    0
}

/// `int pc_saveacctregistry(USER *sd, int flag)` — stub; declared in pc.h but never defined.
///
/// Returns 0 (no-op). Translating the declaration only.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_saveacctregistry(
    _sd: *mut MapSessionData, _flag: c_int,
) -> c_int {
    0
}

// ── NPC integer registry (persisted in MmoCharStatus.npcintreg) ──────────────

/// `int pc_readnpcintreg(USER *sd, const char *reg)` — reads an NPC-scoped integer variable.
///
/// Scans `sd->status.npcintreg[0..MAX_GLOBALNPCREG]` for a case-insensitive match.
/// Returns the integer value or 0. Translated from `pc.c:2932`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_readnpcintreg(
    sd: *mut MapSessionData, reg: *const i8,
) -> c_int {
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
/// Translated from `pc.c:2894`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_setnpcintreg(
    sd: *mut MapSessionData, reg: *const i8, val: c_int,
) -> c_int {
    if sd.is_null() || reg.is_null() { return 0; }
    let sd = &mut *sd;
    let mut exist: c_int = -1;
    for i in 0..MAX_GLOBALNPCREG {
        if libc::strcasecmp(sd.status.npcintreg[i].str.as_ptr(), reg) == 0 {
            exist = i as c_int;
            break;
        }
    }
    if exist != -1 {
        let idx = exist as usize;
        if val == 0 {
            libc::strcpy(sd.status.npcintreg[idx].str.as_mut_ptr() as *mut libc::c_char, c"".as_ptr());
        }
        sd.status.npcintreg[idx].val = val;
        return 0;
    }
    for i in 0..MAX_GLOBALNPCREG {
        if libc::strcasecmp(sd.status.npcintreg[i].str.as_ptr(), c"".as_ptr()) == 0 {
            libc::strcpy(sd.status.npcintreg[i].str.as_mut_ptr() as *mut libc::c_char,
                         reg as *const libc::c_char);
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
/// Returns the integer value or 0. Translated from `pc.c:2993`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_readquestreg(
    sd: *mut MapSessionData, reg: *const i8,
) -> c_int {
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
/// Translated from `pc.c:2955`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_setquestreg(
    sd: *mut MapSessionData, reg: *const i8, val: c_int,
) -> c_int {
    if sd.is_null() || reg.is_null() { return 0; }
    let sd = &mut *sd;
    let mut exist: c_int = -1;
    for i in 0..MAX_GLOBALQUESTREG {
        if libc::strcasecmp(sd.status.questreg[i].str.as_ptr(), reg) == 0 {
            exist = i as c_int;
            break;
        }
    }
    if exist != -1 {
        let idx = exist as usize;
        if val == 0 {
            libc::strcpy(sd.status.questreg[idx].str.as_mut_ptr() as *mut libc::c_char, c"".as_ptr());
        }
        sd.status.questreg[idx].val = val;
        return 0;
    }
    for i in 0..MAX_GLOBALQUESTREG {
        if libc::strcasecmp(sd.status.questreg[i].str.as_ptr(), c"".as_ptr()) == 0 {
            libc::strcpy(sd.status.questreg[i].str.as_mut_ptr() as *mut libc::c_char,
                         reg as *const libc::c_char);
            sd.status.questreg[i].val = val;
            return 0;
        }
    }
    0
}

// ─── Item management functions (ported from c_src/pc.c) ──────────────────────

use crate::game::scripting::types::floor::FloorItemData;

// ─── pc_isinvenspace ─────────────────────────────────────────────────────────

/// `int pc_isinvenspace(USER* sd, int id, int owner, const char* engrave,
///     unsigned int customLook, unsigned int customLookColor,
///     unsigned int customIcon, unsigned int customIconColor)`
///
/// Returns the first inventory slot that can accept an item with the given
/// attributes, or `sd->status.maxinv` when no slot is available.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_isinvenspace(
    sd:               *mut MapSessionData,
    id:               c_int,
    owner:            c_int,
    engrave:          *const c_char,
    custom_look:      c_uint,
    custom_look_color: c_uint,
    custom_icon:      c_uint,
    custom_icon_color: c_uint,
) -> c_int {
    if sd.is_null() { return 0; }
    let sd = &mut *sd;
    let maxinv = sd.status.maxinv as usize;
    let id_u  = id as u32;
    let own_u = owner as u32;

    if itemdb_maxamount(id_u) > 0 {
        // Count how many of this item the player already owns (inventory + equip).
        let mut maxamount: c_int = 0;
        for i in 0..maxinv {
            let inv = &sd.status.inventory[i];
            if inv.id == id_u && itemdb_maxamount(id_u) > 0
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
            if eq.id == id_u && itemdb_maxamount(id_u) > 0
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
                && inv.amount < itemdb_stackamount(id_u)
                && maxamount < itemdb_maxamount(id_u)
                && inv.owner == own_u
                && libc::strcasecmp(inv.real_name.as_ptr(), engrave) == 0
                && inv.custom_look       == custom_look
                && inv.custom_look_color == custom_look_color
                && inv.custom_icon       == custom_icon
                && inv.custom_icon_color == custom_icon_color
            {
                return i as c_int;
            }
        }

        // Find an empty slot under the global cap.
        for i in 0..maxinv {
            if sd.status.inventory[i].id == 0
                && maxamount < itemdb_maxamount(id_u)
            {
                return i as c_int;
            }
        }

        return sd.status.maxinv as c_int;
    } else {
        // No per-player max — just stack or find empty.
        for i in 0..maxinv {
            let inv = &sd.status.inventory[i];
            if inv.id == id_u
                && inv.amount < itemdb_stackamount(id_u)
                && inv.owner == own_u
                && libc::strcasecmp(inv.real_name.as_ptr(), engrave) == 0
                && inv.custom_look       == custom_look
                && inv.custom_look_color == custom_look_color
                && inv.custom_icon       == custom_icon
                && inv.custom_icon_color == custom_icon_color
            {
                return i as c_int;
            }
        }
        for i in 0..maxinv {
            if sd.status.inventory[i].id == 0 {
                return i as c_int;
            }
        }
        return sd.status.maxinv as c_int;
    }
}

// ─── pc_isinvenitemspace ──────────────────────────────────────────────────────

/// `int pc_isinvenitemspace(USER* sd, int num, int id, int owner, char* engrave)`
///
/// Returns the number of additional units of `id` that can be placed in
/// inventory slot `num`.  Returns 0 when the slot is incompatible.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_isinvenitemspace(
    sd:      *mut MapSessionData,
    num:     c_int,
    id:      c_int,
    owner:   c_int,
    engrave: *mut c_char,
) -> c_int {
    if sd.is_null() { return 0; }
    let sd = &mut *sd;
    let id_u  = id as u32;
    let own_u = owner as u32;
    let num   = num as usize;

    if itemdb_maxamount(id_u) > 0 {
        let mut maxamount: c_int = 0;
        let maxinv = sd.status.maxinv as usize;
        for i in 0..maxinv {
            if sd.status.inventory[i].id == id_u && itemdb_maxamount(id_u) > 0 {
                maxamount += sd.status.inventory[i].amount;
            }
        }
        for i in 0..14usize {
            if sd.status.equip[i].id == id_u && itemdb_maxamount(id_u) > 0 {
                // C checks takeoffid: skip the slot being unequipped
                if sd.takeoffid == -1
                    || sd.status.equip[sd.takeoffid as usize].id != id_u
                {
                    maxamount += 1;
                }
            }
        }

        if sd.status.inventory[num].id == 0
            && itemdb_maxamount(id_u) - maxamount >= itemdb_stackamount(id_u)
        {
            return itemdb_stackamount(id_u);
        } else if sd.status.inventory[num].id != id_u
            || sd.status.inventory[num].owner != own_u
            || libc::strcasecmp(sd.status.inventory[num].real_name.as_ptr(), engrave) != 0
        {
            return 0;
        } else {
            return itemdb_maxamount(id_u) - maxamount;
        }
    } else {
        if sd.status.inventory[num].id == 0 {
            return itemdb_stackamount(id_u);
        } else if sd.status.inventory[num].id != id_u
            || sd.status.inventory[num].owner != own_u
            || libc::strcasecmp(sd.status.inventory[num].real_name.as_ptr(), engrave) != 0
        {
            return 0;
        } else {
            return itemdb_stackamount(id_u) - sd.status.inventory[num].amount;
        }
    }
}

// ─── pc_dropitemfull (helper) ─────────────────────────────────────────────────

/// Allocate a `FloorItemData` from `fl2`, attempt to stack it on an existing
/// floor item at the player's cell, and if no match exists add it to the map.
/// Mirrors `int pc_dropitemfull(USER* sd, struct item* fl2)`.
#[cfg(not(test))]
unsafe fn pc_dropitemfull_inner(sd: *mut MapSessionData, fl2: *const Item) -> c_int {
    use std::mem;

    let fl = libc::calloc(1, mem::size_of::<FloorItemData>()) as *mut FloorItemData;
    if fl.is_null() { return 0; }

    (*fl).bl.m = (*sd).bl.m;
    (*fl).bl.x = (*sd).bl.x;
    (*fl).bl.y = (*sd).bl.y;
    // Copy the item into fl->data (BoundItem and Item share the same layout)
    libc::memcpy(
        &mut (*fl).data as *mut _ as *mut libc::c_void,
        fl2 as *const libc::c_void,
        mem::size_of::<Item>(),
    );
    libc::memset(
        (*fl).looters.as_mut_ptr() as *mut libc::c_void,
        0,
        mem::size_of::<u32>() * MAX_GROUP_MEMBERS,
    );

    let mut def = [0i32; 2];

    // Only attempt stacking if item is at full durability.
    if (*fl).data.dura == itemdb_dura((*fl).data.id as c_uint) {
        map_foreachincell_pc(
            rust_pc_addtocurrent2,
            (*fl).bl.m as c_int,
            (*fl).bl.x as c_int,
            (*fl).bl.y as c_int,
            BL_ITEM,
            def.as_mut_ptr(),
            (*fl).data.id as c_int,
            fl,
        );
    }

    if def[0] == 0 {
        map_additem_pc(&mut (*fl).bl);
        map_foreachinarea_pc(
            clif_object_look_sub2,
            (*sd).bl.m as c_int,
            (*sd).bl.x as c_int,
            (*sd).bl.y as c_int,
            AREA,
            BL_PC,
            LOOK_SEND,
            &mut (*fl).bl as *mut BlockList,
        );
    } else {
        libc::free(fl as *mut libc::c_void);
    }
    0
}

/// `int pc_dropitemfull(USER* sd, struct item* fl2)` — public C-callable export.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_dropitemfull(
    sd:  *mut MapSessionData,
    fl2: *mut Item,
) -> c_int {
    if sd.is_null() || fl2.is_null() { return 0; }
    pc_dropitemfull_inner(sd, fl2)
}

// ─── pc_addtocurrent2 (va_list callback) ─────────────────────────────────────

/// va_list callback: attempt to stack `fl2` onto the existing floor item `bl`.
/// Arguments (via va_list): `int* def`, `int id` (unused), `FLOORITEM* fl2`.
/// Sets `def[0] = 1` on a successful merge.
///
/// Mirrors `int pc_addtocurrent2(struct block_list* bl, va_list ap)`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_addtocurrent2(
    bl: *mut BlockList,
    mut ap: ...
) -> c_int {
    if bl.is_null() { return 0; }
    let fl = bl as *mut FloorItemData;

    let def = ap.arg::<*mut c_int>();
    let _id = ap.arg::<c_int>(); // id parameter — not used in comparison
    let fl2 = ap.arg::<*mut FloorItemData>();

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

// ─── pc_addtocurrent (va_list callback) ──────────────────────────────────────

/// va_list callback: stack inventory slot `id` amount onto existing floor item `fl`.
/// Arguments: `int* def`, `int id`, `int type`, `USER* sd`.
/// Sets `def[0] = fl->bl.id` on successful merge.
///
/// Mirrors `int pc_addtocurrent(struct block_list* bl, va_list ap)`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_addtocurrent(
    bl: *mut BlockList,
    mut ap: ...
) -> c_int {
    if bl.is_null() { return 0; }
    let fl = bl as *mut FloorItemData;

    let def  = ap.arg::<*mut c_int>();
    let id   = ap.arg::<c_int>() as usize;   // inventory slot index
    let type_ = ap.arg::<c_int>();            // 0 = drop 1, nonzero = drop all
    let sd   = ap.arg::<*mut MapSessionData>();

    if def.is_null() || sd.is_null() { return 0; }
    if *def != 0 { return 0; }

    // Only stack items at full durability.
    if (*fl).data.dura < itemdb_dura((*fl).data.id as c_uint) { return 0; }
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
        (*fl).lastamount = (*fl).data.amount as c_uint;
        if type_ != 0 {
            (*fl).data.amount += inv.amount;
        } else {
            (*fl).data.amount += 1;
        }
        sl_doscript_blargs_pc(
            c"characterLog".as_ptr(), c"dropWrite".as_ptr(),
            2i32, &mut (*sd).bl as *mut BlockList, &mut (*fl).bl as *mut BlockList,
        );
        *def = (*fl).bl.id as c_int;
    }
    0
}

// ─── pc_npc_drop (va_list callback) ──────────────────────────────────────────

/// va_list callback used by `pc_dropitemmap` to notify floor-NPC scripts.
/// Arguments: `FLOORITEM* fl`, `USER* sd`.
/// Currently a no-op (the C version also does nothing beyond null checks).
///
/// Mirrors `int pc_npc_drop(struct block_list* bl, va_list ap)`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_npc_drop(
    bl: *mut BlockList,
    mut ap: ...
) -> c_int {
    use crate::game::npc::NpcData;
    if bl.is_null() { return 0; }
    let nd = bl as *mut NpcData;
    let _fl = ap.arg::<*mut FloorItemData>();
    let _sd = ap.arg::<*mut MapSessionData>();

    if (*nd).bl.subtype != FLOOR { return 0; }
    // Currently no-op — kept for future NPC floor-item interaction.
    0
}

// ─── pc_additem ───────────────────────────────────────────────────────────────

/// `int pc_additem(USER* sd, struct item* fl)` — add item to inventory with logging.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_additem(
    sd: *mut MapSessionData,
    fl: *mut Item,
) -> c_int {
    if sd.is_null() || fl.is_null() { return 0; }

    // Gold dupe guard: id==0 with amount is bogus.
    if (*fl).id == 0 && (*fl).amount != 0 { return 0; }

    let id_u = (*fl).id;
    let maxinv = (*sd).status.maxinv as c_int;

    let mut num = rust_pc_isinvenspace(
        sd, id_u as c_int, (*fl).owner as c_int,
        (*fl).real_name.as_ptr(),
        (*fl).custom_look, (*fl).custom_look_color,
        (*fl).custom_icon, (*fl).custom_icon_color,
    );

    if num >= maxinv {
        if itemdb_maxamount(id_u) > 0 {
            let mut errbuf = [0i8; 64];
            libc::snprintf(
                errbuf.as_mut_ptr(), 64,
                c"(%s). You can't have more than (%i).".as_ptr(),
                itemdb_name(id_u), itemdb_maxamount(id_u),
            );
            pc_dropitemfull_inner(sd, fl);
            clif_sendminitext(sd, errbuf.as_ptr());
        } else {
            clif_sendminitext(sd, map_msg[MAP_ERRITMFULL].message.as_ptr());
            pc_dropitemfull_inner(sd, fl);
        }
        return 0;
    }

    loop {
        let i = rust_pc_isinvenitemspace(
            sd, num, id_u as c_int, (*fl).owner as c_int, (*fl).real_name.as_mut_ptr(),
        );

        // Escape a C string for logging (result discarded — logging is commented out in C).
        let mut _escape = [0i8; 255];
        Sql_EscapeString(sql_handle, _escape.as_mut_ptr(), (*fl).real_name.as_ptr());

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
        num = rust_pc_isinvenspace(
            sd, id_u as c_int, (*fl).owner as c_int,
            (*fl).real_name.as_ptr(),
            (*fl).custom_look, (*fl).custom_look_color,
            (*fl).custom_icon, (*fl).custom_icon_color,
        );

        if !((*fl).amount != 0 && num < maxinv) { break; }
    }

    if num >= maxinv && (*fl).amount != 0 {
        if itemdb_maxamount(id_u) > 0 {
            let mut errbuf = [0i8; 64];
            libc::snprintf(
                errbuf.as_mut_ptr(), 64,
                c"(%s). You can't have more than (%i).".as_ptr(),
                itemdb_name(id_u), itemdb_maxamount(id_u),
            );
            pc_dropitemfull_inner(sd, fl);
            clif_sendminitext(sd, errbuf.as_ptr());
        } else {
            pc_dropitemfull_inner(sd, fl);
            clif_sendminitext(sd, map_msg[MAP_ERRITMFULL].message.as_ptr());
        }
    }
    0
}

// ─── pc_additemnolog ──────────────────────────────────────────────────────────

/// `int pc_additemnolog(USER* sd, struct item* fl)` — add item without SQL logging.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_additemnolog(
    sd: *mut MapSessionData,
    fl: *mut Item,
) -> c_int {
    if sd.is_null() || fl.is_null() { return 0; }

    if (*fl).id == 0 && (*fl).amount != 0 { return 0; }

    let id_u   = (*fl).id;
    let maxinv = (*sd).status.maxinv as c_int;

    let mut num = rust_pc_isinvenspace(
        sd, id_u as c_int, (*fl).owner as c_int,
        (*fl).real_name.as_ptr(),
        (*fl).custom_look, (*fl).custom_look_color,
        (*fl).custom_icon, (*fl).custom_icon_color,
    );

    if num >= maxinv {
        if itemdb_maxamount(id_u) > 0 {
            let mut errbuf = [0i8; 64];
            libc::snprintf(
                errbuf.as_mut_ptr(), 64,
                c"(%s). You can't have more than (%i).".as_ptr(),
                itemdb_name(id_u), itemdb_maxamount(id_u),
            );
            pc_dropitemfull_inner(sd, fl);
            clif_sendminitext(sd, errbuf.as_ptr());
        } else {
            clif_sendminitext(sd, map_msg[MAP_ERRITMFULL].message.as_ptr());
            pc_dropitemfull_inner(sd, fl);
        }
        return 0;
    }

    loop {
        let i = rust_pc_isinvenitemspace(
            sd, num, id_u as c_int, (*fl).owner as c_int, (*fl).real_name.as_mut_ptr(),
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
        num = rust_pc_isinvenspace(
            sd, id_u as c_int, (*fl).owner as c_int,
            (*fl).real_name.as_ptr(),
            (*fl).custom_look, (*fl).custom_look_color,
            (*fl).custom_icon, (*fl).custom_icon_color,
        );

        if !((*fl).amount != 0 && num < maxinv) { break; }
    }

    if num >= maxinv && (*fl).amount != 0 {
        if itemdb_maxamount(id_u) > 0 {
            let mut errbuf = [0i8; 64];
            libc::snprintf(
                errbuf.as_mut_ptr(), 64,
                c"(%s). You can't have more than (%i).".as_ptr(),
                itemdb_name(id_u), itemdb_maxamount(id_u),
            );
            pc_dropitemfull_inner(sd, fl);
            clif_sendminitext(sd, errbuf.as_ptr());
        } else {
            pc_dropitemfull_inner(sd, fl);
            clif_sendminitext(sd, map_msg[MAP_ERRITMFULL].message.as_ptr());
        }
    }
    0
}

// ─── pc_delitem ───────────────────────────────────────────────────────────────

/// `int pc_delitem(USER* sd, int id, int amount, int type)` — remove `amount`
/// units from inventory slot `id`.  If the slot becomes empty it is zeroed and
/// the client is notified with a delete-item packet; otherwise the client
/// receives an updated add-item count and a mini-text with the item name.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_delitem(
    sd:     *mut MapSessionData,
    id:     c_int,
    amount: c_int,
    type_:  c_int,
) -> c_int {
    if sd.is_null() { return 0; }
    let maxinv = (*sd).status.maxinv as c_int;
    if id < 0 || id >= maxinv { return 0; }
    let inv = &mut (*sd).status.inventory[id as usize];
    if inv.id == 0 { return 0; }

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
            itemdb_name(item_id),
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
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_dropitemmap(
    sd:    *mut MapSessionData,
    id:    c_int,
    type_: c_int,
) -> c_int {
    if sd.is_null() { return 0; }
    let id_u = id as usize;

    if id > (*sd).status.maxinv as c_int { return 0; }
    if (*sd).status.inventory[id_u].id == 0 { return 0; }

    if (*sd).status.inventory[id_u].amount <= 0 {
        clif_senddelitem(sd, id, 1);
        return 0;
    }

    let mut def = [0i32; 2];

    let fl = libc::calloc(1, std::mem::size_of::<FloorItemData>()) as *mut FloorItemData;
    if fl.is_null() { return 0; }

    (*fl).bl.m = (*sd).bl.m;
    (*fl).bl.x = (*sd).bl.x;
    (*fl).bl.y = (*sd).bl.y;
    libc::memcpy(
        &mut (*fl).data as *mut _ as *mut libc::c_void,
        &(*sd).status.inventory[id_u] as *const Item as *const libc::c_void,
        std::mem::size_of::<Item>(),
    );
    libc::memset(
        (*fl).looters.as_mut_ptr() as *mut libc::c_void,
        0,
        std::mem::size_of::<u32>() * MAX_GROUP_MEMBERS,
    );

    // Attempt to stack onto an existing floor item at full durability.
    if (*fl).data.dura == itemdb_dura((*fl).data.id as c_uint) {
        map_foreachincell_pc(
            rust_pc_addtocurrent,
            (*fl).bl.m as c_int,
            (*fl).bl.x as c_int,
            (*fl).bl.y as c_int,
            BL_ITEM,
            def.as_mut_ptr(),
            id,
            type_,
            sd,
        );
    }

    (*sd).status.inventory[id_u].amount -= 1;

    if type_ != 0 || (*sd).status.inventory[id_u].amount == 0 {
        // Full drop: clear the slot.
        let mut _escape = [0i8; 255];
        Sql_EscapeString(sql_handle, _escape.as_mut_ptr(), (*fl).data.real_name.as_ptr());
        libc::memset(
            &mut (*sd).status.inventory[id_u] as *mut Item as *mut libc::c_void,
            0,
            std::mem::size_of::<Item>(),
        );
        clif_senddelitem(sd, id, 1);
    } else {
        // Partial drop: update count.
        let mut _escape = [0i8; 255];
        Sql_EscapeString(sql_handle, _escape.as_mut_ptr(), (*fl).data.real_name.as_ptr());
        (*fl).data.amount = 1;
        clif_sendadditem(sd, id);
    }

    map_foreachincell_pc(
        rust_pc_npc_drop,
        (*fl).bl.m as c_int,
        (*fl).bl.x as c_int,
        (*fl).bl.y as c_int,
        BL_NPC,
        fl,
        sd,
    );

    if def[0] == 0 {
        map_additem_pc(&mut (*fl).bl);
        sl_doscript_blargs_pc(
            c"characterLog".as_ptr(), c"dropWrite".as_ptr(),
            2i32, &mut (*sd).bl as *mut BlockList, &mut (*fl).bl as *mut BlockList,
        );
        map_foreachinarea_pc(
            clif_object_look_sub2,
            (*sd).bl.m as c_int,
            (*sd).bl.x as c_int,
            (*sd).bl.y as c_int,
            AREA,
            BL_PC,
            LOOK_SEND,
            &mut (*fl).bl as *mut BlockList,
        );
    } else {
        libc::free(fl as *mut libc::c_void);
    }
    0
}

// ─── pc_changeitem ────────────────────────────────────────────────────────────

/// `int pc_changeitem(USER* sd, int id1, int id2)` — swap inventory slots `id1`
/// and `id2`, sending the appropriate add/delete packets to the client.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_changeitem(
    sd:  *mut MapSessionData,
    id1: c_int,
    id2: c_int,
) -> c_int {
    if sd.is_null() { return 0; }
    let maxinv = (*sd).status.maxinv as c_int;
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
/// Delegates equip logic to `rust_pc_equipitem`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_useitem(
    sd: *mut MapSessionData,
    id: c_int,
) -> c_int {
    if sd.is_null() { return 0; }
    let maxinv = (*sd).status.maxinv as c_int;
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
    let equip_type = itemdb_type((*sd).status.inventory[id_u].id) - 3;
    if equip_type >= 0 {
        if (*sd).status.equip[equip_type as usize].id > 0 && (*sd).status.gm_level == 0 {
            if itemdb_unequip((*sd).status.equip[equip_type as usize].id) == 1 {
                clif_sendminitext(sd, c"You are unable to unequip that.".as_ptr());
                return 0;
            }
        }
    }

    // Class / path restriction check.
    if itemdb_class((*sd).status.inventory[id_u].id) != 0 {
        if classdb_path((*sd).status.class as c_int) == 5 {
            // GM — no restriction
        } else if itemdb_class((*sd).status.inventory[id_u].id) < 6 {
            if classdb_path((*sd).status.class as c_int)
                != itemdb_class((*sd).status.inventory[id_u].id)
            {
                clif_sendminitext(sd, map_msg[MAP_ERRITMPATH].message.as_ptr());
                return 0;
            }
        } else {
            if (*sd).status.class as c_int != itemdb_class((*sd).status.inventory[id_u].id) {
                clif_sendminitext(sd, map_msg[MAP_ERRITMPATH].message.as_ptr());
                return 0;
            }
        }
        if ((*sd).status.mark as c_int) < itemdb_rank((*sd).status.inventory[id_u].id) {
            clif_sendminitext(sd, map_msg[MAP_ERRITMMARK].message.as_ptr());
            return 0;
        }
    }

    // Ghost / mounted state restrictions.
    if (*sd).status.state == PC_DIE as i8 {
        clif_sendminitext(sd, map_msg[MAP_ERRGHOST].message.as_ptr());
        return 0;
    }
    if (*sd).status.state == PC_MOUNTED as i8 {
        clif_sendminitext(sd, map_msg[MAP_ERRMOUNT].message.as_ptr());
        return 0;
    }

    // Set a timed expiry if the item has one.
    if itemdb_time((*sd).status.inventory[id_u].id) != 0
        && (*sd).status.inventory[id_u].time == 0
    {
        (*sd).status.inventory[id_u].time =
            (libc::time(std::ptr::null_mut()) as u32)
                .wrapping_add(itemdb_time((*sd).status.inventory[id_u].id) as u32);
    }

    let map_ptr = crate::ffi::map_db::get_map_ptr((*sd).bl.m as u16);

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

    let item_type = itemdb_type((*sd).status.inventory[id_u].id);

    match item_type {
        t if t == ITM_EAT => {
            if !can_eat!() {
                clif_sendminitext(sd, c"You cannot eat this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd as *mut c_void);
            sl_doscript_blargs_pc(
                itemdb_yname((*sd).status.inventory[id_u].id),
                c"use".as_ptr(), 1i32, &mut (*sd).bl as *mut BlockList,
            );
            sl_doscript_blargs_pc(c"use".as_ptr(), std::ptr::null(), 1i32,
                &mut (*sd).bl as *mut BlockList);
            rust_pc_delitem(sd, id, 1, 2);
        }
        t if t == ITM_USE => {
            if !can_use!() {
                clif_sendminitext(sd, c"You cannot use this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd as *mut c_void);
            sl_doscript_blargs_pc(
                itemdb_yname((*sd).status.inventory[id_u].id),
                c"use".as_ptr(), 1i32, &mut (*sd).bl as *mut BlockList,
            );
            sl_doscript_blargs_pc(c"use".as_ptr(), std::ptr::null(), 1i32,
                &mut (*sd).bl as *mut BlockList);
            rust_pc_delitem(sd, id, 1, 6);
        }
        t if t == ITM_USESPC => {
            if !can_use!() {
                clif_sendminitext(sd, c"You cannot use this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd as *mut c_void);
            sl_doscript_blargs_pc(
                itemdb_yname((*sd).status.inventory[id_u].id),
                c"use".as_ptr(), 1i32, &mut (*sd).bl as *mut BlockList,
            );
            sl_doscript_blargs_pc(c"use".as_ptr(), std::ptr::null(), 1i32,
                &mut (*sd).bl as *mut BlockList);
            // No auto-delete for USESPC — script decides.
        }
        t if t == ITM_BAG => {
            if !can_use!() {
                clif_sendminitext(sd, c"You cannot use this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd as *mut c_void);
            sl_doscript_blargs_pc(
                itemdb_yname((*sd).status.inventory[id_u].id),
                c"use".as_ptr(), 1i32, &mut (*sd).bl as *mut BlockList,
            );
            sl_doscript_blargs_pc(c"use".as_ptr(), std::ptr::null(), 1i32,
                &mut (*sd).bl as *mut BlockList);
        }
        t if t == ITM_MAP => {
            if !can_use!() {
                clif_sendminitext(sd, c"You cannot use this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd as *mut c_void);
            sl_doscript_blargs_pc(
                itemdb_yname((*sd).status.inventory[id_u].id),
                c"use".as_ptr(), 1i32, &mut (*sd).bl as *mut BlockList,
            );
            sl_doscript_blargs_pc(c"maps".as_ptr(), c"use".as_ptr(), 1i32,
                &mut (*sd).bl as *mut BlockList);
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
            sl_async_freeco(sd as *mut c_void);
            sl_doscript_blargs_pc(
                itemdb_yname((*sd).status.inventory[id_u].id),
                c"use".as_ptr(), 1i32, &mut (*sd).bl as *mut BlockList,
            );
            sl_doscript_blargs_pc(c"onMountItem".as_ptr(), std::ptr::null(), 1i32,
                &mut (*sd).bl as *mut BlockList);
        }
        t if t == ITM_FACE => {
            if !can_use!() {
                clif_sendminitext(sd, c"You cannot use this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd as *mut c_void);
            sl_doscript_blargs_pc(
                itemdb_yname((*sd).status.inventory[id_u].id),
                c"use".as_ptr(), 1i32, &mut (*sd).bl as *mut BlockList,
            );
            sl_doscript_blargs_pc(c"useFace".as_ptr(), std::ptr::null(), 1i32,
                &mut (*sd).bl as *mut BlockList);
        }
        t if t == ITM_SET => {
            if !can_use!() {
                clif_sendminitext(sd, c"You cannot use this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd as *mut c_void);
            sl_doscript_blargs_pc(
                itemdb_yname((*sd).status.inventory[id_u].id),
                c"use".as_ptr(), 1i32, &mut (*sd).bl as *mut BlockList,
            );
            sl_doscript_blargs_pc(c"useSetItem".as_ptr(), std::ptr::null(), 1i32,
                &mut (*sd).bl as *mut BlockList);
        }
        t if t == ITM_SKIN => {
            if !can_use!() {
                clif_sendminitext(sd, c"You cannot use this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd as *mut c_void);
            sl_doscript_blargs_pc(
                itemdb_yname((*sd).status.inventory[id_u].id),
                c"use".as_ptr(), 1i32, &mut (*sd).bl as *mut BlockList,
            );
            sl_doscript_blargs_pc(c"useSkinItem".as_ptr(), std::ptr::null(), 1i32,
                &mut (*sd).bl as *mut BlockList);
        }
        t if t == ITM_HAIR_DYE => {
            if !can_use!() {
                clif_sendminitext(sd, c"You cannot use this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd as *mut c_void);
            sl_doscript_blargs_pc(
                itemdb_yname((*sd).status.inventory[id_u].id),
                c"use".as_ptr(), 1i32, &mut (*sd).bl as *mut BlockList,
            );
            sl_doscript_blargs_pc(c"useHairDye".as_ptr(), std::ptr::null(), 1i32,
                &mut (*sd).bl as *mut BlockList);
        }
        t if t == ITM_FACEACCTWO => {
            if !can_use!() {
                clif_sendminitext(sd, c"You cannot use this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd as *mut c_void);
            sl_doscript_blargs_pc(
                itemdb_yname((*sd).status.inventory[id_u].id),
                c"use".as_ptr(), 1i32, &mut (*sd).bl as *mut BlockList,
            );
            sl_doscript_blargs_pc(c"useBeardItem".as_ptr(), std::ptr::null(), 1i32,
                &mut (*sd).bl as *mut BlockList);
        }
        t if t == ITM_SMOKE => {
            if !can_smoke!() {
                clif_sendminitext(sd, c"You cannot smoke this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd as *mut c_void);
            sl_doscript_blargs_pc(
                itemdb_yname((*sd).status.inventory[id_u].id),
                c"use".as_ptr(), 1i32, &mut (*sd).bl as *mut BlockList,
            );
            sl_doscript_blargs_pc(c"use".as_ptr(), std::ptr::null(), 1i32,
                &mut (*sd).bl as *mut BlockList);
            (*sd).status.inventory[id_u].dura -= 1;
            if (*sd).status.inventory[id_u].dura == 0 {
                rust_pc_delitem(sd, id, 1, 3);
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
            rust_pc_equipitem(sd, id);
        }
        t if t == ITM_ETC => {
            if !can_use!() {
                clif_sendminitext(sd, c"You cannot use this here.".as_ptr());
                return 0;
            }
            (*sd).invslot = id as u8;
            sl_async_freeco(sd as *mut c_void);
            sl_doscript_blargs_pc(
                itemdb_yname((*sd).status.inventory[id_u].id),
                c"use".as_ptr(), 1i32, &mut (*sd).bl as *mut BlockList,
            );
            sl_doscript_blargs_pc(c"use".as_ptr(), std::ptr::null(), 1i32,
                &mut (*sd).bl as *mut BlockList);
        }
        _ => {}
    }

    0
}

// ─── pc_runfloor_sub ──────────────────────────────────────────────────────────

/// `int pc_runfloor_sub(USER* sd)` — check if the player is standing on a FLOOR
/// or sub-2 NPC cell, and if so trigger its script.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_runfloor_sub(sd: *mut MapSessionData) -> c_int {
    use crate::game::npc::NpcData;
    if sd.is_null() { return 0; }

    let bl = map_firstincell((*sd).bl.m as c_int, (*sd).bl.x as c_int, (*sd).bl.y as c_int, BL_NPC);
    if bl.is_null() { return 0; }
    let nd = bl as *mut NpcData;

    if (*nd).bl.subtype != FLOOR && (*nd).bl.subtype != 2 { return 0; }

    if (*nd).bl.subtype == 2 {
        sl_async_freeco(sd as *mut c_void);
        sl_doscript_blargs_pc(
            (*nd).name.as_ptr(), c"click".as_ptr(),
            2i32, &mut (*sd).bl as *mut BlockList, &mut (*nd).bl as *mut BlockList,
        );
    }
    0
}

// ─── pc_getitemmap, pc_getitemsaround, pc_handle_item, pc_handle_item_sub ─────
//
// These four functions are declared in `c_src/pc.h` but have NO implementation
// anywhere in the C source tree (verified by grepping all *.c files).
// They are declared here as stubs returning 0 so that the linker is satisfied
// if any translation unit references them.

/// `int pc_getitemmap(USER* sd, int id)` — declared in pc.h, not implemented in C.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_getitemmap(
    _sd: *mut MapSessionData,
    _id: c_int,
) -> c_int {
    0
}

/// `int pc_getitemsaround(USER* sd)` — declared in pc.h, not implemented in C.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_getitemsaround(_sd: *mut MapSessionData) -> c_int {
    0
}

/// `int pc_handle_item(int a, int b)` — declared in pc.h, not implemented in C.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_handle_item(_a: c_int, _b: c_int) -> c_int {
    0
}

/// `int pc_handle_item_sub(struct block_list* bl, va_list ap)` — declared in pc.h,
/// not implemented in C.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_handle_item_sub(
    _bl: *mut BlockList,
    mut _ap: ...
) -> c_int {
    0
}

// ─── Equipment functions ──────────────────────────────────────────────────────
//
// Ported from `c_src/pc.c`.  All functions are gated on `#[cfg(not(test))]`
// because they call C FFI.  Each exported function is `#[no_mangle]` so C
// translation units can call it via the `rust_pc_*` symbol declared in
// `c_src/pc.h`.

/// `int pc_isequip(USER* sd, int type)` — returns the item id in equip slot
/// `type`, or 0 if the slot is empty.
///
/// Bounds-checked: returns 0 for out-of-range `type`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_isequip(
    sd:   *mut MapSessionData,
    type_: c_int,
) -> c_int {
    if sd.is_null() { return 0; }
    if type_ < 0 || type_ >= 15 { return 0; }
    (*sd).status.equip[type_ as usize].id as c_int
}

/// `int pc_loaditem(USER* sd)` — send all non-empty inventory slots to the
/// client via `clif_sendadditem`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_loaditem(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() { return 0; }
    let maxinv = (*sd).status.maxinv as usize;
    for i in 0..maxinv {
        if (*sd).status.inventory[i].id != 0 {
            clif_sendadditem(sd, i as c_int);
        }
    }
    0
}

/// `int pc_loadequip(USER* sd)` — send all non-empty equip slots to the client
/// via `clif_sendequip`.
///
/// Only slots 0..14 are active equipment positions.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_loadequip(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() { return 0; }
    for i in 0..14 {
        if (*sd).status.equip[i].id > 0 {
            clif_sendequip(sd, i as c_int);
        }
    }
    0
}

/// `int pc_loadequiprealname(USER* sd)` — stub; C implementation returns 0.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_loadequiprealname(_sd: *mut MapSessionData) -> c_int {
    0
}

/// `int pc_loaditemrealname(USER* sd)` — stub; C implementation returns 0.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_loaditemrealname(_sd: *mut MapSessionData) -> c_int {
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
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_canequipitem(
    sd: *mut MapSessionData,
    id: c_int,
) -> c_int {
    if sd.is_null() { return 0; }
    let maxinv = (*sd).status.maxinv as c_int;
    if id < 0 || id >= maxinv { return 0; }

    let itemid = (*sd).status.inventory[id as usize].id;

    // Two-handed weapon conflicts:
    // If a weapon with look 10000..29999 is equipped, a shield cannot be added.
    if rust_pc_isequip(sd, EQ_WEAP) != 0 {
        let weap_look = itemdb_look((*sd).status.equip[EQ_WEAP as usize].id);
        if itemdb_type(itemid) == ITM_SHIELD
            && weap_look >= 10000
            && weap_look <= 29999
        {
            return MAP_ERRITM2H as c_int;
        }
    }

    // If a shield is equipped, a two-handed weapon cannot be added.
    if rust_pc_isequip(sd, EQ_SHIELD) != 0 {
        let itm_look = itemdb_look(itemid);
        if itemdb_type(itemid) == ITM_WEAP
            && itm_look >= 10000
            && itm_look <= 29999
        {
            return MAP_ERRITM2H as c_int;
        }
    }

    if ((*sd).status.level as c_int) < itemdb_level(itemid) {
        return MAP_ERRITMLEVEL as c_int;
    }
    if (*sd).might < itemdb_mightreq(itemid) {
        return MAP_ERRITMMIGHT as c_int;
    }
    let item_sex = itemdb_sex(itemid);
    if ((*sd).status.sex as c_int) != item_sex && item_sex != 2 {
        return MAP_ERRITMSEX as c_int;
    }

    0
}

/// `int pc_canequipstats(USER* sd, int id)` — check whether an item with item-id
/// `id` can be equipped given the player's current HP/MP totals.
///
/// Returns 1 if allowed, 0 if the vita/mana penalty would reduce hp/mp below 0.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_canequipstats(
    sd: *mut MapSessionData,
    id: c_uint,
) -> c_int {
    if sd.is_null() { return 0; }

    let vita = itemdb_vita(id);
    if vita < 0 && vita.unsigned_abs() > (*sd).max_hp {
        return 0;
    }
    let mana = itemdb_mana(id);
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
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_equipitem(
    sd: *mut MapSessionData,
    id: c_int,
) -> c_int {
    if sd.is_null() { return 0; }
    let maxinv = (*sd).status.maxinv as c_int;
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
    let ret = rust_pc_canequipitem(sd, id);
    if ret != 0 {
        clif_sendminitext(sd, map_msg[ret as usize].message.as_ptr());
        return 0;
    }

    // Determine equip slot from item type.  Equip types start at ITM_WEAP=3,
    // so slot = type - 3.  Valid range: 0..=14.
    let slot = itemdb_type((*sd).status.inventory[id_u].id) - 3;
    if slot < 0 || slot > 14 {
        // Not an equip item.
        return 0;
    }

    // Stat check.
    if rust_pc_canequipstats(sd, (*sd).status.inventory[id_u].id) == 0 {
        clif_sendminitext(sd, c"Your stats are too low to equip that.".as_ptr());
        return 0;
    }

    // Store the item id and inventory slot so pc_equipscript can finish the job.
    (*sd).equipid = (*sd).status.inventory[id_u].id;
    (*sd).invslot = id as u8;

    // Fire the Lua equip hooks.
    sl_doscript_blargs_pc(
        c"onEquip".as_ptr(), std::ptr::null(),
        1i32, &mut (*sd).bl as *mut BlockList,
    );
    sl_doscript_blargs_pc(
        itemdb_yname((*sd).status.inventory[id_u].id),
        c"onEquip".as_ptr(),
        1i32, &mut (*sd).bl as *mut BlockList,
    );

    0
}

/// `int pc_equipscript(USER* sd)` — second phase of the equip sequence, called
/// from within the Lua `onEquip` hook.
///
/// Resolves the target slot (handling left/right ring swaps), removes any
/// previously-equipped item in that slot via an `onUnequip` hook, copies the
/// inventory item into the equip array, removes it from the inventory, and then
/// updates client state.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_equipscript(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() { return 0; }

    let mut ret = itemdb_type((*sd).equipid) - 3;

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
        (*sd).target   = (*sd).bl.id as c_int;
        (*sd).attacker = (*sd).bl.id;
        (*sd).takeoffid = ret as i8;
        sl_doscript_blargs_pc(
            c"onUnequip".as_ptr(), std::ptr::null(),
            1i32, &mut (*sd).bl as *mut BlockList,
        );
        sl_doscript_blargs_pc(
            itemdb_yname((*sd).equipid),
            c"equip".as_ptr(),
            1i32, &mut (*sd).bl as *mut BlockList,
        );
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

    rust_pc_delitem(sd, invslot as c_int, 1, 6);
    sl_doscript_blargs_pc(
        itemdb_yname((*sd).equipid),
        c"equip".as_ptr(),
        1i32, &mut (*sd).bl as *mut BlockList,
    );
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

    rust_pc_calcstat(sd);
    clif_sendupdatestatus_onequip(sd);
    map_foreachinarea_pc(
        clif_updatestate,
        (*sd).bl.m as c_int, (*sd).bl.x as c_int, (*sd).bl.y as c_int,
        AREA, BL_PC,
        sd as *mut MapSessionData,
    );

    0
}

/// `int pc_unequip(USER* sd, int type)` — begin the unequip sequence for equip
/// slot `type`.
///
/// If the slot is empty, returns 1 immediately.  Otherwise stores `takeoffid`
/// and fires the `onUnequip` Lua hook so `pc_unequipscript` can finish.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_unequip(
    sd:    *mut MapSessionData,
    type_: c_int,
) -> c_int {
    if sd.is_null() { return 1; }
    if type_ < 0 || type_ >= 15 { return 1; }

    if (*sd).status.equip[type_ as usize].id == 0 { return 1; }

    (*sd).takeoffid = type_ as i8;
    sl_doscript_blargs_pc(
        c"onUnequip".as_ptr(), std::ptr::null(),
        1i32, &mut (*sd).bl as *mut BlockList,
    );
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
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_unequipscript(sd: *mut MapSessionData) -> c_int {
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

        rust_pc_delitem(sd, invslot as c_int, 1, 6);
        rust_pc_additem(sd, &mut it as *mut _);
        clif_sendequip(sd, type_ as c_int);
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

        if rust_pc_additem(sd, &mut it as *mut _) != 0 { return 1; }

        libc::memset(
            &mut (*sd).status.equip[type_] as *mut _ as *mut libc::c_void,
            0,
            std::mem::size_of::<crate::servers::char::charstatus::Item>(),
        );
        (*sd).target   = (*sd).bl.id as c_int;
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
    sl_doscript_blargs_pc(
        itemdb_yname(takeoff),
        c"unequip".as_ptr(),
        1i32, &mut (*sd).bl as *mut BlockList,
    );

    (*sd).takeoffid = -1i8;
    rust_pc_calcstat(sd);
    clif_sendupdatestatus_onequip(sd);
    map_foreachinarea_pc(
        clif_updatestate,
        (*sd).bl.m as c_int, (*sd).bl.x as c_int, (*sd).bl.y as c_int,
        AREA, BL_PC,
        sd as *mut MapSessionData,
    );

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
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_getitemscript(
    sd: *mut MapSessionData,
    id: c_int,
) -> c_int {
    if sd.is_null() { return 0; }

    let fl_raw = map_id2fl(id as c_uint);
    if fl_raw.is_null() { return 0; }
    let fl = fl_raw as *mut FloorItemData;

    if (*fl).data.id == 0 {
        // It's gold — credit the amount and remove from map.
        (*sd).status.money += (*fl).data.amount as u32;
        clif_sendstatus(sd, SFLAG_XPMONEY);
        clif_lookgone_pc(&mut (*fl).bl as *mut BlockList);
        map_delitem((*fl).bl.id);

        let mut _escape = [0i8; 255];
        Sql_EscapeString(sql_handle, _escape.as_mut_ptr(), (*fl).data.real_name.as_ptr());
        return 0;
    }

    // Non-droppable items are blocked for regular players.
    if itemdb_droppable((*fl).data.id) != 0 && (*sd).status.gm_level == 0 {
        clif_sendminitext(sd, c"That item cannot be picked up.".as_ptr());
        return 0;
    }

    let mut it = std::mem::zeroed::<crate::servers::char::charstatus::Item>();
    let add: bool;

    if (*sd).pickuptype == 0
        && itemdb_stackamount((*fl).data.id) == 1
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
        rust_pc_additem(sd, &mut it as *mut _);
    }

    if (*sd).pickuptype > 0 && (*fl).data.amount > 0 {
        return 0;
    }

    0
}

// ─── Position / warp / magic / state / combat functions ───────────────────────
//
// Ported from c_src/pc.c Task 11.

/// `int pc_setpos(USER* sd, int m, int x, int y)` — sets the player's block-list
/// position without sending any client packets.
///
/// Guards against attempting to set position on a mob object (bl.id >= MOB_START_NUM).
/// Sets bl.m, bl.x, bl.y, and bl.type.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_setpos(
    sd: *mut MapSessionData,
    m: c_int,
    x: c_int,
    y: c_int,
) -> c_int {
    use crate::game::mob::{MOB_START_NUM, BL_PC};
    if (*sd).bl.id >= MOB_START_NUM { return 0; }
    (*sd).bl.m  = m as u16;
    (*sd).bl.x  = x as u16;
    (*sd).bl.y  = y as u16;
    (*sd).bl.bl_type = BL_PC as c_uchar;
    0
}

/// `int pc_warp(USER* sd, int m, int x, int y)` — full warp sequence.
///
/// If the target map is not loaded on this server, queries the `Maps` table for
/// the destination map server and calls `clif_transfer`. Otherwise, fires
/// pre-warp Lua hooks, calls `clif_quit` / `pc_setpos` / `clif_spawn` /
/// `clif_refresh`, then fires post-warp Lua hooks.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_warp(
    sd: *mut MapSessionData,
    mut m: c_int,
    mut x: c_int,
    mut y: c_int,
) -> c_int {
    use crate::servers::char::charstatus::{MAX_SPELLS, MAX_MAGIC_TIMERS};
    use crate::ffi::map_db::map_is_loaded;

    if sd.is_null() { return 0; }

    let oldmap = (*sd).bl.m as c_int;

    if m < 0 { m = 0; }
    if m >= MAX_MAP_PER_SERVER { m = MAX_MAP_PER_SERVER - 1; }

    // If the target map is not loaded on this server, hand off to the right server.
    if !map_is_loaded(m as u16) {
        if !rust_session_exists((*sd).fd) {
            rust_session_set_eof((*sd).fd, 20);
            return 0;
        }

        let mut destsrv: c_int = 0;

        let stmt = SqlStmt_Malloc(sql_handle);
        if stmt.is_null() {
            return -1;
        }

        let rc = SqlStmt_Prepare(
            stmt,
            c"SELECT `MapServer` FROM `Maps` WHERE `MapId` = '%d'".as_ptr(),
            m,
        );
        if rc == SQL_ERROR {
            SqlStmt_Free(stmt);
            return -1;
        }

        if SqlStmt_Execute(stmt) == SQL_ERROR {
            SqlStmt_Free(stmt);
            return -1;
        }

        if SqlStmt_BindColumn(
            stmt,
            0,
            SqlDataType::SqlDtInt,
            &mut destsrv as *mut c_int as *mut c_void,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ) == SQL_ERROR {
            SqlStmt_Free(stmt);
            return -1;
        }

        if SqlStmt_NumRows(stmt) == 0 {
            SqlStmt_Free(stmt);
            return 0;
        }

        let _ = SqlStmt_NextRow(stmt);
        SqlStmt_Free(stmt);

        if x < 0 || x > 255 { x = 1; }
        if y < 0 || y > 255 { y = 1; }

        (*sd).status.dest_pos.m = m as u16;
        (*sd).status.dest_pos.x = x as u16;
        (*sd).status.dest_pos.y = y as u16;

        clif_transfer(sd, destsrv, m, x, y);
        return 0;
    }

    // Map is loaded locally — clamp coordinates to map bounds.
    let map_ptr = crate::ffi::map_db::get_map_ptr(m as u16);
    if map_ptr.is_null() { return 0; }
    let xs = (*map_ptr).xs as c_int;
    let ys = (*map_ptr).ys as c_int;
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
        sl_doscript_blargs_pc(
            c"mapLeave".as_ptr(), std::ptr::null(),
            1i32, &mut (*sd).bl as *mut BlockList,
        );
        if can_mount == 0 {
            sl_doscript_blargs_pc(
                c"onDismount".as_ptr(), std::ptr::null(),
                1i32, &mut (*sd).bl as *mut BlockList,
            );
        }
    }

    // Fire passive_before_warp for each known spell.
    for i in 0..MAX_SPELLS {
        sl_doscript_blargs_pc(
            magicdb_yname_pc((*sd).status.skill[i] as c_int),
            c"passive_before_warp".as_ptr(),
            1i32, &mut (*sd).bl as *mut BlockList,
        );
    }

    // Fire before_warp_while_cast for each active aether timer.
    for i in 0..MAX_MAGIC_TIMERS {
        sl_doscript_blargs_pc(
            magicdb_yname_pc((*sd).status.dura_aether[i].id as c_int),
            c"before_warp_while_cast".as_ptr(),
            1i32, &mut (*sd).bl as *mut BlockList,
        );
    }

    // Perform the actual move.
    clif_quit(sd);
    rust_pc_setpos(sd, m, x, y);
    clif_sendtime(sd);
    clif_spawn(sd);
    clif_refresh(sd);

    // Fire map-enter hooks when changing maps.
    if m != oldmap {
        sl_doscript_blargs_pc(
            c"mapEnter".as_ptr(), std::ptr::null(),
            1i32, &mut (*sd).bl as *mut BlockList,
        );
    }

    // Fire passive_on_warp for each known spell.
    for i in 0..MAX_SPELLS {
        sl_doscript_blargs_pc(
            magicdb_yname_pc((*sd).status.skill[i] as c_int),
            c"passive_on_warp".as_ptr(),
            1i32, &mut (*sd).bl as *mut BlockList,
        );
    }

    // Fire on_warp_while_cast for each active aether timer.
    for i in 0..MAX_MAGIC_TIMERS {
        sl_doscript_blargs_pc(
            magicdb_yname_pc((*sd).status.dura_aether[i].id as c_int),
            c"on_warp_while_cast".as_ptr(),
            1i32, &mut (*sd).bl as *mut BlockList,
        );
    }

    0
}

/// `int pc_loadmagic(USER* sd)` — sends each of the player's known spells to
/// the client via `clif_sendmagic`.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_loadmagic(sd: *mut MapSessionData) -> c_int {
    use crate::servers::char::charstatus::MAX_SPELLS;
    for i in 0..MAX_SPELLS {
        if (*sd).status.skill[i] > 0 {
            clif_sendmagic(sd, i as c_int);
        }
    }
    0
}

/// `int pc_magic_startup(USER* sd)` — initialises spell durations at login.
///
/// For each active aether timer, sends the duration bar to the client and
/// calls the `recast` Lua hook on the spell.  Also sends any pending aether
/// (cooldown) values.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_magic_startup(sd: *mut MapSessionData) -> c_int {
    use crate::servers::char::charstatus::MAX_MAGIC_TIMERS;

    if sd.is_null() { return 0; }

    for x in 0..MAX_MAGIC_TIMERS {
        let p = &(*sd).status.dura_aether[x];

        if p.id > 0 {
            if p.duration > 0 {
                let tsd = map_id2sd_pc(p.caster_id);
                clif_send_duration(sd, p.id as c_int, (p.duration / 1000) as c_uint, tsd);

                if !tsd.is_null() {
                    (*sd).target   = p.caster_id as c_int;
                    (*sd).attacker = p.caster_id;
                    sl_doscript_blargs_pc(
                        magicdb_yname_pc(p.id as c_int),
                        c"recast".as_ptr(),
                        2i32,
                        &mut (*sd).bl as *mut BlockList,
                        &mut (*tsd).bl as *mut BlockList,
                    );
                } else {
                    (*sd).target   = (*sd).status.id as c_int;
                    (*sd).attacker = (*sd).status.id;
                    sl_doscript_blargs_pc(
                        magicdb_yname_pc(p.id as c_int),
                        c"recast".as_ptr(),
                        1i32,
                        &mut (*sd).bl as *mut BlockList,
                    );
                }
            }

            if p.aether > 0 {
                clif_send_aether(sd, p.id as c_int, p.aether / 1000);
            }
        }
    }

    0
}

/// `int pc_reload_aether(USER* sd)` — resends active aether (spell cooldown)
/// values to the client.  Called when the client reconnects.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_reload_aether(sd: *mut MapSessionData) -> c_int {
    use crate::servers::char::charstatus::MAX_MAGIC_TIMERS;
    for x in 0..MAX_MAGIC_TIMERS {
        let p = &(*sd).status.dura_aether[x];
        if p.id > 0 && p.aether > 0 {
            clif_send_aether(sd, p.id as c_int, p.aether / 1000);
        }
    }
    0
}

/// `int pc_die(USER* sd)` — fires the `onDeathPlayer` Lua hook.
///
/// The actual stat/state changes are handled by `pc_diescript`; this function
/// just fires the hook so scripts can respond immediately.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_die(sd: *mut MapSessionData) -> c_int {
    sl_doscript_blargs_pc(
        c"onDeathPlayer".as_ptr(), std::ptr::null(),
        1i32, &mut (*sd).bl as *mut BlockList,
    );
    0
}

/// `int pc_diescript(USER* sd)` — full death processing.
///
/// - Clears `deathflag`, sets state to dead, zeroes HP.
/// - Clears all non-dispel-immune aether timers and fires their `uncast` hooks.
/// - Removes the dead player from all mob threat tables.
/// - Resets combat state (enchanted, flank, backstab, dmgshield).
/// - Recalculates stats and broadcasts updated state.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_diescript(sd: *mut MapSessionData) -> c_int {
    use crate::servers::char::charstatus::MAX_MAGIC_TIMERS;
    use crate::game::mob::{
        MAX_THREATCOUNT,
        MOB_SPAWN_START, MOB_SPAWN_MAX,
        MOB_ONETIME_START, MOB_ONETIME_MAX,
        map_id2mob,
    };

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

        if magicdb_dispel(id as c_int) > 0 { continue; }

        (*sd).status.dura_aether[i].duration = 0;
        clif_send_duration(
            sd,
            (*sd).status.dura_aether[i].id as c_int,
            0,
            map_id2sd_pc((*sd).status.dura_aether[i].caster_id),
        );
        (*sd).status.dura_aether[i].caster_id = 0;

        map_foreachinarea_pc(
            clif_sendanimation_pc,
            (*sd).bl.m as c_int, (*sd).bl.x as c_int, (*sd).bl.y as c_int,
            AREA, BL_PC,
            (*sd).status.dura_aether[i].animation as c_int,
            &mut (*sd).bl as *mut BlockList,
            -1i32,
        );
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
            sl_doscript_blargs_pc(
                magicdb_yname_pc(id as c_int),
                c"uncast".as_ptr(),
                2i32,
                &mut (*sd).bl as *mut BlockList,
                caster_bl,
            );
        } else {
            sl_doscript_blargs_pc(
                magicdb_yname_pc(id as c_int),
                c"uncast".as_ptr(),
                1i32,
                &mut (*sd).bl as *mut BlockList,
            );
        }
    }

    // Remove dead player from all spawn-mob threat tables.
    if MOB_SPAWN_START != MOB_SPAWN_MAX {
        let mut x = MOB_SPAWN_START;
        while x < MOB_SPAWN_MAX {
            let tmob = map_id2mob(x);
            if !tmob.is_null() {
                for i in 0..MAX_THREATCOUNT {
                    if (*tmob).threat[i].user == (*sd).bl.id {
                        (*tmob).threat[i].user   = 0;
                        (*tmob).threat[i].amount = 0;
                    }
                }
            }
            x += 1;
        }
    }

    // Remove dead player from all one-time mob threat tables.
    if MOB_ONETIME_START != MOB_ONETIME_MAX {
        let mut x = MOB_ONETIME_START;
        while x < MOB_ONETIME_MAX {
            let tmob = map_id2mob(x);
            if !tmob.is_null() {
                for i in 0..MAX_THREATCOUNT {
                    if (*tmob).threat[i].user == (*sd).bl.id {
                        (*tmob).threat[i].user   = 0;
                        (*tmob).threat[i].amount = 0;
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

    rust_pc_calcstat(sd);
    map_foreachinarea_pc(
        clif_updatestate,
        (*sd).bl.m as c_int, (*sd).bl.x as c_int, (*sd).bl.y as c_int,
        AREA, BL_PC,
        sd as *mut MapSessionData,
    );

    0
}

/// `int pc_res(USER* sd)` — resurrects the player in-place.
///
/// Sets state to alive, restores 100 HP, sends an HP/MP status update, and
/// warps the player to their current position (which re-spawns them for other
/// clients on the same map).
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_res(sd: *mut MapSessionData) -> c_int {
    (*sd).status.state = PC_ALIVE as i8;
    (*sd).status.hp    = 100;
    clif_sendstatus(sd, SFLAG_HPMP);
    rust_pc_warp(sd, (*sd).bl.m as c_int, (*sd).bl.x as c_int, (*sd).bl.y as c_int);
    0
}

/// `int pc_uncast(USER* sd)` — cancels the player's active cast.
///
/// Not implemented in c_src/pc.c (only declared in pc.h); stubbed here as a
/// no-op placeholder until the feature is required.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_uncast(sd: *mut MapSessionData) -> c_int {
    0
}

/// `int pc_checkformail(USER* sd)` — checks for pending mail/parcels.
///
/// The SQL logic in c_src/pc.c is fully commented out; this function is a
/// stub that matches the C no-op behaviour.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn rust_pc_checkformail(sd: *mut MapSessionData) -> c_int {
    0
}
