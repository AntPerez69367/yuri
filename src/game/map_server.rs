//! Rust ports of `c_src/map_server.c` utility functions.
//!
//! Functions are migrated here one at a time as their C dependencies are removed.
//! Each `#[no_mangle]` export directly replaces its C counterpart in `libmap_game.a`.

use std::collections::HashMap;
use std::ffi::{c_char, c_ulong, c_void};
use std::os::raw::{c_int, c_uint};
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

    // encrypt — C function in net_crypt.c
    fn encrypt(fd: c_int) -> c_int;

    static char_fd: c_int;

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

    // game-global registry reader — map_server.c
    fn map_readglobalgamereg(attrname: *const c_char) -> c_int;

    // sl_exec (= rust_sl_exec) — scripting.h
    #[link_name = "rust_sl_exec"]
    fn sl_exec(user: *mut c_void, code: *mut c_char);

    // SQL C FFI (mirrors pc.rs pub extern "C" block).
    #[link_name = "sql_handle"]
    static sql_handle: *mut Sql;
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
