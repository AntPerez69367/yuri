//! Player-character game logic.

pub mod entity;
pub mod types;
pub mod spatial;
pub mod systems;

pub mod prelude {
    // Traits
    pub use crate::common::traits::{Combatant, LegacyEntity, InventoryHolder, Spatial, ScriptTarget};
    
    // Core Structs
    pub use crate::game::player::entity::{PlayerEntity, PcNetworkState, LookAccum, NPost, ScriptReg, ScriptRegStr, SdIgnoreList};
    pub use crate::game::player::types::{PcExchange, PcState, PcBodItems};

    // Common type signatures
    pub use crate::common::types::{Item, Point};
}

// ── Sub-module re-exports ──────────────────────────────────────────────────

pub use entity::{
    PlayerEntity, PcNetworkState, LookAccum, NPost,
    ScriptReg, ScriptRegStr, SdIgnoreList,
};

pub use types::{
    MapSessionData, PcExchange, PcState, PcBodItems,
};

pub use spatial::{
    pc_setpos, pc_warp, pc_warp_sync, pc_die, pc_diescript, pc_res, addtokillreg,
};

pub use systems::{
    // Timers
    pc_item_timer, pc_savetimer, pc_castusetimer, pc_afktimer,
    pc_starttimer, pc_stoptimer, pc_timer, pc_scripttimer,
    pc_atkspeed, pc_disptimertick, pc_sendpong,
    bl_duratimer, bl_secondduratimer, bl_thirdduratimer,
    bl_fourthduratimer, bl_fifthduratimer, bl_aethertimer,
    // Stats
    pc_requestmp, pc_checklevel, pc_givexp, pc_calcstat, pc_calcdamage,
    // Registries
    pc_readreg, pc_setreg, pc_readregstr, pc_setregstr,
    pc_readglobalregstring, pc_setglobalregstring,
    pc_readglobalreg, pc_setglobalreg,
    pc_readparam, pc_setparam,
    pc_readacctreg, pc_setacctreg,
    pc_readnpcintreg, pc_setnpcintreg,
    pc_readquestreg, pc_setquestreg,
    // Items
    pc_isinvenspace, pc_isinvenitemspace, ItemCustomization,
    pc_dropitemfull, pc_addtocurrent2_inner, pc_addtocurrent_inner,
    pc_additem, pc_additemnolog, pc_delitem, pc_dropitemmap,
    pc_changeitem, pc_useitem, pc_runfloor_sub,
    // Equipment
    pc_isequip, pc_loaditem, pc_loadequip,
    pc_canequipitem, pc_canequipstats,
    pc_equipitem, pc_equipscript, pc_unequip, pc_unequipscript,
    pc_getitemscript,
    // Magic
    pc_loadmagic, pc_magic_startup, pc_reload_aether,
};

// ── Constant re-exports (preserve game::pc::* surface) ────────────────────

pub use crate::common::constants::entity::player::{
    MAX_GLOBALREG, MAX_GLOBALPLAYERREG, MAX_GLOBALQUESTREG, MAX_GLOBALNPCREG,
    PC_ALIVE, PC_DIE, PC_INVIS, PC_MOUNTED, PC_DISGUISE,
    OPT_FLAG_STEALTH, OPT_FLAG_NOCLICK, OPT_FLAG_WALKTHROUGH, OPT_FLAG_GHOSTS,
    U_FLAG_NONE, U_FLAG_SILENCED, U_FLAG_CANPK, U_FLAG_CANBEPK,
    U_FLAG_IMMORTAL, U_FLAG_UNPHYSICAL, U_FLAG_EVENTHOST, U_FLAG_CONSTABLE,
    U_FLAG_ARCHON, U_FLAG_GM,
    SFLAG_UNKNOWN1, SFLAG_UNKNOWN2, SFLAG_UNKNOWN3, SFLAG_ALWAYSON,
    SFLAG_XPMONEY, SFLAG_HPMP, SFLAG_FULLSTATS, SFLAG_GMON,
    FLAG_WHISPER, FLAG_GROUP, FLAG_SHOUT, FLAG_ADVICE, FLAG_MAGIC,
    FLAG_WEATHER, FLAG_REALM, FLAG_EXCHANGE, FLAG_FASTMOVE, FLAG_SOUND,
    FLAG_HELM, FLAG_NECKLACE, FLAG_PARCEL, FLAG_MAIL,
    SP_HP, SP_MP, SP_MHP, SP_MMP,
    ITM_EAT, ITM_USE, ITM_SMOKE, ITM_WEAP, ITM_ARMOR, ITM_SHIELD, ITM_HELM,
    ITM_LEFT, ITM_RIGHT, ITM_SUBLEFT, ITM_SUBRIGHT, ITM_FACEACC, ITM_CROWN,
    ITM_MANTLE, ITM_NECKLACE, ITM_BOOTS, ITM_COAT, ITM_HAND, ITM_ETC,
    ITM_USESPC, ITM_TRAPS, ITM_BAG, ITM_MAP, ITM_QUIVER, ITM_MOUNT, ITM_FACE,
    ITM_SET, ITM_SKIN, ITM_HAIR_DYE, ITM_FACEACCTWO,
    EQ_WEAP, EQ_ARMOR, EQ_SHIELD, EQ_HELM, EQ_LEFT, EQ_RIGHT,
    EQ_SUBLEFT, EQ_SUBRIGHT, EQ_FACEACC, EQ_CROWN, EQ_MANTLE,
    EQ_NECKLACE, EQ_BOOTS, EQ_COAT, EQ_FACEACCTWO,
    MAP_WHISPFAIL, MAP_ERRGHOST, MAP_ERRITMLEVEL, MAP_ERRITMMIGHT, MAP_ERRITMGRACE,
    MAP_ERRITMWILL, MAP_ERRITMSEX, MAP_ERRITMFULL, MAP_ERRITMMAX, MAP_ERRITMPATH,
    MAP_ERRITMMARK, MAP_ERRITM2H, MAP_ERRMOUNT,
};
pub use crate::common::constants::entity::{BL_PC, BL_MOB, BL_NPC, BL_ITEM};
pub use crate::common::constants::world::{MAX_MAP_PER_SERVER, MAX_GROUP_MEMBERS};
pub use crate::common::constants::network::{AREA, LOOK_SEND};
pub use crate::common::constants::entity::SUBTYPE_FLOOR as FLOOR;
pub const BLOCK_SIZE_PC: i32 = crate::common::constants::world::BLOCK_SIZE as i32;
pub use crate::game::map_server::{MapMsgData, map_msg};
pub use crate::game::map_server::groups;
