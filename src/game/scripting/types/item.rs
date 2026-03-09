use std::ffi::CStr;
use mlua::{MetaMethod, UserData, UserDataMethods};

use crate::database::item_db::ItemData;
use crate::database::recipe_db::RecipeData;

// ---------------------------------------------------------------------------
// repr(C) structs mirroring C game structs
// ---------------------------------------------------------------------------

/// Live item instance. 880 bytes.
#[repr(C)]
pub struct BoundItem {
    pub id: u32,
    pub owner: u32,
    pub custom: u32,
    pub time: u32,
    pub dura: i32,
    pub amount: i32,
    pub pos: u8,
    pub _pad: [u8; 3],
    pub custom_look: u32,
    pub custom_icon: u32,
    pub custom_look_color: u32,
    pub custom_icon_color: u32,
    pub protected: u32,
    pub traps_table: [u32; 100],
    pub buytext: [u8; 64],
    pub note: [i8; 300],
    pub repair: i8,
    pub real_name: [i8; 64],
}

#[repr(C)]
pub struct BankData {
    pub item_id: u32,
    pub amount: u32,
    pub owner: u32,
    pub time: u32,
    pub custom_icon: u32,
    pub custom_look: u32,
    pub real_name: [i8; 64],
    pub custom_look_color: u32,
    pub custom_icon_color: u32,
    pub protected: u32,
    pub note: [i8; 300],
}

#[repr(C)]
pub struct Parcel {
    pub sender: u32,
    pub pos: i32,
    pub npcflag: i32,
    pub data: BoundItem,
}

// ---------------------------------------------------------------------------
// Lua object wrappers
// ---------------------------------------------------------------------------

pub struct ItemObject     { pub ptr: *mut std::ffi::c_void }
pub struct BItemObject    { pub ptr: *mut std::ffi::c_void }
pub struct BankItemObject { pub ptr: *mut std::ffi::c_void }
pub struct ParcelObject   { pub ptr: *mut std::ffi::c_void }
pub struct RecipeObject   { pub ptr: *mut std::ffi::c_void }

// SAFETY: Each wrapper holds a raw C pointer that is managed entirely by the
// game engine's C layer.  Sending the wrapper across threads is sound only when
// ALL of the following invariants hold:
//
//  1. **Lifetime** – The pointee (game struct) must remain allocated and valid
//     for the entire lifetime of any thread that holds the wrapper.  The C API
//     guarantees this for the duration of a script callback; callers must not
//     cache these objects beyond the callback's scope.
//
//  2. **No concurrent mutation** – Only one thread may access the pointee at a
//     time.  The engine serialises script execution on a single thread; if that
//     invariant is ever broken, data races on the fields will cause UB.
//
//  3. **No `Sync` implied** – `Send` is implemented manually because mlua may
//     move Lua values between threads.  These types are deliberately *not*
//     `Sync`; taking a shared reference on two threads simultaneously is
//     unsound.
//
// Violation of any invariant leads to data races or use-after-free.  The FFI
// layer (script dispatch in `async_coro.rs`) is responsible for upholding them.
unsafe impl Send for ItemObject {}
unsafe impl Send for BItemObject {}
unsafe impl Send for BankItemObject {}
unsafe impl Send for ParcelObject {}
unsafe impl Send for RecipeObject {}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn val_to_int(v: &mlua::Value) -> i32 {
    match v {
        mlua::Value::Integer(i) => (*i).clamp(i32::MIN as i64, i32::MAX as i64) as i32,
        mlua::Value::Number(f)  => {
            if f.is_nan() { return 0; }
            f.clamp(i32::MIN as f64, i32::MAX as f64) as i32
        }
        _ => 0,
    }
}

fn val_to_uint(v: &mlua::Value) -> u32 {
    match v {
        mlua::Value::Integer(i) => (*i).clamp(0, u32::MAX as i64) as u32,
        mlua::Value::Number(f)  => {
            if f.is_nan() || *f < 0.0 { return 0; }
            f.clamp(0.0, u32::MAX as f64) as u32
        }
        _ => 0,
    }
}

/// Safely converts a fixed-length C char array into a Rust `String`.
///
/// Performs a bounded NUL search within `arr`. If a NUL is found the bytes
/// before it are decoded; if the buffer has no NUL terminator the entire
/// slice is decoded lossily. Neither path reads beyond `arr.len()`.
pub fn fixed_str(arr: &[i8]) -> String {
    let nul_pos = arr.iter().position(|&c| c == 0);
    let len = nul_pos.unwrap_or(arr.len());
    let bytes: Vec<u8> = arr[..len].iter().map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

pub fn write_str_field(arr: &mut [i8], s: &mlua::String) {
    if arr.is_empty() { return; }
    let bytes = s.as_bytes();
    let len = bytes.len().min(arr.len() - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const i8, arr.as_mut_ptr(), len);
        arr[len] = 0;
    }
}

/// Shared getattr for an ItemData record — used by all item type fallbacks.
pub unsafe fn item_data_getattr(
    lua: &mlua::Lua,
    d: *const ItemData,
    key: &str,
) -> mlua::Result<mlua::Value> {
    if d.is_null() { return Ok(mlua::Value::Nil); }
    let d = unsafe { &*d };
    macro_rules! int   { ($e:expr) => { Ok(mlua::Value::Integer($e as i64)) }; }
    macro_rules! bool_ { ($e:expr) => { Ok(mlua::Value::Boolean($e != 0)) }; }
    macro_rules! cstr  { ($arr:expr) => {{
        let s = fixed_str($arr);
        Ok(mlua::Value::String(lua.create_string(s)?))
    }}; }
    match key {
        "vita"         => int!(d.vita),
        "mana"         => int!(d.mana),
        "dam"          => int!(d.dam),
        "price"        => int!(d.price),
        "sell"         => int!(d.sell),
        "name"         => cstr!(&d.name),
        "yname"        => cstr!(&d.yname),
        "armor" | "ac" => int!(d.ac),
        "icon"         => int!(d.icon),
        "iconC"        => int!(d.icon_color),
        "look"         => int!(d.look),
        "lookC"        => int!(d.look_color),
        "id"           => int!(d.id),
        "amount"       => int!(d.amount),
        "stackAmount"  => int!(d.stack_amount),
        "maxDura"      => int!(d.dura),
        "type"         => int!(d.typ),
        "depositable"  => bool_!(d.depositable),
        "exchangeable" => bool_!(d.exchangeable),
        "droppable"    => bool_!(d.droppable),
        "sound"        => int!(d.sound),
        "minSDmg"      => int!(d.min_sdam),
        "maxSDmg"      => int!(d.max_sdam),
        "minLDmg"      => int!(d.min_ldam),
        "maxLDmg"      => int!(d.max_ldam),
        "wisdom"       => int!(d.wisdom),
        "thrown"       => bool_!(d.thrown),
        "con"          => int!(d.con),
        "level"        => int!(d.level),
        "might"        => int!(d.might),
        "grace"        => int!(d.grace),
        "will"         => int!(d.will),
        "sex"          => int!(d.sex),
        "hit"          => int!(d.hit),
        "maxAmount"    => int!(d.max_amount),
        "healing"      => int!(d.healing),
        "ethereal"     => bool_!(d.ethereal),
        "soundHit"     => int!(d.sound_hit),
        "class"        => int!(d.class),
        "time"         => int!(d.time),
        "skinnable"    => int!(d.skinnable),
        "BoD"          => int!(d.bod),
        "repairable"   => int!(d.repairable),
        "protection"   => int!(d.protection),
        "reqMight"     => int!(d.mightreq),
        "rank" => {
            let path = unsafe { crate::database::class_db::rust_classdb_path(d.class as i32) };
            let ptr = unsafe { crate::database::class_db::rust_classdb_name(path, d.rank) };
            let s = classdb_name_to_string(ptr);
            Ok(mlua::Value::String(lua.create_string(s)?))
        }
        "baseClass" => {
            int!(unsafe { crate::database::class_db::rust_classdb_path(d.class as i32) })
        }
        "className" => {
            let ptr = unsafe { crate::database::class_db::rust_classdb_name(d.class as i32, d.rank) };
            let s = classdb_name_to_string(ptr);
            Ok(mlua::Value::String(lua.create_string(s)?))
        }
        _ => Ok(mlua::Value::Nil),
    }
}

fn classdb_name_to_string(ptr: *mut i8) -> String {
    if ptr.is_null() { return String::new(); }
    let s = unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() };
    unsafe { crate::database::class_db::rust_classdb_free_name(ptr); }
    s
}

// ---------------------------------------------------------------------------
// ItemObject — item DB entry (read-only, constructed by id or name lookup)
// ---------------------------------------------------------------------------
impl UserData for ItemObject {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            unsafe { item_data_getattr(lua, this.ptr as *const ItemData, &key) }
        });
    }
}

// ---------------------------------------------------------------------------
// BItemObject — bound item in player inventory (read/write)
// ---------------------------------------------------------------------------
impl UserData for BItemObject {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            if this.ptr.is_null() { return Ok(mlua::Value::Nil); }
            let bi = unsafe { &*(this.ptr as *const BoundItem) };
            macro_rules! int  { ($e:expr) => { Ok(mlua::Value::Integer($e as i64)) }; }
            macro_rules! cstr { ($arr:expr) => {{
                let s = fixed_str($arr);
                Ok(mlua::Value::String(lua.create_string(s)?))
            }}; }
            match key.as_str() {
                "amount"          => int!(bi.amount),
                "dura"            => int!(bi.dura),
                "protected"       => int!(bi.protected),
                "owner"           => int!(bi.owner),
                "realName"        => cstr!(&bi.real_name),
                "time"            => int!(bi.time),
                "repairCheck"     => int!(bi.repair),
                "custom"          => int!(bi.custom),
                "customLook"      => int!(bi.custom_look),
                "customLookColor" => int!(bi.custom_look_color),
                "customIcon"      => int!(bi.custom_icon),
                "customIconColor" => int!(bi.custom_icon_color),
                "note"            => cstr!(&bi.note),
                _ => {
                    let db = unsafe { crate::database::item_db::rust_itemdb_search(bi.id) };
                    unsafe { item_data_getattr(lua, db, &key) }
                }
            }
        });

        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (String, mlua::Value)| {
            if this.ptr.is_null() { return Ok(()); }
            let bi = unsafe { &mut *(this.ptr as *mut BoundItem) };
            match key.as_str() {
                "id"              => bi.id              = val_to_uint(&val),
                "amount"          => bi.amount          = val_to_int(&val) as i32,
                "dura"            => bi.dura            = val_to_int(&val),
                "protected"       => bi.protected       = val_to_uint(&val),
                "owner"           => bi.owner           = val_to_uint(&val),
                "time"            => bi.time            = val_to_uint(&val),
                "repairCheck"     => { let v = val_to_int(&val).clamp(i8::MIN as i32, i8::MAX as i32); bi.repair = v as i8; }
                "custom"          => bi.custom          = val_to_uint(&val),
                "customLook"      => bi.custom_look     = val_to_uint(&val),
                "customLookColor" => bi.custom_look_color = val_to_uint(&val),
                "customIcon"      => bi.custom_icon     = val_to_uint(&val),
                "customIconColor" => bi.custom_icon_color = val_to_uint(&val),
                "realName" => {
                    if let mlua::Value::String(ref s) = val {
                        write_str_field(&mut bi.real_name, s);
                    }
                }
                "note" => {
                    if let mlua::Value::String(ref s) = val {
                        write_str_field(&mut bi.note, s);
                    }
                }
                _ => {}
            }
            Ok(())
        });
    }
}

// ---------------------------------------------------------------------------
// BankItemObject — bank slot (read/write)
// ---------------------------------------------------------------------------
impl UserData for BankItemObject {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            if this.ptr.is_null() { return Ok(mlua::Value::Nil); }
            let bd = unsafe { &*(this.ptr as *const BankData) };
            macro_rules! int  { ($e:expr) => { Ok(mlua::Value::Integer($e as i64)) }; }
            macro_rules! cstr { ($arr:expr) => {{
                let s = fixed_str($arr);
                Ok(mlua::Value::String(lua.create_string(s)?))
            }}; }
            match key.as_str() {
                "id"              => int!(bd.item_id),
                "amount"          => int!(bd.amount),
                "protected"       => int!(bd.protected),
                "owner"           => int!(bd.owner),
                "realName"        => cstr!(&bd.real_name),
                "time"            => int!(bd.time),
                "customLook"      => int!(bd.custom_look),
                "customLookColor" => int!(bd.custom_look_color),
                "customIcon"      => int!(bd.custom_icon),
                "customIconColor" => int!(bd.custom_icon_color),
                "note"            => cstr!(&bd.note),
                _ => {
                    let db = unsafe { crate::database::item_db::rust_itemdb_search(bd.item_id) };
                    unsafe { item_data_getattr(lua, db, &key) }
                }
            }
        });

        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (String, mlua::Value)| {
            if this.ptr.is_null() { return Ok(()); }
            let bd = unsafe { &mut *(this.ptr as *mut BankData) };
            match key.as_str() {
                "id"              => bd.item_id         = val_to_uint(&val),
                "amount"          => bd.amount          = val_to_uint(&val),
                "protected"       => bd.protected       = val_to_uint(&val),
                "owner"           => bd.owner           = val_to_uint(&val),
                "time"            => bd.time            = val_to_uint(&val),
                "customLook"      => bd.custom_look     = val_to_uint(&val),
                "customLookColor" => bd.custom_look_color = val_to_uint(&val),
                "customIcon"      => bd.custom_icon     = val_to_uint(&val),
                "customIconColor" => bd.custom_icon_color = val_to_uint(&val),
                "realName" => {
                    if let mlua::Value::String(ref s) = val {
                        write_str_field(&mut bd.real_name, s);
                    }
                }
                "note" => {
                    if let mlua::Value::String(ref s) = val {
                        write_str_field(&mut bd.note, s);
                    }
                }
                _ => {}
            }
            Ok(())
        });
    }
}

// ---------------------------------------------------------------------------
// ParcelObject — mail parcel (read-only from Lua)
// ---------------------------------------------------------------------------
impl UserData for ParcelObject {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            if this.ptr.is_null() { return Ok(mlua::Value::Nil); }
            let p = unsafe { &*(this.ptr as *const Parcel) };
            macro_rules! int  { ($e:expr) => { Ok(mlua::Value::Integer($e as i64)) }; }
            macro_rules! cstr { ($arr:expr) => {{
                let s = fixed_str($arr);
                Ok(mlua::Value::String(lua.create_string(s)?))
            }}; }
            match key.as_str() {
                "id"        => int!(p.data.id),
                "amount"    => int!(p.data.amount),
                "dura"      => int!(p.data.dura),
                "protected" => int!(p.data.protected),
                "owner"     => int!(p.data.owner),
                "realName"  => cstr!(&p.data.real_name),
                "time"      => int!(p.data.time),
                "sender"    => int!(p.sender),
                "pos"       => int!(p.pos),
                "npcFlag"   => int!(p.npcflag),
                _ => {
                    let db = unsafe { crate::database::item_db::rust_itemdb_search(p.data.id) };
                    unsafe { item_data_getattr(lua, db, &key) }
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// RecipeObject — recipe DB entry (read-only, constructed by id or name lookup)
// ---------------------------------------------------------------------------
impl UserData for RecipeObject {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            if this.ptr.is_null() { return Ok(mlua::Value::Nil); }
            let r = unsafe { &*(this.ptr as *const RecipeData) };
            macro_rules! int  { ($e:expr) => { Ok(mlua::Value::Integer($e as i64)) }; }
            macro_rules! cstr { ($arr:expr) => {{
                let s = fixed_str($arr);
                Ok(mlua::Value::String(lua.create_string(s)?))
            }}; }
            match key.as_str() {
                "id"               => int!(r.id),
                "identifier"       => cstr!(&r.identifier),
                "description"      => cstr!(&r.description),
                "critIdentifier"   => cstr!(&r.crit_identifier),
                "critDescription"  => cstr!(&r.crit_description),
                "craftTime"        => int!(r.craft_time),
                "successRate"      => int!(r.success_rate),
                "skillAdvance"     => int!(r.skill_advance),
                "critRate"         => int!(r.crit_rate),
                "bonus"            => int!(r.bonus),
                "skillRequired"    => int!(r.skill_required),
                "tokensRequired"   => int!(r.tokens_required),
                "materials" => {
                    let tbl = lua.create_table()?;
                    for (i, &v) in r.materials.iter().enumerate() {
                        tbl.raw_set(i + 1, v)?;
                    }
                    Ok(mlua::Value::Table(tbl))
                }
                "superiorMaterials" => {
                    let tbl = lua.create_table()?;
                    for (i, &v) in r.superior_materials.iter().enumerate() {
                        tbl.raw_set(i + 1, v)?;
                    }
                    Ok(mlua::Value::Table(tbl))
                }
                _ => Ok(mlua::Value::Nil),
            }
        });
    }
}
