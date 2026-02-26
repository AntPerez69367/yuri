use std::ffi::c_int;
use std::os::raw::c_void;
use mlua::{MetaMethod, UserData, UserDataMethods};

use crate::database::map_db::MapData;
use crate::ffi::map_db::get_map_ptr;
use crate::game::npc::{NpcData, npc_move, npc_warp};
use crate::servers::char::charstatus::MAX_EQUIP;

pub struct NpcObject { pub ptr: *mut c_void }
unsafe impl Send for NpcObject {}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

unsafe fn npc_map(nd: *const NpcData) -> *mut MapData {
    get_map_ptr((*nd).bl.m)
}

unsafe fn cstr_to_string(p: *const std::ffi::c_char) -> String {
    if p.is_null() { return String::new(); }
    std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
}

fn val_to_int(v: &mlua::Value) -> i32 {
    match v {
        mlua::Value::Integer(i) => *i as i32,
        mlua::Value::Number(f)  => *f as i32,
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// UserData implementation
// ---------------------------------------------------------------------------
impl UserData for NpcObject {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {

        // ── __index ─────────────────────────────────────────────────────────
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            let nd = this.ptr as *mut NpcData;
            if nd.is_null() { return Ok(mlua::Value::Nil); }

            // Named methods — return Lua functions capturing the raw pointer.
            // npc:move() desugars to npc.move(npc); the closure ignores `npc`
            // since it already captured `ptr`.
            match key.as_str() {
                "move" => {
                    let ptr = this.ptr;
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, _: mlua::MultiValue| {
                            Ok(unsafe { npc_move(ptr as *mut NpcData) })
                        }
                    )?));
                }
                "warp" => {
                    let ptr = this.ptr;
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |_, (m, x, y): (c_int, c_int, c_int)| {
                            unsafe { npc_warp(ptr as *mut NpcData, m, x, y); }
                            Ok(())
                        }
                    )?));
                }
                "getEquippedItem" => {
                    let ptr = this.ptr;
                    return Ok(mlua::Value::Function(lua.create_function(
                        move |lua, num: usize| -> mlua::Result<mlua::Value> {
                            if num >= MAX_EQUIP { return Ok(mlua::Value::Nil); }
                            let item = unsafe { &(*(ptr as *const NpcData)).equip[num] };
                            if item.id == 0 { return Ok(mlua::Value::Nil); }
                            let t = lua.create_table()?;
                            t.raw_set(1, item.id)?;
                            t.raw_set(2, item.custom)?;
                            Ok(mlua::Value::Table(t))
                        }
                    )?));
                }
                _ => {}
            }

            // ── Field getters ─────────────────────────────────────────────
            let nd = unsafe { &*nd };
            let bl = &nd.bl;

            macro_rules! int  { ($e:expr) => { Ok(mlua::Value::Integer($e as i64)) }; }
            macro_rules! str_ { ($e:expr) => {
                Ok(mlua::Value::String(lua.create_string(
                    unsafe { cstr_to_string($e) }
                )?))
            }; }
            macro_rules! map_int { ($field:ident) => {{
                let mp = unsafe { npc_map(nd as *const NpcData) };
                if mp.is_null() { return Ok(mlua::Value::Nil); }
                int!(unsafe { (*mp).$field })
            }}; }

            match key.as_str() {
                // block_list / map fields
                "x"          => int!(bl.x),
                "y"          => int!(bl.y),
                "m"          => int!(bl.m),
                "blType"     => int!(bl.bl_type),
                "ID"         => int!(bl.id),
                "xmax" => {
                    let mp = unsafe { npc_map(nd as *const NpcData) };
                    if mp.is_null() { return Ok(mlua::Value::Nil); }
                    int!(unsafe { (*mp).xs.saturating_sub(1) })
                }
                "ymax" => {
                    let mp = unsafe { npc_map(nd as *const NpcData) };
                    if mp.is_null() { return Ok(mlua::Value::Nil); }
                    int!(unsafe { (*mp).ys.saturating_sub(1) })
                }
                "mapId"      => map_int!(id),
                "mapTitle"   => {
                    let mp = unsafe { npc_map(nd as *const NpcData) };
                    if mp.is_null() { return Ok(mlua::Value::Nil); }
                    str_!(unsafe { (*mp).title.as_ptr() })
                }
                "mapFile"    => {
                    let mp = unsafe { npc_map(nd as *const NpcData) };
                    if mp.is_null() { return Ok(mlua::Value::Nil); }
                    str_!(unsafe { (*mp).mapfile.as_ptr() })
                }
                "bgm"        => map_int!(bgm),
                "bgmType"    => map_int!(bgmtype),
                "pvp"        => map_int!(pvp),
                "spell"      => map_int!(spell),
                "light"      => map_int!(light),
                "weather"    => map_int!(weather),
                "sweepTime"  => map_int!(sweeptime),
                "canTalk"    => map_int!(cantalk),
                "showGhosts" => map_int!(show_ghosts),
                "region"     => map_int!(region),
                "indoor"     => map_int!(indoor),
                "warpOut"    => map_int!(warpout),
                "bind"       => map_int!(bind),
                "reqLvl"     => map_int!(reqlvl),
                "reqVita"    => map_int!(reqvita),
                "reqMana"    => map_int!(reqmana),
                "maxLvl"     => map_int!(lvlmax),
                "maxVita"    => map_int!(vitamax),
                "maxMana"    => map_int!(manamax),
                "reqPath"    => map_int!(reqpath),
                "reqMark"    => map_int!(reqmark),
                "canSummon"  => map_int!(summon),
                "canUse"     => map_int!(can_use),
                "canEat"     => map_int!(can_eat),
                "canSmoke"   => map_int!(can_smoke),
                "canMount"   => map_int!(can_mount),
                "canGroup"   => map_int!(can_group),
                // NPC-specific fields
                "id"          => int!(nd.id),
                "look"        => int!(bl.graphic_id),
                "lookColor"   => int!(bl.graphic_color),
                "name"        => str_!(nd.name.as_ptr()),
                "yname"       => str_!(nd.npc_name.as_ptr()),
                "subType"     => int!(bl.subtype),
                "npcType"     => int!(nd.npctype),
                "side"        => int!(nd.side),
                "state"       => int!(nd.state),
                "sex"         => int!(nd.sex),
                "face"        => int!(nd.face),
                "faceColor"   => int!(nd.face_color),
                "hair"        => int!(nd.hair),
                "hairColor"   => int!(nd.hair_color),
                "skinColor"   => int!(nd.skin_color),
                "armorColor"  => int!(nd.armor_color),
                "lastAction"  => int!(nd.lastaction),
                "actionTime"  => int!(nd.actiontime),
                "duration"    => int!(nd.duration),
                "owner"       => int!(nd.owner),
                "startM"      => int!(nd.startm),
                "startX"      => int!(nd.startx),
                "startY"      => int!(nd.starty),
                "shopNPC"     => int!(nd.shop_npc),
                "repairNPC"   => int!(nd.repair_npc),
                "retDist"     => int!(nd.retdist),
                "returning"   => Ok(mlua::Value::Boolean(nd.returning != 0)),
                "bankNPC"     => int!(nd.bank_npc),
                "gfxFace"     => int!(nd.gfx.face),
                "gfxHair"     => int!(nd.gfx.hair),
                "gfxHairC"    => int!(nd.gfx.chair),
                "gfxFaceC"    => int!(nd.gfx.cface),
                "gfxSkinC"    => int!(nd.gfx.cskin),
                "gfxDye"      => int!(nd.gfx.dye),
                "gfxTitleColor" => int!(nd.gfx.title_color),
                "gfxWeap"     => int!(nd.gfx.weapon),
                "gfxWeapC"    => int!(nd.gfx.cweapon),
                "gfxArmor"    => int!(nd.gfx.armor),
                "gfxArmorC"   => int!(nd.gfx.carmor),
                "gfxShield"   => int!(nd.gfx.shield),
                "gfxShiedlC"  => int!(nd.gfx.cshield),  // note: C typo preserved
                "gfxHelm"     => int!(nd.gfx.helm),
                "gfxHelmC"    => int!(nd.gfx.chelm),
                "gfxMantle"   => int!(nd.gfx.mantle),
                "gfxMantleC"  => int!(nd.gfx.cmantle),
                "gfxCrown"    => int!(nd.gfx.crown),
                "gfxCrownC"   => int!(nd.gfx.ccrown),
                "gfxFaceA"    => int!(nd.gfx.face_acc),
                "gfxFaceAC"   => int!(nd.gfx.cface_acc),
                "gfxFaceAT"   => int!(nd.gfx.face_acc_t),
                "gfxFaceATC"  => int!(nd.gfx.cface_acc_t),
                "gfxBoots"    => int!(nd.gfx.boots),
                "gfxBootsC"   => int!(nd.gfx.cboots),
                "gfxNeck"     => int!(nd.gfx.necklace),
                "gfxNeckC"    => int!(nd.gfx.cnecklace),
                "gfxName"     => str_!(nd.gfx.name.as_ptr()),
                "gfxClone"    => int!(nd.clone),
                _ => Ok(mlua::Value::Nil),
            }
        });

        // ── __newindex ───────────────────────────────────────────────────────
        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (String, mlua::Value)| {
            let nd = this.ptr as *mut NpcData;
            if nd.is_null() { return Ok(()); }
            let nd = unsafe { &mut *nd };
            let mp = unsafe { npc_map(nd as *const NpcData) };
            let bl = &mut nd.bl;

            macro_rules! map_set { ($field:ident) => {
                if !mp.is_null() { unsafe { (*mp).$field = val_to_int(&val) as _; } }
            }; }

            match key.as_str() {
                // map writable fields
                "mapTitle" => {
                    if let mlua::Value::String(ref s) = val {
                        if !mp.is_null() {
                            let bytes = s.as_bytes();
                            let len = bytes.len().min(63);
                            unsafe {
                                std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const i8,
                                    (*mp).title.as_mut_ptr(), len);
                                (*mp).title[len] = 0;
                            }
                        }
                    }
                }
                "mapFile" => {
                    if let mlua::Value::String(ref s) = val {
                        if !mp.is_null() {
                            let bytes = s.as_bytes();
                            let len = bytes.len().min(1023);
                            unsafe {
                                std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const i8,
                                    (*mp).mapfile.as_mut_ptr(), len);
                                (*mp).mapfile[len] = 0;
                            }
                        }
                    }
                }
                "bgm"        => map_set!(bgm),
                "bgmType"    => map_set!(bgmtype),
                "pvp"        => map_set!(pvp),
                "spell"      => map_set!(spell),
                "light"      => map_set!(light),
                "weather"    => map_set!(weather),
                "sweepTime"  => map_set!(sweeptime),
                "canTalk"    => map_set!(cantalk),
                "showGhosts" => map_set!(show_ghosts),
                "region"     => map_set!(region),
                "indoor"     => map_set!(indoor),
                "warpOut"    => map_set!(warpout),
                "bind"       => map_set!(bind),
                "reqLvl"     => map_set!(reqlvl),
                "reqVita"    => map_set!(reqvita),
                "reqMana"    => map_set!(reqmana),
                "reqPath"    => map_set!(reqpath),
                "reqMark"    => map_set!(reqmark),
                "maxLvl"     => map_set!(lvlmax),
                "maxVita"    => map_set!(vitamax),
                "maxMana"    => map_set!(manamax),
                "canSummon"  => map_set!(summon),
                "canUse"     => map_set!(can_use),
                "canEat"     => map_set!(can_eat),
                "canSmoke"   => map_set!(can_smoke),
                "canMount"   => map_set!(can_mount),
                "canGroup"   => map_set!(can_group),
                // NPC-specific writable fields
                "side"        => nd.side        = val_to_int(&val) as i8,
                "subType"     => bl.subtype     = val_to_int(&val) as u8,
                "look"        => bl.graphic_id  = val_to_int(&val) as u32,
                "lookColor"   => bl.graphic_color = val_to_int(&val) as u32,
                "state"       => nd.state       = val_to_int(&val) as i8,
                "sex"         => nd.sex         = val_to_int(&val) as u16,
                "face"        => nd.face        = val_to_int(&val) as u16,
                "faceColor"   => nd.face_color  = val_to_int(&val) as u16,
                "hair"        => nd.hair        = val_to_int(&val) as u16,
                "hairColor"   => nd.hair_color  = val_to_int(&val) as u16,
                "skinColor"   => nd.skin_color  = val_to_int(&val) as u16,
                "armorColor"  => nd.armor_color = val_to_int(&val) as u16,
                "lastAction"  => nd.lastaction  = val_to_int(&val) as u32,
                "actionTime"  => nd.actiontime  = val_to_int(&val) as u32,
                "duration"    => nd.duration    = val_to_int(&val) as u32,
                "gfxFace"     => nd.gfx.face      = val_to_int(&val) as u8,
                "gfxHair"     => nd.gfx.hair      = val_to_int(&val) as u8,
                "gfxHairC"    => nd.gfx.chair     = val_to_int(&val) as u8,
                "gfxFaceC"    => nd.gfx.cface     = val_to_int(&val) as u8,
                "gfxSkinC"    => nd.gfx.cskin     = val_to_int(&val) as u8,
                "gfxDye"      => nd.gfx.dye       = val_to_int(&val) as u8,
                "gfxTitleColor" => nd.gfx.title_color = val_to_int(&val) as u8,
                "gfxWeap"     => nd.gfx.weapon    = val_to_int(&val) as u16,
                "gfxWeapC"    => nd.gfx.cweapon   = val_to_int(&val) as u8,
                "gfxArmor"    => nd.gfx.armor     = val_to_int(&val) as u16,
                "gfxArmorC"   => nd.gfx.carmor    = val_to_int(&val) as u8,
                "gfxShield"   => nd.gfx.shield    = val_to_int(&val) as u16,
                "gfxShieldC"  => nd.gfx.cshield   = val_to_int(&val) as u8,
                "gfxHelm"     => nd.gfx.helm      = val_to_int(&val) as u16,
                "gfxHelmC"    => nd.gfx.chelm     = val_to_int(&val) as u8,
                "gfxMantle"   => nd.gfx.mantle    = val_to_int(&val) as u16,
                "gfxMantleC"  => nd.gfx.cmantle   = val_to_int(&val) as u8,
                "gfxCrown"    => nd.gfx.crown     = val_to_int(&val) as u16,
                "gfxCrownC"   => nd.gfx.ccrown    = val_to_int(&val) as u8,
                "gfxFaceA"    => nd.gfx.face_acc  = val_to_int(&val) as u16,
                "gfxFaceAC"   => nd.gfx.cface_acc = val_to_int(&val) as u8,
                "gfxFaceAT"   => nd.gfx.face_acc_t  = val_to_int(&val) as u16,
                "gfxFaceATC"  => nd.gfx.cface_acc_t = val_to_int(&val) as u8,
                "gfxBoots"    => nd.gfx.boots     = val_to_int(&val) as u16,
                "gfxBootsC"   => nd.gfx.cboots    = val_to_int(&val) as u8,
                "gfxNeck"     => nd.gfx.necklace  = val_to_int(&val) as u16,
                "gfxNeckC"    => nd.gfx.cnecklace = val_to_int(&val) as u8,
                "gfxName" => {
                    if let mlua::Value::String(ref s) = val {
                        let bytes = s.as_bytes();
                        let len = bytes.len().min(33);
                        unsafe {
                            std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const i8,
                                nd.gfx.name.as_mut_ptr(), len);
                            nd.gfx.name[len] = 0;
                        }
                    }
                }
                "gfxClone"    => nd.clone = val_to_int(&val) as i8,
                _ => {}
            }
            Ok(())
        });
    }
}
