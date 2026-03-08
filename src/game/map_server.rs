//! Rust ports of `c_src/map_server.c` utility functions.
//!
//! Functions are migrated here one at a time as their C dependencies are removed.
//! Each `#[no_mangle]` export directly replaces its C counterpart in `libmap_game.a`.

use std::collections::HashMap;
use std::ffi::{c_char, c_ulong, c_void};
use std::os::raw::{c_int, c_uchar, c_uint, c_ushort};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::database::{blocking_run, blocking_run_async, get_pool};
use crate::game::pc::{
    MapSessionData, Sql, SqlStmt, SqlDataType, SQL_ERROR, SQL_SUCCESS,
    U_FLAG_UNPHYSICAL,
};

use crate::database::map_db::BlockList;

use crate::ffi::session::{
    rust_session_wfifohead, rust_session_wdata_ptr, rust_session_commit,
    rust_session_rdata_ptr,
};

// SQL and session C FFI needed by boards/nmail functions.
// These mirror the extern blocks in pc.rs / map_parse/*.rs.
extern "C" {
    fn rust_session_exists(fd: c_int) -> c_int;
    fn rust_session_get_eof(fd: c_int) -> c_int;
    fn rust_session_get_client_ip(fd: c_int) -> c_uint;
    fn rust_session_set_eof(fd: c_int, val: c_int);
    fn rust_session_get_data(fd: c_int) -> *mut MapSessionData;

    // fd_max — max file-descriptor index; defined in map_server binary (core.c equivalent).
    static fd_max: c_int;

    // encrypt — C function in net_crypt.c
    fn encrypt(fd: c_int) -> c_int;

    // sl_doscript_blargs — Lua call from C
    fn sl_doscript_blargs(
        root: *const c_char,
        method: *const c_char,
        n: c_int,
        ...
    ) -> c_int;

    // boarddb helpers — declared in board_db.h, implemented in Rust
    fn rust_boarddb_script(id: c_int) -> c_int;
    fn rust_boarddb_yname(id: c_int) -> *mut c_char;

    // game-global registry reader — ported to map_server.rs

    // sl_exec (= rust_sl_exec) — scripting.h
    #[link_name = "rust_sl_exec"]
    fn sl_exec(user: *mut c_void, code: *mut c_char);

    // SQL C FFI (mirrors pc.rs pub extern "C" block).
    fn Sql_Query(self_: *mut Sql, query: *const c_char, ...) -> c_int;
    fn Sql_FreeResult(self_: *mut Sql);
    fn Sql_EscapeString(self_: *mut Sql, out_to: *mut c_char, from: *const c_char) -> usize;
    fn Sql_ShowDebug_(self_: *mut Sql, file: *const c_char, line: c_ulong);
    fn SqlStmt_Malloc(sql: *mut Sql) -> *mut SqlStmt;
    fn SqlStmt_Prepare(self_: *mut SqlStmt, query: *const c_char, ...) -> c_int;
    fn SqlStmt_Execute(self_: *mut SqlStmt) -> c_int;
    fn SqlStmt_BindColumn(
        self_: *mut SqlStmt,
        idx: usize,
        buffer_type: SqlDataType,
        buffer: *mut c_void,
        buffer_len: usize,
        out_len: *mut c_ulong,
        is_null: *mut c_int,
    ) -> c_int;
    fn SqlStmt_NextRow(self_: *mut SqlStmt) -> c_int;
    fn SqlStmt_Free(self_: *mut SqlStmt);
    // The C macro SqlStmt_ShowDebug(stmt) expands to SqlStmt_ShowDebug_(stmt, __FILE__, __LINE__).
    fn SqlStmt_ShowDebug_(stmt: *mut SqlStmt, file: *const c_char, line: c_ulong);
}

// ---------------------------------------------------------------------------
// In-game time globals — ported from `c_src/map_server.c`.
//
// These are exported with `#[no_mangle]` so that C translation units that
// reference `cur_time`, `cur_day`, `cur_season`, `cur_year`, and `old_time`
// via the `extern int` declarations in `map_server.h` continue to link.
// ---------------------------------------------------------------------------

/// Current in-game hour (0–23).  Incremented by `change_time_char` every game hour.
#[no_mangle]
pub static mut cur_time: c_int = 0;

/// Current in-game day within the current season (1–91).
#[no_mangle]
pub static mut cur_day: c_int = 0;

/// Current in-game season (1–4).
#[no_mangle]
pub static mut cur_season: c_int = 0;

/// Current in-game year.
#[no_mangle]
pub static mut cur_year: c_int = 0;

/// Previous in-game hour; used by `map_weather` to detect hour transitions.
#[no_mangle]
pub static mut old_time: c_int = 0;

// ---------------------------------------------------------------------------
// Network / session globals — moved from `c_src/map_server.c`.
//
// These are `#[no_mangle]` so that C TUs (sl_compat.c, map_server_stubs.c)
// that reference them via the extern declarations in mmo.h / map_server.h
// continue to link against the single Rust-owned instance.
// ---------------------------------------------------------------------------

/// File descriptor for the char-server connection.
/// Written by `map_char.c` / the Rust map_char handler on connect.
/// Declared `extern int char_fd` in `c_src/mmo.h`.
#[no_mangle]
pub static mut char_fd: c_int = 0;

/// File descriptor for the map network socket (map listen port).
/// Declared `extern int map_fd` in `c_src/map_server.h`.
#[no_mangle]
pub static mut map_fd: c_int = 0;

/// Legacy C MySQL handle — allocated by `Sql_Malloc()` / `Sql_Connect()` in
/// `src/bin/map_server.rs` and also used by sl_compat.c C code.
/// Declared `extern Sql* sql_handle` in `c_src/map_server.h` / `mmo.h`.
#[no_mangle]
pub static mut sql_handle: *mut Sql = std::ptr::null_mut();

/// Online user list (count + per-slot char-id array).
/// Declared `extern struct userlist_data userlist` in `c_src/map_server.h`.
/// `userlist_data` is `{ unsigned int user_count; unsigned int user[10000]; }`.
/// We store it as a flat C-layout array; the Rust side only reads `user_count`.
#[repr(C)]
pub struct UserlistData {
    pub user_count: c_uint,
    pub user: [c_uint; 10000],
}

#[no_mangle]
pub static mut userlist: UserlistData = UserlistData {
    user_count: 0,
    user: [0u32; 10000],
};

/// Authentication-attempt counter.
/// Declared `extern int auth_n` in `c_src/map_server.h`.
#[no_mangle]
pub static mut auth_n: c_int = 0;

// ---------------------------------------------------------------------------
// Floor item ID pool — mirrors `object[]` / `object_n` in C map_server.c
// ---------------------------------------------------------------------------

/// Upper bound on simultaneously active floor items.
/// Matches `MAX_FLOORITEM` in `c_src/map_server.h`.
const MAX_FLOORITEM: usize = 100_000_000;

/// Bitmap tracking which floor item slots are in use (1 = occupied, 0 = free).
/// Allocated on first `map_additem` call; freed by `map_clritem`.
static mut OBJECT: *mut u8 = std::ptr::null_mut();

/// Current allocated length of `OBJECT`.
static mut OBJECT_N: usize = 0;

/// Free all floor item ID slots and release the backing memory.
///
/// Replaces `map_clritem` in `c_src/map_server.c`.
///
/// # Safety
/// Must be called on the game thread. `OBJECT` must be null or a pointer
/// previously allocated by `map_additem` via `libc::realloc`/`libc::calloc`.
#[no_mangle]
pub unsafe extern "C" fn map_clritem() {
    if !OBJECT.is_null() {
        // OBJECT was allocated via libc::calloc / libc::realloc — match with libc::free.
        libc::free(OBJECT as *mut libc::c_void);
        OBJECT = std::ptr::null_mut();
    }
    OBJECT_N = 0;
}

/// Remove a floor item from the world by its ID.
///
/// Unlinks from the ID database and block grid, then frees the `FloorItemData`
/// node. The node was allocated with `libc::calloc` by `map_additem` callers
/// (see `mob.rs`, `pc.rs`), so it is freed with `libc::free`.
///
/// Replaces `map_delitem` in `c_src/map_server.c`.
///
/// # Safety
/// `id` must be a valid floor item ID currently registered in the ID database.
#[no_mangle]
pub unsafe extern "C" fn map_delitem(id: c_uint) {
    use crate::ffi::block::map_delblock;
    let bl = map_id2bl(id) as *mut BlockList;
    if bl.is_null() {
        return;
    }
    map_deliddb(bl);
    map_delblock(bl);
    // FloorItemData nodes are always allocated via libc::calloc (mob.rs, pc.rs).
    libc::free(bl as *mut libc::c_void);

    let idx = id.wrapping_sub(crate::game::mob::FLOORITEM_START_NUM) as usize;
    if !OBJECT.is_null() && idx < OBJECT_N {
        *OBJECT.add(idx) = 0;
    }
}

/// Assign an ID to a new floor item and insert it into the world.
///
/// Scans the bitmap for the first free slot, grows the bitmap if necessary,
/// assigns the item's ID, then registers it in the ID database and block grid.
///
/// Replaces `map_additem` in `c_src/map_server.c`.
///
/// # Safety
/// - `bl` must be a valid non-null pointer to a `FloorItemData` (cast to `BlockList`),
///   allocated via `libc::calloc`, with `m`/`x`/`y` already set.
/// - Must be called on the game thread (single-threaded game loop).
#[no_mangle]
pub unsafe extern "C" fn map_additem(bl: *mut BlockList) {
    use crate::ffi::block::map_addblock;

    // Find first free slot.
    let mut i = 0usize;
    while !OBJECT.is_null() && i < OBJECT_N && *OBJECT.add(i) != 0 {
        i += 1;
    }

    if i >= MAX_FLOORITEM {
        tracing::error!("map_additem: floor item capacity exceeded ({MAX_FLOORITEM})");
        return;
    }

    // Grow bitmap if the free slot is beyond the current allocation.
    if i >= OBJECT_N {
        let new_n = i + 256;
        if OBJECT_N == 0 {
            // First allocation: calloc for a zeroed array.
            OBJECT = libc::calloc(new_n, 1) as *mut u8;
        } else {
            // Grow with realloc; zero the newly added bytes.
            let old_n = OBJECT_N;
            let old_ptr = OBJECT as *mut libc::c_void;
            OBJECT = libc::realloc(old_ptr, new_n) as *mut u8;
            if !OBJECT.is_null() {
                // Zero the newly appended bytes (realloc does not zero them).
                std::ptr::write_bytes(OBJECT.add(old_n), 0, new_n - old_n);
            } else {
                // realloc failed — free original allocation to avoid leak.
                libc::free(old_ptr);
            }
        }
        if OBJECT.is_null() {
            OBJECT_N = 0;
            tracing::error!("map_additem: realloc failed — item pool cleared");
            return;
        }
        OBJECT_N = new_n;
    }

    *OBJECT.add(i) = 1;
    let id = (i as u32).wrapping_add(crate::game::mob::FLOORITEM_START_NUM);
    (*bl).id      = id;
    (*bl).bl_type = crate::game::mob::BL_ITEM as c_uchar;
    (*bl).prev    = std::ptr::null_mut();
    (*bl).next    = std::ptr::null_mut();
    map_addiddb(bl);
    map_addblock(bl);
}

/// Acquire the deferred-free lock.
///
/// The original C implementation was commented out entirely; call sites are
/// also commented out. This is a no-op that returns 0 for ABI compatibility.
///
/// Replaces `map_freeblock_lock` stub in `c_src/map_server.c`.
#[no_mangle]
pub unsafe extern "C" fn map_freeblock_lock() -> c_int {
    0
}

/// Release the deferred-free lock.
///
/// The original C implementation was commented out entirely; call sites are
/// also commented out. This is a no-op that returns 0 for ABI compatibility.
///
/// Replaces `map_freeblock_unlock` stub in `c_src/map_server.c`.
#[no_mangle]
pub unsafe extern "C" fn map_freeblock_unlock() -> c_int {
    0
}

/// Set the IP address and port for a map slot.
///
/// Returns 0 on success, 1 if `id` is out of range.
///
/// Replaces `map_setmapip` in `c_src/map_server.c`.
///
/// # Safety
/// `crate::ffi::map_db::map` must be a valid initialized pointer (non-null, pointing to at
/// least `MAP_SLOTS` slots). Call only after `rust_map_init` has completed.
#[no_mangle]
pub unsafe extern "C" fn map_setmapip(id: c_int, ip: c_uint, port: c_ushort) -> c_int {
    if id < 0 || id as usize >= crate::database::map_db::MAP_SLOTS {
        return 1;
    }
    (*crate::ffi::map_db::map.add(id as usize)).ip = ip;
    (*crate::ffi::map_db::map.add(id as usize)).port = port;
    0
}

/// Free a block-list pointer.
///
/// The original C implementation was commented out (the entire freeblock/lock/unlock
/// block is inside `/* ... */` in `c_src/map_server.c`).  Since `map_freeblock_lock`
/// and `map_freeblock_unlock` are no-op stubs, the lock counter is always 0, so the
/// deferred-free path is unreachable.  This implementation matches the C behaviour for
/// lock == 0: free the pointer immediately with `libc::free` (matching the C `FREE`
/// macro which expands to `free()`).
///
/// Returns 0 (the lock value), matching the original C return convention.
///
/// Provides the ABI symbol declared in `c_src/map_server.h`.
///
/// # Safety
/// `bl`, if non-null, must have been allocated by the C heap allocator (`malloc`/`calloc`/
/// `realloc`) and must not be freed again after this call.
#[no_mangle]
pub unsafe extern "C" fn map_freeblock(bl: *mut c_void) -> c_int {
    if !bl.is_null() {
        libc::free(bl);
    }
    0 // lock is always 0 (stubs); matches C `return bl_free_lock`
}

// ---------------------------------------------------------------------------
// ID database — replaces C uidb_* hash table in map_server.c
// ---------------------------------------------------------------------------

static mut ID_DB: Option<HashMap<u32, *mut c_void>> = None;

unsafe fn id_db() -> &'static mut HashMap<u32, *mut c_void> {
    ID_DB.get_or_insert_with(HashMap::new)
}

#[no_mangle]
pub unsafe extern "C" fn map_initiddb() {
    id_db(); // initialise lazily
}

#[no_mangle]
pub unsafe extern "C" fn map_termiddb() {
    id_db().clear();
}

/// Returns a raw pointer to any game object (USER*, MOB*, NPC*, FLOORITEM*) by ID.
/// Returns null if not found. Callers cast the result to the appropriate type.
#[no_mangle]
pub unsafe extern "C" fn map_id2bl(id: c_uint) -> *mut c_void {
    id_db().get(&id).copied().unwrap_or(std::ptr::null_mut())
}

/// Returns the USER* for a player by character ID. NULL if not found or not a player.
#[no_mangle]
pub unsafe extern "C" fn map_id2sd(id: c_uint) -> *mut c_void {
    map_id2bl(id) // C caller casts to USER*; same raw pointer
}

#[no_mangle]
pub unsafe extern "C" fn map_addiddb(bl: *mut BlockList) {
    if bl.is_null() { return; }
    id_db().insert((*bl).id, bl as *mut c_void);
}

#[no_mangle]
pub unsafe extern "C" fn map_deliddb(bl: *mut BlockList) {
    if bl.is_null() { return; }
    id_db().remove(&(*bl).id);
}

/// Returns the MOB* for an entity by ID. Adjusts IDs below MOB_START_NUM.
///
/// Mirrors `map_id2mob` in `c_src/map_server_stubs.c`.
#[no_mangle]
pub unsafe extern "C" fn map_id2mob(mut id: c_uint) -> *mut crate::game::mob::MobSpawnData {
    use crate::game::mob::{MOB_START_NUM, BL_MOB};
    if id < MOB_START_NUM { id = id.saturating_add(MOB_START_NUM - 1); }
    let bl = map_id2bl(id) as *mut BlockList;
    if bl.is_null() { return std::ptr::null_mut(); }
    if (*bl).bl_type as c_int == BL_MOB { bl as *mut crate::game::mob::MobSpawnData } else { std::ptr::null_mut() }
}

/// Returns the NPC* for an entity by ID. Adjusts IDs below NPC_START_NUM.
///
/// Mirrors `map_id2npc` in `c_src/map_server_stubs.c`.
#[no_mangle]
pub unsafe extern "C" fn map_id2npc(id: c_uint) -> *mut crate::game::npc::NpcData {
    use crate::game::npc::NPC_START_NUM;
    let adj_id = if id < NPC_START_NUM { id.saturating_add(NPC_START_NUM - 2) } else { id };
    let bl = map_id2bl(adj_id) as *mut BlockList;
    if bl.is_null() { return std::ptr::null_mut(); }
    if (*bl).bl_type as c_int == crate::game::pc::BL_NPC { bl as *mut crate::game::npc::NpcData } else { std::ptr::null_mut() }
}

/// Returns the FLOORITEM* for an entity by ID.
///
/// Mirrors `map_id2fl` in `c_src/map_server_stubs.c`.
#[no_mangle]
pub unsafe extern "C" fn map_id2fl(id: c_uint) -> *mut c_void {
    let bl = map_id2bl(id) as *mut BlockList;
    if bl.is_null() { return std::ptr::null_mut(); }
    if (*bl).bl_type as c_int == crate::game::pc::BL_ITEM { bl as *mut c_void } else { std::ptr::null_mut() }
}

/// Find a player session by name (case-insensitive).
///
/// Mirrors `map_name2sd` in `c_src/map_server_stubs.c`.
#[no_mangle]
pub unsafe extern "C" fn map_name2sd(name: *const c_char) -> *mut MapSessionData {
    use crate::ffi::session::{rust_session_exists, rust_session_get_data, rust_session_get_eof};
    if name.is_null() { return std::ptr::null_mut(); }
    for i in 0..fd_max {
        if rust_session_exists(i) == 0 { continue; }
        if rust_session_get_eof(i) != 0 { continue; }
        let sd = rust_session_get_data(i) as *mut MapSessionData;
        if sd.is_null() { continue; }
        if libc::strcasecmp((*sd).status.name.as_ptr(), name) == 0 {
            return sd;
        }
    }
    std::ptr::null_mut()
}

/// Find an NPC by name (case-insensitive). Iterates NPC ID range.
///
/// Mirrors `map_name2npc` in `c_src/map_server_stubs.c`.
#[no_mangle]
pub unsafe extern "C" fn map_name2npc(name: *const c_char) -> *mut c_void {
    use crate::game::npc::{NPC_ID, NPC_START_NUM};
    if name.is_null() { return std::ptr::null_mut(); }
    let mut i = NPC_START_NUM as c_uint;
    while i <= NPC_ID {
        let nd = map_id2npc(i);
        if !nd.is_null() && libc::strcasecmp((*nd).npc_name.as_ptr(), name) == 0 {
            return nd as *mut c_void;
        }
        i += 1;
    }
    std::ptr::null_mut()
}

/// Reload the map registry for a single map — thin shim over `rust_map_loadregistry`.
///
/// Mirrors the C shim `map_loadregistry` in `c_src/map_server_stubs.c`.
#[no_mangle]
pub unsafe extern "C" fn map_loadregistry(id: c_int) -> c_int {
    crate::ffi::map_db::rust_map_loadregistry(id)
}

/// Read a game-global registry value by name (case-insensitive).
///
/// Mirrors `map_readglobalgamereg` in `c_src/map_server_stubs.c`.
#[no_mangle]
pub unsafe extern "C" fn map_readglobalgamereg(reg: *const c_char) -> c_int {
    if reg.is_null() || gamereg.registry.is_null() { return 0; }
    for i in 0..gamereg.registry_num as usize {
        let entry = &*gamereg.registry.add(i);
        if reg_str_eq(&entry.str, reg) { return entry.val; }
    }
    0
}

/// Timer callback — runs Lua cron hooks based on wall-clock seconds.
/// Replaces `map_cronjob` in `c_src/map_server.c`.
///
/// Registered every 1000 ms via `timer_insert` in `map_server.rs`.
/// Must be called on the Lua-owning thread (LocalSet).
#[no_mangle]
pub unsafe extern "C" fn rust_map_cronjob(_id: c_int, _n: c_int) -> c_int {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    if t % 60    == 0 { cron(b"cronJobMin\0");    }
    if t % 300   == 0 { cron(b"cronJob5Min\0");   }
    if t % 1800  == 0 { cron(b"cronJob30Min\0");  }
    if t % 3600  == 0 { cron(b"cronJobHour\0");   }
    if t % 86400 == 0 { cron(b"cronJobDay\0");    }
    cron(b"cronJobSec\0");
    0
}

#[inline]
unsafe fn cron(name: &[u8]) {
    crate::game::scripting::sl_doscript_blargs_vec(
        name.as_ptr() as *const c_char,
        std::ptr::null(),
        0,
        std::ptr::null(),
    );
}

// ---------------------------------------------------------------------------
// Session state helpers
// ---------------------------------------------------------------------------

/// Returns 1 if `sd` is non-null and has an active session.
/// Mirrors `isPlayerActive` in `c_src/map_server.c`.
#[no_mangle]
pub unsafe extern "C" fn isPlayerActive(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() { return 0; }
    let fd = (*sd).fd;
    if fd == 0 { return 0; }
    if rust_session_exists(fd) == 0 {
        let name = std::ffi::CStr::from_ptr((*sd).status.name.as_ptr());
        eprintln!("[map] isPlayerActive: player exists but session does not ({})", name.to_string_lossy());
        return 0;
    }
    1
}

/// Returns 1 if `sd` has a live session with no EOF flag set.
/// Mirrors `isActive` in `c_src/map_server.c`.
#[no_mangle]
pub unsafe extern "C" fn isActive(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() { return 0; }
    let fd = (*sd).fd;
    if rust_session_exists(fd) == 0 { return 0; }
    if rust_session_get_eof(fd) != 0 { return 0; }
    1
}

// ---------------------------------------------------------------------------
// Online status
// ---------------------------------------------------------------------------

/// Updates `Character.ChaOnline`/`ChaLastIP` and fires the "login" Lua hook on first login.
/// Mirrors `mmo_setonline` in `c_src/map_server.c`.
#[no_mangle]
pub unsafe extern "C" fn mmo_setonline(id: c_uint, val: c_int) {
    let sd = map_id2sd(id) as *mut MapSessionData;
    if sd.is_null() { return; }

    let fd = (*sd).fd;
    // rust_session_get_client_ip returns IP in network byte order (sin_addr.s_addr).
    // The C code decomposes it as: a = ip & 0xff, b = (ip>>8)&0xff, c = (ip>>16)&0xff, d = (ip>>24)&0xff.
    let raw_ip = rust_session_get_client_ip(fd);
    let addr = format!(
        "{}.{}.{}.{}",
        raw_ip & 0xff,
        (raw_ip >> 8) & 0xff,
        (raw_ip >> 16) & 0xff,
        (raw_ip >> 24) & 0xff,
    );

    // Check character exists, then fire login script.
    let char_id = id;
    let exists: bool = blocking_run_async(async move {
        let pool = get_pool();
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM `Character` WHERE `ChaId` = ?"
        )
        .bind(char_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0) > 0
    });

    if exists && val != 0 {
        // status.name is [i8; 16] — convert to CStr for display.
        let name_ptr = (*sd).status.name.as_ptr() as *const std::ffi::c_char;
        println!("[map] [login] name={} addr={}",
            std::ffi::CStr::from_ptr(name_ptr).to_string_lossy(), addr);

        // Fire "login" Lua hook: sl_doscript_blargs("login", NULL, 1, &sd->bl)
        let bl_ptr = std::ptr::addr_of_mut!((*sd).bl) as *mut c_void;
        crate::game::scripting::sl_doscript_blargs_vec(
            b"login\0".as_ptr() as *const std::ffi::c_char,
            std::ptr::null(),
            1,
            &bl_ptr as *const *mut c_void,
        );
    }

    // Update online status + last IP regardless of whether character was found in SELECT.
    blocking_run_async(async move {
        let pool = get_pool();
        let _ = sqlx::query(
            "UPDATE `Character` SET `ChaOnline` = ?, `ChaLastIP` = ? WHERE `ChaId` = ?"
        )
        .bind(val)
        .bind(&addr)
        .bind(char_id)
        .execute(pool)
        .await;
    });
}

// ---------------------------------------------------------------------------
// Block grid helpers — map_canmove, map_addmob
// ---------------------------------------------------------------------------

extern "C" {
    static serverid: c_int;
}

/// Returns 1 if the cell `(x, y)` on map `m` is passable, 0 otherwise.
///
/// The `pass` tile array stores the char-ID of the player occupying each cell
/// (non-zero means occupied). A cell with a player is treated as blocked unless
/// that player has `uFlag_unphysical` set.
///
/// Mirrors `map_canmove` in `c_src/map_server.c`.
///
/// # Safety
/// `m` must be a valid loaded map index. `x` and `y` must be within bounds.
#[no_mangle]
pub unsafe extern "C" fn map_canmove(m: c_int, x: c_int, y: c_int) -> c_int {
    // read_pass(m, x, y) expands to map[m].pass[x + y * map[m].xs]
    let slot = &*crate::ffi::map_db::map.add(m as usize);
    let pass_val = *slot.pass.add(x as usize + y as usize * slot.xs as usize);

    if pass_val != 0 {
        // A player ID is stored in the pass cell. Look them up.
        let sd = map_id2sd(pass_val as c_uint) as *mut MapSessionData;
        if sd.is_null() || ((*sd).uFlags & U_FLAG_UNPHYSICAL) == 0 {
            // Cell is occupied by a physical player — blocked.
            return 1;
        }
    }

    0
}

/// Insert a new mob spawn record for the map/position of `sd` into the
/// `Spawns<serverid>` DB table.
///
/// Mirrors `map_addmob` in `c_src/map_server.c`.
///
/// # Safety
/// `sd` must be a valid, non-null `MapSessionData` pointer.
#[no_mangle]
pub unsafe extern "C" fn map_addmob(
    sd:      *mut MapSessionData,
    id:      c_uint,
    start:   c_int,
    end:     c_int,
    replace: c_uint,
) -> c_int {
    let m     = (*sd).bl.m  as i32;
    let x     = (*sd).bl.x  as i32;
    let y     = (*sd).bl.y  as i32;
    let sid   = serverid;

    let sql = format!(
        "INSERT INTO `Spawns{sid}` \
         (`SpnMapId`, `SpnX`, `SpnY`, `SpnMobId`, `SpnLastDeath`, \
          `SpnStartTime`, `SpnEndTime`, `SpnMobIdReplace`) \
         VALUES(?, ?, ?, ?, 0, ?, ?, ?)"
    );

    blocking_run_async(async move {
        let pool = get_pool();
        let _ = sqlx::query(&sql)
            .bind(m)
            .bind(x)
            .bind(y)
            .bind(id)
            .bind(start)
            .bind(end)
            .bind(replace)
            .execute(pool)
            .await;
    });

    0
}

// ---------------------------------------------------------------------------
// Board / N-Mail packet constants (from mmo.h / map_server.h)
// ---------------------------------------------------------------------------

const BOARD_CAN_WRITE: c_int = 1;
const BOARD_CAN_DEL:   c_int = 2;

// ---------------------------------------------------------------------------
// Board / N-Mail inter-server struct layouts
//
// These #[repr(C)] structs mirror the C structs in mmo.h exactly.  They are
// only used to build inter-server packets that are memcpy'd into the WFIFO
// buffer; they are never exported through cbindgen (all excluded below).
// ---------------------------------------------------------------------------

/// `struct board_show_0` from mmo.h — inter-server packet body for 0x3009.
#[repr(C)]
struct BoardShow0 {
    fd:     c_int,
    board:  c_int,
    bcount: c_int,
    flags:  c_int,
    popup:  i8,
    name:   [i8; 16],
}

/// `struct boards_read_post_0` from mmo.h — inter-server packet body for 0x300A.
#[repr(C)]
struct BoardsReadPost0 {
    name:   [i8; 16],
    fd:     c_int,
    post:   c_int,
    board:  c_int,
    flags:  c_int,
}

/// `struct boards_post_0` from mmo.h — inter-server packet body for 0x300C.
#[repr(C)]
struct BoardsPost0 {
    fd:    c_int,
    board: c_int,
    nval:  c_int,
    name:  [i8; 16],
    topic: [i8; 53],
    post:  [i8; 4001],
}

// ---------------------------------------------------------------------------
// Inline helpers for WFIFO writes to char_fd
// ---------------------------------------------------------------------------

/// Write `val` as a little-endian u16 into the char_fd WFIFO at `pos`.
#[inline]
unsafe fn wfifow_char(pos: usize, val: u16) {
    let p = rust_session_wdata_ptr(char_fd, pos) as *mut u16;
    if !p.is_null() { p.write_unaligned(val.to_le()); }
}

/// Write `count` bytes from `src` into the char_fd WFIFO starting at `pos`.
#[inline]
unsafe fn wfifop_copy_char(pos: usize, src: *const u8, count: usize) {
    let dst = rust_session_wdata_ptr(char_fd, pos);
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(src, dst, count);
    }
}

// ---------------------------------------------------------------------------
// nmail_sendmessage — sends a notification message packet to the player's fd.
//
// Mirrors `nmail_sendmessage` in `c_src/map_server.c`.
// Packet layout (pre-encryption):
//   [0]     = 0xAA  (magic)
//   [1..2]  = SWAP16(len+5)  (big-endian total payload len)
//   [3]     = 0x31  (packet id)
//   [4]     = 0x03  (sub-id)
//   [5]     = other (byte)
//   [6]     = type  (byte)
//   [7]     = strlen(message)  (byte)
//   [8..]   = message (null-terminated)
//   [len+7] = 0x07  (terminator)
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn nmail_sendmessage(
    sd:      *mut MapSessionData,
    message: *const c_char,
    other:   c_int,
    r#type:  c_int,
) -> c_int {
    if isPlayerActive(sd) == 0 { return 0; }

    let fd = (*sd).fd;
    if rust_session_exists(fd) == 0 {
        rust_session_set_eof(fd, 8);
        return 0;
    }

    let msg_len = libc_strlen(message);

    rust_session_wfifohead(fd, 65535 + 3);
    let p0 = rust_session_wdata_ptr(fd, 0);
    if p0.is_null() { return 0; }

    *p0 = 0xAA_u8;
    *rust_session_wdata_ptr(fd, 3) = 0x31_u8;
    *rust_session_wdata_ptr(fd, 4) = 0x03_u8;
    *rust_session_wdata_ptr(fd, 5) = other as u8;
    *rust_session_wdata_ptr(fd, 6) = r#type as u8;
    *rust_session_wdata_ptr(fd, 7) = msg_len as u8;
    // copy message bytes (replicating C strcpy, without the null — it is overwritten by the sentinel).
    // C does: len = strlen(message); len++ — effective length is N+1.
    std::ptr::copy_nonoverlapping(
        message as *const u8,
        rust_session_wdata_ptr(fd, 8),
        msg_len,
    );
    *rust_session_wdata_ptr(fd, msg_len + 8) = 0x07_u8; // 0x07 sentinel at [8+N] (matches C: strcpy null is overwritten)
    // big-endian packet length field at [1..2]: (N+1) + 5 = N + 6
    let size_be = ((msg_len as u16) + 6).to_be();
    (rust_session_wdata_ptr(fd, 1) as *mut u16).write_unaligned(size_be);

    let enc_len = encrypt(fd) as usize;
    rust_session_commit(fd, enc_len);
    0
}

// ---------------------------------------------------------------------------
// boards_delete — forwards delete request to char-server (packet 0x3008).
//
// Mirrors `boards_delete` in `c_src/map_server.c`.
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn boards_delete(sd: *mut MapSessionData, board: c_int) -> c_int {
    if sd.is_null() { return 0; }

    // Read the post id from the player's recv buffer (big-endian u16 at offset 8).
    let post = {
        let p = rust_session_rdata_ptr((*sd).fd, 8) as *const u16;
        if p.is_null() { return 0; }
        u16::from_be(p.read_unaligned()) as c_int
    };

    if char_fd == 0 { return 0; }

    // Packet 0x3008 is 28 bytes:
    //   [0..1]   = 0x3008 (opcode, LE)
    //   [2..3]   = sd->fd
    //   [4..5]   = gm_level
    //   [6..7]   = board_candel
    //   [8..9]   = board
    //   [10..11] = post
    //   [12..27] = name (16 bytes)
    const PKT_LEN: usize = 28;
    rust_session_wfifohead(char_fd, PKT_LEN);
    wfifow_char(0, 0x3008_u16);
    wfifow_char(2, (*sd).fd as u16);
    wfifow_char(4, (*sd).status.gm_level as u8 as u16);
    wfifow_char(6, (*sd).board_candel as u16);
    wfifow_char(8, board as u16);
    wfifow_char(10, post as u16);
    wfifop_copy_char(12, (*sd).status.name.as_ptr() as *const u8, 16);
    rust_session_commit(char_fd, PKT_LEN);
    0
}

// ---------------------------------------------------------------------------
// boards_showposts — sets board flags on `sd`, then forwards to char-server.
//
// Mirrors `boards_showposts` in `c_src/map_server.c`.
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn boards_showposts(
    sd:    *mut MapSessionData,
    board: c_int,
) -> c_int {
    if sd.is_null() { return 0; }

    (*sd).board_canwrite = 0;
    (*sd).board_candel   = 0;
    (*sd).boardnameval   = 0;

    if board == 0 {
        // Board 0 == NMail — always writable/deletable
        (*sd).board_canwrite = 1;
        (*sd).board_candel   = 1;
    } else {
        (*sd).board = board;
        if rust_boarddb_script(board) != 0 {
            let yname = rust_boarddb_yname(board);
            sl_doscript_blargs(
                yname,
                b"check\0".as_ptr() as *const c_char,
                1,
                std::ptr::addr_of_mut!((*sd).bl),
            );
        } else {
            (*sd).board_canwrite = 1;
        }
        if (*sd).status.gm_level == 99 {
            (*sd).board_canwrite = 1;
            (*sd).board_candel   = 1;
        }
    }

    let mut flags: c_int = 0;
    if (*sd).board_canwrite != 0 {
        if (*sd).board_canwrite == 6 {
            flags = 6; // special write flag
        } else {
            flags |= BOARD_CAN_WRITE;
        }
    }
    if (*sd).board_candel != 0 {
        flags |= BOARD_CAN_DEL;
    }

    let mut a = BoardShow0 {
        fd:     (*sd).fd,
        board,
        bcount: (*sd).bcount,
        flags,
        popup:  (*sd).board_popup as i8,
        name:   [0i8; 16],
    };
    std::ptr::copy_nonoverlapping(
        (*sd).status.name.as_ptr(),
        a.name.as_mut_ptr(),
        16,
    );

    if char_fd == 0 { return 0; }

    let pkt_size = std::mem::size_of::<BoardShow0>() + 2;
    rust_session_wfifohead(char_fd, pkt_size);
    wfifow_char(0, 0x3009_u16);
    wfifop_copy_char(
        2,
        std::ptr::addr_of!(a) as *const u8,
        std::mem::size_of::<BoardShow0>(),
    );
    rust_session_commit(char_fd, pkt_size);
    0
}

// ---------------------------------------------------------------------------
// boards_readpost — sets board flags and forwards read-post request.
//
// Mirrors `boards_readpost` in `c_src/map_server.c`.
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn boards_readpost(
    sd:    *mut MapSessionData,
    board: c_int,
    post:  c_int,
) -> c_int {
    if board != 0 {
        (*sd).board = board;
        if rust_boarddb_script(board) != 0 {
            let yname = rust_boarddb_yname(board);
            sl_doscript_blargs(
                yname,
                b"check\0".as_ptr() as *const c_char,
                1,
                std::ptr::addr_of_mut!((*sd).bl),
            );
        } else {
            (*sd).board_canwrite = 1;
        }
        if (*sd).status.gm_level == 99 {
            (*sd).board_canwrite = 1;
            (*sd).board_candel   = 1;
        }
    }

    let mut flags: c_int = 0;
    if (*sd).board_canwrite != 0 { flags |= BOARD_CAN_WRITE; }
    if (*sd).board_candel   != 0 { flags |= BOARD_CAN_DEL;   }

    let mut header = BoardsReadPost0 {
        name:  [0i8; 16],
        fd:    (*sd).fd,
        post,
        board,
        flags,
    };
    std::ptr::copy_nonoverlapping(
        (*sd).status.name.as_ptr(),
        header.name.as_mut_ptr(),
        16,
    );

    if char_fd == 0 { return 0; }

    let pkt_size = std::mem::size_of::<BoardsReadPost0>() + 2;
    rust_session_wfifohead(char_fd, pkt_size);
    wfifow_char(0, 0x300A_u16);
    wfifop_copy_char(
        2,
        std::ptr::addr_of!(header) as *const u8,
        std::mem::size_of::<BoardsReadPost0>(),
    );
    rust_session_commit(char_fd, pkt_size);
    0
}

// ---------------------------------------------------------------------------
// boards_post — reads post data from the player's recv buffer, validates it,
// and forwards to char-server (packet 0x300C).
//
// Mirrors `boards_post` in `c_src/map_server.c`.
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn boards_post(sd: *mut MapSessionData, board: c_int) -> c_int {
    if sd.is_null() { return 0; }

    let fd = (*sd).fd;

    let topiclen = *rust_session_rdata_ptr(fd, 8) as usize;
    if topiclen > 52 {
        clif_Hacker(
            (*sd).status.name.as_mut_ptr() as *mut c_char,
            b"Board hacking: TOPIC HACK\0".as_ptr() as *const c_char,
        );
        return 0;
    }

    let postlen = {
        let p = rust_session_rdata_ptr(fd, topiclen + 9) as *const u16;
        if p.is_null() { return 0; }
        u16::from_be(p.read_unaligned()) as usize
    };
    if postlen > 4000 {
        clif_Hacker(
            (*sd).status.name.as_mut_ptr() as *mut c_char,
            b"Board hacking: POST(BODY) HACK\0".as_ptr() as *const c_char,
        );
        return 0;
    }

    if topiclen == 0 {
        nmail_sendmessage(
            sd,
            b"Post must contain subject.\0".as_ptr() as *const c_char,
            6, 0,
        );
        return 0;
    }
    if postlen == 0 {
        nmail_sendmessage(
            sd,
            b"Post must contain a body.\0".as_ptr() as *const c_char,
            6, 0,
        );
        return 0;
    }

    let mut header = BoardsPost0 {
        fd: (*sd).fd,
        board,
        nval: (*sd).boardnameval as c_int,
        name:  [0i8; 16],
        topic: [0i8; 53],
        post:  [0i8; 4001],
    };
    std::ptr::copy_nonoverlapping((*sd).status.name.as_ptr(), header.name.as_mut_ptr(), 16);
    std::ptr::copy_nonoverlapping(
        rust_session_rdata_ptr(fd, 9) as *const i8,
        header.topic.as_mut_ptr(),
        topiclen,
    );
    std::ptr::copy_nonoverlapping(
        rust_session_rdata_ptr(fd, topiclen + 11) as *const i8,
        header.post.as_mut_ptr(),
        postlen,
    );

    if (*sd).status.gm_level != 0 {
        header.nval = 1;
    }

    if char_fd == 0 { return 0; }

    let pkt_size = std::mem::size_of::<BoardsPost0>() + 2;
    rust_session_wfifohead(char_fd, pkt_size);
    wfifow_char(0, 0x300C_u16);
    wfifop_copy_char(
        2,
        std::ptr::addr_of!(header) as *const u8,
        std::mem::size_of::<BoardsPost0>(),
    );
    rust_session_commit(char_fd, pkt_size);
    0
}

// ---------------------------------------------------------------------------
// nmail_read — body is entirely commented out in C; stub that returns 0.
//
// Mirrors `nmail_read` in `c_src/map_server.c`.
// The original SQL implementation was removed long ago (left as commented-out
// code). This function is kept as a noop stub.
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn nmail_read(_sd: *mut MapSessionData, _post: c_int) -> c_int {
    0
}

// ---------------------------------------------------------------------------
// nmail_luascript — inserts a Lua-mail record and runs `sl_exec`.
//
// Uses C FFI SQL (Sql_Query) to match the original pattern.
// Mirrors `nmail_luascript` in `c_src/map_server.c`.
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn nmail_luascript(
    sd:     *mut MapSessionData,
    to:     c_int,
    topic:  c_int,
    msg:    c_int,
) -> c_int {
    let fd = (*sd).fd;
    let mut message = [0i8; 4000];
    let mut escape  = [0i8; 4000];

    std::ptr::copy_nonoverlapping(
        rust_session_rdata_ptr(fd, (to + topic + 12) as usize) as *const i8,
        message.as_mut_ptr(),
        msg as usize,
    );

    Sql_EscapeString(sql_handle, escape.as_mut_ptr(), message.as_ptr());

    if SQL_ERROR == Sql_Query(
        sql_handle,
        c"INSERT INTO `Mail` (`MalChaName`, `MalChaNameDestination`, `MalBody`) VALUES ('%s', 'Lua', '%s')".as_ptr(),
        (*sd).status.name.as_ptr(),
        escape.as_ptr(),
    ) {
        Sql_ShowDebug_(sql_handle, c"map_server.rs".as_ptr(), line!() as c_ulong);
        return 0;
    }

    sl_exec(sd as *mut c_void, message.as_mut_ptr());
    0
}

// ---------------------------------------------------------------------------
// nmail_poemscript — validates, deduplicates, and inserts a poem board post.
//
// Uses C FFI SqlStmt + Sql_Query to match the original pattern.
// Mirrors `nmail_poemscript` in `c_src/map_server.c`.
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn nmail_poemscript(
    sd:      *mut MapSessionData,
    topic:   *const c_char,
    message: *const c_char,
) -> c_int {
    use chrono::Datelike as _;

    // Use chrono::Local::now() to match C's localtime(&t) behaviour.
    // month0() is 0-based (January = 0), matching C's tm_mon.
    // day()   is 1-based (1..=31),      matching C's tm_mday.
    let now   = chrono::Local::now();
    let month = now.month0() as c_int;
    let day   = now.day()    as c_int;

    let stmt = SqlStmt_Malloc(sql_handle);
    if stmt.is_null() {
        SqlStmt_ShowDebug_(stmt, c"map_server.rs".as_ptr(), line!() as c_ulong);
        return -1;
    }

    // Check whether the player already submitted a poem this cycle.
    let mut poemid: c_uint = 0;
    if SQL_ERROR == SqlStmt_Prepare(
        stmt,
        c"SELECT `BrdId` FROM `Boards` WHERE `BrdBnmId` = '19' AND `BrdChaId` = '%d'".as_ptr(),
        (*sd).status.id,
    ) || SQL_ERROR == SqlStmt_Execute(stmt)
      || SQL_ERROR == SqlStmt_BindColumn(
          stmt, 0,
          SqlDataType::SqlDtUInt,
          std::ptr::addr_of_mut!(poemid) as *mut c_void,
          0, std::ptr::null_mut(), std::ptr::null_mut(),
      )
    {
        SqlStmt_ShowDebug_(stmt, c"map_server.rs".as_ptr(), line!() as c_ulong);
        SqlStmt_Free(stmt);
        return 0;
    }

    if SQL_SUCCESS == SqlStmt_NextRow(stmt) {
        // Poem already submitted.
        nmail_sendmessage(
            sd,
            b"You have already submitted a poem.\0".as_ptr() as *const c_char,
            6, 1,
        );
        SqlStmt_Free(stmt);
        return 0;
    }

    // Escape strings for safe SQL insertion.
    let mut escape_topic   = [0i8; 52];
    let mut escape_message = [0i8; 4000];
    Sql_EscapeString(sql_handle, escape_topic.as_mut_ptr(),   topic);
    Sql_EscapeString(sql_handle, escape_message.as_mut_ptr(), message);

    // Find the current maximum board position.
    let mut boardpos: c_uint = 0;
    if SQL_ERROR == SqlStmt_Prepare(
        stmt,
        c"SELECT MAX(`BrdPosition`) FROM `Boards` WHERE `BrdBnmId` = '19'".as_ptr(),
    ) || SQL_ERROR == SqlStmt_Execute(stmt)
      || SQL_ERROR == SqlStmt_BindColumn(
          stmt, 0,
          SqlDataType::SqlDtUInt,
          std::ptr::addr_of_mut!(boardpos) as *mut c_void,
          0, std::ptr::null_mut(), std::ptr::null_mut(),
      )
    {
        SqlStmt_ShowDebug_(stmt, c"map_server.rs".as_ptr(), line!() as c_ulong);
        SqlStmt_Free(stmt);
        return 0;
    }
    // Advance row (or use boardpos=0 if no rows yet).
    let _ = SqlStmt_NextRow(stmt);

    SqlStmt_Free(stmt);

    if SQL_ERROR == Sql_Query(
        sql_handle,
        c"INSERT INTO `Boards` (`BrdBnmId`, `BrdChaName`, `BrdChaId`, `BrdTopic`, `BrdPost`, `BrdMonth`, `BrdDay`, `BrdPosition`) VALUES ('19', '%s', '%d', '%s', '%s', '%d', '%d', '%u')".as_ptr(),
        b"Anonymous\0".as_ptr() as *const c_char,
        (*sd).status.id,
        escape_topic.as_ptr(),
        escape_message.as_ptr(),
        month,
        day,
        boardpos.saturating_add(1),
    ) {
        Sql_ShowDebug_(sql_handle, c"map_server.rs".as_ptr(), line!() as c_ulong);
        Sql_FreeResult(sql_handle);
        return 1;
    }

    nmail_sendmessage(
        sd,
        b"Poem submitted.\0".as_ptr() as *const c_char,
        6, 1,
    );
    0
}

// ---------------------------------------------------------------------------
// nmail_sendmailcopy — forwards a copy-to-self mail to the char-server.
//
// Mirrors `nmail_sendmailcopy` in `c_src/map_server.c`.
// Packet 0x300F:
//   [0..1]     = 0x300F
//   [2..3]     = sd->fd
//   [4..19]    = from name (16 bytes)
//   [20..35]   = to_user (16 bytes, C copies up to 16 chars)
//   [72..123]  = topic (52 bytes)
//   [124..4123]= message (4000 bytes)
// Total: 4124 bytes.
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn nmail_sendmailcopy(
    sd:      *mut MapSessionData,
    to_user: *const c_char,
    topic:   *const c_char,
    message: *const c_char,
) -> c_int {
    if libc_strlen(to_user) > 16
        || libc_strlen(topic) > 52
        || libc_strlen(message) > 4000
    {
        return 0;
    }
    if char_fd == 0 { return 0; }

    const PKT_LEN: usize = 4124;
    rust_session_wfifohead(char_fd, PKT_LEN);
    wfifow_char(0, 0x300F_u16);
    wfifow_char(2, (*sd).fd as u16);
    wfifop_copy_char(4,   (*sd).status.name.as_ptr() as *const u8, 16);
    wfifop_copy_char(20,  to_user as *const u8, 16);
    wfifop_copy_char(72,  topic   as *const u8, 52);
    wfifop_copy_char(124, message as *const u8, 4000);
    rust_session_commit(char_fd, PKT_LEN);
    0
}

// ---------------------------------------------------------------------------
// nmail_write — parses incoming mail write packet, dispatches to Lua/poem/mail.
//
// Mirrors `nmail_write` in `c_src/map_server.c`.
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn nmail_write(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() { return 0; }
    let fd = (*sd).fd;

    let tolen = *rust_session_rdata_ptr(fd, 8) as usize;
    if tolen > 52 {
        clif_Hacker(
            (*sd).status.name.as_mut_ptr() as *mut c_char,
            b"NMAIL: To User\0".as_ptr() as *const c_char,
        );
        return 0;
    }
    let topiclen = *rust_session_rdata_ptr(fd, tolen + 9) as usize;
    if topiclen > 52 {
        clif_Hacker(
            (*sd).status.name.as_mut_ptr() as *mut c_char,
            b"NMAIL: Topic\0".as_ptr() as *const c_char,
        );
        return 0;
    }
    let messagelen = {
        let p = rust_session_rdata_ptr(fd, tolen + topiclen + 10) as *const u16;
        if p.is_null() { return 0; }
        u16::from_be(p.read_unaligned()) as usize
    };
    if messagelen > 4000 {
        clif_Hacker(
            (*sd).status.name.as_mut_ptr() as *mut c_char,
            b"NMAIL: Message\0".as_ptr() as *const c_char,
        );
        return 0;
    }

    let mut to_user  = [0i8; 52];
    let mut topic    = [0i8; 52];
    let mut message  = [0i8; 4000];

    std::ptr::copy_nonoverlapping(
        rust_session_rdata_ptr(fd, 9) as *const i8,
        to_user.as_mut_ptr(), tolen,
    );
    std::ptr::copy_nonoverlapping(
        rust_session_rdata_ptr(fd, tolen + 10) as *const i8,
        topic.as_mut_ptr(), topiclen,
    );
    std::ptr::copy_nonoverlapping(
        rust_session_rdata_ptr(fd, topiclen + tolen + 12) as *const i8,
        message.as_mut_ptr(), messagelen,
    );
    let send_copy = *rust_session_rdata_ptr(fd, topiclen + tolen + 12 + messagelen) as c_int;

    // Case: "lua" — run Lua script mail
    let to_user_cstr = std::ffi::CStr::from_ptr(to_user.as_ptr());
    let to_user_lower = to_user_cstr.to_string_lossy().to_ascii_lowercase();

    if to_user_lower == "lua" {
        std::ptr::copy_nonoverlapping(
            message.as_ptr(),
            (*sd).mail.as_mut_ptr(),
            messagelen.min((*sd).mail.len()),
        );
        (*sd).luaexec = 0;
        sl_doscript_blargs(
            b"canRunLuaMail\0".as_ptr() as *const c_char,
            std::ptr::null(),
            1,
            std::ptr::addr_of_mut!((*sd).bl),
        );
        if (*sd).status.gm_level == 99 || (*sd).luaexec != 0 {
            nmail_luascript(sd, tolen as c_int, topiclen as c_int, messagelen as c_int);
            nmail_sendmessage(
                sd,
                b"LUA script ran!\0".as_ptr() as *const c_char,
                6, 1,
            );
            return 0; // only return if we actually handled the Lua mail
        }
        // permission denied — fall through to poems/standard mail
    }

    // Case: "poems" / "poem"
    if to_user_lower == "poems" || to_user_lower == "poem" {
        if map_readglobalgamereg(b"poemAccept\0".as_ptr() as *const c_char) == 0 {
            nmail_sendmessage(
                sd,
                b"Currently not accepting poem submissions.\0".as_ptr() as *const c_char,
                6, 0,
            );
            return 0;
        }

        std::ptr::copy_nonoverlapping(
            message.as_ptr(),
            (*sd).mail.as_mut_ptr(),
            messagelen.min((*sd).mail.len()),
        );

        if topiclen == 0 {
            nmail_sendmessage(
                sd,
                b"Mail must contain a subject.\0".as_ptr() as *const c_char,
                6, 0,
            );
            return 0;
        }
        if messagelen == 0 {
            nmail_sendmessage(
                sd,
                b"Mail must contain a body.\0".as_ptr() as *const c_char,
                6, 0,
            );
            return 0;
        }

        nmail_poemscript(sd, topic.as_ptr(), message.as_ptr());
        return 0;
    }

    // Standard mail
    if topiclen == 0 {
        nmail_sendmessage(
            sd,
            b"Mail must contain a subject.\0".as_ptr() as *const c_char,
            6, 0,
        );
        return 0;
    }
    if messagelen == 0 {
        nmail_sendmessage(
            sd,
            b"Mail must contain a body.\0".as_ptr() as *const c_char,
            6, 0,
        );
        return 0;
    }

    nmail_sendmail(sd, to_user.as_ptr(), topic.as_ptr(), message.as_ptr());

    if send_copy != 0 {
        // Build "[To NAME] original_topic" (truncated to 51 chars + null).
        let to_str = std::ffi::CStr::from_ptr(to_user.as_ptr()).to_string_lossy();
        let tp_str = std::ffi::CStr::from_ptr(topic.as_ptr()).to_string_lossy();
        let mut a_topic = format!("[To {}] {}", to_str, tp_str);
        a_topic.truncate(51);
        let a_topic_c = std::ffi::CString::new(a_topic).unwrap_or_default();
        nmail_sendmailcopy(
            sd,
            (*sd).status.name.as_ptr() as *const c_char,
            a_topic_c.as_ptr(),
            message.as_ptr(),
        );
    }

    0
}

// ---------------------------------------------------------------------------
// nmail_sendmail — forwards a mail message to the char-server (packet 0x300D).
//
// Packet layout is identical to nmail_sendmailcopy (0x300F) but uses 0x300D.
// Mirrors `nmail_sendmail` in `c_src/map_server.c`.
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn nmail_sendmail(
    sd:      *mut MapSessionData,
    to_user: *const c_char,
    topic:   *const c_char,
    message: *const c_char,
) -> c_int {
    if libc_strlen(to_user) > 16
        || libc_strlen(topic) > 52
        || libc_strlen(message) > 4000
    {
        return 0;
    }
    if char_fd == 0 { return 0; }

    const PKT_LEN: usize = 4124;
    rust_session_wfifohead(char_fd, PKT_LEN);
    wfifow_char(0, 0x300D_u16);
    wfifow_char(2, (*sd).fd as u16);
    wfifop_copy_char(4,   (*sd).status.name.as_ptr() as *const u8, 16);
    wfifop_copy_char(20,  to_user as *const u8, 16);
    wfifop_copy_char(72,  topic   as *const u8, 52);
    wfifop_copy_char(124, message as *const u8, 4000);
    rust_session_commit(char_fd, PKT_LEN);
    0
}

// ---------------------------------------------------------------------------
// map_changepostcolor — SQL UPDATE to set board post highlight color.
//
// Uses C FFI Sql_Query to match the original pattern.
// Mirrors `map_changepostcolor` in `c_src/map_server.c`.
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn map_changepostcolor(
    board: c_int,
    post:  c_int,
    color: c_int,
) -> c_int {
    if SQL_ERROR == Sql_Query(
        sql_handle,
        c"UPDATE `Boards` SET `BrdHighlighted` = '%d' WHERE `BrdBnmId` = '%d' AND `BrdPosition` = '%d'".as_ptr(),
        color, board, post,
    ) {
        Sql_ShowDebug_(sql_handle, c"map_server.rs".as_ptr(), line!() as c_ulong);
    }
    0
}

// ---------------------------------------------------------------------------
// map_getpostcolor — SQL SELECT to retrieve board post highlight color.
//
// Uses C FFI SqlStmt to match the original pattern.
// Mirrors `map_getpostcolor` in `c_src/map_server.c`.
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn map_getpostcolor(board: c_int, post: c_int) -> c_int {
    let stmt = SqlStmt_Malloc(sql_handle);
    if stmt.is_null() {
        SqlStmt_ShowDebug_(stmt, c"map_server.rs".as_ptr(), line!() as c_ulong);
        return -1;
    }

    let mut color: c_int = 0;

    if SQL_ERROR == SqlStmt_Prepare(
        stmt,
        c"SELECT `BrdHighlighted` FROM `Boards` WHERE `BrdBnmId` = '%d' AND `BrdPosition` = '%d'".as_ptr(),
        board, post,
    ) || SQL_ERROR == SqlStmt_Execute(stmt)
      || SQL_ERROR == SqlStmt_BindColumn(
          stmt, 0,
          SqlDataType::SqlDtInt,
          std::ptr::addr_of_mut!(color) as *mut c_void,
          0, std::ptr::null_mut(), std::ptr::null_mut(),
      )
    {
        Sql_ShowDebug_(sql_handle, c"map_server.rs".as_ptr(), line!() as c_ulong);
        SqlStmt_Free(stmt);
        return -1;
    }

    let _ = SqlStmt_NextRow(stmt);
    SqlStmt_Free(stmt);
    color
}

// ---------------------------------------------------------------------------
// libc_strlen — safe strlen wrapper for *const c_char inputs.
// Used by length-check guards in nmail_sendmail/nmail_sendmailcopy.
// ---------------------------------------------------------------------------

#[inline]
unsafe fn libc_strlen(s: *const c_char) -> usize {
    if s.is_null() { return 0; }
    std::ffi::CStr::from_ptr(s).to_bytes().len()
}

// ---------------------------------------------------------------------------
// clif_Hacker — declare here (not in existing extern block).
// ---------------------------------------------------------------------------

extern "C" {
    fn clif_Hacker(name: *mut c_char, reason: *const c_char) -> c_int;
}

// ---------------------------------------------------------------------------
// Language / message table — map_msg[] and lang_read
// ---------------------------------------------------------------------------
//
// Previously defined in `c_src/map_server.c`.  C callers access `map_msg[]`
// via the `extern struct map_msg_data { … } map_msg[MSG_MAX]` declaration in
// `c_src/map_server.h`.

/// Number of named message slots.  Matches `MSG_MAX` in `c_src/map_server.h`.
///
/// Enum values (0-based): MAP_WHISPFAIL…MAP_ERRSUMMON = 29 entries, then MSG_MAX = 30.
pub const MSG_MAX: usize = 30;

/// One message entry in the language table.
///
/// Layout must exactly match `struct map_msg_data` in `c_src/map_server.h`:
/// ```c
/// struct map_msg_data { char message[256]; int len; };
/// ```
#[repr(C)]
pub struct MapMsgData {
    pub message: [libc::c_char; 256],
    pub len:     c_int,
}

/// The global language message table.
///
/// Exported as `map_msg` so C translation units that include `map_server.h`
/// (`extern struct map_msg_data map_msg[MSG_MAX]`) link against this symbol.
///
/// Entries are populated by `lang_read`.  The `message` field is a
/// null-terminated, fixed-length C string stored directly in the struct
/// (no heap allocation); `len` caches `strlen(message)` capped at 255.
#[no_mangle]
pub static mut map_msg: [MapMsgData; MSG_MAX] = {
    // const-initialise all slots to zero / empty string.
    const ZERO: MapMsgData = MapMsgData { message: [0; 256], len: 0 };
    [ZERO; MSG_MAX]
};

/// Mapping from the string key used in the lang file to the `map_msg` slot index.
///
/// Mirrors the `map_msg_db[]` table inside the C `lang_read` function.
static LANG_KEY_MAP: &[(&str, usize)] = &[
    ("MAP_WHISPFAIL",  0),
    ("MAP_ERRGHOST",   1),
    ("MAP_ERRITMLEVEL", 2),
    ("MAP_ERRITMMIGHT", 3),
    ("MAP_ERRITMGRACE", 4),
    ("MAP_ERRITMWILL",  5),
    ("MAP_ERRITMSEX",   6),
    ("MAP_ERRITMFULL",  7),
    ("MAP_ERRITMMAX",   8),
    ("MAP_ERRITMPATH",  9),
    ("MAP_ERRITMMARK",  10),
    ("MAP_ERRITM2H",    11),
    ("MAP_ERRMOUNT",    12),
    ("MAP_EQHELM",      13),
    ("MAP_EQWEAP",      14),
    ("MAP_EQARMOR",     15),
    ("MAP_EQSHIELD",    16),
    ("MAP_EQLEFT",      17),
    ("MAP_EQRIGHT",     18),
    ("MAP_EQSUBLEFT",   19),
    ("MAP_EQSUBRIGHT",  20),
    ("MAP_EQFACEACC",   21),
    ("MAP_EQCROWN",     22),
    ("MAP_EQMANTLE",    23),
    ("MAP_EQNECKLACE",  24),
    ("MAP_EQBOOTS",     25),
    ("MAP_EQCOAT",      26),
    ("MAP_ERRVITA",     27),
    ("MAP_ERRMANA",     28),
    ("MAP_ERRSUMMON",   29),
];

/// Parse the language config file and populate `map_msg[]`.
///
/// The file format is line-based:
/// - Lines starting with `//` are comments and are skipped.
/// - Non-comment lines are parsed as `KEY: value` (separated by `: `).
/// - The key is matched case-insensitively against the known `MAP_*`/`MSG_*` names.
/// - Matching entries are written into `map_msg[index].message` (truncated to 255
///   bytes) and `map_msg[index].len` is set accordingly.
///
/// Replaces `lang_read` in `c_src/map_server.c`.
///
/// Returns 0 on success, 1 if the file cannot be opened.
///
/// # Safety
/// `cfg_file` must be a valid, non-null, null-terminated C string.  This
/// function must only be called from the game thread (no concurrent access to
/// `map_msg`).
#[no_mangle]
pub unsafe extern "C" fn lang_read(cfg_file: *const c_char) -> c_int {
    use std::io::BufRead as _;

    let path = std::ffi::CStr::from_ptr(cfg_file).to_string_lossy();

    let file = match std::fs::File::open(path.as_ref()) {
        Ok(f) => f,
        Err(_) => {
            println!("CFG_ERR: Language file ({path}) not found.");
            return 1;
        }
    };

    for line in std::io::BufReader::new(file).lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        // Skip comment lines.
        if line.starts_with("//") {
            continue;
        }

        // Parse `KEY: value` — split on the first `: ` only.
        let Some(colon_pos) = line.find(": ") else { continue };
        let key = &line[..colon_pos];
        // Value is everything after `: `, stripping any trailing \r\n.
        let value = line[colon_pos + 2..].trim_end_matches(['\r', '\n']);

        // Look up the key (case-insensitive).
        let key_up = key.to_ascii_uppercase();
        let Some(&(_, idx)) = LANG_KEY_MAP.iter().find(|(k, _)| *k == key_up.as_str()) else {
            continue;
        };

        // Copy the value into the fixed message buffer, truncating at 255 bytes.
        let bytes = value.as_bytes();
        let copy_len = bytes.len().min(255);
        let slot = &mut map_msg[idx];
        // Zero the whole buffer first (matches strncpy semantics for short strings).
        slot.message = [0; 256];
        for (i, &b) in bytes[..copy_len].iter().enumerate() {
            slot.message[i] = b as libc::c_char;
        }
        slot.message[copy_len] = 0; // null terminator (already zero, but be explicit)
        slot.len = copy_len as c_int;
    }

    println!("[map] [lang_read] file={path}");
    0
}

// ---------------------------------------------------------------------------
// In-game time functions — ported from `c_src/map_server.c`.
// ---------------------------------------------------------------------------

/// Advance the in-game clock by one hour and broadcast the new time to all
/// connected players.
///
/// Call order: `cur_time` wraps 0–23; each full day advances `cur_day` (1–91);
/// each full season (91 days) advances `cur_season` (1–4); each four seasons
/// advances `cur_year`.  After updating globals the new values are written to
/// the `Time` table and `clif_sendtime` is called for every active session.
///
/// Replaces `change_time_char` in `c_src/map_server.c`.
///
/// # Safety
/// Must be called on the game thread.  `sql_handle` must be a valid live
/// connection.  `fd_max` must reflect the current session table bounds.
#[no_mangle]
pub unsafe extern "C" fn change_time_char(_id: c_int, _n: c_int) -> c_int {
    cur_time += 1;

    if cur_time == 24 {
        cur_time = 0;
        cur_day += 1;
        if cur_day == 92 {
            cur_day = 1;
            cur_season += 1;
            if cur_season == 5 {
                cur_season = 1;
                cur_year += 1;
            }
        }
    }

    // Broadcast updated time to all active sessions.
    for i in 0..fd_max {
        if rust_session_exists(i) != 0 {
            let sd = rust_session_get_data(i);
            if !sd.is_null() {
                crate::game::map_parse::player_state::clif_sendtime(sd);
            }
        }
    }

    // Persist updated time to the database.
    if SQL_ERROR == Sql_Query(
        sql_handle,
        c"UPDATE `Time` SET `TimHour`='%d', `TimDay`='%d', `TimSeason`='%d', `TimYear`='%d'".as_ptr(),
        cur_time, cur_day, cur_season, cur_year,
    ) {
        Sql_ShowDebug_(sql_handle, c"map_server.rs".as_ptr(), line!() as c_ulong);
    }

    0
}

/// Load in-game time from the database and initialise `cur_time`, `cur_day`,
/// `cur_season`, `cur_year`, and `old_time`.
///
/// Reads the first row of the `Time` table.  If the query fails or no row is
/// returned the globals are left at their previous values (zero on startup).
///
/// Replaces `get_time_thing` in `c_src/map_server.c`.
///
/// # Safety
/// Must be called on the game thread after `sql_handle` is live.
#[no_mangle]
pub unsafe extern "C" fn get_time_thing() -> c_int {
    let stmt = SqlStmt_Malloc(sql_handle);
    if stmt.is_null() {
        SqlStmt_ShowDebug_(stmt, c"map_server.rs".as_ptr(), line!() as c_ulong);
        return 0;
    }

    let mut time_val:   c_int = 0;
    let mut day_val:    c_int = 0;
    let mut season_val: c_int = 0;
    let mut year_val:   c_int = 0;

    if SQL_ERROR == SqlStmt_Prepare(
        stmt,
        c"SELECT `TimHour`, `TimDay`, `TimSeason`, `TimYear` FROM `Time`".as_ptr(),
    ) || SQL_ERROR == SqlStmt_Execute(stmt)
      || SQL_ERROR == SqlStmt_BindColumn(
          stmt, 0, SqlDataType::SqlDtInt,
          std::ptr::addr_of_mut!(time_val) as *mut c_void,
          0, std::ptr::null_mut(), std::ptr::null_mut(),
      )
      || SQL_ERROR == SqlStmt_BindColumn(
          stmt, 1, SqlDataType::SqlDtInt,
          std::ptr::addr_of_mut!(day_val) as *mut c_void,
          0, std::ptr::null_mut(), std::ptr::null_mut(),
      )
      || SQL_ERROR == SqlStmt_BindColumn(
          stmt, 2, SqlDataType::SqlDtInt,
          std::ptr::addr_of_mut!(season_val) as *mut c_void,
          0, std::ptr::null_mut(), std::ptr::null_mut(),
      )
      || SQL_ERROR == SqlStmt_BindColumn(
          stmt, 3, SqlDataType::SqlDtInt,
          std::ptr::addr_of_mut!(year_val) as *mut c_void,
          0, std::ptr::null_mut(), std::ptr::null_mut(),
      )
    {
        SqlStmt_ShowDebug_(stmt, c"map_server.rs".as_ptr(), line!() as c_ulong);
        SqlStmt_Free(stmt);
        return 0;
    }

    if SQL_SUCCESS == SqlStmt_NextRow(stmt) {
        old_time   = time_val;
        cur_time   = time_val;
        cur_day    = day_val;
        cur_season = season_val;
        cur_year   = year_val;
    }

    SqlStmt_Free(stmt);
    0
}

/// Record the current UNIX timestamp as the server start time in the `UpTime`
/// table (row `UtmId = 3`).
///
/// Deletes the existing row then inserts the current `time(NULL)` value.
///
/// Replaces `uptime` in `c_src/map_server.c`.
///
/// # Safety
/// Must be called on the game thread after `sql_handle` is live.
#[no_mangle]
pub unsafe extern "C" fn uptime() -> c_int {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as c_int)
        .unwrap_or(0);

    if SQL_ERROR == Sql_Query(
        sql_handle,
        c"DELETE FROM `UpTime` WHERE `UtmId` = '3'".as_ptr(),
    ) {
        Sql_ShowDebug_(sql_handle, c"map_server.rs".as_ptr(), line!() as c_ulong);
    }

    if SQL_ERROR == Sql_Query(
        sql_handle,
        c"INSERT INTO `UpTime`(`UtmId`, `UtmValue`) VALUES('3', '%d')".as_ptr(),
        now,
    ) {
        Sql_ShowDebug_(sql_handle, c"map_server.rs".as_ptr(), line!() as c_ulong);
    }

    0
}

// ---------------------------------------------------------------------------
// objectFlags — static object collision-flag table loaded from static_objects.tbl
//
// `objectFlags` is a heap-allocated byte array indexed by a tile/object ID.
// Each byte encodes directional movement flags (OBJ_UP / OBJ_RIGHT / OBJ_DOWN /
// OBJ_LEFT) for its corresponding object.  The C extern declaration in
// `c_src/map_server.h` (`extern unsigned char *objectFlags;`) is satisfied by
// the `#[no_mangle]` static below; `sl_compat.c` indexes it directly.
// ---------------------------------------------------------------------------

/// Pointer to the static object flag table allocated by `object_flag_init`.
///
/// C extern: `extern unsigned char *objectFlags;` in `map_server.h`.
/// `sl_compat.c` reads `objectFlags[object]` for directional collision checks.
#[no_mangle]
pub static mut objectFlags: *mut u8 = std::ptr::null_mut();

/// Load the static object flag table from `static_objects.tbl`.
///
/// Reads a binary file whose format is:
/// - 4 bytes: total object count (`num`, little-endian `i32`)
/// - 1 byte: initial flag (consumed before the loop)
/// - Then `num` records, each:
///   - 1 byte: count of tile IDs that follow
///   - `count` × 2 bytes: tile IDs
///   - 5 bytes: reserved/padding
///   - 1 byte: flag byte for this object
///
/// Allocates `objectFlags` with `num + 1` bytes via `libc::calloc`.
/// The actual per-object flag assignment is intentionally left commented out,
/// preserving the original C behaviour (table allocated but entries stay zero).
///
/// Replaces `object_flag_init` in `c_src/map_server.c`.
///
/// # Safety
/// Must be called on the game thread before any `sl_compat.c` function that
/// reads `objectFlags`.  `data_dir` (C global from `config.c`) must be valid
/// and point to a null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn object_flag_init() -> c_int {
    // data_dir is a C global (char*) defined in config.c.
    extern "C" {
        static data_dir: *const c_char;
    }

    let filename = b"static_objects.tbl\0";
    let dir_cstr = std::ffi::CStr::from_ptr(data_dir);
    let dir_bytes = dir_cstr.to_bytes();

    // Build full path: data_dir + filename (without the extra NUL added by CString).
    let mut path_bytes: Vec<u8> = Vec::with_capacity(dir_bytes.len() + filename.len() - 1);
    path_bytes.extend_from_slice(dir_bytes);
    path_bytes.extend_from_slice(&filename[..filename.len() - 1]);
    let path_cstr = match std::ffi::CString::new(path_bytes) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("[map] [object_flag_init] path contains interior nul byte");
            std::process::exit(1);
        }
    };

    let path_str = path_cstr.to_string_lossy();
    println!(
        "[map] [object_flag_init] reading static obj table path={}",
        path_str
    );

    let fi = libc::fopen(path_cstr.as_ptr(), b"rb\0".as_ptr().cast());
    if fi.is_null() {
        eprintln!(
            "[map] [error] cannot read static object table path={}",
            path_str
        );
        std::process::exit(1);
    }

    let mut num: i32 = 0;
    libc::fread(std::ptr::addr_of_mut!(num).cast(), 4, 1, fi);

    // Allocate objectFlags with num+1 bytes, zero-initialised (matches C CALLOC).
    objectFlags = libc::calloc((num as usize) + 1, 1).cast();

    let mut flag: i8 = 0;
    libc::fread(std::ptr::addr_of_mut!(flag).cast(), 1, 1, fi);

    let mut z: i32 = 1;
    while libc::feof(fi) == 0 {
        let mut count: i8 = 0;
        libc::fread(std::ptr::addr_of_mut!(count).cast(), 1, 1, fi);
        let mut remaining = count;
        while remaining != 0 {
            let mut tile: i16 = 0;
            libc::fread(std::ptr::addr_of_mut!(tile).cast(), 2, 1, fi);
            remaining -= 1;
        }

        let mut nothing = [0u8; 5];
        libc::fread(nothing.as_mut_ptr().cast(), 5, 1, fi);
        libc::fread(std::ptr::addr_of_mut!(flag).cast(), 1, 1, fi);
        // objectFlags[z as usize] = flag as u8;  // intentionally not assigned, matching C
        z += 1;
    }

    libc::fclose(fi);
    0
}

// ---------------------------------------------------------------------------
// map_src linked-list — ported from `c_src/map_server.c`
//
// The C implementation used a `struct map_src_list` singly-linked list with
// heap-allocated nodes.  Replaced here with a `Vec<MapSrcEntry>` for safety.
// `map_src_clear` frees the list; `map_src_add` appends one parsed entry.
//
// These functions are declared in `c_src/map_server.h` and may be called from
// C (currently unused in the codebase, but retained for ABI completeness).
// ---------------------------------------------------------------------------

// Retained for ABI compatibility — map_src_add/map_src_clear are declared in
// map_server.h; callers may be external or future script paths.
#[allow(dead_code)]
/// One entry in the map source list (equivalent to C `struct map_src_list`).
#[derive(Debug)]
struct MapSrcEntry {
    id: i32,
    pvp: i32,
    spell: i32,
    sweeptime: u32,
    title: [u8; 64],
    cantalk: u8,
    show_ghosts: u8,
    region: u8,
    indoor: u8,
    warpout: u8,
    bind: u8,
    bgm: u16,
    bgmtype: u16,
    light: u16,
    weather: u16,
    mapfile: Vec<u8>,
}

// Retained for ABI compatibility — map_src_add/map_src_clear are declared in
// map_server.h; callers may be external or future script paths.
#[allow(dead_code)]
/// The parsed map source list, replacing the C `map_src_first` / `map_src_last`
/// singly-linked list.
static mut MAP_SRC_LIST: Vec<MapSrcEntry> = Vec::new();

/// Free all entries in the map source list.
///
/// Equivalent to the C `map_src_clear()` which walked the linked list and
/// freed each node and its `mapfile` string.  Dropping `MAP_SRC_LIST` via
/// `clear()` releases all `MapSrcEntry` allocations automatically.
///
/// Replaces `map_src_clear` in `c_src/map_server.c`.
///
/// # Safety
/// Must be called on the game thread.  No other thread may concurrently access
/// `MAP_SRC_LIST`.
#[no_mangle]
pub unsafe extern "C" fn map_src_clear() -> c_int {
    MAP_SRC_LIST.clear();
    0
}

/// Parse one CSV line and append it to the map source list.
///
/// Expected format (matching the C `sscanf` format string):
/// ```text
/// map_id,title,bgm,pvp,spell,light,weather,sweeptime,cantalk,showghosts,
/// region,indoor, warpout,bind,mapfile
/// ```
/// (Note the C format has a leading space before `warpout`: `", %c"`.)
///
/// Returns `0` on success, `-1` if fewer than 13 fields can be parsed.
///
/// Replaces `map_src_add` in `c_src/map_server.c`.
///
/// # Safety
/// `r1` must be a valid, non-null, null-terminated C string.
/// Must be called on the game thread.
#[no_mangle]
pub unsafe extern "C" fn map_src_add(r1: *const c_char) -> c_int {
    let line = std::ffi::CStr::from_ptr(r1).to_string_lossy();

    // Split on commas, matching the C sscanf format (15 fields max).
    // Format: map_id,title,bgm,pvp,spell,light,weather,sweeptime,
    //         cantalk,showghosts,region,indoor, warpout,bind,mapfile
    // The title field may contain spaces but not commas ([^,] in sscanf).
    let parts: Vec<&str> = line.splitn(15, ',').collect();
    if parts.len() < 13 {
        return -1;
    }

    let map_id: i32 = match parts[0].trim().parse() {
        Ok(v) => v,
        Err(_) => return -1,
    };
    let map_title = parts[1];
    let map_bgm: u16 = match parts[2].trim().parse() {
        Ok(v) => v,
        Err(_) => return -1,
    };
    let pvp: i32 = match parts[3].trim().parse() {
        Ok(v) => v,
        Err(_) => return -1,
    };
    let spell: i32 = match parts[4].trim().parse() {
        Ok(v) => v,
        Err(_) => return -1,
    };
    let light: u16 = match parts[5].trim().parse() {
        Ok(v) => v,
        Err(_) => return -1,
    };
    let weather: u16 = match parts[6].trim().parse() {
        Ok(v) => v,
        Err(_) => return -1,
    };
    let sweeptime: u32 = match parts[7].trim().parse() {
        Ok(v) => v,
        Err(_) => return -1,
    };
    // Single-character fields (%c in C sscanf) — read first byte only.
    let cantalk    = parts[8].trim().as_bytes().first().copied().unwrap_or(0);
    let showghosts = parts[9].trim().as_bytes().first().copied().unwrap_or(0);
    let region     = parts[10].trim().as_bytes().first().copied().unwrap_or(0);
    let indoor     = parts[11].trim().as_bytes().first().copied().unwrap_or(0);
    // C format has a leading space before warpout: `", %c"` — trim handles it.
    let warpout    = parts[12].trim().as_bytes().first().copied().unwrap_or(0);
    if parts.len() < 14 {
        return -1;
    }
    let bind       = parts[13].trim().as_bytes().first().copied().unwrap_or(0);
    let map_file   = if parts.len() >= 15 { parts[14].trim() } else { "" };

    let mut title_buf = [0u8; 64];
    let title_bytes = map_title.as_bytes();
    let copy_len = title_bytes.len().min(63);
    title_buf[..copy_len].copy_from_slice(&title_bytes[..copy_len]);

    let entry = MapSrcEntry {
        id: map_id,
        pvp,
        spell,
        sweeptime,
        title: title_buf,
        cantalk,
        show_ghosts: showghosts,
        region,
        indoor,
        warpout,
        bind,
        bgm: map_bgm,
        bgmtype: 0, // not populated from CSV; C calloc'd struct also left this as 0
        light,
        weather,
        mapfile: map_file.as_bytes().to_vec(),
    };

    MAP_SRC_LIST.push(entry);
    0
}

// ---------------------------------------------------------------------------
// gamereg — game-global registry (replaces `struct game_data gamereg` in C)
//
// `gamereg` is the server-wide key/value integer store backed by the
// `GameRegistry<serverid>` table.  The C definition at the top of
// `c_src/map_server.c` (`struct game_data gamereg;`) is deleted and replaced
// by this `#[no_mangle]` Rust static so that both Rust and C (via
// `map_readglobalgamereg`) link against a single symbol.
// ---------------------------------------------------------------------------

/// Capacity of the game-global registry.  Mirrors `MAX_GAMEREG` in `map_server.h`.
const MAX_GAMEREG: usize = 5000;

/// Mirrors `struct game_data` from `c_src/map_server.h`.
/// Must be `#[repr(C)]` and match the C layout exactly.
///
/// ```c
/// struct game_data {
///     struct global_reg *registry;
///     int registry_num;
/// };
/// ```
#[repr(C)]
pub struct GameData {
    pub registry:     *mut crate::database::map_db::GlobalReg,
    pub registry_num: c_int,
}

// SAFETY: `gamereg` is only accessed on the single-threaded game loop.
// No Rust code takes shared references to it across threads.
unsafe impl Send for GameData {}
unsafe impl Sync for GameData {}

/// The game-wide registry global.
///
/// Exported as `gamereg` so the remaining C function `map_readglobalgamereg`
/// in `map_server.c` can access it without change.  Populated by
/// `map_loadgameregistry` and mutated by `map_setglobalgamereg`.
#[no_mangle]
pub static mut gamereg: GameData = GameData {
    registry:     std::ptr::null_mut(),
    registry_num: 0,
};

/// Allocate a zeroed array of `GlobalReg` entries of the given length via the
/// global allocator.  The caller is responsible for freeing via the same allocator.
fn alloc_zeroed_gamereg_registry(len: usize) -> *mut crate::database::map_db::GlobalReg {
    use crate::database::map_db::GlobalReg;
    let layout = std::alloc::Layout::array::<GlobalReg>(len).unwrap();
    let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
    if ptr.is_null() {
        std::alloc::handle_alloc_error(layout);
    }
    ptr as *mut GlobalReg
}

/// ASCII case-insensitive comparison of a `GlobalReg.str` field against a C string.
///
/// Returns `true` if the two null-terminated byte sequences are equal ignoring ASCII case.
/// Equivalent to `strcasecmp` used in the C registry search loops.
unsafe fn reg_str_eq(arr: &[i8; 64], cstr: *const c_char) -> bool {
    if cstr.is_null() {
        return false;
    }
    for i in 0..64usize {
        let a = arr[i] as u8;
        let b = *cstr.add(i) as u8;
        if a.to_ascii_lowercase() != b.to_ascii_lowercase() {
            return false;
        }
        if a == 0 {
            return true; // both null-terminated at the same position
        }
    }
    false
}

/// Copy a C string into a `[i8; 64]` field, null-terminating. Truncates at 63 chars.
unsafe fn copy_cstr_to_reg_str(dest: &mut [i8; 64], src: *const c_char) {
    let mut i = 0usize;
    while i < 63 {
        let b = *src.add(i) as i8;
        dest[i] = b;
        if b == 0 {
            return;
        }
        i += 1;
    }
    dest[63] = 0; // ensure null termination
}

// ---------------------------------------------------------------------------
// map_registrysave — persist one map registry slot to the `MapRegistry` table.
//
// Mirrors `map_registrysave` in `c_src/map_server.c`.
//
// Logic:
//   - SELECT MrgPosition WHERE MrgMapId=m AND MrgIdentifier=str → save_id (-1 if not found)
//   - If found:
//       val==0 → DELETE WHERE MrgMapId=m AND MrgIdentifier=str
//       val!=0 → UPDATE SET MrgIdentifier=str, MrgValue=val WHERE MrgMapId=m AND MrgPosition=save_id
//   - If not found:
//       val>0  → INSERT (MrgMapId, MrgIdentifier, MrgValue, MrgPosition)
// ---------------------------------------------------------------------------

/// Persist one map registry slot at index `i` on map `m` to the `MapRegistry` table.
///
/// Replaces `map_registrysave` in `c_src/map_server.c`.
///
/// # Safety
/// `crate::ffi::map_db::map` must be a valid initialised pointer.  `m` must be a
/// loaded map index and `i` must be within `[0, MAX_MAPREG)`.
#[no_mangle]
pub unsafe extern "C" fn map_registrysave(m: c_int, i: c_int) -> c_int {
    use crate::database::map_db::{GlobalReg, MAP_SLOTS, MAX_MAPREG};

    if m < 0 || m as usize >= MAP_SLOTS { return 0; }
    if i < 0 || i as usize >= MAX_MAPREG { return 0; }

    let slot = &mut *crate::ffi::map_db::map.add(m as usize);
    if slot.registry.is_null() { return 0; }

    let p: &GlobalReg = &*slot.registry.add(i as usize);

    // Read the identifier (null-terminated i8 array) into a Rust String.
    let identifier = {
        let bytes: &[u8] = std::slice::from_raw_parts(p.str.as_ptr() as *const u8, 64);
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(64);
        String::from_utf8_lossy(&bytes[..end]).into_owned()
    };
    let val = p.val;

    let m_u32 = m as u32;
    let i_u32 = i as u32;

    // SELECT existing position.
    let save_id: Option<u32> = blocking_run(
        sqlx::query_scalar::<_, u32>(
            "SELECT MrgPosition FROM MapRegistry \
             WHERE MrgMapId = ? AND MrgIdentifier = ?",
        )
        .bind(m_u32)
        .bind(&identifier)
        .fetch_optional(get_pool()),
    )
    .unwrap_or(None);

    match save_id {
        Some(pos) => {
            if val == 0 {
                // Delete the entry — value cleared.
                let _ = blocking_run(
                    sqlx::query(
                        "DELETE FROM MapRegistry \
                         WHERE MrgMapId = ? AND MrgIdentifier = ?",
                    )
                    .bind(m_u32)
                    .bind(&identifier)
                    .execute(get_pool()),
                );
            } else {
                // Update in-place.
                let _ = blocking_run(
                    sqlx::query(
                        "UPDATE MapRegistry SET MrgIdentifier = ?, MrgValue = ? \
                         WHERE MrgMapId = ? AND MrgPosition = ?",
                    )
                    .bind(&identifier)
                    .bind(val)
                    .bind(m_u32)
                    .bind(pos)
                    .execute(get_pool()),
                );
            }
        }
        None => {
            if val > 0 {
                // Insert new row.
                let _ = blocking_run(
                    sqlx::query(
                        "INSERT INTO MapRegistry \
                         (MrgMapId, MrgIdentifier, MrgValue, MrgPosition) \
                         VALUES (?, ?, ?, ?)",
                    )
                    .bind(m_u32)
                    .bind(&identifier)
                    .bind(val)
                    .bind(i_u32)
                    .execute(get_pool()),
                );
            }
        }
    }

    0
}

// ---------------------------------------------------------------------------
// map_setglobalreg — set a map-level registry key/value in memory and persist.
//
// Mirrors `map_setglobalreg` in `c_src/map_server.c`.
//
// Uses the `map_isloaded` guard (registry != null), then:
//   1. Linear search for an existing entry with the same name (strcasecmp).
//   2. If found: update val, persist, clear str if val==0.
//   3. If not found: reuse the first empty slot, or extend registry_num if capacity allows.
// ---------------------------------------------------------------------------

/// Set a key/value pair in the per-map registry for map `m`, then persist to DB.
///
/// Replaces `map_setglobalreg` in `c_src/map_server.c`.
///
/// # Safety
/// `crate::ffi::map_db::map` must be a valid initialised pointer.  `m` must be within
/// `[0, MAP_SLOTS)`.  `reg` must be a valid non-null null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn map_setglobalreg(m: c_int, reg: *const c_char, val: c_int) -> c_int {
    use crate::database::map_db::MAP_SLOTS;

    if reg.is_null() { return 0; }
    if m < 0 || m as usize >= MAP_SLOTS { return 0; }
    let slot = &mut *crate::ffi::map_db::map.add(m as usize);
    // map_isloaded(m) — registry must be non-null.
    if slot.registry.is_null() { return 0; }

    let num = slot.registry_num as usize;

    // Search for an existing entry.
    let mut exist: Option<usize> = None;
    for idx in 0..num {
        let entry = &*slot.registry.add(idx);
        if reg_str_eq(&entry.str, reg) {
            exist = Some(idx);
            break;
        }
    }

    if let Some(idx) = exist {
        let entry = &mut *slot.registry.add(idx);
        entry.val = val;
        map_registrysave(m, idx as c_int);
        if val == 0 {
            entry.str = [0i8; 64]; // empty registry slot
        }
        return 0;
    }

    // Search for an empty slot to reuse.
    for idx in 0..num {
        let entry = &*slot.registry.add(idx);
        if entry.str[0] == 0 {
            let entry = &mut *slot.registry.add(idx);
            copy_cstr_to_reg_str(&mut entry.str, reg);
            entry.val = val;
            map_registrysave(m, idx as c_int);
            return 0;
        }
    }

    // Extend if capacity allows.
    if num < crate::database::map_db::MAX_MAPREG {
        let new_num = num + 1;
        slot.registry_num = new_num as c_int;
        let entry = &mut *slot.registry.add(num);
        copy_cstr_to_reg_str(&mut entry.str, reg);
        entry.val = val;
        map_registrysave(m, num as c_int);
    }

    0
}

// ---------------------------------------------------------------------------
// map_readglobalreg — read a map-level registry value from memory.
//
// Mirrors `map_readglobalreg` in `c_src/map_server.c`.
// ---------------------------------------------------------------------------

/// Return the value for registry key `reg` on map `m`, or 0 if not found.
///
/// Replaces `map_readglobalreg` in `c_src/map_server.c`.
///
/// # Safety
/// `crate::ffi::map_db::map` must be a valid initialised pointer.  `m` must be within
/// `[0, MAP_SLOTS)`.  `reg` must be a valid non-null null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn map_readglobalreg(m: c_int, reg: *const c_char) -> c_int {
    use crate::database::map_db::MAP_SLOTS;

    if m < 0 || m as usize >= MAP_SLOTS { return 0; }
    let slot = &*crate::ffi::map_db::map.add(m as usize);
    if slot.registry.is_null() { return 0; }

    let num = slot.registry_num as usize;
    for idx in 0..num {
        let entry = &*slot.registry.add(idx);
        if reg_str_eq(&entry.str, reg) {
            return entry.val;
        }
    }
    0
}

// ---------------------------------------------------------------------------
// map_loadgameregistry — load game-global registry from `GameRegistry<id>` table.
//
// Mirrors `map_loadgameregistry` in `c_src/map_server.c`.
//
// Allocates gamereg.registry, queries all rows, copies them into the array.
// ---------------------------------------------------------------------------

/// Load the game-global registry from the `GameRegistry<serverid>` table.
///
/// Replaces `map_loadgameregistry` in `c_src/map_server.c`.
///
/// # Safety
/// Must be called on the game thread after the database pool is initialised.
/// `serverid` must be a valid C extern int.
#[no_mangle]
pub unsafe extern "C" fn map_loadgameregistry() -> c_int {
    extern "C" { static serverid: c_int; }

    let sid = serverid;
    let limit = MAX_GAMEREG as u32;

    gamereg.registry_num = 0;

    // Free previous registry if reload.
    if !gamereg.registry.is_null() {
        let layout = std::alloc::Layout::array::<crate::database::map_db::GlobalReg>(MAX_GAMEREG)
            .expect("layout computation is infallible for MAX_GAMEREG = 5000");
        std::alloc::dealloc(gamereg.registry as *mut u8, layout);
        gamereg.registry = std::ptr::null_mut();
    }

    gamereg.registry = alloc_zeroed_gamereg_registry(MAX_GAMEREG);

    let sql = format!(
        "SELECT GrgIdentifier, GrgValue FROM `GameRegistry{sid}` LIMIT {limit}"
    );

    #[derive(sqlx::FromRow)]
    struct GrgRow {
        #[sqlx(rename = "GrgIdentifier")]
        grg_identifier: String,
        #[sqlx(rename = "GrgValue")]
        grg_value: i32,
    }

    let rows: Vec<GrgRow> = match blocking_run(
        sqlx::query_as::<_, GrgRow>(&sql).fetch_all(get_pool()),
    ) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("[map] map_loadgameregistry failed: {e:#}");
            return 0;
        }
    };

    let count = rows.len().min(MAX_GAMEREG);
    gamereg.registry_num = count as c_int;

    for (i, row) in rows.iter().take(count).enumerate() {
        let entry = &mut *gamereg.registry.add(i);
        let bytes = row.grg_identifier.as_bytes();
        let copy_len = bytes.len().min(63);
        std::ptr::copy_nonoverlapping(
            bytes.as_ptr() as *const i8,
            entry.str.as_mut_ptr(),
            copy_len,
        );
        entry.str[copy_len] = 0;
        entry.val = row.grg_value;
    }

    tracing::info!("[map] [load_game_registry] count={count}");
    0
}

// ---------------------------------------------------------------------------
// map_savegameregistry — persist one game-global registry slot to DB.
//
// Mirrors `map_savegameregistry` in `c_src/map_server.c`.
//
// Logic:
//   - SELECT GrgId WHERE GrgIdentifier=str → save_id (0 if not found)
//   - If found (save_id != 0):
//       val==0 → DELETE WHERE GrgIdentifier=str
//       val!=0 → UPDATE SET GrgIdentifier=str, GrgValue=val WHERE GrgId=save_id
//   - If not found (save_id==0):
//       val>0  → INSERT (GrgIdentifier, GrgValue)
// ---------------------------------------------------------------------------

/// Persist one game-global registry slot at index `i` to `GameRegistry<serverid>`.
///
/// Replaces `map_savegameregistry` in `c_src/map_server.c`.
///
/// # Safety
/// Must be called on the game thread.  `i` must be within `[0, registry_num)`.
/// `gamereg.registry` must be a valid allocated array.
#[no_mangle]
pub unsafe extern "C" fn map_savegameregistry(i: c_int) -> c_int {
    extern "C" { static serverid: c_int; }

    if gamereg.registry.is_null() { return 0; }
    if i < 0 || i as usize >= gamereg.registry_num as usize { return 0; }

    let sid = serverid;
    let entry = &*gamereg.registry.add(i as usize);

    let identifier = {
        let bytes: &[u8] = std::slice::from_raw_parts(entry.str.as_ptr() as *const u8, 64);
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(64);
        String::from_utf8_lossy(&bytes[..end]).into_owned()
    };
    let val = entry.val;

    // SELECT existing GrgId.
    let save_id: Option<u32> = blocking_run(
        sqlx::query_scalar::<_, u32>(&format!(
            "SELECT GrgId FROM `GameRegistry{sid}` WHERE GrgIdentifier = ?",
        ))
        .bind(&identifier)
        .fetch_optional(get_pool()),
    )
    .unwrap_or(None);

    match save_id {
        Some(grg_id) if grg_id != 0 => {
            if val == 0 {
                let _ = blocking_run(
                    sqlx::query(&format!(
                        "DELETE FROM `GameRegistry{sid}` WHERE GrgIdentifier = ?",
                    ))
                    .bind(&identifier)
                    .execute(get_pool()),
                );
            } else {
                let _ = blocking_run(
                    sqlx::query(&format!(
                        "UPDATE `GameRegistry{sid}` \
                         SET GrgIdentifier = ?, GrgValue = ? \
                         WHERE GrgId = ?",
                    ))
                    .bind(&identifier)
                    .bind(val)
                    .bind(grg_id)
                    .execute(get_pool()),
                );
            }
        }
        _ => {
            if val > 0 {
                let _ = blocking_run(
                    sqlx::query(&format!(
                        "INSERT INTO `GameRegistry{sid}` \
                         (GrgIdentifier, GrgValue) VALUES (?, ?)",
                    ))
                    .bind(&identifier)
                    .bind(val)
                    .execute(get_pool()),
                );
            }
        }
    }

    0
}

// ---------------------------------------------------------------------------
// map_setglobalgamereg — set a game-global registry key/value and persist.
//
// Mirrors `map_setglobalgamereg` in `c_src/map_server.c`.
//
// Same three-phase logic as map_setglobalreg but operates on `gamereg`.
// Uses MAX_GLOBALREG as the capacity limit (== MAX_GAMEREG == 5000).
// ---------------------------------------------------------------------------

/// Set a key/value pair in the game-global registry, then persist to DB.
///
/// Replaces `map_setglobalgamereg` in `c_src/map_server.c`.
///
/// # Safety
/// Must be called on the game thread.  `reg` must be a valid non-null
/// null-terminated C string.  `gamereg.registry` must be initialised.
#[no_mangle]
pub unsafe extern "C" fn map_setglobalgamereg(reg: *const c_char, val: c_int) -> c_int {
    if reg.is_null() { return 0; }
    if gamereg.registry.is_null() { return 0; }

    let num = gamereg.registry_num as usize;

    // Search for an existing entry (strcasecmp).
    let mut exist: Option<usize> = None;
    for idx in 0..num {
        let entry = &*gamereg.registry.add(idx);
        if reg_str_eq(&entry.str, reg) {
            exist = Some(idx);
            break;
        }
    }

    if let Some(idx) = exist {
        let entry = &mut *gamereg.registry.add(idx);
        entry.val = val;
        map_savegameregistry(idx as c_int);
        if val == 0 {
            entry.str = [0i8; 64]; // empty slot
        }
        return 0;
    }

    // Reuse an empty slot.
    for idx in 0..num {
        let entry = &*gamereg.registry.add(idx);
        if entry.str[0] == 0 {
            let entry = &mut *gamereg.registry.add(idx);
            copy_cstr_to_reg_str(&mut entry.str, reg);
            entry.val = val;
            map_savegameregistry(idx as c_int);
            return 0;
        }
    }

    // Extend if capacity allows (C used MAX_GLOBALREG == 5000 == MAX_GAMEREG).
    if num < MAX_GAMEREG {
        gamereg.registry_num = (num + 1) as c_int;
        let entry = &mut *gamereg.registry.add(num);
        copy_cstr_to_reg_str(&mut entry.str, reg);
        entry.val = val;
        map_savegameregistry(num as c_int);
    }

    0
}

// ---------------------------------------------------------------------------
// map_registrydelete — no-op stub for ABI completeness.
//
// `c_src/map_server.h` declares `int map_registrydelete(int, int)` but the
// original C implementation was commented out and has no current callers.
// The stub prevents linker failures if any translation unit ever calls it.
// ---------------------------------------------------------------------------

/// Lookup a character's name by ID; allocates a 255-byte heap buffer (caller must free).
/// Returns "None" for id=0, empty string if not found.
///
/// Mirrors `map_id2name` in `c_src/map_server_stubs.c`.
#[no_mangle]
pub unsafe extern "C" fn map_id2name(id: c_uint) -> *mut c_char {
    let buf = libc::calloc(255, 1) as *mut c_char;
    if buf.is_null() { return buf; }
    if id == 0 {
        let none = b"None\0";
        std::ptr::copy_nonoverlapping(none.as_ptr() as *const c_char, buf, none.len());
        return buf;
    }
    let name: Option<String> = crate::database::blocking_run(async move {
        sqlx::query_scalar::<_, String>(
            "SELECT `ChaName` FROM `Character` WHERE `ChaId`=?"
        )
        .bind(id)
        .fetch_optional(crate::database::get_pool()).await
        .ok()
        .flatten()
    });
    if let Some(s) = name {
        let bytes = s.as_bytes();
        let len = bytes.len().min(254);
        std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, buf, len);
        *buf.add(len) = 0;
    }
    buf
}

/// Trigger the mapWeather Lua hook when the in-game hour changes.
///
/// Mirrors `map_weather` in `c_src/map_server_stubs.c`.
#[no_mangle]
pub unsafe extern "C" fn map_weather(_id: c_int, _n: c_int) -> c_int {
    if old_time != cur_time {
        old_time = cur_time;
        crate::game::scripting::sl_doscript_blargs_vec(
            c"mapWeather".as_ptr(), std::ptr::null(), 0, std::ptr::null(),
        );
    }
    0
}

/// Save all online character sessions to the char server.
///
/// Mirrors `map_savechars` in `c_src/map_server_stubs.c`.
#[no_mangle]
pub unsafe extern "C" fn map_savechars(_none: c_int, _nonetoo: c_int) -> c_int {
    use crate::ffi::session::{rust_session_exists, rust_session_get_data, rust_session_get_eof};
    extern "C" { fn sl_intif_save(sd: *mut c_void) -> c_int; }
    for x in 0..fd_max {
        if rust_session_exists(x) == 0 { continue; }
        if rust_session_get_eof(x) != 0 { continue; }
        let sd = rust_session_get_data(x);
        if !sd.is_null() { sl_intif_save(sd); }
    }
    0
}

/// No-op stub — `map_registrydelete` was commented out in C and has no current callers.
/// Retained for ABI completeness.
#[allow(dead_code)]
#[no_mangle]
pub unsafe extern "C" fn map_registrydelete(_m: c_int, _i: c_int) -> c_int {
    0
}

// ---------------------------------------------------------------------------
// map_lastdeath_mob — record a mob's last-death time in the Spawns table.
//
// Mirrors `map_lastdeath_mob` in `c_src/map_server.c`.
//
// SQL: UPDATE `Spawns<serverid>` SET SpnLastDeath=last_death
//      WHERE SpnX=startx AND SpnY=starty AND SpnMapId=bl.m AND SpnId=id
// ---------------------------------------------------------------------------

/// Record the mob's last-death timestamp in the `Spawns<serverid>` DB table.
///
/// Replaces `map_lastdeath_mob` in `c_src/map_server.c`.
///
/// # Safety
/// `p` must be a valid non-null pointer to a `MobSpawnData` struct.
/// Must be called on the game thread after the DB pool is initialised.
#[no_mangle]
pub unsafe extern "C" fn map_lastdeath_mob(
    p: *mut crate::game::mob::MobSpawnData,
) -> c_int {
    extern "C" { static serverid: c_int; }

    if p.is_null() { return 0; }

    let last_death = (*p).last_death;
    let startx     = (*p).startx as i32;
    let starty     = (*p).starty as i32;
    let map_id     = (*p).bl.m  as i32;
    let mob_id     = (*p).bl.id as i32;
    let sid        = serverid;

    let sql = format!(
        "UPDATE `Spawns{sid}` \
         SET SpnLastDeath = ? \
         WHERE SpnX = ? AND SpnY = ? AND SpnMapId = ? AND SpnId = ?",
    );

    blocking_run(
        sqlx::query(&sql)
            .bind(last_death)
            .bind(startx)
            .bind(starty)
            .bind(map_id)
            .bind(mob_id)
            .execute(get_pool()),
    )
    .unwrap_or_else(|e| {
        tracing::error!("[map] map_lastdeath_mob failed: {e:#}");
        Default::default()
    });

    0
}

// ---------------------------------------------------------------------------
// hasCoref — ported from `c_src/map_server.c`
// ---------------------------------------------------------------------------

/// Returns 1 if the player `sd` has an active co-reference or is contained
/// inside another player that is still in the ID database.  Returns 0 otherwise.
///
/// Replaces `hasCoref` in `c_src/map_server.c`.
///
/// # Safety
/// `sd` must be a valid non-null pointer to a `MapSessionData` that is
/// currently registered in the game world.  Must be called on the game thread.
#[no_mangle]
pub unsafe extern "C" fn hasCoref(sd: *mut MapSessionData) -> c_int {
    if sd.is_null() {
        return 0;
    }
    // Direct coref: this player is already flagged.
    if (*sd).coref != 0 {
        return 1;
    }
    // Container coref: the container player must still be in the ID database.
    if (*sd).coref_container != 0 {
        let container = map_id2sd((*sd).coref_container) as *mut MapSessionData;
        if !container.is_null() {
            return 1;
        }
    }
    0
}

// ---------------------------------------------------------------------------
// map_do_term — ported from `c_src/map_server_stubs.c`
// ---------------------------------------------------------------------------

/// Shuts down the map server: save characters, free all map tile/grid
/// allocations, and terminate all subsystem databases.
///
/// Replaces `map_do_term()` in `c_src/map_server_stubs.c`.
///
/// # Safety
/// Must be called exactly once at shutdown, on the game thread, after all
/// clients have been disconnected.
#[no_mangle]
pub unsafe extern "C" fn map_do_term() {
    use crate::database::map_db::{GlobalReg, MAP_SLOTS, MAX_MAPREG};
    use crate::database::map_db::{BlockList, WarpList};

    map_savechars(0, 0);
    map_clritem();
    map_termiddb();

    // Free per-slot tile arrays (Rust Vec alloc) and block grid arrays.
    if !crate::ffi::map_db::map.is_null() {
        let slots = std::slice::from_raw_parts_mut(crate::ffi::map_db::map, MAP_SLOTS);
        for slot in slots.iter_mut() {
            let cells  = slot.xs as usize * slot.ys as usize;
            let bcells = slot.bxs as usize * slot.bys as usize;

            if !slot.tile.is_null() && cells > 0 {
                drop(Vec::from_raw_parts(slot.tile, cells, cells));
                slot.tile = std::ptr::null_mut();
            }
            if !slot.obj.is_null() && cells > 0 {
                drop(Vec::from_raw_parts(slot.obj, cells, cells));
                slot.obj = std::ptr::null_mut();
            }
            if !slot.map.is_null() && cells > 0 {
                drop(Vec::from_raw_parts(slot.map, cells, cells));
                slot.map = std::ptr::null_mut();
            }
            if !slot.pass.is_null() && cells > 0 {
                drop(Vec::from_raw_parts(slot.pass, cells, cells));
                slot.pass = std::ptr::null_mut();
            }
            if !slot.block.is_null() && bcells > 0 {
                drop(Vec::<*mut BlockList>::from_raw_parts(slot.block, bcells, bcells));
                slot.block = std::ptr::null_mut();
            }
            if !slot.block_mob.is_null() && bcells > 0 {
                drop(Vec::<*mut BlockList>::from_raw_parts(slot.block_mob, bcells, bcells));
                slot.block_mob = std::ptr::null_mut();
            }
            if !slot.warp.is_null() && bcells > 0 {
                drop(Vec::<*mut WarpList>::from_raw_parts(slot.warp, bcells, bcells));
                slot.warp = std::ptr::null_mut();
            }
            if !slot.registry.is_null() {
                let layout = std::alloc::Layout::array::<GlobalReg>(MAX_MAPREG).unwrap();
                std::alloc::dealloc(slot.registry as *mut u8, layout);
                slot.registry = std::ptr::null_mut();
            }
        }
    }

    crate::ffi::block::map_termblock();
    crate::ffi::item_db::rust_itemdb_term();
    crate::ffi::magic_db::rust_magicdb_term();
    crate::ffi::class_db::rust_classdb_term();
    println!("[map] Map Server Shutdown");
}
