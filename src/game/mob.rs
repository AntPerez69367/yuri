//! Mob game logic.

#![allow(non_snake_case, dead_code)]

use crate::database::map_db::BLOCK_SIZE;
use crate::database::map_db::{GlobalReg, WarpList};
use crate::database::mob_db::MobDbData;
use crate::database::map_db::{get_map_ptr as ffi_get_map_ptr, map_is_loaded as ffi_map_is_loaded};
use crate::game::pc::MapSessionData;
use crate::game::types::GfxViewer;
use crate::common::types::{Item, SkillInfo};
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
pub use crate::common::constants::entity::{BL_PC, BL_MOB, BL_NPC, BL_ITEM};
pub use crate::common::constants::entity::mob::{
    MOB_START_NUM, MOBOT_START_NUM, MAX_MAGIC_TIMERS, MAX_INVENTORY, MAX_GLOBALMOBREG, MAX_THREATCOUNT,
};
pub use crate::common::constants::entity::npc::NPC_START_NUM;
pub use crate::common::constants::entity::item::FLOORITEM_START_NUM;

// mob state constants
pub use crate::common::constants::entity::mob::{MOB_ALIVE, MOB_DEAD, MOB_PARA, MOB_BLIND, MOB_HIT, MOB_ESCAPE};

use crate::common::constants::entity::SUBTYPE_FLOOR;

// ─── ThreatTable ─────────────────────────────────────────────────────────────

/// Mob threat entry: which player and how much threat they have generated.
#[repr(C)]
pub struct ThreatTable {
    pub user: u32,
    pub amount: u32,
}

// ─── MobSpawnData ─────────────────────────────────────────────────────────────

/// Mob spawn data (spawn parameters and state for a single mob instance).
///
/// Field order and types MUST exactly match C. Verify size with:
/// `cargo test mob_spawn_data_size -- --nocapture`
///
/// Layout:
/// ```text
/// offset  field                    size
///      0  (entity header fields)      48
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
    pub id:            u32,
    pub graphic_id:    u32,
    pub graphic_color: u32,
    pub m:             u16,
    pub x:             u16,
    pub y:             u16,
    pub bl_type:       u8,
    pub subtype:       u8,
    pub da: [SkillInfo; MAX_MAGIC_TIMERS],
    pub inventory: [Item; MAX_INVENTORY],
    pub data: *mut MobDbData,
    pub threat: [ThreatTable; MAX_THREATCOUNT],
    pub registry: [GlobalReg; MAX_GLOBALMOBREG],
    pub gfx: GfxViewer,
    pub startm: u16,
    pub startx: u16,
    pub starty: u16,
    pub prev_x: u16,
    pub prev_y: u16,
    pub look: u16,
    pub miss: i16,
    pub protection: i16,
    pub mobid: u32,
    pub current_vita: u32,
    pub current_mana: u32,
    pub target: u32,
    pub attacker: u32,
    pub owner: u32,
    pub confused_target: u32,
    pub timer: u32,
    pub last_death: u32,
    pub rangeTarget: u32,
    pub ranged: u32,
    pub newmove: u32,
    pub newatk: u32,
    pub lastvita: u32,
    pub maxvita: u32,
    pub maxmana: u32,
    pub replace: u32,
    pub mindam: u32,
    pub maxdam: u32,
    pub amnesia: u32,
    pub exp: u32,
    pub ac: i32,
    pub side: i32,
    pub time_: i32,
    pub spawncheck: i32,
    pub num: i32,
    pub crit: i32,
    pub critchance: i32,
    pub critmult: i32,
    pub snare: i32,
    pub lastaction: i32,
    pub hit: i32,
    pub might: i32,
    pub grace: i32,
    pub will: i32,
    pub state: u8,
    pub canmove: u8,
    pub onetime: u8,
    pub paralyzed: u8,
    pub blind: u8,
    pub confused: u8,
    pub summon: u8,
    pub returning: u8,
    pub look_color: u8,
    pub clone: i8,
    pub start: i8,
    pub end: i8,
    pub block: i8,
    pub charstate: i8,
    // compiler inserts 3 bytes of padding here to align f32 to 4 bytes
    pub sleep: f32,
    pub deduction: f32,
    pub damage: f32,
    pub dmgshield: f32,
    pub invis: f32,
    // compiler inserts padding here to align f64 to 8 bytes
    pub dmgdealt: f64,
    pub dmgtaken: f64,
    pub maxdmg: f64,
    pub dmgindtable: [[f64; 2]; MAX_THREATCOUNT],
    pub dmggrptable: [[f64; 2]; MAX_THREATCOUNT],
    pub cursed: u8,
}

// SAFETY: MobSpawnData contains raw pointers to C-managed entities.
// All access is gated behind unsafe blocks.
unsafe impl Send for MobSpawnData {}
unsafe impl Sync for MobSpawnData {}


// ─── Mutable globals ──────────────────────────────────────────────────────────
// Use #[export_name] for uppercase globals to avoid sqlx #[derive(FromRow)]
// let-binding conflicts (see MEMORY.md: "npc_id #[export_name]").
#[export_name = "mob_id"]
pub static MOB_ID: AtomicU32 = AtomicU32::new(MOB_START_NUM);
#[export_name = "max_normal_id"]
pub static MAX_NORMAL_ID: AtomicU32 = AtomicU32::new(MOB_START_NUM);
#[export_name = "cmob_id"]
pub static CMOB_ID: AtomicU32 = AtomicU32::new(0);
#[export_name = "MOB_SPAWN_MAX"]
pub static MOB_SPAWN_MAX: AtomicU32 = AtomicU32::new(MOB_START_NUM);
#[export_name = "MOB_SPAWN_START"]
pub static MOB_SPAWN_START: AtomicU32 = AtomicU32::new(MOB_START_NUM);
#[export_name = "MOB_ONETIME_MAX"]
pub static MOB_ONETIME_MAX: AtomicU32 = AtomicU32::new(MOBOT_START_NUM);
#[export_name = "MOB_ONETIME_START"]
pub static MOB_ONETIME_START: AtomicU32 = AtomicU32::new(MOBOT_START_NUM);
#[export_name = "MIN_TIMER"]
pub static MIN_TIMER: AtomicU32 = AtomicU32::new(1000);
pub static TIMERCHECK: AtomicU8 = AtomicU8::new(0); // internal only



use crate::game::map_server::{
    map_deliddb, map_additem, map_canmove,
};
use crate::game::block::{map_addblock_id, map_delblock_id, map_moveblock_id};
use crate::game::map_parse::combat::{
    clif_send_pc_health, clif_send_mob_health, clif_sendmob_action,
};
use crate::game::map_parse::visual::clif_lookgone_by_id;
use crate::game::map_parse::combat::clif_mob_kill;
use crate::game::map_parse::player_state::clif_sendstatus as clif_sendstatus_mob;
use crate::game::map_parse::movement::{clif_object_canmove, clif_object_canmove_from};
use crate::game::client::visual::clif_sendmob_side;
use std::sync::Arc;
use crate::database::magic_db;
use crate::database::mob_db;

/// Helper: get magic yname as `&str` by spell ID (for sl_doscript calls).
/// The returned `String` is owned; callers can borrow it.
#[inline]
fn magicdb_yname_str(id: i32) -> String {
    let arc = magic_db::search(id);
    crate::game::scripting::carray_to_str(&arc.yname).to_owned()
}

/// Helper: get magic display name pointer by spell ID.
#[inline]
fn magicdb_name(id: i32) -> *const i8 {
    magic_db::search(id).name.as_ptr()
}

/// Helper: get magic dispel threshold by spell ID.
#[inline]
fn magicdb_dispel(id: i32) -> i32 {
    magic_db::search(id).dispell as i32
}

/// Map mob subtype to its AI script root name.
fn ai_script_name(data: &MobDbData) -> &str {
    match data.subtype {
        0 => "mob_ai_basic",
        1 => "mob_ai_normal",
        2 => "mob_ai_hard",
        3 => "mob_ai_boss",
        5 => "mob_ai_ghost",
        _ => crate::game::scripting::carray_to_str(&data.yname),
    }
}
use crate::game::time_util::gettick;
use crate::game::map_server::cur_time;

// groups[256][256] flat array — defined in map_server.rs as 
use crate::game::map_server::groups as groups_mob;

/// Legacy raw-pointer player lookup for deeply unsafe code paths in mob.rs.
fn map_id2sd_mob(id: u32) -> *mut MapSessionData {
    match crate::game::map_server::map_id2sd_pc(id) {
        Some(arc) => {
            let ptr = &mut *arc.write() as *mut MapSessionData;
            ptr
        }
        None => std::ptr::null_mut(),
    }
}

// Import safe block grid traversal API.
use crate::game::block::AreaType;
use crate::game::block_grid;
use crate::game::map_parse::visual::{
    clif_mob_look_start_func_inner, clif_mob_look_close_func_inner,
    clif_object_look_mob, clif_object_look2_mob, clif_object_look2_item,
    clif_cmoblook,
};
use crate::game::map_parse::movement::clif_mob_move_inner;
use crate::game::map_parse::combat::clif_sendanimation_inner;

/// Helper: broadcast animation removal to nearby PCs via block_grid.
unsafe fn broadcast_animation_to_pcs(mob: &MobSpawnData, anim: i32) {
    let m = mob.m as usize;
    if let Some(grid) = block_grid::get_grid(m) {
        let slot = &*ffi_get_map_ptr(mob.m);
        let ids = block_grid::ids_in_area(grid, mob.x as i32, mob.y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
        for id in ids {
            if let Some(sd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                let sd_guard = sd_arc.read();
                clif_sendanimation_inner(sd_guard.fd, sd_guard.player.appearance.setting_flags, anim, mob.id, -1);
            }
        }
    }
}

/// Dispatch a Lua event with a single entity-ID argument.
fn sl_doscript_simple(root: &str, method: Option<&str>, id: u32) -> i32 {
    crate::game::scripting::doscript_blargs_id(root, method, &[id])
}

/// Dispatch a Lua event with two entity-ID arguments.
fn sl_doscript_2(root: &str, method: Option<&str>, id1: u32, id2: u32) -> i32 {
    crate::game::scripting::doscript_blargs_id(root, method, &[id1, id2])
}

// ─── Mob ID management ────────────────────────────────────────────────────────

pub fn mob_get_new_id() -> u32 {
    MOB_ID.fetch_add(1, Ordering::Relaxed)
}

pub unsafe fn mob_get_free_id() -> u32 {
    let mut x = MOB_ONETIME_START.load(Ordering::Relaxed);
    loop {
        if x >= NPC_START_NUM {
            tracing::warn!("[mob] mob_get_free_id: onetime range exhausted");
            return 0;
        }
        let omax = MOB_ONETIME_MAX.load(Ordering::Relaxed);
        if x == omax {
            if omax >= NPC_START_NUM {
                tracing::warn!("[mob] mob_get_free_id: onetime range full");
                return 0;
            }
            MOB_ONETIME_MAX.store(omax + 1, Ordering::Relaxed);
        }
        if crate::game::map_server::entity_position(x).is_none() {
            return x;
        }
        x += 1;
    }
}

pub unsafe fn free_onetime(mob: *mut MobSpawnData) -> i32 {
    if mob.is_null() {
        return 0;
    }
    (*mob).data = std::ptr::null_mut();
    let id = (*mob).id;
    crate::game::map_server::mob_map_remove(id);
    // Box drop handles deallocation — no libc::free needed.
    // The compaction loop exits early when an unoccupied slot is found.
    // It only compacts MOB_ONETIME_MAX when called for the top-of-range mob.
    // compact onetime range downward
    let mut x = MOB_ONETIME_START.load(Ordering::Relaxed);
    loop {
        let omax = MOB_ONETIME_MAX.load(Ordering::Relaxed);
        if x > omax { break; }
        if crate::game::map_server::entity_position(x).is_none() {
            return 0;
        }
        if x == omax {
            map_deliddb(x);
            MOB_ONETIME_MAX.store(omax - 1, Ordering::Relaxed);
        }
        x += 1;
    }
    0
}

// ─── Stat / respawn functions (forward-defined; also used by Task 8) ─────────

unsafe fn in_spawn_window(mob: *const MobSpawnData) -> bool {
    let s = (*mob).start as i32;
    let e = (*mob).end as i32;
    let ct = cur_time.load(Ordering::Relaxed);
    (s < e && ct >= s && ct <= e)
        || (s > e && (ct >= s || ct <= e))
        || (s == e && ct == s && ct == e)
        || (s == 25 && e == 25)
}

pub unsafe fn mob_respawn_getstats(mob: *mut MobSpawnData) -> i32 {
    if mob.is_null() {
        return 0;
    }
    (*mob).data = if in_spawn_window(mob) {
        Arc::as_ptr(&mob_db::search((*mob).mobid)) as *mut MobDbData
    } else if (*mob).replace != 0 {
        Arc::as_ptr(&mob_db::search((*mob).replace)) as *mut MobDbData
    } else {
        Arc::as_ptr(&mob_db::search((*mob).mobid)) as *mut MobDbData
    };
    if (*mob).data.is_null() {
        return 0;
    }
    let d = &*(*mob).data;
    (*mob).maxvita = d.vita as u32;
    (*mob).maxmana = d.mana as u32;
    (*mob).ac = d.baseac;
    if (*mob).ac < -95 {
        (*mob).ac = -95;
    }
    if (*mob).exp == 0 {
        (*mob).exp = mob_db::experience((*mob).mobid);
    }
    (*mob).miss = d.miss;
    (*mob).newmove = d.movetime as u32;
    (*mob).newatk = d.atktime as u32;
    (*mob).current_vita = (*mob).maxvita;
    (*mob).current_mana = (*mob).maxmana;
    (*mob).maxdmg = (*mob).current_vita as f64;
    (*mob).hit = d.hit;
    (*mob).mindam = d.mindam;
    (*mob).maxdam = d.maxdam;
    (*mob).might = d.might;
    (*mob).grace = d.grace;
    (*mob).will = d.will;
    (*mob).block = d.block;
    (*mob).protection = d.protection;
    (*mob).look = d.look as u16;
    (*mob).look_color = d.look_color as u8;
    (*mob).charstate = d.state;
    (*mob).clone = 0;
    (*mob).time_ = 0;
    (*mob).paralyzed = 0;
    (*mob).blind = 0;
    (*mob).confused = 0;
    (*mob).snare = 0;
    (*mob).target = 0;
    (*mob).attacker = 0;
    (*mob).confused_target = 0;
    (*mob).rangeTarget = 0;
    (*mob).dmgshield = 0.0;
    (*mob).sleep = 1.0;
    (*mob).deduction = 1.0;
    (*mob).damage = 0.0;
    (*mob).critchance = 0;
    (*mob).crit = 0;
    (*mob).critmult = 0;
    (*mob).invis = 1.0;
    0
}

// ─── Spawn table loader ───────────────────────────────────────────────────────

use crate::database::get_pool;

async fn mobspawn_fetch(serverid_val: i32) -> Result<Vec<sqlx::mysql::MySqlRow>, sqlx::Error> {
    let pool = get_pool();
    let query = format!(
        "SELECT `SpnMapId`, `SpnX`, `SpnY`, `SpnMobId`, \
         `SpnLastDeath`, `SpnId`, `SpnStartTime`, `SpnEndTime`, \
         `SpnMobIdReplace` FROM `Spawns{}` ORDER BY `SpnId`",
        serverid_val
    );
    sqlx::query(&query).fetch_all(pool).await
}

pub async unsafe fn mobspawn_read() -> i32 {
    let serverid_val = crate::config::config().server_id;
    let result = mobspawn_fetch(serverid_val).await;

    let rows = match result {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("[mob] spawn read error: {}", e);
            return 0;
        }
    };

    let mut mstr = 0i32;
    for row in &rows {
        use sqlx::Row;
        // All Spawns columns are int(10) unsigned → read as u32, cast to dest type
        let startm: u16 = row.try_get::<u32, _>(0).unwrap_or(0) as u16;
        let startx: u16 = row.try_get::<u32, _>(1).unwrap_or(0) as u16;
        let starty: u16 = row.try_get::<u32, _>(2).unwrap_or(0) as u16;
        let mobid: u32 = row.try_get::<u32, _>(3).unwrap_or(0);
        let last_death: u32 = row.try_get::<u32, _>(4).unwrap_or(0);
        let spn_id: u32 = row.try_get::<u32, _>(5).unwrap_or(0);
        let start: i8 = row.try_get::<u32, _>(6).unwrap_or(25) as i8;
        let end: i8 = row.try_get::<u32, _>(7).unwrap_or(25) as i8;
        let replace: u32 = row.try_get::<u32, _>(8).unwrap_or(0);

        let (db, checkspawn, new_box_option) = match crate::game::map_server::map_id2mob_ref(spn_id) {
            Some(existing_arc) => {
                {
                    let existing = existing_arc.read();
                    map_delblock_id(existing.id, existing.m);
                }
                map_deliddb(spn_id);
                // After deliddb the Arc is removed from global map; create fresh box
                let mut new_mob_box: Box<MobSpawnData> = Box::new_zeroed().assume_init();
                let p: *mut MobSpawnData = new_mob_box.as_mut() as *mut MobSpawnData;
                (p, false, Some(new_mob_box))
            }
            None => {
                let mut new_mob_box: Box<MobSpawnData> = Box::new_zeroed().assume_init();
                let p: *mut MobSpawnData = new_mob_box.as_mut() as *mut MobSpawnData;
                (p, true, Some(new_mob_box))
            }
        };

        if (*db).exp == 0 {
            (*db).exp = mob_db::experience(mobid);
        }

        (*db).id = spn_id;
        (*db).bl_type = BL_MOB as u8;
        (*db).startm = startm;
        (*db).startx = startx;
        (*db).starty = starty;
        (*db).mobid = mobid;
        (*db).start = start;
        (*db).end = end;
        (*db).replace = replace;
        (*db).last_death = last_death;
        (*db).onetime = 0;

        if (*db).id < MOB_START_NUM {
            let new_id = mob_get_new_id();
            MAX_NORMAL_ID.store(new_id, Ordering::Relaxed);
            (*db).m = startm;
            (*db).x = startx;
            (*db).y = starty;
            (*db).id = new_id;
            mob_respawn_getstats(db);
        }

        if checkspawn {
            (*db).state = MOB_DEAD;
        }

        if ffi_map_is_loaded((*db).m) {
            let map_slot = ffi_get_map_ptr((*db).m);
            let xs = (*map_slot).xs;
            let ys = (*map_slot).ys;
            if (*db).x >= xs {
                (*db).x = xs - 1;
            }
            if (*db).y >= ys {
                (*db).y = ys - 1;
            }
        }

        // Insert into MOB_MAP first — this moves the Box data into Arc<RwLock>,
        // freeing the Box. After this, `db` is dangling and must not be used.
        let mob_id = (*db).id;
        if let Some(b) = new_box_option {
            crate::game::map_server::map_addiddb_mob(mob_id, b);
        }
        // Get the live pointer from the Arc<RwLock> (not the freed Box).
        let db = crate::game::map_server::map_id2mob_ref(mob_id)
            .expect("mob just inserted").data_ptr();
        map_addblock_id((*db).id, (*db).bl_type, (*db).m, (*db).x, (*db).y);
        mstr += 1;
    }

    MOB_SPAWN_MAX.store(MOB_ID.load(Ordering::Relaxed), Ordering::Relaxed);
    libc::srand(gettick());
    println!("[mob] [spawn] read done count={}", mstr);
    0
}

// Stubs — no active callers
pub unsafe fn mobspawn2_read() -> i32 {
    0
}
pub unsafe fn mobspeech_read() -> i32 {
    0
}

// ─── Magic timer functions ────────────────────────────────────────────────────

pub unsafe fn mob_duratimer(mob: *mut MobSpawnData) -> i32 {
    if mob.is_null() {
        return 0;
    }
    for x in 0..MAX_MAGIC_TIMERS {
        let id = (*mob).da[x].id as i32;
        if id <= 0 {
            continue;
        }

        let caster_id = (*mob).da[x].caster_id;
        // Resolve caster: check if it's a living mob or a PC.
        let caster_info = if caster_id > 0 {
            crate::game::map_server::entity_position(caster_id)
        } else {
            None
        };

        if (*mob).da[x].duration > 0 {
            (*mob).da[x].duration -= 1000;

            let yname = magicdb_yname_str(id);
            if let Some((_pos, bl_type)) = caster_info {
                let health: i64 = if bl_type == BL_MOB as u8 {
                    crate::game::map_server::map_id2mob_ref(caster_id)
                        .map(|arc| arc.read().current_vita as i64)
                        .unwrap_or(0)
                } else {
                    0
                };
                if health > 0 || bl_type == BL_PC as u8 {
                    sl_doscript_2(&yname, Some("while_cast"), (*mob).id, caster_id);
                }
            } else {
                sl_doscript_simple(&yname, Some("while_cast"), (*mob).id);
            }

            if (*mob).da[x].duration <= 0 {
                (*mob).da[x].duration = 0;
                (*mob).da[x].id = 0;
                (*mob).da[x].caster_id = 0;
                broadcast_animation_to_pcs(&*mob, (*mob).da[x].animation as i32);
                (*mob).da[x].animation = 0;
                if caster_info.is_some() {
                    sl_doscript_2(&yname, Some("uncast"), (*mob).id, caster_id);
                } else {
                    sl_doscript_simple(&yname, Some("uncast"), (*mob).id);
                }
            }
        }
    }
    0
}

/// Common body for the 250 / 500 / 1500 ms timers (no expire logic).
unsafe fn dura_tick(mob: *mut MobSpawnData, event: &str) {
    for x in 0..MAX_MAGIC_TIMERS {
        let id = (*mob).da[x].id as i32;
        if id <= 0 {
            continue;
        }
        let caster_id = (*mob).da[x].caster_id;
        let caster_info = if caster_id > 0 {
            crate::game::map_server::entity_position(caster_id)
        } else {
            None
        };
        if (*mob).da[x].duration > 0 {
            let yname = magicdb_yname_str(id);
            if let Some((_pos, bl_type)) = caster_info {
                let health: i64 = if bl_type == BL_MOB as u8 {
                    crate::game::map_server::map_id2mob_ref(caster_id)
                        .map(|arc| arc.read().current_vita as i64)
                        .unwrap_or(0)
                } else {
                    0
                };
                if health > 0 || bl_type == BL_PC as u8 {
                    sl_doscript_2(&yname, Some(event), (*mob).id, caster_id);
                }
            } else {
                sl_doscript_simple(&yname, Some(event), (*mob).id);
            }
        }
    }
}

pub unsafe fn mob_secondduratimer(mob: *mut MobSpawnData) -> i32 {
    if mob.is_null() {
        return 0;
    }
    dura_tick(mob, "while_cast_250");
    0
}

pub unsafe fn mob_thirdduratimer(mob: *mut MobSpawnData) -> i32 {
    if mob.is_null() {
        return 0;
    }
    dura_tick(mob, "while_cast_500");
    0
}

pub unsafe fn mob_fourthduratimer(mob: *mut MobSpawnData) -> i32 {
    if mob.is_null() {
        return 0;
    }
    dura_tick(mob, "while_cast_1500");
    0
}

pub unsafe fn mob_flushmagic(mob: *mut MobSpawnData) -> i32 {
    for x in 0..MAX_MAGIC_TIMERS {
        let id = (*mob).da[x].id as i32;
        if id <= 0 {
            continue;
        }
        (*mob).da[x].duration = 0;
        (*mob).da[x].id = 0;
        (*mob).da[x].caster_id = 0;
        broadcast_animation_to_pcs(&*mob, (*mob).da[x].animation as i32);
        (*mob).da[x].animation = 0;
        // Note: caster_id is already 0 here (cleared above).
        // Porting C behavior faithfully (C bug: checks stale zeroed field).
        let cid = (*mob).da[x].caster_id;
        let has_caster = cid != (*mob).id
            && cid > 0
            && crate::game::map_server::entity_position(cid).is_some();
        let yname = magicdb_yname_str(id);
        if has_caster {
            sl_doscript_2(&yname, Some("uncast"), (*mob).id, cid);
        } else {
            sl_doscript_simple(&yname, Some("uncast"), (*mob).id);
        }
    }
    0
}

// ─── Main 50ms tick ──────────────────────────────────────────────────────────

// ─── Respawn functions ────────────────────────────────────────────────────────

pub unsafe fn mob_calcstat(mob: *mut MobSpawnData) -> i32 {
    if mob.is_null() || (*mob).data.is_null() {
        return 0;
    }
    let d = &*(*mob).data;
    (*mob).maxvita = d.vita as u32;
    (*mob).maxmana = d.mana as u32;
    (*mob).ac = d.baseac;
    if (*mob).ac < -95 {
        (*mob).ac = -95;
    }
    (*mob).miss = d.miss;
    (*mob).newmove = d.movetime as u32;
    (*mob).newatk = d.atktime as u32;
    (*mob).hit = d.hit;
    (*mob).mindam = d.mindam;
    (*mob).maxdam = d.maxdam;
    (*mob).might = d.might;
    (*mob).grace = d.grace;
    (*mob).will = d.will;
    (*mob).block = d.block;
    (*mob).protection = d.protection;
    (*mob).charstate = d.state;
    (*mob).clone = 0;
    (*mob).paralyzed = 0;
    (*mob).blind = 0;
    (*mob).confused = 0;
    (*mob).snare = 0;
    (*mob).sleep = 1.0;
    (*mob).deduction = 1.0;
    (*mob).crit = 0;
    (*mob).critmult = 0;
    (*mob).invis = 1.0;
    (*mob).amnesia = 0;

    if (*mob).state != MOB_DEAD {
        for x in 0..MAX_MAGIC_TIMERS {
            let p = &(*mob).da[x];
            let id = p.id as i32;
            if id > 0 && p.duration > 0 {
                let caster_id = p.caster_id;
                let has_caster = caster_id > 0 && crate::game::map_server::entity_position(caster_id).is_some();
                let yname = magicdb_yname_str(id);
                if has_caster {
                    sl_doscript_2(&yname, Some("recast"), (*mob).id, caster_id);
                } else {
                    sl_doscript_simple(&yname, Some("recast"), (*mob).id);
                }
            }
        }
    }
    0
}

pub unsafe fn mob_respawn_nousers(mob: *mut MobSpawnData) -> i32 {
    if (*mob).m != (*mob).startm {
        mob_warp(
            mob,
            (*mob).startm as i32,
            (*mob).startx as i32,
            (*mob).starty as i32,
        );
    } else {
        let old_x = (*mob).x;
        let old_y = (*mob).y;
        map_moveblock_id((*mob).id, (*mob).m, old_x, old_y, (*mob).startx, (*mob).starty);
        (*mob).x = (*mob).startx;
        (*mob).y = (*mob).starty;
    }
    (*mob).state = MOB_ALIVE;
    mob_respawn_getstats(mob);
    sl_doscript_simple("on_spawn", None, (*mob).id);
    if !(*mob).data.is_null() {
        let yname = crate::game::scripting::carray_to_str(&(*(*mob).data).yname);
        sl_doscript_simple(yname, Some("on_spawn"), (*mob).id);
    }
    0
}

pub unsafe fn mob_respawn(mob: *mut MobSpawnData) -> i32 {
    if (*mob).m != (*mob).startm {
        mob_warp(
            mob,
            (*mob).startm as i32,
            (*mob).startx as i32,
            (*mob).starty as i32,
        );
    } else {
        let old_x = (*mob).x;
        let old_y = (*mob).y;
        map_moveblock_id((*mob).id, (*mob).m, old_x, old_y, (*mob).startx, (*mob).starty);
        (*mob).x = (*mob).startx;
        (*mob).y = (*mob).starty;
    }
    (*mob).state = MOB_ALIVE;
    mob_respawn_getstats(mob);
    if !(*mob).data.is_null() {
        let d = &*(*mob).data;
        if let Some(grid) = block_grid::get_grid((*mob).m as usize) {
            let slot = &*ffi_get_map_ptr((*mob).m);
            let ids = block_grid::ids_in_area(grid, (*mob).x as i32, (*mob).y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
            if d.mobtype == 1 {
                for id in ids {
                    if let Some(sd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                        clif_cmoblook(&*mob, &*sd_arc.read());
                    }
                }
            } else {
                for id in &ids {
                    if let Some(pe) = crate::game::map_server::map_id2sd_pc(*id) {
                        clif_mob_look_start_func_inner(pe.fd, &mut pe.net.write().look);
                    }
                }
                for id in &ids {
                    if let Some(pe) = crate::game::map_server::map_id2sd_pc(*id) {
                        clif_object_look_mob(pe.fd, &mut pe.net.write().look, &*mob);
                    }
                }
                for id in &ids {
                    if let Some(pe) = crate::game::map_server::map_id2sd_pc(*id) {
                        clif_mob_look_close_func_inner(pe.fd, &mut pe.net.write().look);
                    }
                }
            }
        }
    }
    sl_doscript_simple("on_spawn", None, (*mob).id);
    if !(*mob).data.is_null() {
        let yname = crate::game::scripting::carray_to_str(&(*(*mob).data).yname);
        sl_doscript_simple(yname, Some("on_spawn"), (*mob).id);
    }
    0
}

// mob_warp forward-declared here; full body follows in the movement section.
pub unsafe fn mob_warp(mob: *mut MobSpawnData, m: i32, x: i32, y: i32) -> i32 {
    if mob.is_null() {
        return 0;
    }
    if ((*mob).id) < MOB_START_NUM || ((*mob).id) >= NPC_START_NUM {
        return 0;
    }
    map_delblock_id((*mob).id, (*mob).m);
    clif_lookgone_by_id((*mob).id);
    (*mob).m = m as u16;
    (*mob).x = x as u16;
    (*mob).y = y as u16;
    (*mob).bl_type = BL_MOB as u8;
    if map_addblock_id((*mob).id, (*mob).bl_type, (*mob).m, (*mob).x, (*mob).y) != 0 {
        tracing::warn!("Error warping mob.");
    }
    if let Some(grid) = block_grid::get_grid((*mob).m as usize) {
        let slot = &*ffi_get_map_ptr((*mob).m);
        let ids = block_grid::ids_in_area(grid, (*mob).x as i32, (*mob).y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
        if !(*mob).data.is_null() && (*(*mob).data).mobtype == 1 {
            for id in ids {
                if let Some(sd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                    clif_cmoblook(&*mob, &*sd_arc.read());
                }
            }
        } else {
            for id in ids {
                if let Some(pe) = crate::game::map_server::map_id2sd_pc(id) {
                    clif_object_look2_mob(pe.fd, &*mob);
                }
            }
        }
    }
    0
}

pub async unsafe fn kill_mob(mob: *mut MobSpawnData) -> i32 {
    {
        clif_mob_kill(&mut *mob).await;
        mob_flushmagic(mob);
    }
    0
}

// ─── AI state machine ─────────────────────────────────────────────────────────

pub unsafe fn mob_handle_sub(mob: *mut MobSpawnData) {
    if mob.is_null() {
        return;
    }
    let sptime = libc::time(std::ptr::null_mut()) as u32;

    if in_spawn_window(mob) {
        let data = (*mob).data.as_ref();
        let spawn_delay = data.map_or(0, |d| d.spawntime as u32);
        if (*mob).last_death + spawn_delay <= sptime {
            (*mob).spawncheck = 0;
            if (*mob).state == MOB_DEAD && (*mob).onetime == 0 {
                (*mob).target = 0;
                (*mob).attacker = 0;
                let has_users =
                    ffi_map_is_loaded((*mob).m) && crate::game::block::map_user_count((*mob).m as usize) > 0;
                if has_users {
                    mob_respawn(mob);
                } else {
                    mob_respawn_nousers(mob);
                }
            }
        }
    }

    if (*mob).data.as_ref().map_or(0, |d| d.r#type) >= 2 {
        return;
    }

    let has_users = ffi_map_is_loaded((*mob).m) && crate::game::block::map_user_count((*mob).m as usize) > 0;
    let subtype2 = (*mob).data.as_ref().map_or(0, |d| d.subtype);

    if !has_users && (*mob).onetime != 0 && subtype2 != 2 {
        if (*mob).state != MOB_DEAD {
            return;
        }
    }
    if !has_users && (*mob).onetime == 0 && subtype2 != 4 {
        if (*mob).state != MOB_DEAD {
            return;
        }
    }

    (*mob).time_ = (*mob).time_.wrapping_add(50);

    match (*mob).state {
        MOB_DEAD => {
            if (*mob).onetime != 0 {
                map_delblock_id((*mob).id, (*mob).m);
                map_deliddb((*mob).id);
                free_onetime(mob);
            }
        }
        MOB_ALIVE => {
            let data = if (*mob).data.is_null() {
                return;
            } else {
                &*(*mob).data
            };
            if ((*mob).time_ >= data.movetime && (*mob).time_ >= (*mob).newmove as i32)
                || ((*mob).newmove > 0 && (*mob).time_ >= (*mob).newmove as i32)
            {
                if data.r#type >= 2 {
                    return;
                }
                if data.r#type == 1 && (*mob).target == 0 {
                    if let Some(grid) = block_grid::get_grid((*mob).m as usize) {
                        let slot = &*ffi_get_map_ptr((*mob).m);
                        let ids = block_grid::ids_in_area(grid, (*mob).x as i32, (*mob).y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
                        for id in ids {
                            if let Some(sd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                                mob_find_target_inner(&mut *sd_arc.write() as *mut MapSessionData, mob);
                            }
                        }
                    }
                }
                let target_id = mob_resolve_target(mob);
                let pre_x = (*mob).x;
                let pre_y = (*mob).y;
                (*mob).time_ = 0;
                dispatch_ai(mob, target_id, "move");
                // If the mob didn't actually move but Lua left newmove faster
                // than the base speed (e.g. return-to-start mode while blocked),
                // reset newmove so the mob doesn't rapid-fire move attempts.
                if (*mob).x == pre_x && (*mob).y == pre_y
                    && !(*mob).data.is_null()
                    && (*mob).newmove < (*(*mob).data).movetime as u32
                {
                    (*mob).newmove = (*(*mob).data).movetime as u32;
                }
            }
        }
        MOB_HIT => {
            let data = if (*mob).data.is_null() {
                return;
            } else {
                &*(*mob).data
            };
            if ((*mob).time_ >= data.atktime && (*mob).time_ >= (*mob).newatk as i32)
                || ((*mob).newatk > 0 && (*mob).time_ >= (*mob).newatk as i32)
            {
                if data.r#type >= 2 {
                    return;
                }
                let target_id = mob_resolve_target(mob);
                if target_id == 0 {
                    // mob_resolve_target already cleared target/attacker
                    (*mob).state = MOB_ALIVE;
                    return;
                }
                (*mob).time_ = 0;
                dispatch_ai(mob, target_id, "attack");
            }
        }
        MOB_ESCAPE => {
            let data = if (*mob).data.is_null() {
                return;
            } else {
                &*(*mob).data
            };
            if ((*mob).time_ >= data.movetime && (*mob).time_ >= (*mob).newmove as i32)
                || ((*mob).newmove > 0 && (*mob).time_ >= (*mob).newmove as i32)
            {
                if data.r#type >= 2 {
                    return;
                }
                if data.r#type == 1 && (*mob).target == 0 {
                    if let Some(grid) = block_grid::get_grid((*mob).m as usize) {
                        let slot = &*ffi_get_map_ptr((*mob).m);
                        let ids = block_grid::ids_in_area(grid, (*mob).x as i32, (*mob).y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
                        for id in ids {
                            if let Some(sd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                                mob_find_target_inner(&mut *sd_arc.write() as *mut MapSessionData, mob);
                            }
                        }
                    }
                }
                let target_id = mob_resolve_target(mob);
                (*mob).time_ = 0;
                dispatch_ai(mob, target_id, "escape");
            }
        }
        _ => {}
    }
}

/// Resolves mob->target to a valid target ID. Clears target if dead/invalid.
/// Returns the target entity ID, or 0 if no valid target.
unsafe fn mob_resolve_target(mob: *mut MobSpawnData) -> u32 {
    let target_id = (*mob).target;
    let pos_info = crate::game::map_server::entity_position(target_id);
    let (pos, bl_type) = match pos_info {
        Some(v) => v,
        None => {
            (*mob).target = 0;
            (*mob).attacker = 0;
            return 0;
        }
    };
    if pos.m != (*mob).m {
        (*mob).target = 0;
        (*mob).attacker = 0;
        return 0;
    }
    if bl_type == BL_MOB as u8 {
        if let Some(arc) = crate::game::map_server::map_id2mob_ref(target_id) {
            if arc.read().state == MOB_DEAD {
                (*mob).target = 0;
                (*mob).attacker = 0;
                return 0;
            }
        }
    } else if bl_type == BL_PC as u8 {
        use crate::game::pc::PC_DIE;
        if let Some(arc) = crate::game::map_server::map_id2sd_pc(target_id) {
            if arc.read().player.combat.state == PC_DIE as i8 {
                (*mob).target = 0;
                (*mob).attacker = 0;
                return 0;
            }
        }
    }
    target_id
}

/// Dispatches to the correct Lua AI script based on mob subtype.
unsafe fn dispatch_ai(mob: *mut MobSpawnData, bl_id: u32, event: &str) {
    let data = if (*mob).data.is_null() {
        return;
    } else {
        &*(*mob).data
    };
    match data.subtype {
        0 => { sl_doscript_2("mob_ai_basic", Some(event), (*mob).id, bl_id); }
        1 => { sl_doscript_2("mob_ai_normal", Some(event), (*mob).id, bl_id); }
        2 => { sl_doscript_2("mob_ai_hard", Some(event), (*mob).id, bl_id); }
        3 => { sl_doscript_2("mob_ai_boss", Some(event), (*mob).id, bl_id); }
        4 => {
            let yname = crate::game::scripting::carray_to_str(&data.yname);
            sl_doscript_2(yname, Some(event), (*mob).id, bl_id);
        }
        5 => { sl_doscript_2("mob_ai_ghost", Some(event), (*mob).id, bl_id); }
        _ => {}
    };
}

// ─── mob_trap_look (typed inner callback) ────────────────────────────────────

/// Typed inner: activates NPC trap if mob steps on its cell.
pub unsafe fn mob_trap_look_inner(nd: *mut crate::game::npc::NpcData, mob: *mut MobSpawnData, type_: i32, def: *mut i32) -> i32 {
    if nd.is_null() {
        return 0;
    }
    // Only SUBTYPE_FLOOR (subtype==1) or sub-2 NPCs are traps
    if (*nd).subtype != SUBTYPE_FLOOR && (*nd).subtype != 2 {
        return 0;
    }
    if !def.is_null() && *def != 0 {
        return 0;
    }
    if type_ != 0 && (*nd).subtype == 2 {
        // skip sub-2 NPCs when type_ is non-zero
    } else {
        if !def.is_null() {
            *def = 1;
        }
        let nd_name = crate::game::scripting::carray_to_str(&(*nd).name);
        sl_doscript_2(nd_name, Some("click"), (*mob).id, (*nd).id);
    }
    0
}

/// Called every 50ms by the game loop.
pub unsafe fn mob_timer_spawns() {
    TIMERCHECK.fetch_add(1, Ordering::Relaxed);

    let spawn_start = MOB_SPAWN_START.load(Ordering::Relaxed);
    let spawn_max   = MOB_SPAWN_MAX.load(Ordering::Relaxed);
    if spawn_start != spawn_max {
        let mut x = spawn_start;
        while x < spawn_max {
            if let Some(mob_arc) = crate::game::map_server::map_id2mob_ref(x) {
                // data_ptr() returns raw pointer WITHOUT acquiring any lock.
                // SAFETY: single-threaded game loop, Arc keeps allocation alive.
                // tick_mob → Lua → MobObject.__index acquires its own lock.
                let ptr: *mut MobSpawnData = mob_arc.data_ptr();
                tick_mob(&mut *ptr);
            }
            x += 1;
        }
    }

    let onetime_start = MOB_ONETIME_START.load(Ordering::Relaxed);
    let onetime_max   = MOB_ONETIME_MAX.load(Ordering::Relaxed);
    if onetime_start != onetime_max {
        let mut x = onetime_start;
        while x < onetime_max {
            if let Some(mob_arc) = crate::game::map_server::map_id2mob_ref(x) {
                let ptr: *mut MobSpawnData = mob_arc.data_ptr();
                tick_mob(&mut *ptr);
            }
            x += 1;
        }
    }

    if TIMERCHECK.load(Ordering::Relaxed) >= 30 {
        TIMERCHECK.store(0, Ordering::Relaxed);
    }
}

unsafe fn tick_mob(mob: &mut MobSpawnData) {
    let mob = mob as *mut MobSpawnData;
    let tc = TIMERCHECK.load(Ordering::Relaxed);
    if tc % 5 == 0 {
        mob_secondduratimer(mob);
    }
    if tc % 10 == 0 {
        mob_thirdduratimer(mob);
    }
    if tc % 30 == 0 {
        mob_fourthduratimer(mob);
    }
    if tc % 20 == 0 {
        mob_duratimer(mob);
    }
    mob_handle_sub(mob);
}

// ─── Movement functions ───────────────────────────────────────────────────────

/// Shared warp-tile check used by all three move_mob variants.
unsafe fn warp_at(slot: *mut crate::database::map_db::MapData, dx: i32, dy: i32) -> bool {
    let bxs = (*slot).bxs as usize;
    let xs = (*slot).xs as usize;
    let ys = (*slot).ys as usize;
    let dx = dx as usize;
    let dy = dy as usize;
    if dx >= xs || dy >= ys {
        return false;
    }
    let idx = dx / BLOCK_SIZE + (dy / BLOCK_SIZE) * bxs;
    let warp: *mut WarpList = *(*slot).warp.add(idx);
    let mut w = warp;
    while !w.is_null() {
        if (*w).x == dx as i32 && (*w).y == dy as i32 {
            return true;
        }
        w = (*w).next;
    }
    false
}

/// Compute viewport delta strip for a one-step move in `direction`.
/// Returns `(x0, y0, x1, y1, dx, dy, nothingnew)`.
unsafe fn viewport_delta(
    mob: *const MobSpawnData,
    slot: *mut crate::database::map_db::MapData,
) -> (i32, i32, i32, i32, i32, i32, bool) {
    let backx = (*mob).x as i32;
    let backy = (*mob).y as i32;
    let xs = (*slot).xs as i32;
    let ys = (*slot).ys as i32;
    let (mut x0, mut y0) = (backx, backy);
    let (mut x1, mut y1) = (0, 0);
    let mut dx = backx;
    let mut dy = backy;
    let mut nothingnew = false;

    match (*mob).side {
        0 => {
            // UP
            if backy > 0 {
                dy = backy - 1;
                x0 -= 9;
                if x0 < 0 {
                    x0 = 0;
                }
                y0 -= 9;
                y1 = 1;
                x1 = 19;
                if y0 < 7 {
                    nothingnew = true;
                }
                if y0 == 7 {
                    y1 += 7;
                    y0 = 0;
                }
                if x0 + 19 + 9 >= xs {
                    x1 += 9 - ((x0 + 19 + 9) - xs);
                }
                if x0 <= 8 {
                    x1 += x0;
                    x0 = 0;
                }
            }
        }
        1 => {
            // Right
            if backx < xs {
                x0 += 10;
                y0 -= 8;
                if y0 < 0 {
                    y0 = 0;
                }
                dx = backx + 1;
                y1 = 17;
                x1 = 1;
                if x0 > xs - 9 {
                    nothingnew = true;
                }
                if x0 == xs - 9 {
                    x1 += 9;
                }
                if y0 + 17 + 8 >= ys {
                    y1 += 8 - ((y0 + 17 + 8) - ys);
                }
                if y0 <= 7 {
                    y1 += y0;
                    y0 = 0;
                }
            }
        }
        2 => {
            // Down
            if backy < ys {
                x0 -= 9;
                if x0 < 0 {
                    x0 = 0;
                }
                y0 += 9;
                dy = backy + 1;
                y1 = 1;
                x1 = 19;
                if y0 + 8 > ys {
                    nothingnew = true;
                }
                if y0 + 8 == ys {
                    y1 += 8;
                }
                if x0 + 19 + 9 >= xs {
                    x1 += 9 - ((x0 + 19 + 9) - xs);
                }
                if x0 <= 8 {
                    x1 += x0;
                    x0 = 0;
                }
            }
        }
        3 => {
            // Left
            if backx > 0 {
                x0 -= 10;
                y0 -= 8;
                if y0 < 0 {
                    y0 = 0;
                }
                y1 = 17;
                x1 = 1;
                dx = backx - 1;
                if x0 < 8 {
                    nothingnew = true;
                }
                if x0 == 8 {
                    x0 = 0;
                    x1 += 8;
                }
                if y0 + 17 + 8 >= ys {
                    y1 += 8 - ((y0 + 17 + 8) - ys);
                }
                if y0 <= 7 {
                    y1 += y0;
                    y0 = 0;
                }
            }
        }
        _ => {}
    }
    (x0, y0, x1, y1, dx, dy, nothingnew)
}

/// Shared post-move broadcast used by move_mob variants.
unsafe fn broadcast_move(
    mob: *mut MobSpawnData,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    nothingnew: bool,
) {
    let m = (*mob).m as usize;
    let mut subt = [0i32; 1];
    if let Some(grid) = block_grid::get_grid(m) {
        if !nothingnew {
            let rect_ids = grid.ids_in_rect(x0, y0, x0 + x1 - 1, y0 + y1 - 1);
            if !(*mob).data.is_null() && (*(*mob).data).mobtype == 1 {
                for id in &rect_ids {
                    if let Some(sd_arc) = crate::game::map_server::map_id2sd_pc(*id) {
                        clif_cmoblook(&*mob, &*sd_arc.read());
                    }
                }
            } else {
                for id in &rect_ids {
                    if let Some(pe) = crate::game::map_server::map_id2sd_pc(*id) {
                        clif_mob_look_start_func_inner(pe.fd, &mut pe.net.write().look);
                    }
                }
                for id in &rect_ids {
                    if let Some(pe) = crate::game::map_server::map_id2sd_pc(*id) {
                        clif_object_look_mob(pe.fd, &mut pe.net.write().look, &*mob);
                    }
                }
                for id in &rect_ids {
                    if let Some(pe) = crate::game::map_server::map_id2sd_pc(*id) {
                        clif_mob_look_close_func_inner(pe.fd, &mut pe.net.write().look);
                    }
                }
            }
        }
        // NPC trap check at mob's current cell
        {
            let cell_ids = grid.ids_at_tile((*mob).x, (*mob).y);
            let def_ptr = subt.as_mut_ptr();
            for id in cell_ids {
                if let Some(npc_arc) = crate::game::map_server::map_id2npc_ref(id) {
                    mob_trap_look_inner(&mut *npc_arc.write() as *mut crate::game::npc::NpcData, mob, 0, def_ptr);
                }
            }
        }
        // Send mob move to nearby PCs
        {
            let slot = &*ffi_get_map_ptr((*mob).m);
            let ids = block_grid::ids_in_area(grid, (*mob).x as i32, (*mob).y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
            for id in ids {
                if let Some(sd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                    { let guard = sd_arc.read(); clif_mob_move_inner(&*guard, mob); }
                }
            }
        }
    }
}

unsafe fn check_mob_collision(moving_mob: *mut MobSpawnData, m: i32, x: i32, y: i32) {
    if (*moving_mob).canmove == 1 { return; }
    if x < 0 || y < 0 { return; }
    let self_id = (*moving_mob).id;
    if let Some(grid) = crate::game::block_grid::get_grid(m as usize) {
        for id in grid.ids_at_tile(x as u16, y as u16) {
            if id == self_id { continue; }
            if let Some(mob_arc) = crate::game::map_server::map_id2mob_ref(id) {
                let mob = mob_arc.read();
                if mob.x as i32 == x && mob.y as i32 == y && mob.state != MOB_DEAD {
                    (*moving_mob).canmove = 1;
                    return;
                }
            }
        }
    }
}

/// PC-collision check — sets `moving_mob.canmove = 1` if a physical, non-GM, non-dead player occupies `(x, y)`.
unsafe fn check_pc_collision(moving_mob: *mut MobSpawnData, m: i32, x: i32, y: i32) {
    use crate::game::pc::PC_DIE;
    if (*moving_mob).canmove == 1 { return; }
    if x < 0 || y < 0 { return; }
    let slot = ffi_get_map_ptr(m as u16);
    if slot.is_null() { return; }
    let show_ghosts = (*slot).show_ghosts;
    if let Some(grid) = crate::game::block_grid::get_grid(m as usize) {
        for id in grid.ids_at_tile(x as u16, y as u16) {
            if let Some(sd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                let sd = sd_arc.read();
                if sd.x as i32 == x && sd.y as i32 == y {
                    let state  = sd.player.combat.state;
                    let gm_lvl = sd.player.identity.gm_level;
                    let passable = (show_ghosts != 0 && state == PC_DIE as i8)
                        || state == -1
                        || gm_lvl >= 50;
                    if !passable {
                        (*moving_mob).canmove = 1;
                        return;
                    }
                }
            }
        }
    }
}

pub unsafe fn move_mob(mob: *mut MobSpawnData) -> i32 {
    let m = (*mob).m as i32;
    let backx = (*mob).x as i32;
    let backy = (*mob).y as i32;
    let slot = ffi_get_map_ptr((*mob).m);
    if slot.is_null() {
        return 0;
    }

    let (x0, y0, x1, y1, mut dx, mut dy, nothingnew) = viewport_delta(mob, slot);

    let xs = (*slot).xs as i32;
    let ys = (*slot).ys as i32;

    if dx >= xs {
        dx = xs - 1;
    }
    if dy >= ys {
        dy = ys - 1;
    }

    if warp_at(slot, dx, dy) {
        return 0;
    }

    check_mob_collision(mob, m, dx, dy);
    check_pc_collision(mob, m, dx, dy);
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let cell_ids = grid.ids_at_tile(dx as u16, dy as u16);
        for id in cell_ids {
            mob_move_inner_id(id, mob);
        }
    }

    if clif_object_canmove(m, dx, dy, (*mob).side) != 0 {
        (*mob).canmove = 0;
        return 0;
    }
    if clif_object_canmove_from(m, backx, backy, (*mob).side) != 0 {
        (*mob).canmove = 0;
        return 0;
    }
    if map_canmove(m, dx, dy) == 1 || (*mob).canmove == 1 {
        (*mob).canmove = 0;
        return 0;
    }

    // clamp after collision checks
    let dx = if dx >= xs {
        backx
    } else if dx < 0 {
        backx
    } else {
        dx
    };
    let dy = if dy >= ys {
        backy
    } else if dy < 0 {
        backy
    } else {
        dy
    };

    if dx != backx || dy != backy {
        (*mob).prev_x = backx as u16;
        (*mob).prev_y = backy as u16;
        let old_x = (*mob).x;
        let old_y = (*mob).y;
        map_moveblock_id((*mob).id, (*mob).m, old_x, old_y, dx as u16, dy as u16);
        (*mob).x = dx as u16;
        (*mob).y = dy as u16;
        broadcast_move(mob, x0, y0, x1, y1, nothingnew);
        return 1;
    }
    0
}

pub unsafe fn move_mob_ignore_object(mob: *mut MobSpawnData) -> i32 {
    let m = (*mob).m as i32;
    let backx = (*mob).x as i32;
    let backy = (*mob).y as i32;
    let slot = ffi_get_map_ptr((*mob).m);
    if slot.is_null() {
        return 0;
    }

    let (x0, y0, x1, y1, mut dx, mut dy, nothingnew) = viewport_delta(mob, slot);
    let xs = (*slot).xs as i32;
    let ys = (*slot).ys as i32;
    if dx >= xs {
        dx = xs - 1;
    }
    if dy >= ys {
        dy = ys - 1;
    }
    if warp_at(slot, dx, dy) {
        return 0;
    }

    // No collision callbacks — ignore objects
    if clif_object_canmove(m, dx, dy, (*mob).side) != 0 {
        (*mob).canmove = 0;
        return 0;
    }
    if clif_object_canmove_from(m, backx, backy, (*mob).side) != 0 {
        (*mob).canmove = 0;
        return 0;
    }

    let dx = if dx >= xs {
        backx
    } else if dx < 0 {
        backx
    } else {
        dx
    };
    let dy = if dy >= ys {
        backy
    } else if dy < 0 {
        backy
    } else {
        dy
    };

    if dx != backx || dy != backy {
        (*mob).prev_x = backx as u16;
        (*mob).prev_y = backy as u16;
        let old_x = (*mob).x;
        let old_y = (*mob).y;
        map_moveblock_id((*mob).id, (*mob).m, old_x, old_y, dx as u16, dy as u16);
        (*mob).x = dx as u16;
        (*mob).y = dy as u16;
        broadcast_move(mob, x0, y0, x1, y1, nothingnew);
        return 1;
    }
    0
}

pub unsafe fn moveghost_mob(mob: *mut MobSpawnData) -> i32 {
    let m = (*mob).m as i32;
    let backx = (*mob).x as i32;
    let backy = (*mob).y as i32;
    let slot = ffi_get_map_ptr((*mob).m);
    if slot.is_null() {
        return 0;
    }

    let (x0, y0, x1, y1, mut dx, mut dy, nothingnew) = viewport_delta(mob, slot);
    let xs = (*slot).xs as i32;
    let ys = (*slot).ys as i32;
    if dx >= xs {
        dx = xs - 1;
    }
    if dy >= ys {
        dy = ys - 1;
    }
    if warp_at(slot, dx, dy) {
        return 0;
    }

    check_mob_collision(mob, m, dx, dy);
    check_pc_collision(mob, m, dx, dy);
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let cell_ids = grid.ids_at_tile(dx as u16, dy as u16);
        for id in cell_ids {
            mob_move_inner_id(id, mob);
        }
    }

    // Collision checks only apply when mob has no target
    if (*mob).target == 0 {
        if clif_object_canmove(m, dx, dy, (*mob).side) != 0 {
            (*mob).canmove = 0;
            return 0;
        }
        if clif_object_canmove_from(m, backx, backy, (*mob).side) != 0 {
            (*mob).canmove = 0;
            return 0;
        }
        if map_canmove(m, dx, dy) == 1 || (*mob).canmove == 1 {
            (*mob).canmove = 0;
            return 0;
        }
    }

    let dx = if dx >= xs {
        backx
    } else if dx < 0 {
        backx
    } else {
        dx
    };
    let dy = if dy >= ys {
        backy
    } else if dy < 0 {
        backy
    } else {
        dy
    };

    if dx != backx || dy != backy {
        (*mob).prev_x = backx as u16;
        (*mob).prev_y = backy as u16;
        let old_x = (*mob).x;
        let old_y = (*mob).y;
        map_moveblock_id((*mob).id, (*mob).m, old_x, old_y, dx as u16, dy as u16);
        (*mob).x = dx as u16;
        (*mob).y = dy as u16;
        broadcast_move(mob, x0, y0, x1, y1, nothingnew);
        return 1;
    }
    0
}

pub unsafe fn mob_move2(mob: *mut MobSpawnData, x: i32, y: i32, side: i32) -> i32 {
    if (*mob).canmove != 0 {
        return 1;
    }
    if x < 0 || y < 0 {
        return 0;
    }
    let m = (*mob).m as i32;
    (*mob).side = side;
    check_mob_collision(mob, m, x, y);
    check_pc_collision(mob, m, x, y);
    let cm = (*mob).canmove;
    if map_canmove(m, x, y) == 0 && cm == 0 {
        (*mob).prev_x = (*mob).x;
        (*mob).prev_y = (*mob).y;
        map_moveblock_id((*mob).id, (*mob).m, (*mob).x, (*mob).y, x as u16, y as u16);
        (*mob).x = x as u16;
        (*mob).y = y as u16;
        if let Some(grid) = block_grid::get_grid(m as usize) {
            let slot = &*ffi_get_map_ptr((*mob).m);
            let ids = block_grid::ids_in_area(grid, (*mob).x as i32, (*mob).y as i32, AreaType::Area, slot.xs as i32, slot.ys as i32);
            for id in ids {
                if let Some(sd_arc) = crate::game::map_server::map_id2sd_pc(id) {
                    { let guard = sd_arc.read(); clif_mob_move_inner(&*guard, mob); }
                }
            }
        }
        (*mob).canmove = 1;
    } else {
        (*mob).canmove = 0;
        return 0;
    }
    1
}

pub unsafe fn move_mob_intent(mob: *mut MobSpawnData, target_x: i32, target_y: i32) -> i32 {
    (*mob).canmove = 0;
    let mx = (*mob).x as i32;
    let my = (*mob).y as i32;
    let px = target_x;
    let py = target_y;
    let ax = (mx - px).abs();
    let ay = (my - py).abs();
    let side = (*mob).side;
    if (ax == 0 && ay == 1) || (ax == 1 && ay == 0) {
        if mx < px {
            (*mob).side = 1;
        }
        if mx > px {
            (*mob).side = 3;
        }
        if my < py {
            (*mob).side = 2;
        }
        if my > py {
            (*mob).side = 0;
        }
        if side != (*mob).side {
            clif_sendmob_side(mob);
        }
        return 1;
    }
    0
}

// ─── Registry ─────────────────────────────────────────────────────────────────

pub unsafe fn mob_readglobalreg(mob: *mut MobSpawnData, reg: *const i8) -> i32 {
    if mob.is_null() || reg.is_null() {
        return 0;
    }
    for i in 0..MAX_GLOBALMOBREG {
        if libc::strcasecmp((*mob).registry[i].str.as_ptr(), reg) == 0 {
            return (*mob).registry[i].val;
        }
    }
    0
}

pub unsafe fn mob_setglobalreg(mob: *mut MobSpawnData, reg: *const i8, val: i32) -> i32 {
    if mob.is_null() || reg.is_null() {
        return 1;
    }
    // find existing slot
    for i in 0..MAX_GLOBALMOBREG {
        if libc::strcasecmp((*mob).registry[i].str.as_ptr(), reg) == 0 {
            if val == 0 {
                libc::strcpy((*mob).registry[i].str.as_mut_ptr(), c"".as_ptr());
            }
            (*mob).registry[i].val = val;
            return 0;
        }
    }
    // find empty slot
    for i in 0..MAX_GLOBALMOBREG {
        if libc::strcasecmp((*mob).registry[i].str.as_ptr(), c"".as_ptr()) == 0 {
            let dst = (*mob).registry[i].str.as_mut_ptr();
            let dst_len = core::mem::size_of_val(&(*mob).registry[i].str);
            libc::strncpy(dst, reg, dst_len - 1);
            *dst.add(dst_len - 1) = 0;
            (*mob).registry[i].val = val;
            return 0;
        }
    }
    tracing::warn!(
        "[mob] mob_setglobalreg: couldn't set {:?}",
        std::ffi::CStr::from_ptr(reg)
    );
    1
}

// ─── Item / drop helpers ──────────────────────────────────────────────────────

/// Typed inner: sets def[0]=1 on first hit (used as a foreachincell "any-present" test).
pub unsafe fn mob_thing_yeah_inner(_entity_id: u32, def: *mut i32) -> i32 {
    if !def.is_null() {
        *def = 1;
    }
    0
}

/// Typed inner: merge item `fl2` into an existing floor-item `fl` if IDs match.
/// Args: `int* def`, `int id` (unused), `FLOORITEM* fl2`, `USER* sd` (unused).
pub unsafe fn mob_addtocurrent_inner(fl: *mut crate::game::scripting::types::floor::FloorItemData, def: *mut i32, _id: i32, fl2: *mut crate::game::scripting::types::floor::FloorItemData, _sd: *mut MapSessionData) -> i32 {
    if fl.is_null() {
        return 0;
    }
    if def.is_null() || fl2.is_null() {
        return 0;
    }
    if *def != 0 {
        return 0;
    }
    if (*fl).data.id == (*fl2).data.id {
        (*fl).data.amount += (*fl2).data.amount;
        *def = 1;
    }
    0
}

/// Drop an item onto the ground at (m, x, y).
/// Reads `attacker->group_count` and `groups[]` to populate floor-item looters.
pub unsafe fn mob_dropitem(
    blockid: u32,
    id: u32,
    amount: i32,
    dura: i32,
    protected_: i32,
    owner: i32,
    m: i32,
    x: i32,
    y: i32,
    sd: *mut MapSessionData,
) -> i32 {
    use crate::common::constants::world::MAX_GROUP_MEMBERS;
    use crate::game::scripting::types::floor::FloorItemData;
    let mob_arc_holder = if blockid >= MOB_START_NUM as u32 && blockid < FLOORITEM_START_NUM as u32 {
        crate::game::map_server::map_id2mob_ref(blockid)
    } else {
        None
    };
    let mob: *mut MobSpawnData = match mob_arc_holder {
        Some(ref arc) => &mut *arc.write() as *mut MobSpawnData,
        None => std::ptr::null_mut(),
    };

    let mut def: i32 = 0;
    let mut fl = Box::new(unsafe { std::mem::zeroed::<FloorItemData>() });
    (*fl).m = m as u16;
    (*fl).x = x as u16;
    (*fl).y = y as u16;
    (*fl).data.id = id;
    (*fl).data.amount = amount;
    (*fl).data.dura = dura;
    (*fl).data.protected = protected_ as u32;
    (*fl).data.owner = owner as u32;

    if let Some(grid) = block_grid::get_grid(m as usize) {
        let def_ptr = &raw mut def;
        let fl_ptr = fl.as_mut() as *mut FloorItemData;
        let sd_ptr = sd;
        let cell_ids = grid.ids_at_tile(x as u16, y as u16);
        for cid in cell_ids {
            if let Some(fl_arc) = crate::game::map_server::map_id2fl_ref(cid) {
                mob_addtocurrent_inner(&mut *fl_arc.write() as *mut crate::game::scripting::types::floor::FloorItemData, def_ptr, id as i32, fl_ptr, sd_ptr);
            }
        }
    }

    (*fl).timer = libc::time(std::ptr::null_mut()) as u32;
    // looters is already zeroed by mem::zeroed()

    if !mob.is_null() {
        let attacker = map_id2sd_mob((*mob).attacker);
        if !attacker.is_null() {
            if (*attacker).group_count > 0 {
                let safe_count = if (*attacker).group_count < MAX_GROUP_MEMBERS as i32 {
                    (*attacker).group_count as usize
                } else {
                    MAX_GROUP_MEMBERS
                };
                let gid = (*attacker).groupid as usize;
                if gid < 256 {
                    let grp = groups_mob();
                    for z in 0..safe_count {
                        let idx = gid * MAX_GROUP_MEMBERS + z;
                        if idx < grp.len() {
                            (*fl).looters[z] = grp[idx];
                        }
                    }
                }
            } else {
                (*fl).looters[0] = (*attacker).id;
            }
        }
    }

    if def == 0 {
        let fl_raw = Box::into_raw(fl);
        map_additem(fl_raw);
        if let Some(grid) = block_grid::get_grid(m as usize) {
            let slot = &*ffi_get_map_ptr(m as u16);
            let ids = block_grid::ids_in_area(grid, x, y, AreaType::Area, slot.xs as i32, slot.ys as i32);
            for id in ids {
                if let Some(pe) = crate::game::map_server::map_id2sd_pc(id) {
                    clif_object_look2_item(pe.fd, pe.read().player.identity.id, &*fl_raw);
                }
            }
        }
    } else {
        drop(fl);
    }
    0
}

pub unsafe fn mobdb_drops(mob: *mut MobSpawnData, sd: *mut MapSessionData) -> i32 {
    sl_doscript_2("mobDrops", None, (*sd).id, (*mob).id);
    for i in 0..MAX_INVENTORY {
        let slot = &(*mob).inventory[i];
        if slot.id != 0 && slot.amount >= 1 {
            mob_dropitem(
                (*mob).id,
                slot.id as u32,
                slot.amount,
                slot.dura,
                slot.protected as i32,
                slot.owner as i32,
                (*mob).m as i32,
                (*mob).x as i32,
                (*mob).y as i32,
                sd,
            );
            (*mob).inventory[i].id = 0;
            (*mob).inventory[i].amount = 0;
            (*mob).inventory[i].owner = 0;
            (*mob).inventory[i].dura = 0;
            (*mob).inventory[i].protected = 0;
        }
    }
    0
}

// ─── Mob AI and behavior functions ────────────────────────────────────────────

/// Typed inner: selects a PC as this mob's target.
/// Reads `sd->status.dura_aether` to check sneak/cloak/hide, then conditionally
/// updates `mob->target` based on `sd->status.gm_level` and a random roll.
pub unsafe fn mob_find_target_inner(sd: *mut MapSessionData, mob: *mut MobSpawnData) -> i32 {
    use crate::game::pc::PC_DIE;
    if sd.is_null() {
        return 0;
    }
    if mob.is_null() {
        return 0;
    }
    let seeinvis = if (*mob).data.is_null() {
        0i8
    } else {
        (*(*mob).data).seeinvis
    };
    let mut invis: u8 = 0;
    for i in 0..MAX_MAGIC_TIMERS {
        if (&(*sd).player.spells.dura_aether)[i].duration > 0 {
            let name = magicdb_name((&(*sd).player.spells.dura_aether)[i].id as i32);
            if !name.is_null() {
                if libc::strcasecmp(name, c"sneak".as_ptr()) == 0 {
                    invis = 1;
                }
                if libc::strcasecmp(name, c"cloak".as_ptr()) == 0 {
                    invis = 2;
                }
                if libc::strcasecmp(name, c"hide".as_ptr()) == 0 {
                    invis = 3;
                }
            }
        }
    }
    match invis {
        1 => {
            if seeinvis != 1 && seeinvis != 3 && seeinvis != 5 {
                return 0;
            }
        }
        2 => {
            if seeinvis != 2 && seeinvis != 3 && seeinvis != 5 {
                return 0;
            }
        }
        3 => {
            if seeinvis != 4 && seeinvis != 5 {
                return 0;
            }
        }
        _ => {}
    }
    if (*sd).player.combat.state == PC_DIE as i8 {
        return 0;
    }
    if (*mob).confused != 0 && (*mob).confused_target == (*sd).id {
        return 0;
    }
    if (*mob).target != 0 {
        let num = (rand::random::<u32>() & 0x00FF_FFFF) % 1000;
        if num <= 499 && (*sd).player.identity.gm_level < 50 {
            (*mob).target = (*sd).player.identity.id;
        }
    } else if (*sd).player.identity.gm_level < 50 {
        (*mob).target = (*sd).player.identity.id;
    }
    0
}

/// Mob attacks a player (or another mob) by ID.
/// Reads `sd->uFlags` and `sd->optFlags` to check immortal/stealth before attacking.
/// Calls scripting hooks `hitCritChance` and `swingDamage`, then sends network damage.
pub unsafe fn mob_attack(mob: *mut MobSpawnData, id: i32) -> i32 {
    use crate::game::pc::{OPT_FLAG_STEALTH, SFLAG_HPMP, U_FLAG_IMMORTAL};
    if id < 0 {
        return 0;
    }
    let target = id as u32;
    // Try typed lookups — target is either a PC or another mob.
    let sd: *mut MapSessionData = crate::game::map_server::map_id2sd_pc(target)
        .map(|arc| arc.data_ptr())
        .unwrap_or(std::ptr::null_mut());
    let tmob: *mut MobSpawnData = if sd.is_null() {
        crate::game::map_server::map_id2mob_ref(target)
            .map(|arc| arc.data_ptr())
            .unwrap_or(std::ptr::null_mut())
    } else {
        std::ptr::null_mut()
    };
    if sd.is_null() && tmob.is_null() {
        return 0;
    }
    if !sd.is_null() {
        if ((*sd).uFlags & U_FLAG_IMMORTAL != 0) || ((*sd).optFlags & OPT_FLAG_STEALTH != 0) {
            (*mob).target = 0;
            (*mob).attacker = 0;
            return 0;
        }
    }
    let target_id = id as u32;
    if !sd.is_null() {
        sl_doscript_2("hitCritChance", None, (*mob).id, target_id);
    } else if !tmob.is_null() {
        sl_doscript_2("hitCritChance", None, (*mob).id, target_id);
    }
    if (*mob).critchance != 0 {
        let sound = if !(*mob).data.is_null() { (*(*mob).data).sound } else { 0 };
        clif_sendmob_action(&mut *mob, 1, 20, sound);
        if !sd.is_null() {
            sl_doscript_2("swingDamage", None, (*mob).id, target_id);
            for x in 0..MAX_MAGIC_TIMERS {
                if (*mob).da[x].id > 0 && (*mob).da[x].duration > 0 {
                    let yname = magicdb_yname_str((*mob).da[x].id as i32);
                    sl_doscript_2(&yname, Some("on_hit_while_cast"), (*mob).id, target_id);
                }
            }
        } else if !tmob.is_null() {
            sl_doscript_2("swingDamage", None, (*mob).id, target_id);
            for x in 0..MAX_MAGIC_TIMERS {
                if (*mob).da[x].id > 0 && (*mob).da[x].duration > 0 {
                    let yname = magicdb_yname_str((*mob).da[x].id as i32);
                    sl_doscript_2(&yname, Some("on_hit_while_cast"), (*mob).id, target_id);
                }
            }
        }
        let dmg = ((*mob).damage + 0.5f32) as i32;
        if !sd.is_null() {
            if (*mob).critchance == 1 {
                clif_send_pc_health(&mut *sd, dmg, 33);
            } else {
                clif_send_pc_health(&mut *sd, dmg, 255);
            }
            clif_sendstatus_mob(sd, SFLAG_HPMP);
        } else if !tmob.is_null() {
            if (*mob).critchance == 1 {
                clif_send_mob_health(&mut *tmob, dmg, 33);
            } else {
                clif_send_mob_health(&mut *tmob, dmg, 255);
            }
        }
    }
    0
}

/// Calculate and set `mob->critchance` based on mob stats vs player stats.
/// Returns 0 (miss), 1 (normal hit), or 2 (critical hit).
pub unsafe fn mob_calc_critical(
    mob: *mut MobSpawnData,
    sd: *mut MapSessionData,
) -> i32 {
    if mob.is_null() || sd.is_null() {
        return 0;
    }
    let db = (*mob).data;
    if db.is_null() {
        return 0;
    }
    let equat = ((*db).hit + (*db).level + ((*db).might / 5) + 20)
        - ((*sd).player.progression.level as i32 + ((*sd).grace / 2));
    let mut equat = equat - ((*sd).grace / 4) + (*sd).player.progression.level as i32;
    let chance = ((rand::random::<u32>() & 0x00FF_FFFF) % 100) as i32;
    if equat < 5 {
        equat = 5;
    }
    if equat > 95 {
        equat = 95;
    }
    if chance < equat {
        let crit = equat as f32 * 0.33f32;
        if (chance as f32) < crit {
            2
        } else {
            1
        }
    } else {
        0
    }
}

/// Typed inner: check whether an entity blocks mob movement.
/// Sets `mob->canmove = 1` if the entity occupies the cell and is not a valid ghost/GM.
pub unsafe fn mob_move_inner_id(entity_id: u32, mob: *mut MobSpawnData) -> i32 {
    use crate::game::pc::PC_DIE;
    if mob.is_null() { return 0; }
    if (*mob).canmove == 1 { return 0; }

    if let Some(arc) = crate::game::map_server::map_id2npc_ref(entity_id) {
        let npc = &*arc.data_ptr();
        if npc.subtype != 0 { return 0; }
    } else if let Some(arc) = crate::game::map_server::map_id2mob_ref(entity_id) {
        let m2 = &*arc.data_ptr();
        if m2.state == MOB_DEAD { return 0; }
    } else if let Some(arc) = crate::game::map_server::map_id2sd_pc(entity_id) {
        let sd = arc.read();
        let show_ghosts = if ffi_map_is_loaded((*mob).m) {
            (*ffi_get_map_ptr((*mob).m)).show_ghosts
        } else {
            0
        };
        if (show_ghosts != 0 && sd.player.combat.state == PC_DIE as i8)
            || sd.player.combat.state == -1
            || sd.player.identity.gm_level >= 50
        {
            return 0;
        }
    } else {
        return 0;
    }
    (*mob).canmove = 1;
    0
}

// ─── mobspawn_onetime ─────────────────────────────────────────────────────────

pub unsafe fn mobspawn_onetime(
    id: u32,
    m: i32,
    x: i32,
    y: i32,
    times: i32,
    start: i32,
    end: i32,
    replace: u32,
    owner: u32,
) -> Vec<u32> {
    const MAX_ONETIME_SPAWNS: i32 = 1024;
    if times <= 0 || times > MAX_ONETIME_SPAWNS {
        return Vec::new();
    }
    let mut spawnedmobs: Vec<u32> = Vec::with_capacity(times as usize);
    for _z in 0..times {
        let mut mob_box: Box<MobSpawnData> = Box::new_zeroed().assume_init();
        let db: *mut MobSpawnData = mob_box.as_mut() as *mut MobSpawnData;

        if (*db).exp == 0 {
            (*db).exp = mob_db::experience(id);
        }
        (*db).startm = m as u16;
        (*db).startx = x as u16;
        (*db).starty = y as u16;
        (*db).mobid = id;
        (*db).start = start as i8;
        (*db).end = end as i8;
        (*db).replace = replace;
        (*db).state = MOB_DEAD;
        (*db).bl_type = BL_MOB as u8;
        (*db).m = m as u16;
        (*db).x = x as u16;
        (*db).y = y as u16;
        (*db).owner = owner;
        (*db).onetime = 1;
        (*db).spawncheck = 0;

        let new_id = mob_get_free_id();
        if new_id == 0 {
            tracing::warn!("[mob] mobspawn_onetime: no free onetime ID, skipping spawn");
            // mob_box is dropped here automatically, no manual free needed
            continue;
        }
        (*db).id = new_id;

        spawnedmobs.push(new_id);
        // Insert into MOB_MAP first — this moves the Box data into Arc<RwLock>,
        // freeing the Box. After this, `db` is dangling and must not be used.
        crate::game::map_server::map_addiddb_mob(new_id, mob_box);
        // Get the live pointer from the Arc<RwLock>.
        let db = crate::game::map_server::map_id2mob_ref(new_id)
            .expect("mob just inserted").data_ptr();
        map_addblock_id((*db).id, (*db).bl_type, (*db).m, (*db).x, (*db).y);

        let has_users = ffi_map_is_loaded((*db).m) && crate::game::block::map_user_count((*db).m as usize) > 0;
        if has_users {
            mob_respawn(db);
        } else {
            mob_respawn_nousers(db);
        }
    }
    spawnedmobs
}

// ─── Mob Lua scripting glue ───────────────────────────────────────────────────

/// Heal mob: fire on_healed Lua event then send the negative-damage health packet.
pub async unsafe fn sl_mob_addhealth(mob: *mut MobSpawnData, damage: i32) {
    use crate::game::map_parse::combat::clif_send_mob_healthscript;
    if mob.is_null() { return; }
    let attacker = (*mob).attacker;
    let has_attacker = attacker > 0 && crate::game::map_server::entity_position(attacker).is_some();
    let data = (*mob).data;
    if !data.is_null() && damage > 0 {
        let yname = ai_script_name(&*data);
        if has_attacker {
            sl_doscript_2(yname, Some("on_healed"), (*mob).id, attacker);
        } else {
            sl_doscript_simple(yname, Some("on_healed"), (*mob).id);
        }
    }
    clif_send_mob_healthscript(&mut *mob, -damage, 0).await;
}

/// Damage mob: set attacker/damage fields then send the health packet.
pub async unsafe fn sl_mob_removehealth(mob: *mut MobSpawnData, damage: i32, caster_id: u32) {
    use crate::game::map_parse::combat::clif_send_mob_healthscript;
    if mob.is_null() { return; }
    let resolved_id = if caster_id > 0 {
        (*mob).attacker = caster_id;
        caster_id
    } else {
        (*mob).attacker
    };
    // Set damage/critchance on the resolved attacker entity.
    let mut set_on_attacker = false;
    if resolved_id > 0 {
        if let Some(arc) = crate::game::map_server::map_id2sd_pc(resolved_id) {
            let mut sd = arc.write();
            sd.damage = damage as f32;
            sd.critchance = 0;
            set_on_attacker = true;
        } else if let Some(arc) = crate::game::map_server::map_id2mob_ref(resolved_id) {
            let tmob = arc.data_ptr();
            (*tmob).damage = damage as f32;
            (*tmob).critchance = 0;
            set_on_attacker = true;
        }
    }
    if !set_on_attacker {
        (*mob).damage = damage as f32;
        (*mob).critchance = 0;
    }
    if (*mob).state != MOB_DEAD {
        clif_send_mob_healthscript(&mut *mob, damage, 0).await;
    }
}

/// Return accumulated threat amount from a specific player on this mob.
pub unsafe fn sl_mob_checkthreat(mob: *mut MobSpawnData, player_id: u32) -> i32 {
    if mob.is_null() { return 0; }
    let tsd = map_id2sd_mob(player_id);
    if tsd.is_null() { return 0; }
    let uid = (*tsd).id;
    for x in 0..MAX_THREATCOUNT {
        if (*mob).threat[x].user == uid {
            return (*mob).threat[x].amount as i32;
        }
    }
    0
}

/// Add individual damage from player to mob's dmgindtable.
pub unsafe fn sl_mob_setinddmg(mob: *mut MobSpawnData, player_id: u32, dmg: f32) -> i32 {
    if mob.is_null() { return 0; }
    let sd = map_id2sd_mob(player_id);
    if sd.is_null() { return 0; }
    let cid = (*sd).player.identity.id;
    for x in 0..MAX_THREATCOUNT {
        if (*mob).dmgindtable[x][0] as u32 == cid || (*mob).dmgindtable[x][0] == 0.0 {
            (*mob).dmgindtable[x][0] = cid as f64;
            (*mob).dmgindtable[x][1] += dmg as f64;
            return 1;
        }
    }
    0
}

/// Add group damage from player to mob's dmggrptable.
pub unsafe fn sl_mob_setgrpdmg(mob: *mut MobSpawnData, player_id: u32, dmg: f32) -> i32 {
    if mob.is_null() { return 0; }
    let sd = map_id2sd_mob(player_id);
    if sd.is_null() { return 0; }
    let gid = (*sd).groupid;
    for x in 0..MAX_THREATCOUNT {
        if (*mob).dmggrptable[x][0] as u32 == gid || (*mob).dmggrptable[x][0] == 0.0 {
            (*mob).dmggrptable[x][0] = gid as f64;
            (*mob).dmggrptable[x][1] += dmg as f64;
            return 1;
        }
    }
    0
}

/// Call a named event on this mob's custom AI script.
pub unsafe fn sl_mob_callbase(mob: *mut MobSpawnData, script: &str) -> i32 {
    if mob.is_null() { return 0; }
    let attacker = (*mob).attacker;
    let yname = crate::game::scripting::carray_to_str(&(*(*mob).data).yname);
    let attacker_id = if attacker > 0 && crate::game::map_server::entity_position(attacker).is_some() {
        attacker
    } else {
        (*mob).id
    };
    sl_doscript_2(yname, Some(script), (*mob).id, attacker_id);
    1
}

/// Return 1 if the mob can step forward in its current direction, 0 if blocked.
pub unsafe fn sl_mob_checkmove(mob: *mut MobSpawnData) -> i32 {
    if mob.is_null() { return 0; }
    let m = (*mob).m as i32;
    let mut dx = (*mob).x as i32;
    let mut dy = (*mob).y as i32;
    let direction = (*mob).side;
    match direction {
        0 => dy -= 1,
        1 => dx += 1,
        2 => dy += 1,
        3 => dx -= 1,
        _ => {}
    }
    let slot = ffi_get_map_ptr((*mob).m);
    if slot.is_null() { return 0; }
    dx = dx.max(0).min((*slot).xs as i32 - 1);
    dy = dy.max(0).min((*slot).ys as i32 - 1);
    if warp_at(slot, dx, dy) { return 0; }
    (*mob).canmove = 0;
    if let Some(grid) = block_grid::get_grid(m as usize) {
        let cell_ids = grid.ids_at_tile(dx as u16, dy as u16);
        for id in cell_ids {
            // Skip floor items — they don't block movement
            if id >= FLOORITEM_START_NUM && id < NPC_START_NUM { continue; }
            mob_move_inner_id(id, mob);
        }
    }
    if clif_object_canmove(m, dx, dy, direction) != 0 { return 0; }
    if clif_object_canmove_from(m, (*mob).x as i32, (*mob).y as i32, direction) != 0 { return 0; }
    if map_canmove(m, dx, dy) == 1 || (*mob).canmove == 1 { return 0; }
    1
}

/// Set or clear a magic-effect duration slot on the mob.
pub unsafe fn sl_mob_setduration(
    mob: *mut MobSpawnData, name: *const i8,
    mut time: i32, caster_id: u32, recast: i32,
) {
    if mob.is_null() { return; }
    let id = magic_db::id_by_name(&std::ffi::CStr::from_ptr(name).to_string_lossy());
    if time > 0 && time < 1000 { time = 1000; }
    let mut alreadycast = 0i32;
    for x in 0..MAX_MAGIC_TIMERS {
        if (*mob).da[x].id as i32 == id && (*mob).da[x].caster_id == caster_id && (*mob).da[x].duration > 0 {
            alreadycast = 1;
        }
    }
    for x in 0..MAX_MAGIC_TIMERS {
        let mid = (*mob).da[x].id as i32;
        if mid == id && time <= 0 && (*mob).da[x].caster_id == caster_id && alreadycast == 1 {
            let saved_caster_id = (*mob).da[x].caster_id;
            (*mob).da[x].duration = 0; (*mob).da[x].id = 0; (*mob).da[x].caster_id = 0;
            broadcast_animation_to_pcs(&*mob, (*mob).da[x].animation as i32);
            (*mob).da[x].animation = 0;
            let has_caster = saved_caster_id != (*mob).id
                && saved_caster_id > 0
                && crate::game::map_server::entity_position(saved_caster_id).is_some();
            let yname = magicdb_yname_str(mid);
            if has_caster { sl_doscript_2(&yname, Some("uncast"), (*mob).id, saved_caster_id); }
            else          { sl_doscript_simple(&yname, Some("uncast"), (*mob).id); }
            return;
        } else if (*mob).da[x].id as i32 == id && (*mob).da[x].caster_id == caster_id
                && ((*mob).da[x].duration > time || recast == 1) && alreadycast == 1 {
            (*mob).da[x].duration = time;
            return;
        } else if (*mob).da[x].id == 0 && (*mob).da[x].duration == 0 && time != 0 && alreadycast != 1 {
            (*mob).da[x].id = id as u16;
            (*mob).da[x].duration = time;
            (*mob).da[x].caster_id = caster_id;
            return;
        }
    }
}

/// Clear magic-effect timers in id range [minid..maxid], firing uncast Lua events.
pub unsafe fn sl_mob_flushduration(mob: *mut MobSpawnData, dis: i32, minid: i32, maxid: i32) {
    if mob.is_null() { return; }
    let maxid = if maxid < minid { minid } else { maxid };
    for x in 0..MAX_MAGIC_TIMERS {
        let id = (*mob).da[x].id as i32;
        if id == 0 { continue; }
        if magicdb_dispel(id) > dis { continue; }
        let flush = if minid <= 0 { true } else if maxid <= 0 { id == minid } else { id >= minid && id <= maxid };
        if flush {
            (*mob).da[x].duration = 0;
            broadcast_animation_to_pcs(&*mob, (*mob).da[x].animation as i32);
            (*mob).da[x].animation = 0; (*mob).da[x].id = 0;
            let cid = (*mob).da[x].caster_id;
            let has_caster = cid > 0 && crate::game::map_server::entity_position(cid).is_some();
            (*mob).da[x].caster_id = 0;
            let yname = magicdb_yname_str(id);
            if has_caster { sl_doscript_2(&yname, Some("uncast"), (*mob).id, cid); }
            else          { sl_doscript_simple(&yname, Some("uncast"), (*mob).id); }
        }
    }
}

/// Clear magic-effect timers without firing uncast Lua events.
pub unsafe fn sl_mob_flushdurationnouncast(mob: *mut MobSpawnData, dis: i32, minid: i32, maxid: i32) {
    if mob.is_null() { return; }
    let maxid = if maxid < minid { minid } else { maxid };
    for x in 0..MAX_MAGIC_TIMERS {
        let id = (*mob).da[x].id as i32;
        if id == 0 { continue; }
        if magicdb_dispel(id) > dis { continue; }
        let flush = if minid <= 0 { true } else if maxid <= 0 { id == minid } else { id >= minid && id <= maxid };
        if flush {
            (*mob).da[x].duration = 0; (*mob).da[x].caster_id = 0;
            broadcast_animation_to_pcs(&*mob, (*mob).da[x].animation as i32);
            (*mob).da[x].animation = 0; (*mob).da[x].id = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn mob_spawn_data_size() {
        const EXPECTED: usize = 61088;
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



