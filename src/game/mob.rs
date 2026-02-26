//! Mob game logic — replaces `c_src/mob.c`.

#![allow(non_snake_case, dead_code)]

use std::ffi::{c_char, c_double, c_float, c_int, c_schar, c_short, c_uchar, c_uint, c_ushort};
use crate::database::map_db::{BlockList, GlobalReg, WarpList};
#[cfg(not(test))]
use crate::database::map_db::BLOCK_SIZE;
#[cfg(not(test))]
use crate::ffi::map_db::{get_map_ptr as ffi_get_map_ptr, map_is_loaded as ffi_map_is_loaded};
use crate::database::mob_db::MobDbData;
use crate::servers::char::charstatus::{Item, SkillInfo};
use crate::game::types::GfxViewer;

// ─── Constants ──────────────────────────────────────────────────────────────
pub const MOB_START_NUM:       u32 = 1073741823;
pub const MOBOT_START_NUM:     u32 = 1173741823;
pub const NPC_START_NUM:       u32 = 3221225472;
pub const FLOORITEM_START_NUM: u32 = 2047483647;

pub const MAX_MAGIC_TIMERS:  usize = 200;
pub const MAX_INVENTORY:     usize = 52;
pub const MAX_GLOBALMOBREG:  usize = 50;
pub const MAX_THREATCOUNT:   usize = 50;

pub const BL_PC:   c_int = 0x01;
pub const BL_MOB:  c_int = 0x02;
pub const BL_NPC:  c_int = 0x04;
pub const BL_ITEM: c_int = 0x08;

// mob state constants
pub const MOB_ALIVE:  u8 = 0;
pub const MOB_DEAD:   u8 = 1;
pub const MOB_PARA:   u8 = 2;
pub const MOB_BLIND:  u8 = 3;
pub const MOB_HIT:    u8 = 4;
pub const MOB_ESCAPE: u8 = 5;

/// `AREA=4` from enum in `c_src/map_parse.h`.
const AREA: c_int = 4;
/// `LOOK_SEND=1` from enum `{ LOOK_GET=0, LOOK_SEND=1 }` in `c_src/map_parse.h`.
const LOOK_SEND: c_int = 1;
/// `FLOOR=1` from enum `{ SCRIPT=0, FLOOR=1 }` in `c_src/map_server.h`.
const FLOOR: c_uchar = 1;

// ─── ThreatTable ─────────────────────────────────────────────────────────────

/// Mirrors `struct threat_table` from `map_server.h`. 8 bytes.
#[repr(C)]
pub struct ThreatTable {
    pub user:   c_uint,
    pub amount: c_uint,
}

// ─── MobSpawnData ─────────────────────────────────────────────────────────────

/// Mirrors `struct mobspawn_data` from `map_server.h`. (`MOB` typedef in C.)
///
/// Field order and types MUST exactly match C. Verify size with:
/// `cargo test --features map-game mob_spawn_data_size -- --nocapture`
///
/// Layout:
/// ```text
/// offset  field                    size
///      0  bl                         48  (BlockList)
///     48  da[200]                  9600  (200 × SkillInfo@48)
///   9648  inventory[52]           45760  (52 × Item@880)
///  55408  data*                       8  (pointer)
///  55416  threat[50]                400  (50 × ThreatTable@8)
///  55816  registry[50]             3400  (50 × GlobalReg@68)
///  59216  gfx                        72  (GfxViewer)
///  59288  startm..look               12  (6 × u16)
///  59300  miss, protection            4  (2 × i16)
///  59304  id..exp                    72  (18 × u32)
///  59376  ac..will                   44  (11 × i32)
///  59420  state..look_color           9  (9 × u8)
///  59429  clone..charstate            5  (5 × i8)  → compiler pads 3 bytes here
///  59437  sleep..invis               20  (5 × f32) — offset 59437 is wrong after pad
/// ```
/// (Use the size test to verify total = 61120.)
#[repr(C)]
pub struct MobSpawnData {
    pub bl:           BlockList,
    pub da:           [SkillInfo; MAX_MAGIC_TIMERS],
    pub inventory:    [Item; MAX_INVENTORY],
    pub data:         *mut MobDbData,
    pub threat:       [ThreatTable; MAX_THREATCOUNT],
    pub registry:     [GlobalReg; MAX_GLOBALMOBREG],
    pub gfx:          GfxViewer,
    pub startm:       c_ushort,
    pub startx:       c_ushort,
    pub starty:       c_ushort,
    pub bx:           c_ushort,
    pub by_:          c_ushort,
    pub look:         c_ushort,
    pub miss:         c_short,
    pub protection:   c_short,
    pub id:           c_uint,
    pub mobid:        c_uint,
    pub current_vita: c_uint,
    pub current_mana: c_uint,
    pub target:       c_uint,
    pub attacker:     c_uint,
    pub owner:        c_uint,
    pub confused_target: c_uint,
    pub timer:        c_uint,
    pub last_death:   c_uint,
    pub rangeTarget:  c_uint,
    pub ranged:       c_uint,
    pub newmove:      c_uint,
    pub newatk:       c_uint,
    pub lastvita:     c_uint,
    pub maxvita:      c_uint,
    pub maxmana:      c_uint,
    pub replace:      c_uint,
    pub mindam:       c_uint,
    pub maxdam:       c_uint,
    pub amnesia:      c_uint,
    pub exp:          c_uint,
    pub ac:           c_int,
    pub side:         c_int,
    pub time_:        c_int,
    pub spawncheck:   c_int,
    pub num:          c_int,
    pub crit:         c_int,
    pub critchance:   c_int,
    pub critmult:     c_int,
    pub snare:        c_int,
    pub lastaction:   c_int,
    pub hit:          c_int,
    pub might:        c_int,
    pub grace:        c_int,
    pub will:         c_int,
    pub state:        c_uchar,
    pub canmove:      c_uchar,
    pub onetime:      c_uchar,
    pub paralyzed:    c_uchar,
    pub blind:        c_uchar,
    pub confused:     c_uchar,
    pub summon:       c_uchar,
    pub returning:    c_uchar,
    pub look_color:   c_uchar,
    pub clone:        c_schar,
    pub start:        c_schar,
    pub end:          c_schar,
    pub block:        c_schar,
    pub charstate:    c_schar,
    // compiler inserts 3 bytes of padding here to align c_float to 4 bytes
    pub sleep:        c_float,
    pub deduction:    c_float,
    pub damage:       c_float,
    pub dmgshield:    c_float,
    pub invis:        c_float,
    // compiler inserts padding here to align c_double to 8 bytes
    pub dmgdealt:     c_double,
    pub dmgtaken:     c_double,
    pub maxdmg:       c_double,
    pub dmgindtable:  [[c_double; 2]; MAX_THREATCOUNT],
    pub dmggrptable:  [[c_double; 2]; MAX_THREATCOUNT],
    pub cursed:       c_uchar,
}

// SAFETY: MobSpawnData contains raw pointers to C-managed entities.
// All access is gated behind unsafe blocks.
unsafe impl Send for MobSpawnData {}
unsafe impl Sync for MobSpawnData {}

// ─── Mutable globals (unsafe statics, C-compatible) ──────────────────────────
// Use #[export_name] for uppercase globals to avoid sqlx #[derive(FromRow)]
// let-binding conflicts (see MEMORY.md: "npc_id #[export_name]").
#[export_name = "mob_id"]          pub static mut MOB_ID:           c_uint = MOB_START_NUM;
#[export_name = "max_normal_id"]   pub static mut MAX_NORMAL_ID:    c_uint = MOB_START_NUM;
#[export_name = "cmob_id"]         pub static mut CMOB_ID:          c_uint = 0;
#[export_name = "MOB_SPAWN_MAX"]   pub static mut MOB_SPAWN_MAX:    c_uint = MOB_START_NUM;
#[export_name = "MOB_SPAWN_START"] pub static mut MOB_SPAWN_START:  c_uint = MOB_START_NUM;
#[export_name = "MOB_ONETIME_MAX"] pub static mut MOB_ONETIME_MAX:  c_uint = MOBOT_START_NUM;
#[export_name = "MOB_ONETIME_START"] pub static mut MOB_ONETIME_START: c_uint = MOBOT_START_NUM;
#[export_name = "MIN_TIMER"]       pub static mut MIN_TIMER:        c_uint = 1000;
pub static mut TIMERCHECK: c_uchar = 0;  // internal only

// ─── Extern C declarations ────────────────────────────────────────────────────

#[cfg(not(test))]
extern "C" {
    // map entity lookup
    pub fn map_id2bl(id: c_uint) -> *mut BlockList;
    pub fn map_id2mob(id: c_uint) -> *mut MobSpawnData;
    pub fn map_id2sd(id: c_uint) -> *mut std::ffi::c_void;   // USER* — opaque
    pub fn map_addiddb(bl: *mut BlockList);
    pub fn map_deliddb(bl: *mut BlockList);
    pub fn map_addblock(bl: *mut BlockList) -> c_int;
    pub fn map_delblock(bl: *mut BlockList) -> c_int;
    pub fn map_moveblock(bl: *mut BlockList, x: c_int, y: c_int);
    pub fn map_additem(bl: *mut BlockList);
    pub fn map_canmove(m: c_int, x: c_int, y: c_int) -> c_int;
    pub fn map_foreachinarea(
        f: unsafe extern "C" fn(*mut BlockList, ...) -> c_int,
        m: c_int, x: c_int, y: c_int, range: c_int, bl_type: c_int, ...
    ) -> c_int;
    pub fn map_foreachincell(
        f: unsafe extern "C" fn(*mut BlockList, ...) -> c_int,
        m: c_int, x: c_int, y: c_int, bl_type: c_int, ...
    ) -> c_int;
    pub fn map_foreachinblock(
        f: unsafe extern "C" fn(*mut BlockList, ...) -> c_int,
        m: c_int, x0: c_int, y0: c_int, x1: c_int, y1: c_int, bl_type: c_int, ...
    ) -> c_int;

    // clif_* network helpers
    pub fn clif_mob_kill(mob: *mut MobSpawnData);
    pub fn clif_lookgone(bl: *mut BlockList);
    pub fn clif_mob_move(bl: *mut BlockList, ...) -> c_int;
    pub fn clif_cmoblook_sub(bl: *mut BlockList, ...) -> c_int;
    pub fn clif_mob_look_start_func(bl: *mut BlockList, ...) -> c_int;
    pub fn clif_mob_look_close_func(bl: *mut BlockList, ...) -> c_int;
    pub fn clif_object_look_sub(bl: *mut BlockList, ...) -> c_int;
    pub fn clif_object_look_sub2(bl: *mut BlockList, ...) -> c_int;
    pub fn clif_sendanimation(bl: *mut BlockList, ...) -> c_int;
    pub fn clif_sendmob_side(mob: *mut MobSpawnData);
    pub fn clif_object_canmove(m: c_int, x: c_int, y: c_int, dir: c_int) -> c_int;
    pub fn clif_object_canmove_from(m: c_int, x: c_int, y: c_int, dir: c_int) -> c_int;

    // scripting
    pub fn sl_doscript_blargs(
        yname: *const c_char, event: *const c_char, nargs: c_int, ...
    ) -> c_int;

    // magic db lookup — static inline in C, redirect to actual Rust symbols
    #[link_name = "rust_magicdb_yname"]
    pub fn magicdb_yname(id: c_int) -> *mut c_char;
    #[link_name = "rust_magicdb_name"]
    pub fn magicdb_name(id: c_int) -> *mut c_char;

    // mob_db lookups — static inline in C, redirect to actual Rust symbols
    #[link_name = "rust_mobdb_experience"]
    pub fn mobdb_experience(mobid: c_uint) -> c_uint;
    #[link_name = "rust_mobdb_search"]
    pub fn mobdb_search(id: c_uint) -> *mut MobDbData;

    // C helper callbacks that stay in mob.c (USER-dependent or floor-item logic)
    pub fn mob_find_target(bl: *mut BlockList, ...) -> c_int;
    pub fn mob_move(bl: *mut BlockList, ...) -> c_int;
    pub fn mob_attack(mob: *mut MobSpawnData, id: c_int) -> c_int;
    pub fn mobdb_dropitem(
        blockid: c_uint, id: c_uint, amount: c_int,
        dura: c_int, protected_: c_int, owner: c_int,
        m: c_int, x: c_int, y: c_int,
        sd: *mut std::ffi::c_void,   // USER*
    ) -> c_int;

    // mob_free_helper: single-line C wrapper for FREE() macro
    pub fn mob_free_helper(mob: *mut MobSpawnData);

    // rnd / tick / time
    pub fn rnd(n: c_int) -> c_int;
    pub fn gettick() -> c_uint;
    static cur_time: c_int;
    static serverid: c_int;
}

// sl_doscript_simple is a C macro; replicate it as an inline Rust helper.
#[cfg(not(test))]
unsafe fn sl_doscript_simple(yname: *const c_char, event: *const c_char, bl: *mut BlockList) -> c_int {
    sl_doscript_blargs(yname, event, 1, bl)
}

// ─── Mob ID management ────────────────────────────────────────────────────────

pub unsafe fn mob_get_new_id() -> c_uint {
    let id = MOB_ID;
    MOB_ID += 1;
    id
}

#[cfg(not(test))]
pub unsafe fn mob_get_free_id() -> c_uint {
    let mut x = MOB_ONETIME_START;
    loop {
        if x >= NPC_START_NUM {
            eprintln!("[mob] mob_get_free_id: onetime range exhausted");
            return 0;
        }
        if x == MOB_ONETIME_MAX {
            if MOB_ONETIME_MAX >= NPC_START_NUM {
                eprintln!("[mob] mob_get_free_id: onetime range full");
                return 0;
            }
            MOB_ONETIME_MAX += 1;
        }
        if map_id2bl(x).is_null() { return x; }
        x += 1;
    }
}

#[cfg(not(test))]
pub unsafe fn onetime_avail(id: c_uint) -> *mut BlockList {
    map_id2bl(id)
}

#[cfg(not(test))]
pub unsafe fn free_onetime(mob: *mut MobSpawnData) -> c_int {
    if mob.is_null() { return 0; }
    (*mob).data = std::ptr::null_mut();
    mob_free_helper(mob);
    // compact onetime range downward
    let mut x = MOB_ONETIME_START;
    while x <= MOB_ONETIME_MAX {
        let bl = map_id2bl(x);
        if bl.is_null() { return 0; }
        if x == MOB_ONETIME_MAX {
            map_deliddb(bl);
            MOB_ONETIME_MAX -= 1;
        }
        x += 1;
    }
    0
}

// ─── Stat / respawn functions (forward-defined; also used by Task 8) ─────────

#[cfg(not(test))]
unsafe fn in_spawn_window(mob: *const MobSpawnData) -> bool {
    let s = (*mob).start as c_int;
    let e = (*mob).end  as c_int;
    let ct = cur_time;
    (s < e && ct >= s && ct <= e)
    || (s > e && (ct >= s || ct <= e))
    || (s == e && ct == s && ct == e)
    || (s == 25 && e == 25)
}

#[cfg(not(test))]
pub unsafe fn mob_respawn_getstats(mob: *mut MobSpawnData) -> c_int {
    if mob.is_null() { return 0; }
    (*mob).data = if in_spawn_window(mob) {
        mobdb_search((*mob).mobid)
    } else if (*mob).replace != 0 {
        mobdb_search((*mob).replace)
    } else {
        mobdb_search((*mob).mobid)
    };
    if (*mob).data.is_null() { return 0; }
    let d = &*(*mob).data;
    (*mob).maxvita     = d.vita as c_uint;
    (*mob).maxmana     = d.mana as c_uint;
    (*mob).ac          = d.baseac;
    if (*mob).ac < -95 { (*mob).ac = -95; }
    if (*mob).exp == 0 { (*mob).exp = mobdb_experience((*mob).mobid); }
    (*mob).miss        = d.miss;
    (*mob).newmove     = d.movetime as c_uint;
    (*mob).newatk      = d.atktime as c_uint;
    (*mob).current_vita = (*mob).maxvita;
    (*mob).current_mana = (*mob).maxmana;
    (*mob).maxdmg      = (*mob).current_vita as c_double;
    (*mob).hit         = d.hit;
    (*mob).mindam      = d.mindam;
    (*mob).maxdam      = d.maxdam;
    (*mob).might       = d.might;
    (*mob).grace       = d.grace;
    (*mob).will        = d.will;
    (*mob).block       = d.block;
    (*mob).protection  = d.protection;
    (*mob).look        = d.look as c_ushort;
    (*mob).look_color  = d.look_color as c_uchar;
    (*mob).charstate   = d.state;
    (*mob).clone       = 0;
    (*mob).time_       = 0;
    (*mob).paralyzed   = 0;
    (*mob).blind       = 0;
    (*mob).confused    = 0;
    (*mob).snare       = 0;
    (*mob).target      = 0;
    (*mob).attacker    = 0;
    (*mob).confused_target = 0;
    (*mob).rangeTarget = 0;
    (*mob).dmgshield   = 0.0;
    (*mob).sleep       = 1.0;
    (*mob).deduction   = 1.0;
    (*mob).damage      = 0.0;
    (*mob).critchance  = 0;
    (*mob).crit        = 0;
    (*mob).critmult    = 0;
    (*mob).invis       = 1.0;
    0
}

// ─── Spawn table loader ───────────────────────────────────────────────────────

#[cfg(not(test))]
use crate::database::{blocking_run, get_pool};

#[cfg(not(test))]
pub unsafe fn mobspawn_read() -> c_int {
    let serverid_val = serverid;
    let result = blocking_run(async move {
        let pool = get_pool();
        let query = format!(
            "SELECT `SpnMapId`, `SpnX`, `SpnY`, `SpnMobId`, \
             `SpnLastDeath`, `SpnId`, `SpnStartTime`, `SpnEndTime`, \
             `SpnMobIdReplace` FROM `Spawns{}` ORDER BY `SpnId`",
            serverid_val
        );
        sqlx::query(&query).fetch_all(pool).await
    });

    let rows = match result {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[mob] spawn read error: {}", e);
            return 0;
        }
    };

    let mut mstr = 0i32;
    for row in &rows {
        use sqlx::Row;
        // All Spawns columns are int(10) unsigned → read as u32, cast to dest type
        let startm:     c_ushort = row.try_get::<u32, _>(0).unwrap_or(0) as c_ushort;
        let startx:     c_ushort = row.try_get::<u32, _>(1).unwrap_or(0) as c_ushort;
        let starty:     c_ushort = row.try_get::<u32, _>(2).unwrap_or(0) as c_ushort;
        let mobid:      c_uint   = row.try_get::<u32, _>(3).unwrap_or(0);
        let last_death: c_uint   = row.try_get::<u32, _>(4).unwrap_or(0);
        let spn_id:     c_uint   = row.try_get::<u32, _>(5).unwrap_or(0);
        let start:      c_schar  = row.try_get::<u32, _>(6).unwrap_or(25) as c_schar;
        let end:        c_schar  = row.try_get::<u32, _>(7).unwrap_or(25) as c_schar;
        let replace:    c_uint   = row.try_get::<u32, _>(8).unwrap_or(0);

        let db = map_id2mob(spn_id);
        let (db, checkspawn) = if db.is_null() {
            let p = libc::calloc(1, std::mem::size_of::<MobSpawnData>()) as *mut MobSpawnData;
            (p, true)
        } else {
            map_delblock(&mut (*db).bl);
            map_deliddb(&mut (*db).bl);
            (db, false)
        };

        if db.is_null() { continue; }

        if (*db).exp == 0 { (*db).exp = mobdb_experience(mobid); }

        (*db).id           = spn_id;
        (*db).bl.bl_type   = BL_MOB as c_uchar;
        (*db).startm       = startm;
        (*db).startx       = startx;
        (*db).starty       = starty;
        (*db).mobid        = mobid;
        (*db).start        = start;
        (*db).end          = end;
        (*db).replace      = replace;
        (*db).last_death   = last_death;
        (*db).bl.prev      = std::ptr::null_mut();
        (*db).bl.next      = std::ptr::null_mut();
        (*db).onetime      = 0;

        if (*db).bl.id < MOB_START_NUM {
            let new_id = mob_get_new_id();
            MAX_NORMAL_ID    = new_id;
            (*db).bl.m       = startm;
            (*db).bl.x       = startx;
            (*db).bl.y       = starty;
            (*db).bl.id      = new_id;
            mob_respawn_getstats(db);
        }

        if checkspawn { (*db).state = MOB_DEAD; }

        if ffi_map_is_loaded((*db).bl.m) {
            let map_slot = ffi_get_map_ptr((*db).bl.m);
            let xs = (*map_slot).xs;
            let ys = (*map_slot).ys;
            if (*db).bl.x >= xs { (*db).bl.x = xs - 1; }
            if (*db).bl.y >= ys { (*db).bl.y = ys - 1; }
        }

        map_addblock(&mut (*db).bl);
        map_addiddb(&mut (*db).bl);
        mstr += 1;
    }

    MOB_SPAWN_MAX = MOB_ID;
    libc::srand(gettick());
    println!("[mob] [spawn] read done count={}", mstr);
    0
}

// Stubs — unused in this server but keep C callers happy
#[no_mangle]
pub unsafe extern "C" fn mobspawn2_read() -> c_int { 0 }
#[no_mangle]
pub unsafe extern "C" fn mobspeech_read() -> c_int { 0 }

// ─── Magic timer functions ────────────────────────────────────────────────────

#[cfg(not(test))]
pub unsafe fn mob_duratimer(mob: *mut MobSpawnData) -> c_int {
    if mob.is_null() { return 0; }
    for x in 0..MAX_MAGIC_TIMERS {
        let id = (*mob).da[x].id as c_int;
        if id <= 0 { continue; }

        let tbl = if (*mob).da[x].caster_id > 0 {
            map_id2bl((*mob).da[x].caster_id)
        } else {
            std::ptr::null_mut()
        };

        if (*mob).da[x].duration > 0 {
            (*mob).da[x].duration -= 1000;

            if !tbl.is_null() {
                let health: i64 = if (*tbl).bl_type as c_int == BL_MOB {
                    let tmob = tbl as *mut MobSpawnData;
                    (*tmob).current_vita as i64
                } else { 0 };
                if health > 0 || (*tbl).bl_type as c_int == BL_PC {
                    sl_doscript_blargs(magicdb_yname(id), c"while_cast".as_ptr(),
                        2, &raw mut (*mob).bl, tbl);
                }
            } else {
                sl_doscript_blargs(magicdb_yname(id), c"while_cast".as_ptr(),
                    1, &raw mut (*mob).bl);
            }

            if (*mob).da[x].duration <= 0 {
                (*mob).da[x].duration  = 0;
                (*mob).da[x].id        = 0;
                (*mob).da[x].caster_id = 0;
                map_foreachinarea(clif_sendanimation, (*mob).bl.m as c_int,
                    (*mob).bl.x as c_int, (*mob).bl.y as c_int, AREA,
                    BL_PC, (*mob).da[x].animation as c_int, &raw mut (*mob).bl, -1i32);
                (*mob).da[x].animation = 0;
                if !tbl.is_null() {
                    sl_doscript_blargs(magicdb_yname(id), c"uncast".as_ptr(),
                        2, &raw mut (*mob).bl, tbl);
                } else {
                    sl_doscript_blargs(magicdb_yname(id), c"uncast".as_ptr(),
                        1, &raw mut (*mob).bl);
                }
            }
        }
    }
    0
}

/// Common body for the 250 / 500 / 1500 ms timers (no expire logic).
#[cfg(not(test))]
unsafe fn dura_tick(mob: *mut MobSpawnData, event: *const c_char) {
    for x in 0..MAX_MAGIC_TIMERS {
        let id = (*mob).da[x].id as c_int;
        if id <= 0 { continue; }
        let tbl = if (*mob).da[x].caster_id > 0 {
            map_id2bl((*mob).da[x].caster_id)
        } else {
            std::ptr::null_mut()
        };
        if (*mob).da[x].duration > 0 {
            if !tbl.is_null() {
                let health: i64 = if (*tbl).bl_type as c_int == BL_MOB {
                    let tmob = tbl as *mut MobSpawnData;
                    (*tmob).current_vita as i64
                } else { 0 };
                if health > 0 || (*tbl).bl_type as c_int == BL_PC {
                    sl_doscript_blargs(magicdb_yname(id), event, 2, &raw mut (*mob).bl, tbl);
                }
            } else {
                sl_doscript_blargs(magicdb_yname(id), event, 1, &raw mut (*mob).bl);
            }
        }
    }
}

#[cfg(not(test))]
pub unsafe fn mob_secondduratimer(mob: *mut MobSpawnData) -> c_int {
    if mob.is_null() { return 0; }
    dura_tick(mob, c"while_cast_250".as_ptr());
    0
}

#[cfg(not(test))]
pub unsafe fn mob_thirdduratimer(mob: *mut MobSpawnData) -> c_int {
    if mob.is_null() { return 0; }
    dura_tick(mob, c"while_cast_500".as_ptr());
    0
}

#[cfg(not(test))]
pub unsafe fn mob_fourthduratimer(mob: *mut MobSpawnData) -> c_int {
    if mob.is_null() { return 0; }
    dura_tick(mob, c"while_cast_1500".as_ptr());
    0
}

#[cfg(not(test))]
pub unsafe fn mob_flushmagic(mob: *mut MobSpawnData) -> c_int {
    for x in 0..MAX_MAGIC_TIMERS {
        let id = (*mob).da[x].id as c_int;
        if id <= 0 { continue; }
        (*mob).da[x].duration  = 0;
        (*mob).da[x].id        = 0;
        (*mob).da[x].caster_id = 0;
        map_foreachinarea(clif_sendanimation, (*mob).bl.m as c_int,
            (*mob).bl.x as c_int, (*mob).bl.y as c_int, AREA,
            BL_PC, (*mob).da[x].animation as c_int, &raw mut (*mob).bl, -1i32);
        (*mob).da[x].animation = 0;
        // Note: caster_id is already 0 here; map_id2bl(0) returns NULL.
        // Porting C behavior faithfully (C bug: checks stale zeroed field).
        let bl = if (*mob).da[x].caster_id != (*mob).bl.id {
            map_id2bl((*mob).da[x].caster_id)
        } else {
            std::ptr::null_mut()
        };
        if !bl.is_null() {
            sl_doscript_blargs(magicdb_yname(id), c"uncast".as_ptr(),
                2, &raw mut (*mob).bl, bl);
        } else {
            sl_doscript_blargs(magicdb_yname(id), c"uncast".as_ptr(),
                1, &raw mut (*mob).bl);
        }
    }
    0
}

// ─── Main 50ms tick ──────────────────────────────────────────────────────────

// ─── Respawn functions ────────────────────────────────────────────────────────

#[cfg(not(test))]
pub unsafe fn mob_calcstat(mob: *mut MobSpawnData) -> c_int {
    if mob.is_null() || (*mob).data.is_null() { return 0; }
    let d = &*(*mob).data;
    (*mob).maxvita   = d.vita as c_uint;
    (*mob).maxmana   = d.mana as c_uint;
    (*mob).ac        = d.baseac;
    if (*mob).ac < -95 { (*mob).ac = -95; }
    (*mob).miss      = d.miss;
    (*mob).newmove   = d.movetime as c_uint;
    (*mob).newatk    = d.atktime as c_uint;
    (*mob).hit       = d.hit;
    (*mob).mindam    = d.mindam;
    (*mob).maxdam    = d.maxdam;
    (*mob).might     = d.might;
    (*mob).grace     = d.grace;
    (*mob).will      = d.will;
    (*mob).block     = d.block;
    (*mob).protection = d.protection;
    (*mob).charstate = d.state;
    (*mob).clone     = 0;
    (*mob).paralyzed = 0;
    (*mob).blind     = 0;
    (*mob).confused  = 0;
    (*mob).snare     = 0;
    (*mob).sleep     = 1.0;
    (*mob).deduction = 1.0;
    (*mob).crit      = 0;
    (*mob).critmult  = 0;
    (*mob).invis     = 1.0;
    (*mob).amnesia   = 0;

    if (*mob).state != MOB_DEAD {
        for x in 0..MAX_MAGIC_TIMERS {
            let p = &(*mob).da[x];
            let id = p.id as c_int;
            if id > 0 && p.duration > 0 {
                let tsd = map_id2sd(p.caster_id) as *mut BlockList;
                if !tsd.is_null() {
                    sl_doscript_blargs(magicdb_yname(id), c"recast".as_ptr(),
                        2, &raw mut (*mob).bl, tsd);
                } else {
                    sl_doscript_simple(magicdb_yname(id), c"recast".as_ptr(),
                        &raw mut (*mob).bl);
                }
            }
        }
    }
    0
}

#[cfg(not(test))]
pub unsafe fn mob_respawn_nousers(mob: *mut MobSpawnData) -> c_int {
    if (*mob).bl.m != (*mob).startm {
        mob_warp(mob, (*mob).startm as c_int, (*mob).startx as c_int, (*mob).starty as c_int);
    } else {
        map_moveblock(&mut (*mob).bl, (*mob).startx as c_int, (*mob).starty as c_int);
    }
    (*mob).state = MOB_ALIVE;
    mob_respawn_getstats(mob);
    sl_doscript_blargs(c"on_spawn".as_ptr(), std::ptr::null(), 1, &raw mut (*mob).bl);
    if !(*mob).data.is_null() {
        sl_doscript_blargs((*(*mob).data).yname.as_ptr(), c"on_spawn".as_ptr(),
            1, &raw mut (*mob).bl);
    }
    0
}

#[cfg(not(test))]
pub unsafe fn mob_respawn(mob: *mut MobSpawnData) -> c_int {
    if (*mob).bl.m != (*mob).startm {
        mob_warp(mob, (*mob).startm as c_int, (*mob).startx as c_int, (*mob).starty as c_int);
    } else {
        map_moveblock(&mut (*mob).bl, (*mob).startx as c_int, (*mob).starty as c_int);
    }
    (*mob).state = MOB_ALIVE;
    mob_respawn_getstats(mob);
    if !(*mob).data.is_null() {
        let d = &*(*mob).data;
        if d.mobtype == 1 {
            map_foreachinarea(clif_cmoblook_sub,
                (*mob).bl.m as c_int, (*mob).bl.x as c_int, (*mob).bl.y as c_int,
                AREA, BL_PC, LOOK_SEND, mob as *mut _);
        } else {
            map_foreachinarea(clif_mob_look_start_func,
                (*mob).bl.m as c_int, (*mob).bl.x as c_int, (*mob).bl.y as c_int,
                AREA, BL_PC);
            map_foreachinarea(clif_object_look_sub,
                (*mob).bl.m as c_int, (*mob).bl.x as c_int, (*mob).bl.y as c_int,
                AREA, BL_PC, LOOK_SEND, &raw mut (*mob).bl);
            map_foreachinarea(clif_mob_look_close_func,
                (*mob).bl.m as c_int, (*mob).bl.x as c_int, (*mob).bl.y as c_int,
                AREA, BL_PC);
        }
    }
    sl_doscript_blargs(c"on_spawn".as_ptr(), std::ptr::null(), 1, &raw mut (*mob).bl);
    if !(*mob).data.is_null() {
        sl_doscript_blargs((*(*mob).data).yname.as_ptr(), c"on_spawn".as_ptr(),
            1, &raw mut (*mob).bl);
    }
    0
}

// mob_warp forward-declared here; full body follows in the movement section.
#[cfg(not(test))]
pub unsafe fn mob_warp(mob: *mut MobSpawnData, m: c_int, x: c_int, y: c_int) -> c_int {
    if mob.is_null() { return 0; }
    if ((*mob).bl.id) < MOB_START_NUM || ((*mob).bl.id) >= NPC_START_NUM { return 0; }
    map_delblock(&mut (*mob).bl);
    clif_lookgone(&mut (*mob).bl);
    (*mob).bl.m      = m as c_ushort;
    (*mob).bl.x      = x as c_ushort;
    (*mob).bl.y      = y as c_ushort;
    (*mob).bl.bl_type = BL_MOB as c_uchar;
    if map_addblock(&mut (*mob).bl) != 0 {
        eprintln!("Error warping mob.");
    }
    if !(*mob).data.is_null() && (*(*mob).data).mobtype == 1 {
        map_foreachinarea(clif_cmoblook_sub,
            (*mob).bl.m as c_int, (*mob).bl.x as c_int, (*mob).bl.y as c_int,
            AREA, BL_PC, LOOK_SEND, mob as *mut _);
    } else {
        map_foreachinarea(clif_object_look_sub2,
            (*mob).bl.m as c_int, (*mob).bl.x as c_int, (*mob).bl.y as c_int,
            AREA, BL_PC, LOOK_SEND, mob as *mut _);
    }
    0
}

pub unsafe fn kill_mob(mob: *mut MobSpawnData) -> c_int {
    #[cfg(not(test))]
    {
        clif_mob_kill(mob);
        mob_flushmagic(mob);
    }
    0
}

// ─── AI state machine ─────────────────────────────────────────────────────────

#[cfg(not(test))]
pub unsafe fn mob_handle_sub(mob: *mut MobSpawnData) {
    if mob.is_null() { return; }
    let sptime = libc::time(std::ptr::null_mut()) as u32;

    if in_spawn_window(mob) {
        let data = (*mob).data.as_ref();
        let spawn_delay = data.map_or(0, |d| d.spawntime as u32);
        if (*mob).last_death + spawn_delay <= sptime {
            (*mob).spawncheck = 0;
            if (*mob).state == MOB_DEAD && (*mob).onetime == 0 {
                (*mob).target   = 0;
                (*mob).attacker = 0;
                let has_users = ffi_map_is_loaded((*mob).bl.m)
                    && (*ffi_get_map_ptr((*mob).bl.m)).user > 0;
                if has_users { mob_respawn(mob); } else { mob_respawn_nousers(mob); }
            }
        }
    }

    if (*mob).data.as_ref().map_or(0, |d| d.r#type) >= 2 { return; }

    let has_users = ffi_map_is_loaded((*mob).bl.m)
        && (*ffi_get_map_ptr((*mob).bl.m)).user > 0;
    let subtype2 = (*mob).data.as_ref().map_or(0, |d| d.subtype);

    if !has_users && (*mob).onetime != 0 && subtype2 != 2 {
        if (*mob).state != MOB_DEAD { return; }
    }
    if !has_users && (*mob).onetime == 0 && subtype2 != 4 {
        if (*mob).state != MOB_DEAD { return; }
    }

    (*mob).time_ = (*mob).time_.wrapping_add(50);

    match (*mob).state {
        MOB_DEAD => {
            if (*mob).onetime != 0 {
                map_delblock(&mut (*mob).bl);
                map_deliddb(&mut (*mob).bl);
                free_onetime(mob);
            }
        }
        MOB_ALIVE => {
            let data = if (*mob).data.is_null() { return; } else { &*(*mob).data };
            if ((*mob).time_ >= data.movetime && (*mob).time_ >= (*mob).newmove as c_int)
                || ((*mob).newmove > 0 && (*mob).time_ >= (*mob).newmove as c_int)
            {
                if data.r#type >= 2 { return; }
                if data.r#type == 1 && (*mob).target == 0 {
                    map_foreachinarea(mob_find_target,
                        (*mob).bl.m as c_int, (*mob).bl.x as c_int, (*mob).bl.y as c_int,
                        AREA, BL_PC, mob as *mut _);
                }
                let bl = mob_resolve_target(mob);
                (*mob).time_ = 0;
                dispatch_ai(mob, bl, c"move".as_ptr());
            }
        }
        MOB_HIT => {
            let data = if (*mob).data.is_null() { return; } else { &*(*mob).data };
            if ((*mob).time_ >= data.atktime && (*mob).time_ >= (*mob).newatk as c_int)
                || ((*mob).newatk > 0 && (*mob).time_ >= (*mob).newatk as c_int)
            {
                if data.r#type >= 2 { return; }
                let bl = mob_resolve_target(mob);
                if bl.is_null() {
                    (*mob).target   = 0;
                    (*mob).attacker = 0;
                    (*mob).state    = MOB_ALIVE;
                    return;
                }
                if (*bl).m != (*mob).bl.m {
                    (*mob).target   = 0;
                    (*mob).attacker = 0;
                    (*mob).state    = MOB_ALIVE;
                    return;
                }
                (*mob).time_ = 0;
                dispatch_ai(mob, bl, c"attack".as_ptr());
            }
        }
        MOB_ESCAPE => {
            let data = if (*mob).data.is_null() { return; } else { &*(*mob).data };
            if ((*mob).time_ >= data.movetime && (*mob).time_ >= (*mob).newmove as c_int)
                || ((*mob).newmove > 0 && (*mob).time_ >= (*mob).newmove as c_int)
            {
                if data.r#type >= 2 { return; }
                if data.r#type == 1 && (*mob).target == 0 {
                    map_foreachinarea(mob_find_target,
                        (*mob).bl.m as c_int, (*mob).bl.x as c_int, (*mob).bl.y as c_int,
                        AREA, BL_PC, mob as *mut _);
                }
                let bl = mob_resolve_target(mob);
                (*mob).time_ = 0;
                dispatch_ai(mob, bl, c"escape".as_ptr());
            }
        }
        _ => {}
    }
}

/// Resolves mob->target to a block_list*. Clears target if dead/invalid.
#[cfg(not(test))]
unsafe fn mob_resolve_target(mob: *mut MobSpawnData) -> *mut BlockList {
    let bl = map_id2bl((*mob).target);
    if bl.is_null() {
        (*mob).target   = 0;
        (*mob).attacker = 0;
        return std::ptr::null_mut();
    }
    if (*bl).m != (*mob).bl.m {
        (*mob).target   = 0;
        (*mob).attacker = 0;
        return std::ptr::null_mut();
    }
    if (*bl).bl_type as c_int == BL_MOB {
        let tmob = bl as *mut MobSpawnData;
        if (*tmob).state == MOB_DEAD {
            (*mob).target   = 0;
            (*mob).attacker = 0;
            return std::ptr::null_mut();
        }
    }
    bl
}

/// Dispatches to the correct Lua AI script based on mob subtype.
#[cfg(not(test))]
unsafe fn dispatch_ai(mob: *mut MobSpawnData, bl: *mut BlockList, event: *const c_char) {
    let data = if (*mob).data.is_null() { return; } else { &*(*mob).data };
    let script: *const c_char = match data.subtype {
        0 => c"mob_ai_basic".as_ptr(),
        1 => c"mob_ai_normal".as_ptr(),
        2 => c"mob_ai_hard".as_ptr(),
        3 => c"mob_ai_boss".as_ptr(),
        4 => data.yname.as_ptr(),
        5 => c"mob_ai_ghost".as_ptr(),
        _ => return,
    };
    sl_doscript_blargs(script, event, 2, &raw mut (*mob).bl, bl);
}

// ─── mob_trap_look (va_list callback) ────────────────────────────────────────

/// va_list callback: activates NPC trap if mob steps on its cell.
/// Stays as Rust FFI export so C `map_foreachincell` can call it.
#[cfg(not(test))]
pub unsafe extern "C" fn mob_trap_look_ffi(bl: *mut BlockList, mut ap: ...) -> c_int {
    use crate::game::npc::NpcData;
    // Only FLOOR (subtype==1) or sub-2 NPCs are traps
    if (*bl).subtype != FLOOR && (*bl).subtype != 2 { return 0; }
    if bl.is_null() { return 0; }
    let nd = bl as *mut NpcData;
    let mob    = ap.arg::<*mut MobSpawnData>();
    let type_  = ap.arg::<c_int>();
    let def    = ap.arg::<*mut c_int>();
    if !def.is_null() && *def != 0 { return 0; }
    if type_ != 0 && (*bl).subtype == 2 {
        // skip sub-2 NPCs when type_ is non-zero
    } else {
        if !def.is_null() { *def = 1; }
        sl_doscript_blargs((*nd).name.as_ptr(), c"click".as_ptr(),
            2, &raw mut (*mob).bl, &raw mut (*nd).bl);
    }
    0
}

/// Called every 50ms by the timer system.
#[cfg(not(test))]
pub unsafe fn mob_timer_spawns(_id: c_int, _n: c_int) -> c_int {
    TIMERCHECK = TIMERCHECK.wrapping_add(1);

    if MOB_SPAWN_START != MOB_SPAWN_MAX {
        let mut x = MOB_SPAWN_START;
        while x < MOB_SPAWN_MAX {
            let mob = map_id2mob(x);
            if !mob.is_null() { tick_mob(mob); }
            x += 1;
        }
    }

    if MOB_ONETIME_START != MOB_ONETIME_MAX {
        let mut x = MOB_ONETIME_START;
        while x < MOB_ONETIME_MAX {
            let mob = map_id2mob(x);
            if !mob.is_null() { tick_mob(mob); }
            x += 1;
        }
    }

    if TIMERCHECK >= 30 { TIMERCHECK = 0; }
    0
}

#[cfg(not(test))]
unsafe fn tick_mob(mob: *mut MobSpawnData) {
    let tc = TIMERCHECK;
    if tc % 5  == 0 { mob_secondduratimer(mob); }
    if tc % 10 == 0 { mob_thirdduratimer(mob); }
    if tc % 30 == 0 { mob_fourthduratimer(mob); }
    if tc % 20 == 0 { mob_duratimer(mob); }
    mob_handle_sub(mob);
}

// ─── Movement functions ───────────────────────────────────────────────────────

/// Shared warp-tile check used by all three move_mob variants.
#[cfg(not(test))]
unsafe fn warp_at(slot: *mut crate::database::map_db::MapData, dx: c_int, dy: c_int) -> bool {
    let bxs  = (*slot).bxs as usize;
    let xs   = (*slot).xs as usize;
    let ys   = (*slot).ys as usize;
    let dx   = dx as usize;
    let dy   = dy as usize;
    if dx >= xs || dy >= ys { return false; }
    let idx = dx / BLOCK_SIZE + (dy / BLOCK_SIZE) * bxs;
    let warp: *mut WarpList = *(*slot).warp.add(idx);
    let mut w = warp;
    while !w.is_null() {
        if (*w).x == dx as c_int && (*w).y == dy as c_int { return true; }
        w = (*w).next;
    }
    false
}

/// Compute viewport delta strip for a one-step move in `direction`.
/// Returns `(x0, y0, x1, y1, dx, dy, nothingnew)`.
#[cfg(not(test))]
unsafe fn viewport_delta(
    mob: *const MobSpawnData,
    slot: *mut crate::database::map_db::MapData,
) -> (c_int, c_int, c_int, c_int, c_int, c_int, bool) {
    let backx  = (*mob).bl.x as c_int;
    let backy  = (*mob).bl.y as c_int;
    let xs     = (*slot).xs as c_int;
    let ys     = (*slot).ys as c_int;
    let (mut x0, mut y0) = (backx, backy);
    let (mut x1, mut y1) = (0, 0);
    let mut dx = backx;
    let mut dy = backy;
    let mut nothingnew = false;

    match (*mob).side {
        0 => { // UP
            if backy > 0 {
                dy = backy - 1;
                x0 -= 9;  if x0 < 0 { x0 = 0; }
                y0 -= 9;  y1 = 1;  x1 = 19;
                if y0 < 7 { nothingnew = true; }
                if y0 == 7 { y1 += 7; y0 = 0; }
                if x0 + 19 + 9 >= xs { x1 += 9 - ((x0 + 19 + 9) - xs); }
                if x0 <= 8 { x1 += x0; x0 = 0; }
            }
        }
        1 => { // Right
            if backx < xs {
                x0 += 10;  y0 -= 8;  if y0 < 0 { y0 = 0; }
                dx = backx + 1;  y1 = 17;  x1 = 1;
                if x0 > xs - 9 { nothingnew = true; }
                if x0 == xs - 9 { x1 += 9; }
                if y0 + 17 + 8 >= ys { y1 += 8 - ((y0 + 17 + 8) - ys); }
                if y0 <= 7 { y1 += y0; y0 = 0; }
            }
        }
        2 => { // Down
            if backy < ys {
                x0 -= 9;  if x0 < 0 { x0 = 0; }
                y0 += 9;  dy = backy + 1;  y1 = 1;  x1 = 19;
                if y0 + 8 > ys { nothingnew = true; }
                if y0 + 8 == ys { y1 += 8; }
                if x0 + 19 + 9 >= xs { x1 += 9 - ((x0 + 19 + 9) - xs); }
                if x0 <= 8 { x1 += x0; x0 = 0; }
            }
        }
        3 => { // Left
            if backx > 0 {
                x0 -= 10;  y0 -= 8;  if y0 < 0 { y0 = 0; }
                y1 = 17;  x1 = 1;  dx = backx - 1;
                if x0 < 8 { nothingnew = true; }
                if x0 == 8 { x0 = 0; x1 += 8; }
                if y0 + 17 + 8 >= ys { y1 += 8 - ((y0 + 17 + 8) - ys); }
                if y0 <= 7 { y1 += y0; y0 = 0; }
            }
        }
        _ => {}
    }
    (x0, y0, x1, y1, dx, dy, nothingnew)
}

/// Shared post-move broadcast used by move_mob variants.
#[cfg(not(test))]
unsafe fn broadcast_move(mob: *mut MobSpawnData, x0: c_int, y0: c_int, x1: c_int, y1: c_int, nothingnew: bool) {
    let m = (*mob).bl.m as c_int;
    let mut subt = [0i32; 1];
    if !nothingnew {
        if !(*mob).data.is_null() && (*(*mob).data).mobtype == 1 {
            map_foreachinblock(clif_cmoblook_sub,
                m, x0, y0, x0 + x1 - 1, y0 + y1 - 1,
                BL_PC, LOOK_SEND, mob as *mut _);
        } else {
            map_foreachinblock(clif_mob_look_start_func,
                m, x0, y0, x0 + x1 - 1, y0 + y1 - 1, BL_PC, mob as *mut _);
            map_foreachinblock(clif_object_look_sub,
                m, x0, y0, x0 + x1 - 1, y0 + y1 - 1,
                BL_PC, LOOK_SEND, &raw mut (*mob).bl);
            map_foreachinblock(clif_mob_look_close_func,
                m, x0, y0, x0 + x1 - 1, y0 + y1 - 1, BL_PC, mob as *mut _);
        }
    }
    map_foreachincell(mob_trap_look_ffi,
        m, (*mob).bl.x as c_int, (*mob).bl.y as c_int,
        BL_NPC, mob as *mut _, 0i32, subt.as_mut_ptr());
    map_foreachinarea(clif_mob_move,
        m, (*mob).bl.x as c_int, (*mob).bl.y as c_int,
        AREA, BL_PC, LOOK_SEND, mob as *mut _);
}

#[cfg(not(test))]
pub unsafe fn move_mob(mob: *mut MobSpawnData) -> c_int {
    let m       = (*mob).bl.m as c_int;
    let backx   = (*mob).bl.x as c_int;
    let backy   = (*mob).bl.y as c_int;
    let slot = ffi_get_map_ptr((*mob).bl.m);
    if slot.is_null() { return 0; }

    let (x0, y0, x1, y1, mut dx, mut dy, nothingnew) = viewport_delta(mob, slot);

    let xs   = (*slot).xs as c_int;
    let ys   = (*slot).ys as c_int;

    if dx >= xs { dx = xs - 1; }
    if dy >= ys { dy = ys - 1; }

    if warp_at(slot, dx, dy) { return 0; }

    map_foreachincell(mob_move, m, dx, dy, BL_MOB, mob as *mut _);
    map_foreachincell(mob_move, m, dx, dy, BL_PC,  mob as *mut _);
    map_foreachincell(mob_move, m, dx, dy, BL_NPC, mob as *mut _);

    if clif_object_canmove(m, dx, dy, (*mob).side) != 0 { (*mob).canmove = 0; return 0; }
    if clif_object_canmove_from(m, backx, backy, (*mob).side) != 0 { (*mob).canmove = 0; return 0; }
    if map_canmove(m, dx, dy) == 1 || (*mob).canmove == 1 { (*mob).canmove = 0; return 0; }

    // clamp after collision checks
    let dx = if dx >= xs { backx } else if dx < 0 { backx } else { dx };
    let dy = if dy >= ys { backy } else if dy < 0 { backy } else { dy };

    if dx != backx || dy != backy {
        (*mob).bx  = backx as c_ushort;
        (*mob).by_ = backy as c_ushort;
        map_moveblock(&mut (*mob).bl, dx, dy);
        broadcast_move(mob, x0, y0, x1, y1, nothingnew);
        return 1;
    }
    0
}

#[cfg(not(test))]
pub unsafe fn move_mob_ignore_object(mob: *mut MobSpawnData) -> c_int {
    let m       = (*mob).bl.m as c_int;
    let backx   = (*mob).bl.x as c_int;
    let backy   = (*mob).bl.y as c_int;
    let slot = ffi_get_map_ptr((*mob).bl.m);
    if slot.is_null() { return 0; }

    let (x0, y0, x1, y1, mut dx, mut dy, nothingnew) = viewport_delta(mob, slot);
    let xs   = (*slot).xs as c_int;
    let ys   = (*slot).ys as c_int;
    if dx >= xs { dx = xs - 1; }
    if dy >= ys { dy = ys - 1; }
    if warp_at(slot, dx, dy) { return 0; }

    // No collision callbacks — ignore objects
    if clif_object_canmove(m, dx, dy, (*mob).side) != 0 { (*mob).canmove = 0; return 0; }
    if clif_object_canmove_from(m, backx, backy, (*mob).side) != 0 { (*mob).canmove = 0; return 0; }

    let dx = if dx >= xs { backx } else if dx < 0 { backx } else { dx };
    let dy = if dy >= ys { backy } else if dy < 0 { backy } else { dy };

    if dx != backx || dy != backy {
        (*mob).bx  = backx as c_ushort;
        (*mob).by_ = backy as c_ushort;
        map_moveblock(&mut (*mob).bl, dx, dy);
        broadcast_move(mob, x0, y0, x1, y1, nothingnew);
        return 1;
    }
    0
}

#[cfg(not(test))]
pub unsafe fn moveghost_mob(mob: *mut MobSpawnData) -> c_int {
    let m       = (*mob).bl.m as c_int;
    let backx   = (*mob).bl.x as c_int;
    let backy   = (*mob).bl.y as c_int;
    let slot = ffi_get_map_ptr((*mob).bl.m);
    if slot.is_null() { return 0; }

    let (x0, y0, x1, y1, mut dx, mut dy, nothingnew) = viewport_delta(mob, slot);
    let xs   = (*slot).xs as c_int;
    let ys   = (*slot).ys as c_int;
    if dx >= xs { dx = xs - 1; }
    if dy >= ys { dy = ys - 1; }
    if warp_at(slot, dx, dy) { return 0; }

    map_foreachincell(mob_move, m, dx, dy, BL_MOB, mob as *mut _);
    map_foreachincell(mob_move, m, dx, dy, BL_PC,  mob as *mut _);
    map_foreachincell(mob_move, m, dx, dy, BL_NPC, mob as *mut _);

    // Collision checks only apply when mob has no target
    if (*mob).target == 0 {
        if clif_object_canmove(m, dx, dy, (*mob).side) != 0 { (*mob).canmove = 0; return 0; }
        if clif_object_canmove_from(m, backx, backy, (*mob).side) != 0 { (*mob).canmove = 0; return 0; }
        if map_canmove(m, dx, dy) == 1 || (*mob).canmove == 1 { (*mob).canmove = 0; return 0; }
    }

    let dx = if dx >= xs { backx } else if dx < 0 { backx } else { dx };
    let dy = if dy >= ys { backy } else if dy < 0 { backy } else { dy };

    if dx != backx || dy != backy {
        (*mob).bx  = backx as c_ushort;
        (*mob).by_ = backy as c_ushort;
        map_moveblock(&mut (*mob).bl, dx, dy);
        broadcast_move(mob, x0, y0, x1, y1, nothingnew);
        return 1;
    }
    0
}

#[cfg(not(test))]
pub unsafe fn mob_move2(mob: *mut MobSpawnData, x: c_int, y: c_int, side: c_int) -> c_int {
    if (*mob).canmove != 0 { return 1; }
    let m = (*mob).bl.m as c_int;
    (*mob).side = side;
    map_foreachincell(mob_move, m, x, y, BL_MOB, mob as *mut _);
    map_foreachincell(mob_move, m, x, y, BL_PC,  mob as *mut _);
    let cm = (*mob).canmove;
    if map_canmove(m, x, y) == 0 && cm == 0 {
        (*mob).bx    = (*mob).bl.x;
        (*mob).by_   = (*mob).bl.y;
        (*mob).bl.x  = x as c_ushort;
        (*mob).bl.y  = y as c_ushort;
        map_foreachinarea(clif_mob_move,
            m, (*mob).bl.x as c_int, (*mob).bl.y as c_int,
            AREA, BL_PC, LOOK_SEND, mob as *mut _);
        (*mob).canmove = 1;
    } else {
        (*mob).canmove = 0;
        return 0;
    }
    1
}

#[cfg(not(test))]
pub unsafe fn move_mob_intent(mob: *mut MobSpawnData, bl: *mut BlockList) -> c_int {
    if bl.is_null() { return 0; }
    (*mob).canmove = 0;
    let mx = (*mob).bl.x as c_int;
    let my = (*mob).bl.y as c_int;
    let px = (*bl).x as c_int;
    let py = (*bl).y as c_int;
    let ax = (mx - px).abs();
    let ay = (my - py).abs();
    let side = (*mob).side;
    if (ax == 0 && ay == 1) || (ax == 1 && ay == 0) {
        if mx < px { (*mob).side = 1; }
        if mx > px { (*mob).side = 3; }
        if my < py { (*mob).side = 2; }
        if my > py { (*mob).side = 0; }
        if side != (*mob).side { clif_sendmob_side(mob); }
        return 1;
    }
    0
}

// ─── Registry ─────────────────────────────────────────────────────────────────

pub unsafe fn mob_readglobalreg(mob: *mut MobSpawnData, reg: *const c_char) -> c_int {
    if mob.is_null() || reg.is_null() { return 0; }
    for i in 0..MAX_GLOBALMOBREG {
        if libc::strcasecmp((*mob).registry[i].str.as_ptr(), reg) == 0 {
            return (*mob).registry[i].val;
        }
    }
    0
}

pub unsafe fn mob_setglobalreg(mob: *mut MobSpawnData, reg: *const c_char, val: c_int) -> c_int {
    if mob.is_null() || reg.is_null() { return 1; }
    // find existing slot
    for i in 0..MAX_GLOBALMOBREG {
        if libc::strcasecmp((*mob).registry[i].str.as_ptr(), reg) == 0 {
            if val == 0 { libc::strcpy((*mob).registry[i].str.as_mut_ptr(), c"".as_ptr()); }
            (*mob).registry[i].val = val;
            return 0;
        }
    }
    // find empty slot
    for i in 0..MAX_GLOBALMOBREG {
        if libc::strcasecmp((*mob).registry[i].str.as_ptr(), c"".as_ptr()) == 0 {
            libc::strcpy((*mob).registry[i].str.as_mut_ptr(), reg);
            (*mob).registry[i].val = val;
            return 0;
        }
    }
    eprintln!("[mob] mob_setglobalreg: couldn't set {:?}", std::ffi::CStr::from_ptr(reg));
    1
}

// ─── Item / drop helpers ──────────────────────────────────────────────────────

/// va_list callback: sets def[0]=1 on first hit (used as a foreachincell "any-present" test).
pub unsafe extern "C" fn mob_thing_yeah(_bl: *mut BlockList, mut ap: ...) -> c_int {
    let def = ap.arg::<*mut c_int>();
    if !def.is_null() { *def = 1; }
    0
}

#[cfg(not(test))]
pub unsafe fn mobdb_drops(mob: *mut MobSpawnData, sd: *mut std::ffi::c_void) -> c_int {
    // sd->bl is the first field — cast gives the block_list* for sl_doscript_blargs
    sl_doscript_blargs(c"mobDrops".as_ptr(), std::ptr::null(), 2,
        sd as *mut BlockList, &raw mut (*mob).bl);
    for i in 0..MAX_INVENTORY {
        let slot = &(*mob).inventory[i];
        if slot.id != 0 && slot.amount >= 1 {
            mobdb_dropitem(
                (*mob).bl.id, slot.id, slot.amount, slot.dura,
                slot.protected as c_int, slot.owner as c_int,
                (*mob).bl.m as c_int, (*mob).bl.x as c_int, (*mob).bl.y as c_int,
                sd,
            );
            (*mob).inventory[i].id       = 0;
            (*mob).inventory[i].amount   = 0;
            (*mob).inventory[i].owner    = 0;
            (*mob).inventory[i].dura     = 0;
            (*mob).inventory[i].protected = 0;
        }
    }
    0
}

// ─── mobspawn_onetime ─────────────────────────────────────────────────────────

#[cfg(not(test))]
pub unsafe fn mobspawn_onetime(
    id: c_uint, m: c_int, x: c_int, y: c_int,
    times: c_int, start: c_int, end: c_int,
    replace: c_uint, owner: c_uint,
) -> *mut c_uint {
    let spawnedmobs = libc::calloc(times as usize, std::mem::size_of::<c_uint>()) as *mut c_uint;
    for z in 0..times {
        let db = libc::calloc(1, std::mem::size_of::<MobSpawnData>()) as *mut MobSpawnData;
        if db.is_null() { continue; }
        if (*db).exp == 0 { (*db).exp = mobdb_experience(id); }
        (*db).startm   = m as c_ushort;
        (*db).startx   = x as c_ushort;
        (*db).starty   = y as c_ushort;
        (*db).mobid    = id;
        (*db).start    = start as c_schar;
        (*db).end      = end as c_schar;
        (*db).replace  = replace;
        (*db).state    = MOB_DEAD;
        (*db).bl.bl_type = BL_MOB as c_uchar;
        (*db).bl.m     = m as c_ushort;
        (*db).bl.x     = x as c_ushort;
        (*db).bl.y     = y as c_ushort;
        (*db).owner    = owner;
        (*db).onetime  = 1;
        (*db).spawncheck = 0;
        (*db).bl.prev  = std::ptr::null_mut();
        (*db).bl.next  = std::ptr::null_mut();

        loop {
            (*db).bl.id = mob_get_free_id();
            if map_id2bl((*db).bl.id).is_null() { break; }
        }

        *spawnedmobs.add(z as usize) = (*db).bl.id;
        map_addblock(&mut (*db).bl);
        map_addiddb(&mut (*db).bl);

        let has_users = ffi_map_is_loaded((*db).bl.m)
            && (*ffi_get_map_ptr((*db).bl.m)).user > 0;
        if has_users { mob_respawn(db); } else { mob_respawn_nousers(db); }
    }
    spawnedmobs
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn mob_spawn_data_size() {
        const EXPECTED: usize = 61120;
        assert_eq!(
            size_of::<MobSpawnData>(),
            EXPECTED,
            "MobSpawnData size mismatch — check field types and padding"
        );
        println!("MobSpawnData = {} bytes", size_of::<MobSpawnData>());
        println!("SkillInfo    = {} bytes", size_of::<SkillInfo>());
        println!("ThreatTable  = {} bytes", size_of::<ThreatTable>());
        println!("Item         = {} bytes", size_of::<Item>());
        println!("GlobalReg    = {} bytes", size_of::<GlobalReg>());
        println!("GfxViewer    = {} bytes", size_of::<GfxViewer>());
    }
}
