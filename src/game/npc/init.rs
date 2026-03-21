//! NPC and warp database loading.

#![allow(non_snake_case)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::RwLock;

use super::entity::{NpcData, NpcEntity, NPC_ID};
use crate::common::traits::LegacyEntity;
use super::spatial::npc_warp;
use crate::common::constants::entity::npc::{F1_NPC, NPC_START_NUM};
use crate::common::constants::entity::BL_NPC;
use crate::common::player::inventory::MAX_EQUIP;
use crate::config::Point;
use crate::database::map_db::{get_map_ptr, map_is_loaded, WarpList, BLOCK_SIZE};
use crate::database::{blocking_run_async, get_pool};
use crate::game::block::{map_addblock_id, map_delblock_id};
use crate::game::util::carray_to_str;

// ---------------------------------------------------------------------------
// warp_init — load Warps table into the map block grid
// ---------------------------------------------------------------------------

/// Loads warp data from the `Warps` table into the map grid.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn warp_init_async() -> i32 {
    let p = get_pool();

    #[derive(sqlx::FromRow)]
    struct WarpRow {
        warp_id: u32, // int(10) unsigned
        src_map: i32, // int(10) signed
        src_x: i32,   // int(10) signed
        src_y: i32,   // int(10) signed
        dst_map: i32, // int(10) signed
        dst_x: i32,   // int(10) signed
        dst_y: i32,   // int(10) signed
    }

    let rows: Vec<WarpRow> = match sqlx::query_as(
        "SELECT `WarpId` AS warp_id, `SourceMapId` AS src_map, \
         `SourceX` AS src_x, `SourceY` AS src_y, \
         `DestinationMapId` AS dst_map, `DestinationX` AS dst_x, \
         `DestinationY` AS dst_y FROM `Warps`",
    )
    .fetch_all(p)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("[warp] query error: {e}");
            return -1;
        }
    };

    let mut count = 0u32;
    for row in &rows {
        let md_src = get_map_ptr(row.src_map as u16);

        if !map_is_loaded(row.src_map as u16) || !map_is_loaded(row.dst_map as u16) {
            tracing::error!(
                "[warp] src or dst map not loaded warp_id={} src={} dst={}",
                row.warp_id,
                row.src_map,
                row.dst_map
            );
            continue;
        }

        let md = &mut *md_src;

        if row.src_x < 0
            || row.src_y < 0
            || row.src_x > md.xs as i32 - 1
            || row.src_y > md.ys as i32 - 1
        {
            tracing::error!(
                "[warp] map id: {}, x: {}, y: {}, source out of bounds",
                row.src_map,
                row.src_x,
                row.src_y
            );
            continue;
        }

        // Check destination coords too (log only, don't skip — matches C behavior)
        let md_dst = &*get_map_ptr(row.dst_map as u16);
        if row.dst_x > md_dst.xs as i32 - 1 || row.dst_y > md_dst.ys as i32 - 1 {
            tracing::error!(
                "[warp] map id: {}, x: {}, y: {}, destination out of bounds",
                row.dst_map,
                row.dst_x,
                row.dst_y
            );
        }

        let war = Box::new(WarpList {
            x: row.src_x,
            y: row.src_y,
            tm: row.dst_map,
            tx: row.dst_x,
            ty: row.dst_y,
            next: std::ptr::null_mut(),
            prev: std::ptr::null_mut(),
        });
        let war_ptr = Box::into_raw(war);

        let idx =
            (row.src_x as usize / BLOCK_SIZE) + (row.src_y as usize / BLOCK_SIZE) * md.bxs as usize;
        // SAFETY: idx is in bounds when src coords are valid (checked above).
        // If coords are out of bounds, idx can exceed bxs*bys — this is an inherited
        // C behavior (npc.c does not guard this either).

        let existing = md.warp.add(idx).read();
        (*war_ptr).next = existing;
        if !existing.is_null() {
            (*existing).prev = war_ptr;
        }
        md.warp.add(idx).write(war_ptr);

        count += 1;
    }

    tracing::info!("[npc] warps_loaded count={count}");
    0
}

/// Blocking wrapper. Must be called after the sqlx pool is initialized.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn warp_init() -> i32 {
    blocking_run_async(crate::database::assert_send(async {
        unsafe { warp_init_async().await }
    }))
}

// ---------------------------------------------------------------------------
// npc_init — load NPCs from DB into the map block grid
// ---------------------------------------------------------------------------

fn server_id() -> u32 {
    crate::config::config().server_id as u32
}

fn copy_str_to_array<const N: usize>(s: &str, dst: &mut [i8; N]) {
    let copy_len = s.len().min(N - 1);
    for (d, b) in dst.iter_mut().zip(s.bytes().take(copy_len)) {
        *d = b as i8;
    }
    dst[copy_len] = 0;
}

/// Async implementation of npc_init. Loads all NPCs from DB, allocates NpcData
/// structs, registers them in the block grid, then loads equipment for npctype==1.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub async unsafe fn npc_init_async() -> i32 {
    let p = get_pool();
    let sid = server_id();

    #[derive(sqlx::FromRow)]
    struct NpcRow {
        row_npc_id: u32,
        npc_identifier: String,
        npc_description: String,
        npc_type: u32, // SQLDT_UCHAR in C — use u32, downcast
        npc_map_id: u32,
        npc_x: u32,
        npc_y: u32,
        npc_look: u32,
        npc_look_color: u32,
        npc_timer: u32,
        npc_sex: u32,
        npc_side: u32,  // SQLDT_UCHAR — use u32, downcast
        npc_state: u32, // SQLDT_UCHAR — use u32, downcast
        npc_face: u32,
        npc_face_color: u32,
        npc_hair: u32,
        npc_hair_color: u32,
        npc_skin_color: u32,
        npc_is_char: u32, // SQLDT_UCHAR — use u32, downcast
        npc_is_f1npc: u32,
        npc_is_repair: u32,   // SQLDT_UCHAR — use u32, downcast
        npc_is_shop: u32,     // SQLDT_UCHAR — use u32, downcast
        npc_is_bank: u32,     // SQLDT_UCHAR — use u32, downcast
        npc_return_dist: u32, // SQLDT_UCHAR — use u32, downcast
        npc_move_time: u32,
        npc_can_receive: u32, // SQLDT_UCHAR — use u32, downcast
    }

    let sql = format!(
        "SELECT `NpcId` AS row_npc_id, `NpcIdentifier` AS npc_identifier, \
         `NpcDescription` AS npc_description, `NpcType` AS npc_type, \
         `NpcMapId` AS npc_map_id, `NpcX` AS npc_x, `NpcY` AS npc_y, \
         `NpcLook` AS npc_look, `NpcLookColor` AS npc_look_color, \
         `NpcTimer` AS npc_timer, `NpcSex` AS npc_sex, `NpcSide` AS npc_side, \
         `NpcState` AS npc_state, `NpcFace` AS npc_face, `NpcFaceColor` AS npc_face_color, \
         `NpcHair` AS npc_hair, `NpcHairColor` AS npc_hair_color, \
         `NpcSkinColor` AS npc_skin_color, `NpcIsChar` AS npc_is_char, \
         `NpcIsF1Npc` AS npc_is_f1npc, `NpcIsRepairNpc` AS npc_is_repair, \
         `NpcIsShopNpc` AS npc_is_shop, `NpcIsBankNpc` AS npc_is_bank, \
         `NpcReturnDistance` AS npc_return_dist, `NpcMoveTime` AS npc_move_time, \
         `NpcCanReceiveItem` AS npc_can_receive \
         FROM `NPCs{sid}` ORDER BY `NpcId`"
    );

    let rows: Vec<NpcRow> = match sqlx::query_as(&sql).fetch_all(p).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("[npc] query error: {e}");
            return -1;
        }
    };

    let count = rows.len() as u32;

    for row in &rows {
        // Check if an NPC with this DB id already exists (reload case)
        let mut nd: *mut NpcData = crate::game::map_server::map_id2npc_ref(row.row_npc_id)
            .map(|arc| &mut *arc.write() as *mut NpcData)
            .unwrap_or(std::ptr::null_mut());

        let mut is_new_alloc = false;
        if row.npc_is_f1npc == 1 {
            // This is the F1 (special) NPC — use F1_NPC id
            nd = crate::game::map_server::map_id2npc_ref(F1_NPC)
                .map(|arc| &mut *arc.write() as *mut NpcData)
                .unwrap_or(std::ptr::null_mut());
            if nd.is_null() {
                nd = Box::into_raw(Box::new(std::mem::zeroed::<NpcData>()));
                is_new_alloc = true;
            } else {
                // Reload: unlink from block grid for re-add below; stay in NPC_MAP.
                map_delblock_id((*nd).id, (*nd).m);
            }
        } else if nd.is_null() {
            // New NPC — allocate
            nd = Box::into_raw(Box::new(std::mem::zeroed::<NpcData>()));
            is_new_alloc = true;
        } else {
            // Reload: unlink from block grid for re-add below; stay in NPC_MAP.
            map_delblock_id((*nd).id, (*nd).m);
        }

        // Copy name strings (C uses memcpy with sizeof(name) = 45 into nd->name[64])
        copy_str_to_array(&row.npc_identifier, &mut (*nd).name);
        copy_str_to_array(&row.npc_description, &mut (*nd).npc_name);

        // Set block_list fields
        (*nd).bl_type = BL_NPC as u8;
        (*nd).subtype = row.npc_type as u8;
        (*nd).graphic_id = row.npc_look;
        (*nd).graphic_color = row.npc_look_color;

        // Call npc_warp only if position changed (or if newly allocated — bl fields are 0)
        let m = row.npc_map_id;
        let xc = row.npc_x;
        let yc = row.npc_y;
        if m as u16 != (*nd).startm || xc as u16 != (*nd).startx || yc as u16 != (*nd).starty {
            npc_warp(nd, m as i32, xc as i32, yc as i32);
        }

        (*nd).startm = m as u16;
        (*nd).startx = xc as u16;
        (*nd).starty = yc as u16;
        (*nd).id = row.row_npc_id;
        (*nd).actiontime = row.npc_timer;
        (*nd).sex = row.npc_sex as u16;
        (*nd).side = row.npc_side as i8;
        (*nd).state = row.npc_state as i8;
        (*nd).face = row.npc_face as u16;
        (*nd).face_color = row.npc_face_color as u16;
        (*nd).hair = row.npc_hair as u16;
        (*nd).hair_color = row.npc_hair_color as u16;
        (*nd).armor_color = 0;
        (*nd).skin_color = row.npc_skin_color as u16;
        (*nd).npctype = row.npc_is_char as i8;
        (*nd).shop_npc = row.npc_is_shop as i8;
        (*nd).repair_npc = row.npc_is_repair as i8;
        (*nd).bank_npc = row.npc_is_bank as i8;
        (*nd).retdist = row.npc_return_dist as i8;
        (*nd).movetime = row.npc_move_time;
        (*nd).receive_item = row.npc_can_receive as i8;

        // ID assignment: if bl.id < NPC_START_NUM, this is a new/fresh NPC
        if (*nd).id < NPC_START_NUM {
            (*nd).m = m as u16;
            (*nd).x = xc as u16;
            (*nd).y = yc as u16;

            if row.npc_is_f1npc == 1 {
                (*nd).id = F1_NPC;
            } else if row.row_npc_id >= 2 {
                (*nd).id = NPC_START_NUM + row.row_npc_id - 2;
                NPC_ID.store(NPC_START_NUM + row.row_npc_id - 2, Ordering::Relaxed);
            } else {
                tracing::error!(
                    "[npc] row_npc_id={} < 2, cannot compute NPC ID",
                    row.row_npc_id
                );
            }
        }

        // New NPCs: wrap in NpcEntity and insert into NPC_MAP.
        if is_new_alloc {
            let id = (*nd).id;
            let npc_data = *Box::from_raw(nd);
            let entity = Arc::new(NpcEntity {
                id,
                pos_atomic: AtomicU64::new(Point::new(npc_data.m, npc_data.x, npc_data.y).to_u64()),
                name: carray_to_str(&npc_data.name).to_owned(),
                npc_name: carray_to_str(&npc_data.npc_name).to_owned(),
                legacy: RwLock::new(npc_data),
            });
            crate::game::map_server::map_addiddb_npc(id, entity);
            // nd is dangling after from_raw; get the live pointer from the Arc.
            nd = crate::game::map_server::map_id2npc_ref(id)
                .expect("npc just inserted")
                .legacy
                .data_ptr();
        }

        // Add to block grid only if subtype < 3 (using live pointer)
        if (*nd).subtype < 3 {
            map_addblock_id((*nd).id, (*nd).bl_type, (*nd).m, (*nd).x, (*nd).y);
        }
    }

    // Equipment loading: loop from NPC_START_NUM to NPC_ID
    // For each NPC with npctype == 1, load equipment from NPCEquipment table
    let mut x = NPC_START_NUM;
    let npc_hi = NPC_ID.load(Ordering::Relaxed);
    while x <= npc_hi {
        let nd: *mut NpcData = crate::game::map_server::map_id2npc_ref(x)
            .map(|arc| &mut *arc.write() as *mut NpcData)
            .unwrap_or(std::ptr::null_mut());
        if !nd.is_null() && (*nd).npctype == 1 {
            let nd_id = (*nd).id;

            #[derive(sqlx::FromRow)]
            struct EquipRow {
                neq_look: u32,
                neq_color: u32,
                neq_slot: u32, // SQLDT_UCHAR in C — use u32, downcast
            }

            let equip_sql = format!(
                "SELECT `NeqLook` AS neq_look, `NeqColor` AS neq_color, \
                 `NeqSlot` AS neq_slot \
                 FROM `NPCEquipment{sid}` WHERE `NeqNpcId` = {nd_id} LIMIT 14"
            );

            let equip_rows: Vec<EquipRow> = match sqlx::query_as(&equip_sql).fetch_all(p).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!("[npc] equipment query error for NPC_ID={nd_id}: {e}");
                    x += 1;
                    continue;
                }
            };

            for erow in &equip_rows {
                let pos = erow.neq_slot as usize;
                if pos < MAX_EQUIP {
                    // C: memcpy(&nd->equip[(int)pos], &item, sizeof(item))
                    // item.id = NeqLook, item.custom = NeqColor; all others zeroed
                    // (the SELECT uses '' and literal '1','0','0' for other fields
                    //  but the binding assigns: real_name='', id=NeqLook, amount=1,
                    //  dura=0, owner=0, custom=NeqColor, pos=NeqSlot)
                    // We zero the slot first then set the relevant fields
                    (*nd).equip[pos] = std::mem::zeroed();
                    (*nd).equip[pos].id = erow.neq_look;
                    (*nd).equip[pos].custom = erow.neq_color;
                    (*nd).equip[pos].amount = 1;
                    (*nd).equip[pos].pos = pos as u8; // C copies NeqSlot → item.pos via memcpy
                }
            }
        }
        x += 1;
    }

    tracing::info!("[npc] read done count={count}");
    0
}

/// Blocking wrapper. Must be called after the sqlx pool is initialized.
/// # Safety
///
/// Caller must ensure all pointer arguments are valid and non-null.
pub unsafe fn npc_init() -> i32 {
    blocking_run_async(crate::database::assert_send(async {
        unsafe { npc_init_async().await }
    }))
}
