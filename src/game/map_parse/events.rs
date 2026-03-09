
#![allow(non_snake_case, clippy::wildcard_imports, clippy::too_many_lines)]


use crate::database::{blocking_run, get_pool};

use crate::game::pc::{
    MapSessionData,
};

use super::packet::{
    rfifob,
    wfifob, wfifohead, wfifoset,
    encrypt,
};


use crate::database::item_db::{
    rust_itemdb_name as itemdb_name, rust_itemdb_icon as itemdb_icon,
    rust_itemdb_iconcolor as itemdb_iconcolor,
};
use crate::game::map_server::{nmail_sendmail, cur_year, cur_season};
use std::sync::atomic::Ordering as AtomicOrd;
use crate::game::pc::rust_pc_readglobalreg as pc_readglobalreg;
use crate::game::map_parse::chat::clif_sendmsg;
use crate::session::rust_session_wdata_ptr;

// ─── wfifop_copy helper ───────────────────────────────────────────────────────

/// Copy `len` bytes from `src` into the send-buffer at `pos`.
#[inline]
unsafe fn wfifop_copy(fd: i32, pos: usize, src: *const u8, len: usize) {
    let dst = rust_session_wdata_ptr(fd, pos);
    if !dst.is_null() {
        std::ptr::copy_nonoverlapping(src, dst, len);
    }
}

/// Write a big-endian u16 into the send buffer at `pos`.
#[inline]
unsafe fn wfifow_be(fd: i32, pos: usize, val: u16) {
    let p = rust_session_wdata_ptr(fd, pos) as *mut u16;
    if !p.is_null() { p.write_unaligned(val.to_be()); }
}

/// Write a big-endian u32 into the send buffer at `pos`.
#[inline]
unsafe fn wfifol_be(fd: i32, pos: usize, val: u32) {
    let p = rust_session_wdata_ptr(fd, pos) as *mut u32;
    if !p.is_null() { p.write_unaligned(val.to_be()); }
}

// ─── sqlx row structs ─────────────────────────────────────────────────────────

/// Row struct for clif_getReward: all columns from RankingEvents used by that function.
#[derive(sqlx::FromRow)]
struct EventRewardRow {
    #[sqlx(rename = "EventName")]              event_name:  String,
    #[sqlx(rename = "EventLegend")]            event_legend: String,
    #[sqlx(rename = "EventRewardRanks_Display")] reward_ranks: i32,
    #[sqlx(rename = "EventLegendIcon1")]       icon1:        i32,
    #[sqlx(rename = "EventLegendIcon1Color")]  icon1_color:  i32,
    #[sqlx(rename = "EventLegendIcon2")]       icon2:        i32,
    #[sqlx(rename = "EventLegendIcon2Color")]  icon2_color:  i32,
    #[sqlx(rename = "EventLegendIcon3")]       icon3:        i32,
    #[sqlx(rename = "EventLegendIcon3Color")]  icon3_color:  i32,
    #[sqlx(rename = "EventLegendIcon4")]       icon4:        i32,
    #[sqlx(rename = "EventLegendIcon4Color")]  icon4_color:  i32,
    #[sqlx(rename = "EventLegendIcon5")]       icon5:        i32,
    #[sqlx(rename = "EventLegendIcon5Color")]  icon5_color:  i32,
    // int(10) unsigned columns — must be u32 or sqlx decode fails silently
    #[sqlx(rename = "1stPlaceReward1_ItmId")]  r1_itm1:      u32,
    #[sqlx(rename = "1stPlaceReward1_Amount")] r1_amt1:      u32,
    #[sqlx(rename = "1stPlaceReward2_ItmId")]  r1_itm2:      u32,
    #[sqlx(rename = "1stPlaceReward2_Amount")] r1_amt2:      u32,
    #[sqlx(rename = "2ndPlaceReward1_ItmId")]  r2_itm1:      u32,
    #[sqlx(rename = "2ndPlaceReward1_Amount")] r2_amt1:      u32,
    #[sqlx(rename = "2ndPlaceReward2_ItmId")]  r2_itm2:      u32,
    #[sqlx(rename = "2ndPlaceReward2_Amount")] r2_amt2:      u32,
    #[sqlx(rename = "3rdPlaceReward1_ItmId")]  r3_itm1:      u32,
    #[sqlx(rename = "3rdPlaceReward1_Amount")] r3_amt1:      u32,
    #[sqlx(rename = "3rdPlaceReward2_ItmId")]  r3_itm2:      u32,
    #[sqlx(rename = "3rdPlaceReward2_Amount")] r3_amt2:      u32,
    #[sqlx(rename = "4thPlaceReward1_ItmId")]  r4_itm1:      u32,
    #[sqlx(rename = "4thPlaceReward1_Amount")] r4_amt1:      u32,
    #[sqlx(rename = "4thPlaceReward2_ItmId")]  r4_itm2:      u32,
    #[sqlx(rename = "4thPlaceReward2_Amount")] r4_amt2:      u32,
    #[sqlx(rename = "5thPlaceReward1_ItmId")]  r5_itm1:      u32,
    #[sqlx(rename = "5thPlaceReward1_Amount")] r5_amt1:      u32,
    #[sqlx(rename = "5thPlaceReward2_ItmId")]  r5_itm2:      u32,
    #[sqlx(rename = "5thPlaceReward2_Amount")] r5_amt2:      u32,
}

/// Row struct for clif_parseranking: same columns minus EventName.
#[derive(sqlx::FromRow)]
struct RankingEventRow {
    #[sqlx(rename = "EventRewardRanks_Display")] reward_ranks: i32,
    #[sqlx(rename = "EventLegend")]            event_legend: String,
    #[sqlx(rename = "EventLegendIcon1")]       icon1:        i32,
    #[sqlx(rename = "EventLegendIcon1Color")]  icon1_color:  i32,
    #[sqlx(rename = "EventLegendIcon2")]       icon2:        i32,
    #[sqlx(rename = "EventLegendIcon2Color")]  icon2_color:  i32,
    #[sqlx(rename = "EventLegendIcon3")]       icon3:        i32,
    #[sqlx(rename = "EventLegendIcon3Color")]  icon3_color:  i32,
    #[sqlx(rename = "EventLegendIcon4")]       icon4:        i32,
    #[sqlx(rename = "EventLegendIcon4Color")]  icon4_color:  i32,
    #[sqlx(rename = "EventLegendIcon5")]       icon5:        i32,
    #[sqlx(rename = "EventLegendIcon5Color")]  icon5_color:  i32,
    // int(10) unsigned columns — must be u32 or sqlx decode fails silently
    #[sqlx(rename = "1stPlaceReward1_ItmId")]  r1_itm1:      u32,
    #[sqlx(rename = "1stPlaceReward1_Amount")] r1_amt1:      u32,
    #[sqlx(rename = "1stPlaceReward2_ItmId")]  r1_itm2:      u32,
    #[sqlx(rename = "1stPlaceReward2_Amount")] r1_amt2:      u32,
    #[sqlx(rename = "2ndPlaceReward1_ItmId")]  r2_itm1:      u32,
    #[sqlx(rename = "2ndPlaceReward1_Amount")] r2_amt1:      u32,
    #[sqlx(rename = "2ndPlaceReward2_ItmId")]  r2_itm2:      u32,
    #[sqlx(rename = "2ndPlaceReward2_Amount")] r2_amt2:      u32,
    #[sqlx(rename = "3rdPlaceReward1_ItmId")]  r3_itm1:      u32,
    #[sqlx(rename = "3rdPlaceReward1_Amount")] r3_amt1:      u32,
    #[sqlx(rename = "3rdPlaceReward2_ItmId")]  r3_itm2:      u32,
    #[sqlx(rename = "3rdPlaceReward2_Amount")] r3_amt2:      u32,
    #[sqlx(rename = "4thPlaceReward1_ItmId")]  r4_itm1:      u32,
    #[sqlx(rename = "4thPlaceReward1_Amount")] r4_amt1:      u32,
    #[sqlx(rename = "4thPlaceReward2_ItmId")]  r4_itm2:      u32,
    #[sqlx(rename = "4thPlaceReward2_Amount")] r4_amt2:      u32,
    #[sqlx(rename = "5thPlaceReward1_ItmId")]  r5_itm1:      u32,
    #[sqlx(rename = "5thPlaceReward1_Amount")] r5_amt1:      u32,
    #[sqlx(rename = "5thPlaceReward2_ItmId")]  r5_itm2:      u32,
    #[sqlx(rename = "5thPlaceReward2_Amount")] r5_amt2:      u32,
}

/// Row struct for retrieveEventDates.
#[derive(sqlx::FromRow)]
struct EventDates {
    #[sqlx(rename = "FromDate")] from_date: i32,
    #[sqlx(rename = "FromTime")] from_time: i32,
    #[sqlx(rename = "ToDate")]   to_date:   i32,
    #[sqlx(rename = "ToTime")]   to_time:   i32,
}

// ─── clif_intcheck ────────────────────────────────────────────────────────────

/// Write `number` into the WFIFO at `field`, using the minimal encoding.
///
/// - `number` <= 254: write one byte at `field`
/// - `number` <= 65535: write two bytes (big-endian) at `field - 1`
/// - otherwise: write four bytes (big-endian) at `field - 3`
///
/// C line 4883.
pub unsafe fn clif_intcheck(number: i32, field: i32, fd: i32) {
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
pub unsafe fn sendRewardParcel(
    sd:           *mut MapSessionData,
    eventid:      i32,
    rank:         i32,
    rewarditem:   i32,
    rewardamount: i32,
) -> i32 {
    let _ = eventid; // used in reward message only (via sprintf); not in SQL directly

    let receiver = (*sd).status.id as u32;
    let rewarditem_u = rewarditem as u32;

    // Build escape string: "name,\nCongratulations on attaining Rank N!\nHere is your reward: (amount) name"
    let mut escape = [0i8; 255];
    {
        let item_name = itemdb_name(rewarditem_u);
        libc::sprintf(
            escape.as_mut_ptr(),
            b"%s,\nCongratulations on attaining Rank %i!\nHere is your reward: (%i) %s\0"
                .as_ptr() as *const i8,
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

    let sender: u32 = 1;
    let owner:  u32 = 0;
    let npcflag: i32 = 1;

    // Find highest existing position for this receiver.
    // ParPosition is int(10) unsigned — query_scalar must use u32.
    let newest: i32 = (blocking_run(async move {
        sqlx::query_scalar::<_, Option<u32>>(
            "SELECT MAX(`ParPosition`) FROM `Parcels` WHERE `ParChaIdDestination` = ?"
        )
        .bind(receiver)
        .fetch_one(get_pool())
        .await
        .ok()
        .flatten()
        .unwrap_or(0) as i32
    })) + 1;

    // engrave is item name (up to 30 chars); use it directly in parameterized query
    let engrave_str = std::ffi::CStr::from_ptr(engrave.as_ptr())
        .to_str()
        .unwrap_or("")
        .to_owned();

    let ok = blocking_run(async move {
        sqlx::query(
            "INSERT INTO `Parcels` (`ParChaIdDestination`, `ParSender`, `ParItmId`, \
`ParAmount`, `ParChaIdOwner`, `ParEngrave`, `ParPosition`, `ParNpc`) \
VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(receiver)
        .bind(sender)
        .bind(rewarditem as u32)
        .bind(rewardamount as u32)
        .bind(owner)
        .bind(engrave_str)
        .bind(newest)
        .bind(npcflag)
        .execute(get_pool())
        .await
        .is_ok()
    });
    if !ok {
        return 1;
    }

    1 // success = 1
}

// ─── clif_getReward ───────────────────────────────────────────────────────────

/// Handle the "get reward" packet for an event: look up the event/rank data,
/// award parcels, send a mail confirmation, and update the claim flag.
///
/// C line 4186.
pub unsafe fn clif_getReward(sd: *mut MapSessionData, fd: i32) -> i32 {
    let eventid = rfifob(fd, 7) as i32;
    let g_cur_year   = cur_year.load(AtomicOrd::Relaxed);
    let g_cur_season = cur_season.load(AtomicOrd::Relaxed);

    let mut legend: [i8; 17]     = [0; 17];
    let mut eventname: [i8; 41]  = [0; 41];
    let mut monthyear: [i8; 7]   = [0; 7];
    let mut season: [i8; 7]      = [0; 7];

    libc::sprintf(
        monthyear.as_mut_ptr(),
        b"Moon %i\0".as_ptr() as *const i8,
        g_cur_year,
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

    // Query 1: event metadata + per-rank rewards
    let event_id_u = eventid as u32;
    let Some(er) = blocking_run(async move {
        sqlx::query_as::<_, EventRewardRow>(
            "SELECT `EventName`, `EventLegend`, `EventRewardRanks_Display`, \
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
             FROM `RankingEvents` WHERE `EventId` = ?"
        )
        .bind(event_id_u)
        .fetch_optional(get_pool())
        .await
        .ok()
        .flatten()
    }) else { return 0; };

    // Copy fields from row into local variables
    {
        let name_bytes = er.event_name.as_bytes();
        let copy_len = name_bytes.len().min(eventname.len() - 1);
        std::ptr::copy_nonoverlapping(name_bytes.as_ptr() as *const i8, eventname.as_mut_ptr(), copy_len);
        eventname[copy_len] = 0;
    }
    {
        let leg_bytes = er.event_legend.as_bytes();
        let copy_len = leg_bytes.len().min(legend.len() - 1);
        std::ptr::copy_nonoverlapping(leg_bytes.as_ptr() as *const i8, legend.as_mut_ptr(), copy_len);
        legend[copy_len] = 0;
    }
    rewardranks        = er.reward_ranks;
    legendicon1        = er.icon1;
    legendicon1color   = er.icon1_color;
    legendicon2        = er.icon2;
    legendicon2color   = er.icon2_color;
    legendicon3        = er.icon3;
    legendicon3color   = er.icon3_color;
    legendicon4        = er.icon4;
    legendicon4color   = er.icon4_color;
    legendicon5        = er.icon5;
    legendicon5color   = er.icon5_color;
    _1stPlaceReward1_ItmId  = er.r1_itm1 as i32;
    _1stPlaceReward1_Amount = er.r1_amt1 as i32;
    _1stPlaceReward2_ItmId  = er.r1_itm2 as i32;
    _1stPlaceReward2_Amount = er.r1_amt2 as i32;
    _2ndPlaceReward1_ItmId  = er.r2_itm1 as i32;
    _2ndPlaceReward1_Amount = er.r2_amt1 as i32;
    _2ndPlaceReward2_ItmId  = er.r2_itm2 as i32;
    _2ndPlaceReward2_Amount = er.r2_amt2 as i32;
    _3rdPlaceReward1_ItmId  = er.r3_itm1 as i32;
    _3rdPlaceReward1_Amount = er.r3_amt1 as i32;
    _3rdPlaceReward2_ItmId  = er.r3_itm2 as i32;
    _3rdPlaceReward2_Amount = er.r3_amt2 as i32;
    _4thPlaceReward1_ItmId  = er.r4_itm1 as i32;
    _4thPlaceReward1_Amount = er.r4_amt1 as i32;
    _4thPlaceReward2_ItmId  = er.r4_itm2 as i32;
    _4thPlaceReward2_Amount = er.r4_amt2 as i32;
    _5thPlaceReward1_ItmId  = er.r5_itm1 as i32;
    _5thPlaceReward1_Amount = er.r5_amt1 as i32;
    _5thPlaceReward2_ItmId  = er.r5_itm2 as i32;
    _5thPlaceReward2_Amount = er.r5_amt2 as i32;

    // Query 2: player's rank for this event
    // ChaId is int(10) signed — bind as i32; status.id is u32 so cast
    let cha_id = (*sd).status.id as i32;
    let Some(rank) = blocking_run(async move {
        sqlx::query_scalar::<_, i32>(
            "SELECT `Rank` FROM `RankingScores` WHERE `ChaId` = ? AND `EventId` = ?"
        )
        .bind(cha_id)
        .bind(event_id_u)
        .fetch_optional(get_pool())
        .await
        .ok()
        .flatten()
    }) else { return 0; };

    // Determine season string
    if g_cur_season == 1 { libc::strcpy(season.as_mut_ptr(), b"Winter\0".as_ptr() as *const i8); }
    if g_cur_season == 2 { libc::strcpy(season.as_mut_ptr(), b"Spring\0".as_ptr() as *const i8); }
    if g_cur_season == 3 { libc::strcpy(season.as_mut_ptr(), b"Summer\0".as_ptr() as *const i8); }
    if g_cur_season == 4 { libc::strcpy(season.as_mut_ptr(), b"Fall\0".as_ptr()   as *const i8); }

    if rank == 1 { libc::strcpy(rankname.as_mut_ptr(), b"1st\0".as_ptr() as *const i8); }
    if rank == 2 { libc::strcpy(rankname.as_mut_ptr(), b"2nd\0".as_ptr() as *const i8); }
    if rank == 3 { libc::strcpy(rankname.as_mut_ptr(), b"3rd\0".as_ptr() as *const i8); }
    if rank == 4 { libc::strcpy(rankname.as_mut_ptr(), b"4th\0".as_ptr() as *const i8); }
    if rank == 5 { libc::strcpy(rankname.as_mut_ptr(), b"5th\0".as_ptr() as *const i8); }
    if rank == 6 { libc::strcpy(rankname.as_mut_ptr(), b"6th\0".as_ptr() as *const i8); }

    match rank {
        1 => {
            libc::sprintf(legendbuf.as_mut_ptr(), b"%s [%s] (Moon %i, %s)\0".as_ptr() as *const i8, legend.as_ptr(), rankname.as_ptr(), g_cur_year, season.as_ptr());
            legendicon      = legendicon1;
            legendiconcolor = legendicon1color;
            reward1item     = _1stPlaceReward1_ItmId;
            reward1amount   = _1stPlaceReward1_Amount;
            reward2item     = _1stPlaceReward2_ItmId;
            reward2amount   = _1stPlaceReward2_Amount;
        }
        2 => {
            libc::sprintf(legendbuf.as_mut_ptr(), b"%s [%s] (Moon %i, %s)\0".as_ptr() as *const i8, legend.as_ptr(), rankname.as_ptr(), g_cur_year, season.as_ptr());
            legendicon      = legendicon2;
            legendiconcolor = legendicon2color;
            reward1item     = _2ndPlaceReward1_ItmId;
            reward1amount   = _2ndPlaceReward1_Amount;
            reward2item     = _2ndPlaceReward2_ItmId;
            reward2amount   = _2ndPlaceReward2_Amount;
        }
        3 => {
            libc::sprintf(legendbuf.as_mut_ptr(), b"%s [%s] (Moon %i, %s)\0".as_ptr() as *const i8, legend.as_ptr(), rankname.as_ptr(), g_cur_year, season.as_ptr());
            legendicon      = legendicon3;
            legendiconcolor = legendicon3color;
            reward1item     = _3rdPlaceReward1_ItmId;
            reward1amount   = _3rdPlaceReward1_Amount;
            reward2item     = _3rdPlaceReward2_ItmId;
            reward2amount   = _3rdPlaceReward2_Amount;
        }
        4 => {
            libc::sprintf(legendbuf.as_mut_ptr(), b"%s [%s] (Moon %i, %s)\0".as_ptr() as *const i8, legend.as_ptr(), rankname.as_ptr(), g_cur_year, season.as_ptr());
            legendicon      = legendicon4;
            legendiconcolor = legendicon4color;
            reward1item     = _4thPlaceReward1_ItmId;
            reward1amount   = _4thPlaceReward1_Amount;
            reward2item     = _4thPlaceReward2_ItmId;
            reward2amount   = _4thPlaceReward2_Amount;
        }
        _ => {
            libc::sprintf(legendbuf.as_mut_ptr(), b"%s [%s] (Moon %i, %s)\0".as_ptr() as *const i8, legend.as_ptr(), rankname.as_ptr(), g_cur_year, season.as_ptr());
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

        if libc::strcmp(leg_name_ptr, b"\0".as_ptr() as *const i8) == 0
            && libc::strcasecmp(leg_name1_ptr, b"\0".as_ptr() as *const i8) == 0
        {
            libc::strcpy((*sd).status.legends[i].text.as_mut_ptr(), legendbuf.as_ptr());
            libc::sprintf(
                (*sd).status.legends[i].name.as_mut_ptr(),
                b"Event %i Place: %i\0".as_ptr() as *const i8,
                eventid, rank,
            );
            (*sd).status.legends[i].icon  = legendicon as u16;
            (*sd).status.legends[i].color = legendiconcolor as u16;
            break;
        }
    }

    libc::sprintf(
        topic.as_mut_ptr(),
        b"%s Prize\0".as_ptr() as *const i8,
        eventname.as_ptr(),
    );

    let mut sent_parcel_success: i32 = 0;

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
                    .as_ptr() as *const i8,
                eventname.as_ptr(), (*sd).status.name.as_ptr(),
                reward1amount, itemdb_name(reward1item as u32),
                reward2amount, itemdb_name(reward2item as u32),
            );
            libc::sprintf(
                msg.as_mut_ptr(),
                b"Congratulations on winning the event, %s! Please visit your post office to collect your winnings.\0"
                    .as_ptr() as *const i8,
                (*sd).status.name.as_ptr(),
            );
            nmail_sendmail(sd, (*sd).status.name.as_ptr(), topic.as_ptr(), message.as_ptr());
        } else {
            libc::sprintf(
                message.as_mut_ptr(),
                b"Thanks for participating in the %s Event, %s.\n\nRank:%s Place\n\n\
You have been rewarded: (%i) %s, (%i) %s.\n\nPlease continue to play for more great rewards!\0"
                    .as_ptr() as *const i8,
                eventname.as_ptr(), (*sd).status.name.as_ptr(), rankname.as_ptr(),
                reward1amount, itemdb_name(reward1item as u32),
                reward2amount, itemdb_name(reward2item as u32),
            );
            libc::sprintf(
                msg.as_mut_ptr(),
                b"Thanks for participating in the Event, %s! Please visit your post office to collect your winnings.\0"
                    .as_ptr() as *const i8,
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
                    .as_ptr() as *const i8,
                eventname.as_ptr(), (*sd).status.name.as_ptr(),
                reward1amount, itemdb_name(reward1item as u32),
            );
            libc::sprintf(
                msg.as_mut_ptr(),
                b"Congratulations on winning the event, %s! Please visit your post office to collect your winnings.\0"
                    .as_ptr() as *const i8,
                (*sd).status.name.as_ptr(),
            );
            nmail_sendmail(sd, (*sd).status.name.as_ptr(), topic.as_ptr(), message.as_ptr());
        } else {
            libc::sprintf(
                message.as_mut_ptr(),
                b"Thanks for participating in the %s Event, %s.\n\nRank:%s Place\n\n\
You have been rewarded: (%i) %s.\n\nPlease continue to play for more great rewards!\0"
                    .as_ptr() as *const i8,
                eventname.as_ptr(), (*sd).status.name.as_ptr(), rankname.as_ptr(),
                reward1amount, itemdb_name(reward1item as u32),
            );
            libc::sprintf(
                msg.as_mut_ptr(),
                b"Thanks for participating in the event, %s. Please visit your post office to collect your winnings.\0"
                    .as_ptr() as *const i8,
                (*sd).status.name.as_ptr(),
            );
            nmail_sendmail(sd, (*sd).status.name.as_ptr(), topic.as_ptr(), message.as_ptr());
        }
    }

    if sent_parcel_success == 0 {
        libc::sprintf(
            msg.as_mut_ptr(),
            b"Sorry %s, there was an error encountered while attempting to send your rewards in a parcel. Please contact a GM for assistance.\0"
                .as_ptr() as *const i8,
            (*sd).status.name.as_ptr(),
        );
    }

    clif_sendmsg(sd, 0, msg.as_ptr());

    if sent_parcel_success >= 1 {
        // EventId is int(10) signed — i32 bind is correct
        let eventid_i32 = eventid;
        // ChaId is int(10) signed — bind as i32; status.id is u32 so cast
        let cha_id_i32  = (*sd).status.id as i32;
        let _ = blocking_run(async move {
            sqlx::query(
                "UPDATE `RankingScores` SET `EventClaim` = 2 WHERE `EventId` = ? AND `ChaId` = ?"
            )
            .bind(eventid_i32)
            .bind(cha_id_i32)
            .execute(get_pool())
            .await
        });
    }

    clif_parseranking(sd, fd);
    0
}

// ─── clif_sendRewardInfo ──────────────────────────────────────────────────────

/// Build and send the reward-info packet (0x7D / subtype 0x05) for an event.
///
/// Iterates `rewardranks` times, writing per-rank legend title, icon, and
/// item reward information into the WFIFO.  C line 4561.
pub unsafe fn clif_sendRewardInfo(sd: *mut MapSessionData, fd: i32) -> i32 {
    let g_cur_year = cur_year.load(AtomicOrd::Relaxed);
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
        b"Moon %i\0".as_ptr() as *const i8,
        g_cur_year,
    );

    let eventid = rfifob(fd, 7) as u32;

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

    let Some(rr) = blocking_run(async move {
        sqlx::query_as::<_, RankingEventRow>(
            "SELECT `EventRewardRanks_Display`, `EventLegend`, \
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
             FROM `RankingEvents` WHERE `EventId` = ?"
        )
        .bind(eventid as u32)
        .fetch_optional(get_pool())
        .await
        .ok()
        .flatten()
    }) else { return 0; };

    // Copy fields from row into local variables
    rewardranks       = rr.reward_ranks;
    {
        let leg_bytes = rr.event_legend.as_bytes();
        let copy_len = leg_bytes.len().min(legend.len() - 1);
        std::ptr::copy_nonoverlapping(leg_bytes.as_ptr() as *const i8, legend.as_mut_ptr(), copy_len);
        legend[copy_len] = 0;
    }
    legendicon1       = rr.icon1;
    legendicon1color  = rr.icon1_color;
    legendicon2       = rr.icon2;
    legendicon2color  = rr.icon2_color;
    legendicon3       = rr.icon3;
    legendicon3color  = rr.icon3_color;
    legendicon4       = rr.icon4;
    legendicon4color  = rr.icon4_color;
    legendicon5       = rr.icon5;
    legendicon5color  = rr.icon5_color;
    _1stPlaceReward1_ItmId  = rr.r1_itm1 as i32;
    _1stPlaceReward1_Amount = rr.r1_amt1 as i32;
    _1stPlaceReward2_ItmId  = rr.r1_itm2 as i32;
    _1stPlaceReward2_Amount = rr.r1_amt2 as i32;
    _2ndPlaceReward1_ItmId  = rr.r2_itm1 as i32;
    _2ndPlaceReward1_Amount = rr.r2_amt1 as i32;
    _2ndPlaceReward2_ItmId  = rr.r2_itm2 as i32;
    _2ndPlaceReward2_Amount = rr.r2_amt2 as i32;
    _3rdPlaceReward1_ItmId  = rr.r3_itm1 as i32;
    _3rdPlaceReward1_Amount = rr.r3_amt1 as i32;
    _3rdPlaceReward2_ItmId  = rr.r3_itm2 as i32;
    _3rdPlaceReward2_Amount = rr.r3_amt2 as i32;
    _4thPlaceReward1_ItmId  = rr.r4_itm1 as i32;
    _4thPlaceReward1_Amount = rr.r4_amt1 as i32;
    _4thPlaceReward2_ItmId  = rr.r4_itm2 as i32;
    _4thPlaceReward2_Amount = rr.r4_amt2 as i32;
    _5thPlaceReward1_ItmId  = rr.r5_itm1 as i32;
    _5thPlaceReward1_Amount = rr.r5_amt1 as i32;
    _5thPlaceReward2_ItmId  = rr.r5_itm2 as i32;
    _5thPlaceReward2_Amount = rr.r5_amt2 as i32;

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
        let rank_num = (i as i32) + 1;

        wfifob(fd, pos,     rank); // Rank 1st #
        wfifob(fd, pos + 1, 1);   // squigley
        wfifob(fd, pos + 2, rank); // Rank #

        pos += 3;

        let mut legendicon = 0i32;
        let mut legendiconcolor = 0i32;

        match rank_num {
            1 => {
                libc::sprintf(buf.as_mut_ptr(), b"%s [%ist] %s\0".as_ptr() as *const i8, legend.as_ptr(), rank_num, monthyear.as_ptr());
                legendicon = legendicon1; legendiconcolor = legendicon1color;
                rewarditm = _1stPlaceReward1_ItmId; rewardamount = _1stPlaceReward1_Amount;
                reward2itm = _1stPlaceReward2_ItmId; reward2amount = _1stPlaceReward2_Amount;
            }
            2 => {
                libc::sprintf(buf.as_mut_ptr(), b"%s [%ind] %s\0".as_ptr() as *const i8, legend.as_ptr(), rank_num, monthyear.as_ptr());
                legendicon = legendicon2; legendiconcolor = legendicon2color;
                rewarditm = _2ndPlaceReward1_ItmId; rewardamount = _2ndPlaceReward1_Amount;
                reward2itm = _2ndPlaceReward2_ItmId; reward2amount = _2ndPlaceReward2_Amount;
            }
            3 => {
                libc::sprintf(buf.as_mut_ptr(), b"%s [%ird] %s\0".as_ptr() as *const i8, legend.as_ptr(), rank_num, monthyear.as_ptr());
                legendicon = legendicon3; legendiconcolor = legendicon3color;
                rewarditm = _3rdPlaceReward1_ItmId; rewardamount = _3rdPlaceReward1_Amount;
                reward2itm = _3rdPlaceReward2_ItmId; reward2amount = _3rdPlaceReward2_Amount;
            }
            4 => {
                libc::sprintf(buf.as_mut_ptr(), b"%s [%ith] %s\0".as_ptr() as *const i8, legend.as_ptr(), rank_num, monthyear.as_ptr());
                legendicon = legendicon4; legendiconcolor = legendicon4color;
                rewarditm = _4thPlaceReward1_ItmId; rewardamount = _4thPlaceReward1_Amount;
                reward2itm = _4thPlaceReward2_ItmId; reward2amount = _4thPlaceReward2_Amount;
            }
            _ => {
                libc::sprintf(buf.as_mut_ptr(), b"%s [%ith] %s\0".as_ptr() as *const i8, legend.as_ptr(), rank_num, monthyear.as_ptr());
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
        libc::sprintf(buf.as_mut_ptr(), b"%s\0".as_ptr() as *const i8, itemdb_name(rewarditm as u32));
        let buf_len = libc::strlen(buf.as_ptr());
        wfifob(fd, pos, buf_len as u8);
        pos += 1;
        wfifop_copy(fd, pos, buf.as_ptr() as *const u8, buf_len);
        pos += buf_len;
        pos += 3; // padding

        clif_intcheck(rewardamount, pos as i32, fd);
        pos += 2;
        clif_intcheck(itemdb_icon(rewarditm as u32) - 49152, pos as i32, fd);
        pos += 1;
        wfifob(fd, pos, itemdb_iconcolor(rewarditm as u32) as u8);
        pos += 1;

        if reward2amount == 0 {
            wfifob(fd, pos, 1);
            pos += 1;
            continue;
        }

        // Reward 2 name
        libc::sprintf(buf.as_mut_ptr(), b"%s\0".as_ptr() as *const i8, itemdb_name(reward2itm as u32));
        let buf_len = libc::strlen(buf.as_ptr());
        wfifob(fd, pos, buf_len as u8);
        pos += 1;
        wfifop_copy(fd, pos, buf.as_ptr() as *const u8, buf_len);
        pos += buf_len;
        pos += 3;

        clif_intcheck(reward2amount, pos as i32, fd);
        pos += 2;
        clif_intcheck(itemdb_icon(reward2itm as u32) - 49152, pos as i32, fd);
        pos += 1;
        wfifob(fd, pos, itemdb_iconcolor(reward2itm as u32) as u8);
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
pub unsafe fn retrieveEventDates(eventid: i32, pos: i32, fd: i32) {
    let event_id_u = eventid as u32;
    let Some(dates) = blocking_run(async move {
        sqlx::query_as::<_, EventDates>(
            "SELECT `FromDate`, `FromTime`, `ToDate`, `ToTime` FROM `RankingEvents` WHERE `EventId` = ?"
        )
        .bind(event_id_u)
        .fetch_optional(get_pool())
        .await
        .ok()
        .flatten()
    }) else { return; };

    clif_intcheck(dates.from_date, pos + 7,  fd);
    clif_intcheck(dates.from_time, pos + 11, fd);
    clif_intcheck(dates.to_date,   pos + 15, fd);
    clif_intcheck(dates.to_time,   pos + 19, fd);
}

// ─── checkPlayerScore ─────────────────────────────────────────────────────────

/// Return the player's score for `eventid`, or 0 if not found / on error.
///
/// C line 4951.
pub unsafe fn checkPlayerScore(eventid: i32, sd: *mut MapSessionData) -> i32 {
    // EventId is int(10) signed — i32 bind is correct
    let event_id_i = eventid;
    // ChaId is int(10) signed — bind as i32; status.id is u32 so cast
    let cha_id = (*sd).status.id as i32;
    blocking_run(async move {
        sqlx::query_scalar::<_, i32>(
            "SELECT `Score` FROM `RankingScores` WHERE `EventId` = ? AND `ChaId` = ?"
        )
        .bind(event_id_i)
        .bind(cha_id)
        .fetch_optional(get_pool())
        .await
        .ok()
        .flatten()
        .unwrap_or(0)
    })
}

// ─── updateRanks ──────────────────────────────────────────────────────────────

/// Re-rank all scores for `eventid` using a MySQL user-variable counter.
///
/// Issues `SET @r=0` then `UPDATE … SET Rank = @r := (@r+1) ORDER BY Score DESC`.
/// C line 4983.
pub unsafe fn updateRanks(eventid: i32) {
    // EventId is int(10) signed — i32 bind is correct
    // The vestigial SELECT is dropped; just run the rank-update pair.
    blocking_run(async move {
        let pool = get_pool();
        let _ = sqlx::query("SET @r=0").execute(pool).await;
        let _ = sqlx::query(
            "UPDATE `RankingScores` SET `Rank` = @r := (@r+1) WHERE `EventId` = ? ORDER BY `Score` DESC"
        )
        .bind(eventid)
        .execute(pool)
        .await;
    });
}

// ─── checkPlayerRank ──────────────────────────────────────────────────────────

/// Return the player's current rank for `eventid`, or 0 if not found / on error.
///
/// C line 5018.
pub unsafe fn checkPlayerRank(eventid: i32, sd: *mut MapSessionData) -> i32 {
    // EventId is int(10) signed — i32 bind is correct
    let event_id_i = eventid;
    // ChaId is int(10) signed — bind as i32; status.id is u32 so cast
    let cha_id = (*sd).status.id as i32;
    blocking_run(async move {
        sqlx::query_scalar::<_, i32>(
            "SELECT `Rank` FROM `RankingScores` WHERE `EventId` = ? AND `ChaId` = ?"
        )
        .bind(event_id_i)
        .bind(cha_id)
        .fetch_optional(get_pool())
        .await
        .ok()
        .flatten()
        .unwrap_or(0)
    })
}

// ─── checkevent_claim ─────────────────────────────────────────────────────────

/// Return the `EventClaim` value for a player/event pair.
/// Returns the column value on success, or 2 if no row is found.
///
/// SQL: SELECT EventClaim FROM RankingScores WHERE EventId=? AND ChaId=?
pub unsafe fn checkevent_claim(eventid: i32, _fd: i32, sd: *mut MapSessionData) -> i32 {
    // ChaId is int(10) signed — bind as i32; status.id is u32 so cast
    let cha_id = (*sd).status.id as i32;
    // EventId is int(10) signed — i32 bind is correct
    let event_id = eventid;

    blocking_run(async move {
        sqlx::query_scalar::<_, i32>(
            "SELECT `EventClaim` FROM `RankingScores` WHERE `EventId` = ? AND `ChaId` = ?"
        )
        .bind(event_id)
        .bind(cha_id)
        .fetch_optional(get_pool())
        .await
        .ok()
        .flatten()
        .unwrap_or(2) // 2 = "not found / not claimed"
    })
}

// ─── dateevent_block ──────────────────────────────────────────────────────────

/// Write a date-event block into the WFIFO at position `pos`.
///
/// Writes the event header bytes and delegates date fields to `retrieveEventDates`,
/// then appends the claim byte at pos+20.
///
pub unsafe fn dateevent_block(pos: i32, eventid: i32, fd: i32, sd: *mut MapSessionData) {
    wfifob(fd, pos as usize,       0);
    wfifob(fd, pos as usize + 1,   eventid as u8);
    wfifob(fd, pos as usize + 2,   142);
    wfifob(fd, pos as usize + 3,   227);
    retrieveEventDates(eventid, pos, fd);
    wfifob(fd, pos as usize + 20,  checkevent_claim(eventid, fd, sd) as u8);
}

// ─── filler_block ─────────────────────────────────────────────────────────────

/// Write the "filler" event block into the WFIFO at position `pos`.
///
/// Writes player rank / score / claim bytes for the chosen event.
pub unsafe fn filler_block(pos: i32, eventid: i32, fd: i32, sd: *mut MapSessionData) {
    let player_score = checkPlayerScore(eventid, sd);
    let player_rank  = checkPlayerRank(eventid, sd);

    wfifob(fd, pos as usize + 1,  rfifob(fd, 7));
    wfifob(fd, pos as usize + 2,  142);
    wfifob(fd, pos as usize + 3,  227);
    wfifob(fd, pos as usize + 4,  1);
    clif_intcheck(player_rank,  pos + 8,  fd);
    clif_intcheck(player_score, pos + 12, fd);
    wfifob(fd, pos as usize + 13, checkevent_claim(eventid, fd, sd) as u8);
}

// ─── gettotalscores ───────────────────────────────────────────────────────────

/// Return the number of score rows for `eventid` in `RankingScores`.
///
/// SQL: SELECT COUNT(*) FROM RankingScores WHERE EventId=?
pub unsafe fn gettotalscores(eventid: i32) -> i32 {
    let event_id = eventid as u32;
    blocking_run(async move {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM `RankingScores` WHERE `EventId` = ?"
        )
        .bind(event_id)
        .fetch_one(get_pool())
        .await
        .unwrap_or(0) as i32
    })
}

// ─── getevents ────────────────────────────────────────────────────────────────

/// Return the number of rows in `RankingEvents`.
///
/// SQL: SELECT COUNT(*) FROM RankingEvents
pub unsafe fn getevents() -> i32 {
    blocking_run(async {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM `RankingEvents`"
        )
        .fetch_one(get_pool())
        .await
        .unwrap_or(0) as i32
    })
}

// ─── getevent_name ────────────────────────────────────────────────────────────

/// Query all event names from `RankingEvents`, write a `dateevent_block` for each,
/// then write the name length byte + name bytes into WFIFO.
///
/// Returns updated `pos` after writing all blocks.
/// SQL: SELECT EventName FROM RankingEvents
pub unsafe fn getevent_name(mut pos: i32, fd: i32, sd: *mut MapSessionData) -> i32 {
    // EventId is int(10) signed — i32 bind is correct
    struct EventRow { event_id: i32, name: String }
    impl sqlx::FromRow<'_, sqlx::mysql::MySqlRow> for EventRow {
        fn from_row(row: &sqlx::mysql::MySqlRow) -> sqlx::Result<Self> {
            use sqlx::Row;
            Ok(EventRow {
                event_id: row.try_get(0).unwrap_or(0),
                name:     row.try_get(1).unwrap_or_default(),
            })
        }
    }

    let rows: Vec<EventRow> = blocking_run(async {
        sqlx::query_as::<_, EventRow>("SELECT `EventId`, `EventName` FROM `RankingEvents`")
            .fetch_all(get_pool())
            .await
            .unwrap_or_default()
    });

    for row in rows.iter() {
        dateevent_block(pos, row.event_id as i32, fd, sd);
        pos += 21;
        let name_bytes = row.name.as_bytes();
        let name_len   = name_bytes.len();
        wfifob(fd, pos as usize, name_len as u8);
        pos += 1;
        wfifop_copy(fd, pos as usize, name_bytes.as_ptr(), name_len);
        pos += name_len as i32;
    }

    pos
}

// ─── getevent_playerscores ────────────────────────────────────────────────────

/// Query the top-10 player scores for `eventid` (with optional offset) and write
/// them into the WFIFO. Adjusts the row-count byte when fewer than 10 rows exist.
///
/// SQL: SELECT ChaName, Score, Rank FROM RankingScores WHERE EventId=? ORDER BY Rank ASC LIMIT 10 [OFFSET ?]
pub unsafe fn getevent_playerscores(
    eventid:     i32,
    totalscores: i32,
    mut pos:     i32,
    fd:          i32,
) -> i32 {
    // The C code reads an offset byte from RFIFO position 17 and subtracts 10.
    let offset: i64 = rfifob(fd, 17) as i64 - 10;

    struct ScoreRow { name: String, score: i32, rank: i32 }
    impl sqlx::FromRow<'_, sqlx::mysql::MySqlRow> for ScoreRow {
        fn from_row(row: &sqlx::mysql::MySqlRow) -> sqlx::Result<Self> {
            use sqlx::Row;
            Ok(ScoreRow {
                name:  row.try_get(0).unwrap_or_default(),
                score: row.try_get(1).unwrap_or(0),
                rank:  row.try_get(2).unwrap_or(0),
            })
        }
    }

    let event_id = eventid as u32;
    let rows: Vec<ScoreRow> = blocking_run(async move {
        if totalscores > 10 {
            sqlx::query_as::<_, ScoreRow>(
                "SELECT `ChaName`, `Score`, `Rank` FROM `RankingScores` \
                 WHERE `EventId` = ? ORDER BY `Rank` ASC LIMIT 10 OFFSET ?"
            )
            .bind(event_id)
            .bind(offset)
            .fetch_all(get_pool())
            .await
            .unwrap_or_default()
        } else {
            sqlx::query_as::<_, ScoreRow>(
                "SELECT `ChaName`, `Score`, `Rank` FROM `RankingScores` \
                 WHERE `EventId` = ? ORDER BY `Rank` ASC LIMIT 10"
            )
            .bind(event_id)
            .fetch_all(get_pool())
            .await
            .unwrap_or_default()
        }
    });

    // If fewer than 10 rows, patch the count byte written just before `pos`
    if (rows.len() as i32) < 10 {
        wfifob(fd, (pos - 1) as usize, rows.len() as u8);
    }

    for row in &rows {
        let name_bytes = row.name.as_bytes();
        let name_len   = name_bytes.len();
        wfifob(fd, pos as usize, name_len as u8);
        pos += 1;
        wfifop_copy(fd, pos as usize, name_bytes.as_ptr(), name_len);
        pos += name_len as i32;
        pos += 3; // 3 padding bytes (matches C)
        wfifob(fd, pos as usize, row.rank as u8);
        pos += 4; // rank byte + 3 more padding bytes
        clif_intcheck(row.score, pos, fd);
        pos += 1;
    }

    pos
}

// ─── clif_parseranking ────────────────────────────────────────────────────────

/// Build and send the ranking packet (0xAA/0x02) to the client.
///
/// Assembles: event count, event name/date blocks, filler block, score list,
/// total score count, encrypts and sends.
///
pub unsafe fn clif_parseranking(sd: *mut MapSessionData, fd: i32) -> i32 {
    wfifohead(fd, 0);
    wfifob(fd, 0, 0xAA);
    wfifob(fd, 1, 0x02);
    wfifob(fd, 3, 0x7D);
    wfifob(fd, 5, 0x03);
    wfifob(fd, 6, 0);

    // Zero out bytes 8..1500
    for i in 8..1500usize {
        wfifob(fd, i, 0);
    }

    wfifob(fd, 7, getevents() as u8);
    let chosen_event = rfifob(fd, 7) as i32;

    updateRanks(chosen_event);

    let mut pos: i32 = 8;
    pos = getevent_name(pos, fd, sd);
    filler_block(pos, chosen_event, fd, sd);
    pos += 15;
    wfifob(fd, pos as usize, 10);
    let totalscores = gettotalscores(chosen_event);
    pos += 1;
    pos = getevent_playerscores(chosen_event, totalscores, pos, fd);
    pos += 3;
    wfifob(fd, pos as usize, totalscores as u8);
    pos += 1;

    wfifob(fd, 2, (pos - 3) as u8);
    wfifoset(fd, encrypt(fd) as usize);

    0
}

// ─── canusepowerboards ────────────────────────────────────────────────────────

/// Return 1 if the player is allowed to use power boards, 0 otherwise.
///
/// Allowed when: GM, or has `carnagehost` global reg set, and map id is 2001..=2099.
///
/// Pure logic — no SQL.
pub unsafe fn canusepowerboards(sd: *mut MapSessionData) -> i32 {
    if (*sd).status.gm_level != 0 { return 1; }
    if pc_readglobalreg(sd, c"carnagehost".as_ptr()) == 0 { return 0; }
    if (*sd).bl.m >= 2001 && (*sd).bl.m <= 2099 { return 1; }
    0
}
