//! Port of event/ranking functions from `c_src/map_parse.c`.
//!
//! Functions declared `#[no_mangle] pub unsafe extern "C"` so they remain
//! callable from any remaining C code that has not yet been ported.

#![allow(non_snake_case, clippy::wildcard_imports, clippy::too_many_lines)]

use std::ffi::{c_char, c_int, c_uint, c_ulong, c_void};

use crate::game::pc::{
    MapSessionData,
    Sql, SqlStmt, SqlDataType,
    SQL_ERROR, SQL_SUCCESS,
};

use super::packet::{
    rfifob,
    wfifob, wfifohead, wfifoset,
    encrypt,
};

// ─── C FFI declarations ───────────────────────────────────────────────────────

extern "C" {
    // SQL
    #[link_name = "sql_handle"]
    static sql_handle: *mut Sql;

    fn Sql_Query(handle: *mut Sql, fmt: *const c_char, ...) -> c_int;
    fn Sql_NextRow(handle: *mut Sql) -> c_int;
    fn Sql_FreeResult(handle: *mut Sql);
    fn SqlStmt_Malloc(handle: *mut Sql) -> *mut SqlStmt;
    // SqlStmt_ShowDebug(stmt) C macro → SqlStmt_ShowDebug_(stmt, file, line).
    // We use a fixed file/line for the Rust call site.
    #[link_name = "SqlStmt_ShowDebug_"]
    fn SqlStmt_ShowDebug(stmt: *mut SqlStmt, file: *const c_char, line: c_ulong);
    fn SqlStmt_Free(stmt: *mut SqlStmt);
    fn SqlStmt_Prepare(stmt: *mut SqlStmt, query: *const c_char, ...) -> c_int;
    fn SqlStmt_Execute(stmt: *mut SqlStmt) -> c_int;
    fn SqlStmt_BindColumn(
        stmt: *mut SqlStmt,
        idx: usize,
        buf_type: SqlDataType,
        buf: *mut c_void,
        buf_len: usize,
        out_len: *mut c_ulong,
        is_null: *mut c_int,
    ) -> c_int;
    fn SqlStmt_NextRow(stmt: *mut SqlStmt) -> c_int;
    fn SqlStmt_NumRows(stmt: *mut SqlStmt) -> u64;
    fn Sql_EscapeString(handle: *mut Sql, out_to: *mut c_char, from: *const c_char) -> usize;
    fn Sql_ShowDebug_(self_: *mut Sql, file: *const c_char, line: c_ulong);

    // item db (static inline wrappers are `rust_*` underneath — call directly)
    #[link_name = "rust_itemdb_name"]
    fn itemdb_name(id: c_uint) -> *mut c_char;
    #[link_name = "rust_itemdb_icon"]
    fn itemdb_icon(id: c_uint) -> c_int;
    #[link_name = "rust_itemdb_iconcolor"]
    fn itemdb_iconcolor(id: c_uint) -> c_int;

    // mail
    fn nmail_sendmail(
        sd: *mut MapSessionData,
        to: *const c_char,
        topic: *const c_char,
        body: *const c_char,
    ) -> c_int;

    // ranking UI (stays in C for now; used from clif_getReward)
    fn clif_parseranking(sd: *mut MapSessionData, fd: c_int) -> c_int;

    // chat
    fn clif_sendmsg(sd: *mut MapSessionData, kind: c_int, msg: *const c_char) -> c_int;

    // global time variables (extern in map_server.h)
    #[link_name = "cur_year"]
    static cur_year: c_int;
    #[link_name = "cur_season"]
    static cur_season: c_int;

    // WFIFOP: pointer into wdata at pos — used for strncpy into wdata
    #[link_name = "rust_session_wdata_ptr"]
    fn rust_session_wdata_ptr(fd: c_int, pos: usize) -> *mut u8;
}

// ─── wfifop_copy helper ───────────────────────────────────────────────────────

/// Copy `len` bytes from `src` into the send-buffer at `pos`.
#[inline]
unsafe fn wfifop_copy(fd: c_int, pos: usize, src: *const u8, len: usize) {
    let dst = rust_session_wdata_ptr(fd, pos);
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(src, dst, len);
    }
}

/// Write a big-endian u16 into the send buffer at `pos`.
#[inline]
unsafe fn wfifow_be(fd: c_int, pos: usize, val: u16) {
    let p = rust_session_wdata_ptr(fd, pos) as *mut u16;
    if !p.is_null() { p.write_unaligned(val.to_be()); }
}

/// Write a big-endian u32 into the send buffer at `pos`.
#[inline]
unsafe fn wfifol_be(fd: c_int, pos: usize, val: u32) {
    let p = rust_session_wdata_ptr(fd, pos) as *mut u32;
    if !p.is_null() { p.write_unaligned(val.to_be()); }
}

// ─── clif_intcheck ────────────────────────────────────────────────────────────

/// Write `number` into the WFIFO at `field`, using the minimal encoding.
///
/// - `number` <= 254: write one byte at `field`
/// - `number` <= 65535: write two bytes (big-endian) at `field - 1`
/// - otherwise: write four bytes (big-endian) at `field - 3`
///
/// C line 4883.
#[no_mangle]
pub unsafe extern "C" fn clif_intcheck(number: c_int, field: c_int, fd: c_int) {
    if field != 0 {
        if number > 254 {
            if number > 65535 {
                wfifol_be(fd, (field - 3) as usize, number as u32);
            } else {
                wfifow_be(fd, (field - 1) as usize, number as u16);
            }
        } else {
            wfifob(fd, field as usize, number as u8);
        }
    }
}

// ─── sendRewardParcel ─────────────────────────────────────────────────────────

/// Insert a reward parcel row for `sd` and return 1 on success, 0 or 1 on error.
///
/// Finds the highest existing `ParPosition` for the destination character and
/// inserts a new row one slot higher.  C line 4100.
#[no_mangle]
pub unsafe extern "C" fn sendRewardParcel(
    sd:           *mut MapSessionData,
    eventid:      c_int,
    rank:         c_int,
    rewarditem:   c_int,
    rewardamount: c_int,
) -> c_int {
    let _ = eventid; // used in reward message only (via sprintf); not in SQL directly

    let mut pos: c_int = -1;
    let mut newest: c_int = -1;

    let receiver = (*sd).status.id as c_uint;
    let rewarditem_u = rewarditem as c_uint;

    // Build escape string: "name,\nCongratulations on attaining Rank N!\nHere is your reward: (amount) name"
    let mut escape = [0i8; 255];
    {
        let item_name = itemdb_name(rewarditem_u);
        libc::sprintf(
            escape.as_mut_ptr(),
            b"%s,\nCongratulations on attaining Rank %i!\nHere is your reward: (%i) %s\0"
                .as_ptr() as *const c_char,
            (*sd).status.name.as_ptr(),
            rank,
            rewardamount,
            item_name,
        );
    }

    // engrave = item name (up to 30 chars)
    let mut engrave = [0i8; 31];
    {
        let item_name = itemdb_name(rewarditem_u);
        libc::strcpy(engrave.as_mut_ptr(), item_name);
    }

    let sender: c_uint = 1;
    let owner:  c_uint = 0;
    let npcflag: c_int = 1;

    let stmt = SqlStmt_Malloc(sql_handle);
    if stmt.is_null() {
        SqlStmt_ShowDebug(stmt, c"events.rs".as_ptr(), line!() as c_ulong);
        return 1;
    }

    // Find highest existing position for this receiver
    if SQL_ERROR == SqlStmt_Prepare(
        stmt,
        b"SELECT `ParPosition` FROM `Parcels` WHERE `ParChaIdDestination` = '%u'\0"
            .as_ptr() as *const c_char,
        receiver,
    ) || SQL_ERROR == SqlStmt_Execute(stmt)
      || SQL_ERROR == SqlStmt_BindColumn(
            stmt, 0,
            SqlDataType::SqlDtInt,
            &mut pos as *mut c_int as *mut c_void,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    {
        SqlStmt_ShowDebug(stmt, c"events.rs".as_ptr(), line!() as c_ulong);
        SqlStmt_Free(stmt);
        return 1;
    }

    if SqlStmt_NumRows(stmt) > 0 {
        let num_rows = SqlStmt_NumRows(stmt);
        let mut i: u64 = 0;
        while i < num_rows && SQL_SUCCESS == SqlStmt_NextRow(stmt) {
            if pos > newest {
                newest = pos;
            }
            i += 1;
        }
    }

    newest += 1;
    SqlStmt_Free(stmt);

    // Escapes `engrave` into `escape` buffer — but INSERT uses `engrave` (unescaped).
    // Pre-existing C bug: the escaped buffer is computed but never used in the query.
    Sql_EscapeString(sql_handle, escape.as_mut_ptr(), engrave.as_ptr());

    if SQL_ERROR == Sql_Query(
        sql_handle,
        b"INSERT INTO `Parcels` (`ParChaIdDestination`, `ParSender`, `ParItmId`, \
`ParAmount`, `ParChaIdOwner`, `ParEngrave`, `ParPosition`, `ParNpc`) \
VALUES ('%u', '%u', '%u', '%u', '%u', '%s', '%d', '%d')\0"
            .as_ptr() as *const c_char,
        receiver,
        sender,
        rewarditem as c_uint,
        rewardamount as c_uint,
        owner,
        engrave.as_ptr(),
        newest,
        npcflag,
    ) {
        Sql_ShowDebug_(sql_handle, c"events.rs".as_ptr(), line!() as c_ulong);
        return 1;
    }

    1 // success = 1
}

// ─── clif_getReward ───────────────────────────────────────────────────────────

/// Handle the "get reward" packet for an event: look up the event/rank data,
/// award parcels, send a mail confirmation, and update the claim flag.
///
/// C line 4186.
#[no_mangle]
pub unsafe extern "C" fn clif_getReward(sd: *mut MapSessionData, fd: c_int) -> c_int {
    let eventid = rfifob(fd, 7) as c_int;

    let mut legend: [i8; 17]     = [0; 17];
    let mut eventname: [i8; 41]  = [0; 41];
    let mut monthyear: [i8; 7]   = [0; 7];
    let mut season: [i8; 7]      = [0; 7];

    libc::sprintf(
        monthyear.as_mut_ptr(),
        b"Moon %i\0".as_ptr() as *const c_char,
        cur_year,
    );

    let mut legendicon = 0i32;
    let mut legendiconcolor = 0i32;
    let mut legendicon1 = 0i32; let mut legendicon1color = 0i32;
    let mut legendicon2 = 0i32; let mut legendicon2color = 0i32;
    let mut legendicon3 = 0i32; let mut legendicon3color = 0i32;
    let mut legendicon4 = 0i32; let mut legendicon4color = 0i32;
    let mut legendicon5 = 0i32; let mut legendicon5color = 0i32;
    let mut reward1amount = 0i32; let mut reward2amount = 0i32;
    let mut reward1item = 0i32;   let mut reward2item = 0i32;
    let mut rewardranks = 0i32;
    let mut rank = 0i32;

    let mut _1stPlaceReward1_ItmId = 0i32;    let mut _1stPlaceReward1_Amount = 0i32;
    let mut _1stPlaceReward2_ItmId = 0i32;    let mut _1stPlaceReward2_Amount = 0i32;
    let mut _2ndPlaceReward1_ItmId = 0i32;    let mut _2ndPlaceReward1_Amount = 0i32;
    let mut _2ndPlaceReward2_ItmId = 0i32;    let mut _2ndPlaceReward2_Amount = 0i32;
    let mut _3rdPlaceReward1_ItmId = 0i32;    let mut _3rdPlaceReward1_Amount = 0i32;
    let mut _3rdPlaceReward2_ItmId = 0i32;    let mut _3rdPlaceReward2_Amount = 0i32;
    let mut _4thPlaceReward1_ItmId = 0i32;    let mut _4thPlaceReward1_Amount = 0i32;
    let mut _4thPlaceReward2_ItmId = 0i32;    let mut _4thPlaceReward2_Amount = 0i32;
    let mut _5thPlaceReward1_ItmId = 0i32;    let mut _5thPlaceReward1_Amount = 0i32;
    let mut _5thPlaceReward2_ItmId = 0i32;    let mut _5thPlaceReward2_Amount = 0i32;

    let mut rankname: [i8; 4]      = [0; 4];
    let mut legendbuf: [i8; 255]   = [0; 255];
    let mut message: [i8; 4000]    = [0; 4000];
    let mut msg: [i8; 4000]        = [0; 4000];
    let mut topic: [i8; 52]        = [0; 52];

    let stmt = SqlStmt_Malloc(sql_handle);
    if stmt.is_null() {
        SqlStmt_ShowDebug(stmt, c"events.rs".as_ptr(), line!() as c_ulong);
        return 0;
    }

    // Query 1: event metadata + per-rank rewards
    if SQL_ERROR == SqlStmt_Prepare(
        stmt,
        b"SELECT `EventName`, `EventLegend`, `EventRewardRanks_Display`, \
`EventLegend`, `EventLegendIcon1`, `EventLegendIcon1Color`, \
`EventLegendIcon2`, `EventLegendIcon2Color`, \
`EventLegendIcon3`, `EventLegendIcon3Color`, \
`EventLegendIcon4`, `EventLegendIcon4Color`, \
`EventLegendIcon5`, `EventLegendIcon5Color`, \
`1stPlaceReward1_ItmId`, `1stPlaceReward1_Amount`, \
`1stPlaceReward2_ItmId`, `1stPlaceReward2_Amount`, \
`2ndPlaceReward1_ItmId`, `2ndPlaceReward1_Amount`, \
`2ndPlaceReward2_ItmId`, `2ndPlaceReward2_Amount`, \
`3rdPlaceReward1_ItmId`, `3rdPlaceReward1_Amount`, \
`3rdPlaceReward2_ItmId`, `3rdPlaceReward2_Amount`, \
`4thPlaceReward1_ItmId`, `4thPlaceReward1_Amount`, \
`4thPlaceReward2_ItmId`, `4thPlaceReward2_Amount`, \
`5thPlaceReward1_ItmId`, `5thPlaceReward1_Amount`, \
`5thPlaceReward2_ItmId`, `5thPlaceReward2_Amount` \
FROM `RankingEvents` WHERE `EventId` = '%u'\0"
            .as_ptr() as *const c_char,
        eventid as c_uint,
    ) || SQL_ERROR == SqlStmt_Execute(stmt)
      || SQL_ERROR == SqlStmt_BindColumn(stmt,  0, SqlDataType::SqlDtString,   eventname.as_mut_ptr() as *mut c_void, std::mem::size_of_val(&eventname), std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt,  1, SqlDataType::SqlDtString,   legend.as_mut_ptr() as *mut c_void,    std::mem::size_of_val(&legend),    std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt,  2, SqlDataType::SqlDtInt,      &mut rewardranks as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt,  3, SqlDataType::SqlDtString,   legend.as_mut_ptr() as *mut c_void,    std::mem::size_of_val(&legend),    std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt,  4, SqlDataType::SqlDtInt,      &mut legendicon1       as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt,  5, SqlDataType::SqlDtInt,      &mut legendicon1color  as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt,  6, SqlDataType::SqlDtInt,      &mut legendicon2       as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt,  7, SqlDataType::SqlDtInt,      &mut legendicon2color  as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt,  8, SqlDataType::SqlDtInt,      &mut legendicon3       as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt,  9, SqlDataType::SqlDtInt,      &mut legendicon3color  as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 10, SqlDataType::SqlDtInt,      &mut legendicon4       as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 11, SqlDataType::SqlDtInt,      &mut legendicon4color  as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 12, SqlDataType::SqlDtInt,      &mut legendicon5       as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 13, SqlDataType::SqlDtInt,      &mut legendicon5color  as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 14, SqlDataType::SqlDtInt,      &mut _1stPlaceReward1_ItmId    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 15, SqlDataType::SqlDtInt,      &mut _1stPlaceReward1_Amount   as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 16, SqlDataType::SqlDtInt,      &mut _1stPlaceReward2_ItmId    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 17, SqlDataType::SqlDtInt,      &mut _1stPlaceReward2_Amount   as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 18, SqlDataType::SqlDtInt,      &mut _2ndPlaceReward1_ItmId    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 19, SqlDataType::SqlDtInt,      &mut _2ndPlaceReward1_Amount   as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 20, SqlDataType::SqlDtInt,      &mut _2ndPlaceReward2_ItmId    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 21, SqlDataType::SqlDtInt,      &mut _2ndPlaceReward2_Amount   as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 22, SqlDataType::SqlDtInt,      &mut _3rdPlaceReward1_ItmId    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 23, SqlDataType::SqlDtInt,      &mut _3rdPlaceReward1_Amount   as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 24, SqlDataType::SqlDtInt,      &mut _3rdPlaceReward2_ItmId    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 25, SqlDataType::SqlDtInt,      &mut _3rdPlaceReward2_Amount   as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 26, SqlDataType::SqlDtInt,      &mut _4thPlaceReward1_ItmId    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 27, SqlDataType::SqlDtInt,      &mut _4thPlaceReward1_Amount   as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 28, SqlDataType::SqlDtInt,      &mut _4thPlaceReward2_ItmId    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 29, SqlDataType::SqlDtInt,      &mut _4thPlaceReward2_Amount   as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 30, SqlDataType::SqlDtInt,      &mut _5thPlaceReward1_ItmId    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 31, SqlDataType::SqlDtInt,      &mut _5thPlaceReward1_Amount   as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 32, SqlDataType::SqlDtInt,      &mut _5thPlaceReward2_ItmId    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 33, SqlDataType::SqlDtInt,      &mut _5thPlaceReward2_Amount   as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
    {
        SqlStmt_ShowDebug(stmt, c"events.rs".as_ptr(), line!() as c_ulong);
        SqlStmt_Free(stmt);
        return 0;
    }

    if SQL_SUCCESS != SqlStmt_NextRow(stmt) {
        SqlStmt_ShowDebug(stmt, c"events.rs".as_ptr(), line!() as c_ulong);
        SqlStmt_Free(stmt);
        return 0;
    }

    SqlStmt_Free(stmt);

    // Query 2: player's rank for this event (reuse stmt slot — re-malloc is needed since stmt was freed)
    let stmt2 = SqlStmt_Malloc(sql_handle);
    if stmt2.is_null() {
        SqlStmt_ShowDebug(stmt2, c"events.rs".as_ptr(), line!() as c_ulong);
        return 0;
    }

    if SQL_ERROR == SqlStmt_Prepare(
        stmt2,
        b"SELECT `Rank` FROM `RankingScores` WHERE `ChaId` = '%i' AND `EventId` = '%i'\0"
            .as_ptr() as *const c_char,
        (*sd).status.id,
        eventid,
    ) || SQL_ERROR == SqlStmt_Execute(stmt2)
      || SQL_ERROR == SqlStmt_BindColumn(
            stmt2, 0,
            SqlDataType::SqlDtInt,
            &mut rank as *mut c_int as *mut c_void,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    {
        SqlStmt_ShowDebug(stmt2, c"events.rs".as_ptr(), line!() as c_ulong);
        SqlStmt_Free(stmt2);
        return 0;
    }

    if SQL_SUCCESS != SqlStmt_NextRow(stmt2) {
        SqlStmt_ShowDebug(stmt2, c"events.rs".as_ptr(), line!() as c_ulong);
        SqlStmt_Free(stmt2);
        return 0;
    }

    SqlStmt_Free(stmt2);

    // Determine season string
    if cur_season == 1 { libc::strcpy(season.as_mut_ptr(), b"Winter\0".as_ptr() as *const c_char); }
    if cur_season == 2 { libc::strcpy(season.as_mut_ptr(), b"Spring\0".as_ptr() as *const c_char); }
    if cur_season == 3 { libc::strcpy(season.as_mut_ptr(), b"Summer\0".as_ptr() as *const c_char); }
    if cur_season == 4 { libc::strcpy(season.as_mut_ptr(), b"Fall\0".as_ptr()   as *const c_char); }

    if rank == 1 { libc::strcpy(rankname.as_mut_ptr(), b"1st\0".as_ptr() as *const c_char); }
    if rank == 2 { libc::strcpy(rankname.as_mut_ptr(), b"2nd\0".as_ptr() as *const c_char); }
    if rank == 3 { libc::strcpy(rankname.as_mut_ptr(), b"3rd\0".as_ptr() as *const c_char); }
    if rank == 4 { libc::strcpy(rankname.as_mut_ptr(), b"4th\0".as_ptr() as *const c_char); }
    if rank == 5 { libc::strcpy(rankname.as_mut_ptr(), b"5th\0".as_ptr() as *const c_char); }
    if rank == 6 { libc::strcpy(rankname.as_mut_ptr(), b"6th\0".as_ptr() as *const c_char); }

    match rank {
        1 => {
            libc::sprintf(legendbuf.as_mut_ptr(), b"%s [%s] (Moon %i, %s)\0".as_ptr() as *const c_char, legend.as_ptr(), rankname.as_ptr(), cur_year, season.as_ptr());
            legendicon      = legendicon1;
            legendiconcolor = legendicon1color;
            reward1item     = _1stPlaceReward1_ItmId;
            reward1amount   = _1stPlaceReward1_Amount;
            reward2item     = _1stPlaceReward2_ItmId;
            reward2amount   = _1stPlaceReward2_Amount;
        }
        2 => {
            libc::sprintf(legendbuf.as_mut_ptr(), b"%s [%s] (Moon %i, %s)\0".as_ptr() as *const c_char, legend.as_ptr(), rankname.as_ptr(), cur_year, season.as_ptr());
            legendicon      = legendicon2;
            legendiconcolor = legendicon2color;
            reward1item     = _2ndPlaceReward1_ItmId;
            reward1amount   = _2ndPlaceReward1_Amount;
            reward2item     = _2ndPlaceReward2_ItmId;
            reward2amount   = _2ndPlaceReward2_Amount;
        }
        3 => {
            libc::sprintf(legendbuf.as_mut_ptr(), b"%s [%s] (Moon %i, %s)\0".as_ptr() as *const c_char, legend.as_ptr(), rankname.as_ptr(), cur_year, season.as_ptr());
            legendicon      = legendicon3;
            legendiconcolor = legendicon3color;
            reward1item     = _3rdPlaceReward1_ItmId;
            reward1amount   = _3rdPlaceReward1_Amount;
            reward2item     = _3rdPlaceReward2_ItmId;
            reward2amount   = _3rdPlaceReward2_Amount;
        }
        4 => {
            libc::sprintf(legendbuf.as_mut_ptr(), b"%s [%s] (Moon %i, %s)\0".as_ptr() as *const c_char, legend.as_ptr(), rankname.as_ptr(), cur_year, season.as_ptr());
            legendicon      = legendicon4;
            legendiconcolor = legendicon4color;
            reward1item     = _4thPlaceReward1_ItmId;
            reward1amount   = _4thPlaceReward1_Amount;
            reward2item     = _4thPlaceReward2_ItmId;
            reward2amount   = _4thPlaceReward2_Amount;
        }
        _ => {
            libc::sprintf(legendbuf.as_mut_ptr(), b"%s [%s] (Moon %i, %s)\0".as_ptr() as *const c_char, legend.as_ptr(), rankname.as_ptr(), cur_year, season.as_ptr());
            legendicon      = legendicon5;
            legendiconcolor = legendicon5color;
            reward1item     = _5thPlaceReward1_ItmId;
            reward1amount   = _5thPlaceReward1_Amount;
            reward2item     = _5thPlaceReward2_ItmId;
            reward2amount   = _5thPlaceReward2_Amount;
        }
    }

    // Suppress unused-variable warnings for icon values not otherwise used
    let _ = (legendicon, legendiconcolor, rewardranks);

    // Assign legend slot
    use crate::servers::char::charstatus::MAX_LEGENDS;
    for i in 0..MAX_LEGENDS {
        let leg_name_ptr  = (*sd).status.legends[i].name.as_ptr();
        let leg_name1_ptr = if i + 1 < MAX_LEGENDS { (*sd).status.legends[i + 1].name.as_ptr() } else { b"\0".as_ptr() as *const i8 };

        if libc::strcmp(leg_name_ptr, b"\0".as_ptr() as *const c_char) == 0
            && libc::strcasecmp(leg_name1_ptr, b"\0".as_ptr() as *const c_char) == 0
        {
            libc::strcpy((*sd).status.legends[i].text.as_mut_ptr(), legendbuf.as_ptr());
            libc::sprintf(
                (*sd).status.legends[i].name.as_mut_ptr(),
                b"Event %i Place: %i\0".as_ptr() as *const c_char,
                eventid, rank,
            );
            (*sd).status.legends[i].icon  = legendicon as u16;
            (*sd).status.legends[i].color = legendiconcolor as u16;
            break;
        }
    }

    libc::sprintf(
        topic.as_mut_ptr(),
        b"%s Prize\0".as_ptr() as *const c_char,
        eventname.as_ptr(),
    );

    let mut sent_parcel_success: c_int = 0;

    if reward1amount >= 1 && reward2amount >= 1 {
        sent_parcel_success  = sendRewardParcel(sd, eventid, rank, reward1item, reward1amount);
        sent_parcel_success += sendRewardParcel(sd, eventid, rank, reward2item, reward2amount);
    }
    if reward1amount >= 1 && reward2amount == 0 {
        sent_parcel_success = sendRewardParcel(sd, eventid, rank, reward1item, reward1amount);
    }

    if sent_parcel_success == 2 {
        if rank == 1 {
            libc::sprintf(
                message.as_mut_ptr(),
                b"Congratulations on winning the %s Event, %s!\n\nYou have been rewarded: \
(%i) %s, (%i) %s.\n\nPlease continue to play for more great rewards!\0"
                    .as_ptr() as *const c_char,
                eventname.as_ptr(), (*sd).status.name.as_ptr(),
                reward1amount, itemdb_name(reward1item as c_uint),
                reward2amount, itemdb_name(reward2item as c_uint),
            );
            libc::sprintf(
                msg.as_mut_ptr(),
                b"Congratulations on winning the event, %s! Please visit your post office to collect your winnings.\0"
                    .as_ptr() as *const c_char,
                (*sd).status.name.as_ptr(),
            );
            nmail_sendmail(sd, (*sd).status.name.as_ptr(), topic.as_ptr(), message.as_ptr());
        } else {
            libc::sprintf(
                message.as_mut_ptr(),
                b"Thanks for participating in the %s Event, %s.\n\nRank:%s Place\n\n\
You have been rewarded: (%i) %s, (%i) %s.\n\nPlease continue to play for more great rewards!\0"
                    .as_ptr() as *const c_char,
                eventname.as_ptr(), (*sd).status.name.as_ptr(), rankname.as_ptr(),
                reward1amount, itemdb_name(reward1item as c_uint),
                reward2amount, itemdb_name(reward2item as c_uint),
            );
            libc::sprintf(
                msg.as_mut_ptr(),
                b"Thanks for participating in the Event, %s! Please visit your post office to collect your winnings.\0"
                    .as_ptr() as *const c_char,
                (*sd).status.name.as_ptr(),
            );
            nmail_sendmail(sd, (*sd).status.name.as_ptr(), topic.as_ptr(), message.as_ptr());
        }
    }

    if sent_parcel_success == 1 {
        if rank == 1 {
            libc::sprintf(
                message.as_mut_ptr(),
                b"Congratulations on winning the %s Event, %s!\n\nYou have been rewarded: \
(%i) %s.\n\nPlease continue to play for more great rewards!\0"
                    .as_ptr() as *const c_char,
                eventname.as_ptr(), (*sd).status.name.as_ptr(),
                reward1amount, itemdb_name(reward1item as c_uint),
            );
            libc::sprintf(
                msg.as_mut_ptr(),
                b"Congratulations on winning the event, %s! Please visit your post office to collect your winnings.\0"
                    .as_ptr() as *const c_char,
                (*sd).status.name.as_ptr(),
            );
            nmail_sendmail(sd, (*sd).status.name.as_ptr(), topic.as_ptr(), message.as_ptr());
        } else {
            libc::sprintf(
                message.as_mut_ptr(),
                b"Thanks for participating in the %s Event, %s.\n\nRank:%s Place\n\n\
You have been rewarded: (%i) %s.\n\nPlease continue to play for more great rewards!\0"
                    .as_ptr() as *const c_char,
                eventname.as_ptr(), (*sd).status.name.as_ptr(), rankname.as_ptr(),
                reward1amount, itemdb_name(reward1item as c_uint),
            );
            libc::sprintf(
                msg.as_mut_ptr(),
                b"Thanks for participating in the event, %s. Please visit your post office to collect your winnings.\0"
                    .as_ptr() as *const c_char,
                (*sd).status.name.as_ptr(),
            );
            nmail_sendmail(sd, (*sd).status.name.as_ptr(), topic.as_ptr(), message.as_ptr());
        }
    }

    if sent_parcel_success == 0 {
        libc::sprintf(
            msg.as_mut_ptr(),
            b"Sorry %s, there was an error encountered while attempting to send your rewards in a parcel. Please contact a GM for assistance.\0"
                .as_ptr() as *const c_char,
            (*sd).status.name.as_ptr(),
        );
    }

    clif_sendmsg(sd, 0, msg.as_ptr());

    if sent_parcel_success >= 1 {
        if SQL_ERROR == Sql_Query(
            sql_handle,
            b"UPDATE `RankingScores` SET `EventClaim` = 2 WHERE `EventId` = '%u' AND `ChaId` = '%u'\0"
                .as_ptr() as *const c_char,
            eventid as c_uint,
            (*sd).status.id,
        ) {
            Sql_ShowDebug_(sql_handle, c"events.rs".as_ptr(), line!() as c_ulong);
            return -1;
        }
        if SQL_SUCCESS != Sql_NextRow(sql_handle) {
            Sql_FreeResult(sql_handle);
            clif_parseranking(sd, fd);
            return 0;
        }
    }

    0
}

// ─── clif_sendRewardInfo ──────────────────────────────────────────────────────

/// Build and send the reward-info packet (0x7D / subtype 0x05) for an event.
///
/// Iterates `rewardranks` times, writing per-rank legend title, icon, and
/// item reward information into the WFIFO.  C line 4561.
#[no_mangle]
pub unsafe extern "C" fn clif_sendRewardInfo(sd: *mut MapSessionData, fd: c_int) -> c_int {
    wfifohead(fd, 0);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 1, 0x01);
    wfifob(fd, 3, 0x7D);
    wfifob(fd, 5, 0x05);
    wfifob(fd, 6, 0);
    wfifob(fd, 7, rfifob(fd, 7));
    wfifob(fd, 8, 142);
    wfifob(fd, 9, 227);
    wfifob(fd, 10, 0);
    wfifob(fd, 12, 1);

    let _ = sd; // sd is passed by C but not used in this function

    let mut buf: [i8; 40] = [0; 40];
    let mut legend: [i8; 17] = [0; 17];
    let mut monthyear: [i8; 7] = [0; 7];

    libc::sprintf(
        monthyear.as_mut_ptr(),
        b"Moon %i\0".as_ptr() as *const c_char,
        cur_year,
    );

    let eventid = rfifob(fd, 7) as c_uint;

    let mut rewardranks = 0i32;
    let mut legendicon1 = 0i32; let mut legendicon1color = 0i32;
    let mut legendicon2 = 0i32; let mut legendicon2color = 0i32;
    let mut legendicon3 = 0i32; let mut legendicon3color = 0i32;
    let mut legendicon4 = 0i32; let mut legendicon4color = 0i32;
    let mut legendicon5 = 0i32; let mut legendicon5color = 0i32;
    let mut reward2amount = 0i32; let mut rewardamount = 0i32;
    let mut rewarditm = 0i32;     let mut reward2itm = 0i32;
    let mut _1stPlaceReward1_ItmId = 0i32;    let mut _1stPlaceReward1_Amount = 0i32;
    let mut _1stPlaceReward2_ItmId = 0i32;    let mut _1stPlaceReward2_Amount = 0i32;
    let mut _2ndPlaceReward1_ItmId = 0i32;    let mut _2ndPlaceReward1_Amount = 0i32;
    let mut _2ndPlaceReward2_ItmId = 0i32;    let mut _2ndPlaceReward2_Amount = 0i32;
    let mut _3rdPlaceReward1_ItmId = 0i32;    let mut _3rdPlaceReward1_Amount = 0i32;
    let mut _3rdPlaceReward2_ItmId = 0i32;    let mut _3rdPlaceReward2_Amount = 0i32;
    let mut _4thPlaceReward1_ItmId = 0i32;    let mut _4thPlaceReward1_Amount = 0i32;
    let mut _4thPlaceReward2_ItmId = 0i32;    let mut _4thPlaceReward2_Amount = 0i32;
    let mut _5thPlaceReward1_ItmId = 0i32;    let mut _5thPlaceReward1_Amount = 0i32;
    let mut _5thPlaceReward2_ItmId = 0i32;    let mut _5thPlaceReward2_Amount = 0i32;

    let stmt = SqlStmt_Malloc(sql_handle);
    if stmt.is_null() {
        SqlStmt_ShowDebug(stmt, c"events.rs".as_ptr(), line!() as c_ulong);
        return 0;
    }

    if SQL_ERROR == SqlStmt_Prepare(
        stmt,
        b"SELECT `EventRewardRanks_Display`, `EventLegend`, \
`EventLegendIcon1`, `EventLegendIcon1Color`, \
`EventLegendIcon2`, `EventLegendIcon2Color`, \
`EventLegendIcon3`, `EventLegendIcon3Color`, \
`EventLegendIcon4`, `EventLegendIcon4Color`, \
`EventLegendIcon5`, `EventLegendIcon5Color`, \
`1stPlaceReward1_ItmId`, `1stPlaceReward1_Amount`, \
`1stPlaceReward2_ItmId`, `1stPlaceReward2_Amount`, \
`2ndPlaceReward1_ItmId`, `2ndPlaceReward1_Amount`, \
`2ndPlaceReward2_ItmId`, `2ndPlaceReward2_Amount`, \
`3rdPlaceReward1_ItmId`, `3rdPlaceReward1_Amount`, \
`3rdPlaceReward2_ItmId`, `3rdPlaceReward2_Amount`, \
`4thPlaceReward1_ItmId`, `4thPlaceReward1_Amount`, \
`4thPlaceReward2_ItmId`, `4thPlaceReward2_Amount`, \
`5thPlaceReward1_ItmId`, `5thPlaceReward1_Amount`, \
`5thPlaceReward2_ItmId`, `5thPlaceReward2_Amount` \
FROM `RankingEvents` WHERE `EventId` = '%u'\0"
            .as_ptr() as *const c_char,
        eventid,
    ) || SQL_ERROR == SqlStmt_Execute(stmt)
      || SQL_ERROR == SqlStmt_BindColumn(stmt,  0, SqlDataType::SqlDtInt,    &mut rewardranks         as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt,  1, SqlDataType::SqlDtString, legend.as_mut_ptr() as *mut c_void, std::mem::size_of_val(&legend), std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt,  2, SqlDataType::SqlDtInt,    &mut legendicon1         as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt,  3, SqlDataType::SqlDtInt,    &mut legendicon1color    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt,  4, SqlDataType::SqlDtInt,    &mut legendicon2         as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt,  5, SqlDataType::SqlDtInt,    &mut legendicon2color    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt,  6, SqlDataType::SqlDtInt,    &mut legendicon3         as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt,  7, SqlDataType::SqlDtInt,    &mut legendicon3color    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt,  8, SqlDataType::SqlDtInt,    &mut legendicon4         as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt,  9, SqlDataType::SqlDtInt,    &mut legendicon4color    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 10, SqlDataType::SqlDtInt,    &mut legendicon5         as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 11, SqlDataType::SqlDtInt,    &mut legendicon5color    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 12, SqlDataType::SqlDtInt,    &mut _1stPlaceReward1_ItmId    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 13, SqlDataType::SqlDtInt,    &mut _1stPlaceReward1_Amount   as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 14, SqlDataType::SqlDtInt,    &mut _1stPlaceReward2_ItmId    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 15, SqlDataType::SqlDtInt,    &mut _1stPlaceReward2_Amount   as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 16, SqlDataType::SqlDtInt,    &mut _2ndPlaceReward1_ItmId    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 17, SqlDataType::SqlDtInt,    &mut _2ndPlaceReward1_Amount   as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 18, SqlDataType::SqlDtInt,    &mut _2ndPlaceReward2_ItmId    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 19, SqlDataType::SqlDtInt,    &mut _2ndPlaceReward2_Amount   as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 20, SqlDataType::SqlDtInt,    &mut _3rdPlaceReward1_ItmId    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 21, SqlDataType::SqlDtInt,    &mut _3rdPlaceReward1_Amount   as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 22, SqlDataType::SqlDtInt,    &mut _3rdPlaceReward2_ItmId    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 23, SqlDataType::SqlDtInt,    &mut _3rdPlaceReward2_Amount   as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 24, SqlDataType::SqlDtInt,    &mut _4thPlaceReward1_ItmId    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 25, SqlDataType::SqlDtInt,    &mut _4thPlaceReward1_Amount   as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 26, SqlDataType::SqlDtInt,    &mut _4thPlaceReward2_ItmId    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 27, SqlDataType::SqlDtInt,    &mut _4thPlaceReward2_Amount   as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 28, SqlDataType::SqlDtInt,    &mut _5thPlaceReward1_ItmId    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 29, SqlDataType::SqlDtInt,    &mut _5thPlaceReward1_Amount   as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 30, SqlDataType::SqlDtInt,    &mut _5thPlaceReward2_ItmId    as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 31, SqlDataType::SqlDtInt,    &mut _5thPlaceReward2_Amount   as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
    {
        SqlStmt_ShowDebug(stmt, c"events.rs".as_ptr(), line!() as c_ulong);
        SqlStmt_Free(stmt);
        return 0;
    }

    if SQL_SUCCESS != SqlStmt_NextRow(stmt) {
        SqlStmt_ShowDebug(stmt, c"events.rs".as_ptr(), line!() as c_ulong);
        SqlStmt_Free(stmt);
        rewardranks = 0;
        return 0;
    }

    SqlStmt_Free(stmt);

    // If rewardranks == 0, goto end (blank page)
    if rewardranks == 0 {
        return 0;
    }

    // Zero out packet bytes 13..900
    for i in 13..900usize {
        wfifob(fd, i, 0);
    }

    wfifob(fd, 11, rewardranks as u8);

    let mut pos: usize = 13;

    for i in 0..rewardranks as usize {
        let rank = (i as u8) + 49; // '1'..'5' etc.
        let rank_num = (i as c_int) + 1;

        wfifob(fd, pos,     rank); // Rank 1st #
        wfifob(fd, pos + 1, 1);   // squigley
        wfifob(fd, pos + 2, rank); // Rank #

        pos += 3;

        let mut legendicon = 0i32;
        let mut legendiconcolor = 0i32;

        match rank_num {
            1 => {
                libc::sprintf(buf.as_mut_ptr(), b"%s [%ist] %s\0".as_ptr() as *const c_char, legend.as_ptr(), rank_num, monthyear.as_ptr());
                legendicon = legendicon1; legendiconcolor = legendicon1color;
                rewarditm = _1stPlaceReward1_ItmId; rewardamount = _1stPlaceReward1_Amount;
                reward2itm = _1stPlaceReward2_ItmId; reward2amount = _1stPlaceReward2_Amount;
            }
            2 => {
                libc::sprintf(buf.as_mut_ptr(), b"%s [%ind] %s\0".as_ptr() as *const c_char, legend.as_ptr(), rank_num, monthyear.as_ptr());
                legendicon = legendicon2; legendiconcolor = legendicon2color;
                rewarditm = _2ndPlaceReward1_ItmId; rewardamount = _2ndPlaceReward1_Amount;
                reward2itm = _2ndPlaceReward2_ItmId; reward2amount = _2ndPlaceReward2_Amount;
            }
            3 => {
                libc::sprintf(buf.as_mut_ptr(), b"%s [%ird] %s\0".as_ptr() as *const c_char, legend.as_ptr(), rank_num, monthyear.as_ptr());
                legendicon = legendicon3; legendiconcolor = legendicon3color;
                rewarditm = _3rdPlaceReward1_ItmId; rewardamount = _3rdPlaceReward1_Amount;
                reward2itm = _3rdPlaceReward2_ItmId; reward2amount = _3rdPlaceReward2_Amount;
            }
            4 => {
                libc::sprintf(buf.as_mut_ptr(), b"%s [%ith] %s\0".as_ptr() as *const c_char, legend.as_ptr(), rank_num, monthyear.as_ptr());
                legendicon = legendicon4; legendiconcolor = legendicon4color;
                rewarditm = _4thPlaceReward1_ItmId; rewardamount = _4thPlaceReward1_Amount;
                reward2itm = _4thPlaceReward2_ItmId; reward2amount = _4thPlaceReward2_Amount;
            }
            _ => {
                libc::sprintf(buf.as_mut_ptr(), b"%s [%ith] %s\0".as_ptr() as *const c_char, legend.as_ptr(), rank_num, monthyear.as_ptr());
                legendicon = legendicon5; legendiconcolor = legendicon5color;
                rewarditm = _5thPlaceReward1_ItmId; rewardamount = _5thPlaceReward1_Amount;
                reward2itm = _5thPlaceReward2_ItmId; reward2amount = _5thPlaceReward2_Amount;
            }
        }

        let buf_len = libc::strlen(buf.as_ptr());
        wfifob(fd, pos, buf_len as u8);
        pos += 1;
        wfifop_copy(fd, pos, buf.as_ptr() as *const u8, buf_len);
        pos += buf_len;

        wfifob(fd, pos,     legendicon as u8);       // ICON
        pos += 1;
        wfifob(fd, pos,     legendiconcolor as u8);  // COLOR
        pos += 1;

        if reward2amount == 0 {
            wfifob(fd, pos, 1); // 1 reward for this rank
        } else {
            wfifob(fd, pos, 2); // 2 rewards
        }
        pos += 1;

        // Reward 1 name
        libc::sprintf(buf.as_mut_ptr(), b"%s\0".as_ptr() as *const c_char, itemdb_name(rewarditm as c_uint));
        let buf_len = libc::strlen(buf.as_ptr());
        wfifob(fd, pos, buf_len as u8);
        pos += 1;
        wfifop_copy(fd, pos, buf.as_ptr() as *const u8, buf_len);
        pos += buf_len;
        pos += 3; // padding

        clif_intcheck(rewardamount, pos as c_int, fd);
        pos += 2;
        clif_intcheck(itemdb_icon(rewarditm as c_uint) - 49152, pos as c_int, fd);
        pos += 1;
        wfifob(fd, pos, itemdb_iconcolor(rewarditm as c_uint) as u8);
        pos += 1;

        if reward2amount == 0 {
            wfifob(fd, pos, 1);
            pos += 1;
            continue;
        }

        // Reward 2 name
        libc::sprintf(buf.as_mut_ptr(), b"%s\0".as_ptr() as *const c_char, itemdb_name(reward2itm as c_uint));
        let buf_len = libc::strlen(buf.as_ptr());
        wfifob(fd, pos, buf_len as u8);
        pos += 1;
        wfifop_copy(fd, pos, buf.as_ptr() as *const u8, buf_len);
        pos += buf_len;
        pos += 3;

        clif_intcheck(reward2amount, pos as c_int, fd);
        pos += 2;
        clif_intcheck(itemdb_icon(reward2itm as c_uint) - 49152, pos as c_int, fd);
        pos += 1;
        wfifob(fd, pos, itemdb_iconcolor(reward2itm as c_uint) as u8);
        pos += 1;
        wfifob(fd, pos, 1);
        pos += 1;
    }

    // packetsize: pos - 3 (encryption appends 3 bytes)
    wfifob(fd, 2, (pos - 3) as u8);
    wfifoset(fd, encrypt(fd) as usize);

    0
}

// ─── retrieveEventDates ───────────────────────────────────────────────────────

/// Query and write event date/time fields into the WFIFO at the given offset.
///
/// Writes 4 ints via `clif_intcheck` at `pos+7`, `pos+11`, `pos+15`, `pos+19`.
/// C line 4900.
#[no_mangle]
pub unsafe extern "C" fn retrieveEventDates(eventid: c_int, pos: c_int, fd: c_int) {
    let mut from_date = 0i32;
    let mut from_time = 0i32;
    let mut to_date   = 0i32;
    let mut to_time   = 0i32;

    let stmt = SqlStmt_Malloc(sql_handle);
    if stmt.is_null() {
        SqlStmt_ShowDebug(stmt, c"events.rs".as_ptr(), line!() as c_ulong);
        return;
    }

    if SQL_ERROR == SqlStmt_Prepare(
        stmt,
        b"SELECT `FromDate`, `FromTime`, `ToDate`, `ToTime` FROM `RankingEvents` WHERE `EventId` = '%u'\0"
            .as_ptr() as *const c_char,
        eventid as c_uint,
    ) || SQL_ERROR == SqlStmt_Execute(stmt)
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SqlDataType::SqlDtInt, &mut from_date as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 1, SqlDataType::SqlDtInt, &mut from_time as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 2, SqlDataType::SqlDtInt, &mut to_date   as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
      || SQL_ERROR == SqlStmt_BindColumn(stmt, 3, SqlDataType::SqlDtInt, &mut to_time   as *mut c_int as *mut c_void, 0, std::ptr::null_mut(), std::ptr::null_mut())
    {
        SqlStmt_ShowDebug(stmt, c"events.rs".as_ptr(), line!() as c_ulong);
        SqlStmt_Free(stmt);
        return;
    }

    if SQL_SUCCESS != SqlStmt_NextRow(stmt) {
        SqlStmt_ShowDebug(stmt, c"events.rs".as_ptr(), line!() as c_ulong);
        SqlStmt_Free(stmt);
        return;
    }

    SqlStmt_Free(stmt);

    clif_intcheck(from_date, pos + 7,  fd);
    clif_intcheck(from_time, pos + 11, fd);
    clif_intcheck(to_date,   pos + 15, fd);
    clif_intcheck(to_time,   pos + 19, fd);
}

// ─── checkPlayerScore ─────────────────────────────────────────────────────────

/// Return the player's score for `eventid`, or 0 if not found / on error.
///
/// C line 4951.
#[no_mangle]
pub unsafe extern "C" fn checkPlayerScore(eventid: c_int, sd: *mut MapSessionData) -> c_int {
    let mut score = 0i32;

    let stmt = SqlStmt_Malloc(sql_handle);
    if stmt.is_null() {
        SqlStmt_ShowDebug(stmt, c"events.rs".as_ptr(), line!() as c_ulong);
        return 0;
    }

    if SQL_ERROR == SqlStmt_Prepare(
        stmt,
        b"SELECT `Score` FROM `RankingScores` WHERE `EventId` = '%u' AND `ChaId` = '%u'\0"
            .as_ptr() as *const c_char,
        eventid as c_uint,
        (*sd).status.id,
    ) || SQL_ERROR == SqlStmt_Execute(stmt)
      || SQL_ERROR == SqlStmt_BindColumn(
            stmt, 0,
            SqlDataType::SqlDtInt,
            &mut score as *mut c_int as *mut c_void,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    {
        SqlStmt_ShowDebug(stmt, c"events.rs".as_ptr(), line!() as c_ulong);
        SqlStmt_Free(stmt);
        return 0;
    }

    if SQL_SUCCESS != SqlStmt_NextRow(stmt) {
        SqlStmt_Free(stmt);
        return 0;
    }

    SqlStmt_Free(stmt);
    score
}

// ─── updateRanks ──────────────────────────────────────────────────────────────

/// Re-rank all scores for `eventid` using a MySQL user-variable counter.
///
/// Issues `SET @r=0` then `UPDATE … SET Rank = @r := (@r+1) ORDER BY Score DESC`.
/// C line 4983.
#[no_mangle]
pub unsafe extern "C" fn updateRanks(eventid: c_int) {
    let stmt = SqlStmt_Malloc(sql_handle);
    if stmt.is_null() {
        SqlStmt_ShowDebug(stmt, c"events.rs".as_ptr(), line!() as c_ulong);
        SqlStmt_Free(stmt);
        return;
    }

    if SQL_ERROR == SqlStmt_Prepare(
        stmt,
        b"SELECT `Rank` FROM `RankingScores` WHERE `EventId` = '%i' ORDER BY `Score` DESC\0"
            .as_ptr() as *const c_char,
        eventid,
    ) || SQL_ERROR == SqlStmt_Execute(stmt)
    {
        SqlStmt_ShowDebug(stmt, c"events.rs".as_ptr(), line!() as c_ulong);
        SqlStmt_Free(stmt);
        return;
    }

    SqlStmt_Free(stmt);

    if SQL_ERROR == Sql_Query(sql_handle, b"SET @r=0\0".as_ptr() as *const c_char) {
        Sql_ShowDebug_(sql_handle, c"events.rs".as_ptr(), line!() as c_ulong);
        Sql_FreeResult(sql_handle);
        return;
    }

    if SQL_ERROR == Sql_Query(
        sql_handle,
        b"UPDATE `RankingScores` SET `Rank`= @r:= (@r+1) WHERE `EventId` = '%i' ORDER BY `Score` DESC\0"
            .as_ptr() as *const c_char,
        eventid,
    ) {
        Sql_ShowDebug_(sql_handle, c"events.rs".as_ptr(), line!() as c_ulong);
        Sql_FreeResult(sql_handle);
    }
}

// ─── checkPlayerRank ──────────────────────────────────────────────────────────

/// Return the player's current rank for `eventid`, or 0 if not found / on error.
///
/// C line 5018.
#[no_mangle]
pub unsafe extern "C" fn checkPlayerRank(eventid: c_int, sd: *mut MapSessionData) -> c_int {
    let mut rank = 0i32;

    let stmt = SqlStmt_Malloc(sql_handle);
    if stmt.is_null() {
        SqlStmt_ShowDebug(stmt, c"events.rs".as_ptr(), line!() as c_ulong);
        return 0;
    }

    if SQL_ERROR == SqlStmt_Prepare(
        stmt,
        b"SELECT `Rank` FROM `RankingScores` WHERE `EventId` = '%u' AND `ChaId` = '%i'\0"
            .as_ptr() as *const c_char,
        eventid as c_uint,
        (*sd).status.id,
    ) || SQL_ERROR == SqlStmt_Execute(stmt)
      || SQL_ERROR == SqlStmt_BindColumn(
            stmt, 0,
            SqlDataType::SqlDtInt,
            &mut rank as *mut c_int as *mut c_void,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    {
        SqlStmt_Free(stmt);
        return 0;
    }

    if SQL_SUCCESS != SqlStmt_NextRow(stmt) {
        SqlStmt_Free(stmt);
        return 0;
    }

    rank
}
