pub mod network {
    // Broadcast target scope — controls which sessions receive a packet
    // Canonical source: old_src/c_src/map_parse.h
    pub const ALL_CLIENT: i32 = 0;
    pub const SAMESRV: i32 = 1;
    pub const SAMEMAP: i32 = 2;
    pub const SAMEMAP_WOS: i32 = 3;
    pub const AREA: i32 = 4;
    pub const AREA_WOS: i32 = 5;
    pub const SAMEAREA: i32 = 6;
    pub const SAMEAREA_WOS: i32 = 7;
    pub const CORNER: i32 = 8;
    pub const SELF: i32 = 9;

    // Look/appearance update direction
    pub const LOOK_GET: i32 = 0;
    pub const LOOK_SEND: i32 = 1;
}

pub mod world {
    // Spatial block grid size
    // Canonical source: old_src/c_src/map_server.h
    pub const BLOCK_SIZE: usize = 8;
    pub const MAX_MAPREG: usize = 500;
    pub const MAX_GAMEREG: usize = 5000;
    pub const MAX_MAP_PER_SERVER: i32 = 65535;

    // Area grid dimensions (used for block-list iteration)
    pub const AREAX_SIZE: i32 = 18;
    pub const AREAY_SIZE: i32 = 16;

    // Client viewport size in map cells
    // VIEW_W = AREAX_SIZE + 1, VIEW_H = AREAY_SIZE + 1
    pub const VIEW_W: i32 = 19;
    pub const VIEW_H: i32 = 17;
    pub const VIEW_OX: i32 = 9;
    pub const VIEW_OY: i32 = 8;

    // Group limits
    // Canonical source: old_src/c_src/map_server.h
    pub const MAX_GROUPS: usize = 256;
    pub const MAX_GROUP_MEMBERS: usize = 256;

    // Board permission flags
    // Canonical source: old_src/c_src/mmo.h
    pub const BOARD_CAN_WRITE: i32 = 1;
    pub const BOARD_CAN_DEL: i32 = 2;
}

pub mod entity {
    // Block-list subtype for scripting items
    // Canonical source: old_src/c_src/mmo.h
    pub const SUBTYPE_SCRIPT: u8 = 0;
    pub const SUBTYPE_FLOOR: u8 = 1;

    // Block-list type flags — identify what kind of entity a block_list is
    // Canonical source: old_src/c_src/mmo.h
    pub const BL_NUL: i32 = 0x00;
    pub const BL_PC: i32 = 0x01;
    pub const BL_MOB: i32 = 0x02;
    pub const BL_NPC: i32 = 0x04;
    pub const BL_ITEM: i32 = 0x08;
    pub const BL_ALL: i32 = 0x0F;
    pub const BL_MOBPC: i32 = 0x1E;

    // u8 variants used by block_grid and visual packet handlers
    pub const BL_PC_U8: u8 = 0x01;
    pub const BL_MOB_U8: u8 = 0x02;
    pub const BL_NPC_U8: u8 = 0x04;
    pub const BL_ITEM_U8: u8 = 0x08;
    pub const BL_ALL_U8: u8 = 0x0F;

    pub mod player {
        // Player state codes — match wire protocol state field
        // Canonical source: old_src/c_src/mmo.h
        pub const PC_ALIVE: i32 = 0;
        pub const PC_DIE: i32 = 1;
        pub const PC_INVIS: i32 = 2;
        pub const PC_MOUNTED: i32 = 3;
        pub const PC_DISGUISE: i32 = 4;

        // i8 variants used in movement packet parsing
        pub const PC_DIE_I8: i8 = 1;
        pub const PC_INVIS_I8: i8 = 2;
        pub const PC_MOUNTED_I8: i8 = 3;
        pub const PC_DISGUISE_I8: i8 = 4;

        // Status update flags — bitmask controlling which fields appear in status packet 0x08
        // Canonical source: old_src/c_src/mmo.h
        pub const SFLAG_UNKNOWN1: i32 = 0x01;
        pub const SFLAG_UNKNOWN2: i32 = 0x02;
        pub const SFLAG_UNKNOWN3: i32 = 0x04;
        pub const SFLAG_ALWAYSON: i32 = 0x08;
        pub const SFLAG_XPMONEY: i32 = 0x10;
        pub const SFLAG_HPMP: i32 = 0x20;
        pub const SFLAG_FULLSTATS: i32 = 0x40;
        pub const SFLAG_GMON: i32 = 0x80;

        // Option/display flags — control visibility and interaction behavior
        // Canonical source: old_src/c_src/map_server.h
        pub const OPT_FLAG_STEALTH: u64 = 32;
        pub const OPT_FLAG_NOCLICK: u64 = 64;
        pub const OPT_FLAG_WALKTHROUGH: u64 = 128;
        pub const OPT_FLAG_GHOSTS: u64 = 256;

        // User permission flags
        // Canonical source: old_src/c_src/map_server.h
        pub const U_FLAG_NONE: u64 = 0;
        pub const U_FLAG_SILENCED: u64 = 1;
        pub const U_FLAG_CANPK: u64 = 2;
        pub const U_FLAG_CANBEPK: u64 = 3;
        pub const U_FLAG_IMMORTAL: u64 = 8;
        pub const U_FLAG_UNPHYSICAL: u64 = 16;
        pub const U_FLAG_EVENTHOST: u64 = 32;
        pub const U_FLAG_CONSTABLE: u64 = 64;
        pub const U_FLAG_ARCHON: u64 = 128;
        pub const U_FLAG_GM: u64 = 256;

        // Client setting flags (player preferences)
        // Canonical source: old_src/c_src/mmo.h
        pub const FLAG_WHISPER: u32 = 1;
        pub const FLAG_GROUP: u32 = 2;
        pub const FLAG_SHOUT: u32 = 4;
        pub const FLAG_ADVICE: u32 = 8;
        pub const FLAG_MAGIC: u32 = 16;
        pub const FLAG_WEATHER: u32 = 32;
        pub const FLAG_REALM: u32 = 64;
        pub const FLAG_EXCHANGE: u32 = 128;
        pub const FLAG_FASTMOVE: u32 = 256;
        pub const FLAG_SOUND: u32 = 4096;
        pub const FLAG_HELM: u32 = 8192;
        pub const FLAG_NECKLACE: u32 = 16384;

        // Normal flags
        pub const FLAG_PARCEL: u64 = 1;
        pub const FLAG_MAIL: u64 = 16;

        // Item type codes — match item database 'type' field
        // Canonical source: old_src/c_src/item_db.h
        pub const ITM_EAT: i32 = 0;
        pub const ITM_USE: i32 = 1;
        pub const ITM_SMOKE: i32 = 2;
        pub const ITM_WEAP: i32 = 3;
        pub const ITM_ARMOR: i32 = 4;
        pub const ITM_SHIELD: i32 = 5;
        pub const ITM_HELM: i32 = 6;
        pub const ITM_LEFT: i32 = 7;
        pub const ITM_RIGHT: i32 = 8;
        pub const ITM_SUBLEFT: i32 = 9;
        pub const ITM_SUBRIGHT: i32 = 10;
        pub const ITM_FACEACC: i32 = 11;
        pub const ITM_CROWN: i32 = 12;
        pub const ITM_MANTLE: i32 = 13;
        pub const ITM_NECKLACE: i32 = 14;
        pub const ITM_BOOTS: i32 = 15;
        pub const ITM_COAT: i32 = 16;
        pub const ITM_HAND: i32 = 17;
        pub const ITM_ETC: i32 = 18;
        pub const ITM_USESPC: i32 = 19;
        pub const ITM_TRAPS: i32 = 20;
        pub const ITM_BAG: i32 = 21;
        pub const ITM_MAP: i32 = 22;
        pub const ITM_QUIVER: i32 = 23;
        pub const ITM_MOUNT: i32 = 24;
        pub const ITM_FACE: i32 = 25;
        pub const ITM_SET: i32 = 26;
        pub const ITM_SKIN: i32 = 27;
        pub const ITM_HAIR_DYE: i32 = 28;
        pub const ITM_FACEACCTWO: i32 = 29;
        /// u8 variant used by item_db loader (item.typ field is u8).
        pub const ITM_ETC_U8: u8 = 18;

        // Status parameter identifiers
        // Canonical source: old_src/c_src/mmo.h
        pub const SP_HP: i32 = 0;
        pub const SP_MP: i32 = 1;
        pub const SP_MHP: i32 = 2;
        pub const SP_MMP: i32 = 3;
        pub const SP_ZENY: i32 = 4;
        pub const SP_BHP: i32 = 5;
        pub const SP_BMP: i32 = 6;

        // Error/message indices into map_msg[] array
        // Canonical source: old_src/c_src/mmo.h (sequential enum from 0)
        pub const MAP_WHISPFAIL: usize = 0;
        pub const MAP_ERRGHOST: usize = 1;
        pub const MAP_ERRITMLEVEL: usize = 2;
        pub const MAP_ERRITMMIGHT: usize = 3;
        pub const MAP_ERRITMGRACE: usize = 4;
        pub const MAP_ERRITMWILL: usize = 5;
        pub const MAP_ERRITMSEX: usize = 6;
        pub const MAP_ERRITMFULL: usize = 7;
        pub const MAP_ERRITMMAX: usize = 8;
        pub const MAP_ERRITMPATH: usize = 9;
        pub const MAP_ERRITMMARK: usize = 10;
        pub const MAP_ERRITM2H: usize = 11;
        pub const MAP_ERRMOUNT: usize = 12;
        pub const MAP_EQHELM: usize = 13;
        pub const MAP_EQWEAP: usize = 14;
        pub const MAP_EQARMOR: usize = 15;
        pub const MAP_EQSHIELD: usize = 16;
        pub const MAP_EQLEFT: usize = 17;
        pub const MAP_EQRIGHT: usize = 18;
        pub const MAP_EQSUBLEFT: usize = 19;
        pub const MAP_EQSUBRIGHT: usize = 20;
        pub const MAP_EQFACEACC: usize = 21;
        pub const MAP_EQCROWN: usize = 22;
        pub const MAP_EQMANTLE: usize = 23;
        pub const MAP_EQNECKLACE: usize = 24;
        pub const MAP_EQBOOTS: usize = 25;
        pub const MAP_EQCOAT: usize = 26;
        pub const MAP_ERRVITA: usize = 27;
        pub const MAP_ERRMANA: usize = 28;
        // NOTE: MAP_ERRSUMMON = 29 (index into map_msg[MSG_MAX=30]).
        // MSG_MAX is defined in game::map_server as 30 (array size).
        pub const MAP_ERRSUMMON: usize = 29;

        // Equipment slot indices into equip[] array
        // Canonical source: old_src/c_src/item_db.h (sequential enum from 0)
        pub const EQ_WEAP: i32 = 0;
        pub const EQ_ARMOR: i32 = 1;
        pub const EQ_SHIELD: i32 = 2;
        pub const EQ_HELM: i32 = 3;
        pub const EQ_LEFT: i32 = 4;
        pub const EQ_RIGHT: i32 = 5;
        pub const EQ_SUBLEFT: i32 = 6;
        pub const EQ_SUBRIGHT: i32 = 7;
        pub const EQ_FACEACC: i32 = 8;
        pub const EQ_CROWN: i32 = 9;
        pub const EQ_MANTLE: i32 = 10;
        pub const EQ_NECKLACE: i32 = 11;
        pub const EQ_BOOTS: i32 = 12;
        pub const EQ_COAT: i32 = 13;
        pub const EQ_FACEACCTWO: i32 = 14;

        // Registry and storage limits
        // Canonical source: old_src/c_src/mmo.h
        pub const MAX_GLOBALREG: usize = 5000;
        pub const MAX_GLOBALPLAYERREG: usize = 500;
        pub const MAX_GLOBALQUESTREG: usize = 250;
        pub const MAX_GLOBALNPCREG: usize = 100;
        pub const MAX_INVENTORY: usize = 52;
        pub const MAX_EQUIP: usize = 15;
        pub const MAX_BANK_SLOTS: usize = 255;
        pub const MAX_SPELLS: usize = 52;
        pub const MAX_MAGIC_TIMERS: usize = 200;
        pub const MAX_LEGENDS: usize = 1000;
    }

    pub mod mob {
        // Mob state codes
        // Canonical source: old_src/c_src/mob.h (sequential enum from 0)
        pub const MOB_ALIVE: u8 = 0;
        pub const MOB_DEAD: u8 = 1;
        pub const MOB_PARA: u8 = 2;
        pub const MOB_BLIND: u8 = 3;
        pub const MOB_HIT: u8 = 4;
        pub const MOB_ESCAPE: u8 = 5;

        // Entity ID range boundaries for mobs
        // Canonical source: old_src/c_src/map_server.h
        pub const MOB_START_NUM: u32 = 1_073_741_823;
        pub const MOBOT_START_NUM: u32 = 1_173_741_823;
        pub const MAX_MOB: u32 = 100_000_000;

        // Mob-specific limits
        // Canonical source: old_src/c_src/mmo.h
        pub const MAX_GLOBALMOBREG: usize = 50;
        pub const MAX_THREATCOUNT: usize = 50;
        pub const MAX_INVENTORY: usize = 52;
        pub const MAX_MAGIC_TIMERS: usize = 200;
    }

    pub mod npc {
        // Entity ID range boundaries for NPCs
        // Canonical source: old_src/c_src/map_server.h
        pub const NPC_START_NUM: u32 = 3_221_225_472;
        pub const NPCT_START_NUM: u32 = 3_321_225_472;
        /// Sentinel value used as a placeholder NPC ID (u32::MAX).
        pub const F1_NPC: u32 = 4_294_967_295;
        pub const MAX_NPC: u32 = 100_000_000;
    }

    pub mod item {
        // Entity ID range boundaries for floor items
        // Canonical source: old_src/c_src/map_server.h
        pub const FLOORITEM_START_NUM: u32 = 2_047_483_647;
        pub const MAX_FLOORITEM: u32 = 100_000_000;
    }
}
