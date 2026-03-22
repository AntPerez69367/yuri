#![allow(non_snake_case, dead_code, unused_variables)]

use super::entity::{NPost, PcNetworkState, ScriptReg, ScriptRegStr, SdIgnoreList};
use crate::common::player::PlayerData;
use crate::common::types::Item;
use crate::game::types::GfxViewer;
use crate::session::SessionId;

// ─── Nested sub-structs for MapSessionData ────────────────────────────────────

/// Player exchange/trade state.
#[repr(C)]
pub struct PcExchange {
    pub item: [Item; 52],
    pub item_count: i32,
    pub exchange_done: i32,
    pub list_count: i32,
    pub gold: u32,
    pub target: u32,
}

/// Player UI state flags.
#[repr(C)]
pub struct PcState {
    pub menu_or_input: i32,
}

/// Break-on-death items — items that are destroyed when the player dies.
#[repr(C)]
pub struct PcBodItems {
    pub item: [Item; 52],
    pub bod_count: i32,
}

// ─── MapSessionData ────────────────────────────────────────────────────────────

/// Player session state — the legacy monolith that shrinks as fields migrate to
/// domain sub-structs in `PlayerEntity`.
#[repr(C)]
pub struct MapSessionData {
    pub id: u32,
    pub graphic_id: u32,
    pub graphic_color: u32,
    pub m: u16,
    pub x: u16,
    pub y: u16,
    pub bl_type: u8,
    pub subtype: u8,
    pub fd: SessionId,

    // Domain-typed player persistence data (replaces MmoCharStatus).
    pub player: PlayerData,

    // status timers
    pub equiptimer: u64,
    pub ambushtimer: u64,

    // unsigned int group (multi-field C declarations, split individually)
    pub max_hp: u32,
    pub max_mp: u32,
    pub tempmax_hp: u32,
    pub attacker: u32,
    pub rangeTarget: u32,
    pub equipid: u32,
    pub breakid: u32,
    pub pvp: [[u32; 2]; 20],
    pub killspvp: u32,
    pub timevalues: [u32; 5],
    pub lastvita: u32,
    pub groupid: u32,
    pub disptimer: u32,
    pub disptimertick: u32,
    pub basemight: u32,
    pub basewill: u32,
    pub basegrace: u32,
    pub basearmor: u32,
    pub intpercentage: u32,
    pub profileStatus: u32,

    // int combat stats (first C declaration line)
    pub might: i32,
    pub will: i32,
    pub grace: i32,
    pub armor: i32,
    pub minSdam: i32,
    pub maxSdam: i32,
    pub minLdam: i32,
    pub maxLdam: i32,
    pub hit: i32,
    pub dam: i32,
    pub healing: i32,
    pub healingtimer: i32,
    pub pongtimer: i32,
    pub backstab: i32,

    pub heartbeat: i32,

    // int status flags (second C declaration line)
    pub flank: i32,
    pub polearm: i32,
    pub tooclose: i32,
    pub canmove: i32,
    pub iswalking: i32,
    pub paralyzed: i32,
    pub blind: i32,
    pub drunk: i32,
    pub snare: i32,
    pub silence: i32,
    pub critchance: i32,
    pub afk: i32,
    pub afktime: i32,
    pub totalafktime: i32,
    pub afktimer: i32,
    pub extendhit: i32,
    pub speed: i32,

    // int timers/misc (third C declaration line)
    pub crit: i32,
    pub duratimer: i32,
    pub scripttimer: i32,
    pub scripttick: i32,
    pub secondduratimer: i32,
    pub thirdduratimer: i32,
    pub fourthduratimer: i32,
    pub fifthduratimer: i32,
    pub wisdom: i32,
    pub bindx: i32,
    pub bindy: i32,
    pub hunter: i32,

    // short stats
    pub protection: i16,
    pub miss: i16,
    pub attack_speed: i16,
    pub con: i16,

    // float stats
    pub rage: f32,
    pub enchanted: f32,
    pub sleep: f32,
    pub deduction: f32,
    pub damage: f32,
    pub invis: f32,
    pub fury: f32,
    pub critmult: f32,
    pub dmgshield: f32,
    pub vregenoverflow: f32,
    pub mregenoverflow: f32,

    // double stats
    pub dmgdealt: f64,
    pub dmgtaken: f64,

    // char arrays / single chars
    pub afkmessage: [i8; 80],
    pub mail: [i8; 4000],
    pub ipaddress: [i8; 255],

    pub takeoffid: i8,
    pub attacked: i8,
    pub boardshow: i8,
    pub clone: i8,
    pub action: i8,
    pub luaexec: i8,
    pub deathflag: i8,
    pub selfbar: i8,
    pub groupbars: i8,
    pub mobbars: i8,
    pub disptimertype: i8,
    pub sendstatus_tick: i8,

    pub dialogtype: i8,
    pub alignment: i8,
    pub boardnameval: i8,

    // unsigned short flags
    pub disguise: u16,
    pub disguise_color: u16,

    pub cursed: u8,
    pub castusetimer: i32,
    pub fakeDrop: u8,

    // unsigned char status bytes
    pub confused: u8,
    pub talktype: u8,
    pub pickuptype: u8,
    pub invslot: u8,
    pub equipslot: u8,
    pub spottraps: u8,

    // unsigned short coords
    pub throwx: u16,
    pub throwy: u16,
    pub viewx: u16,
    pub viewy: u16,
    pub bindmap: u16,

    // encryption hash buffer (0x401 = 1025 bytes)
    pub EncHash: [u8; 0x401],

    // npc
    pub npc_id: i32,
    pub npc_pos: i32,
    pub npc_lastpos: i32,
    pub npc_menu: i32,
    pub npc_amount: i32,
    pub npc_g: i32,
    pub npc_gc: i32,
    pub target: i32,
    pub time: i32,
    pub time2: i32,
    pub lasttime: i32,
    pub timer: i32,
    pub npc_stack: i32,
    pub npc_stackmax: i32,

    pub npc_script: *mut i8,
    pub npc_scriptroot: *mut i8,

    // registry
    pub reg: *mut ScriptReg,
    pub regstr: *mut ScriptRegStr,
    pub npcp: NPost,
    pub reg_num: i32,
    pub regstr_num: i32,

    // group
    pub bcount: i32,
    pub group_count: i32,
    pub group_on: i32,
    pub group_leader: u32,

    // exchange
    pub exchange_on: i32,
    pub exchange: PcExchange,
    pub state: PcState,
    pub boditems: PcBodItems,

    // lua
    pub coref: u32,
    pub coref_container: u32,

    // creation system
    pub creation_works: i32,
    pub creation_item: i32,
    pub creation_itemamount: i32,

    // boards
    pub board_candel: i32,
    pub board_canwrite: i32,
    pub board: i32,
    pub board_popup: i32,
    pub co_timer: i32,

    pub question: [i8; 64],
    pub speech: [i8; 255],
    pub profilepic_data: [i8; 65535],
    pub profile_data: [i8; 255],

    pub profilepic_size: u16,
    pub profile_size: u8,

    pub net: PcNetworkState,

    pub msPing: i32,
    pub pbColor: i32,

    pub time_check: u32,
    pub time_hash: u32,
    pub last_click: u32,

    pub chat_timer: i32,
    pub savetimer: i32,

    pub gfx: GfxViewer,
    pub IgnoreList: *mut SdIgnoreList,

    pub optFlags: u64,
    pub uFlags: u64,
    pub LastPongStamp: u64,
    pub LastPingTick: u64,
    pub flags: u64,
    pub LastWalkTick: u64,

    pub PrevSeed: u8,
    pub NextSeed: u8,
    pub LastWalk: u8,
    pub loaded: u8,
}

// SAFETY: Single game thread with RwLock guards. Sync required for Arc<RwLock<T>>.
unsafe impl Send for MapSessionData {}
unsafe impl Sync for MapSessionData {}
