use std::ffi::{CString, c_int, c_ulong};
use std::os::raw::c_void;
use mlua::{MetaMethod, UserData, UserDataMethods};

use crate::game::scripting::ffi as sffi;

pub struct RegObject       { pub ptr: *mut c_void }
pub struct RegStringObject { pub ptr: *mut c_void }
pub struct NpcRegObject    { pub ptr: *mut c_void }
pub struct MobRegObject    { pub ptr: *mut c_void }
pub struct MapRegObject    { pub ptr: *mut c_void }
pub struct GameRegObject   { pub ptr: *mut c_void }
pub struct QuestRegObject  { pub ptr: *mut c_void }

unsafe impl Send for RegObject {}
unsafe impl Send for RegStringObject {}
unsafe impl Send for NpcRegObject {}
unsafe impl Send for MobRegObject {}
unsafe impl Send for MapRegObject {}
unsafe impl Send for GameRegObject {}
unsafe impl Send for QuestRegObject {}

fn val_to_int(v: &mlua::Value) -> c_int {
    match v {
        mlua::Value::Integer(i) => *i as c_int,
        mlua::Value::Number(f)  => *f as c_int,
        _ => 0,
    }
}

fn val_to_ulong(v: &mlua::Value) -> c_ulong {
    match v {
        mlua::Value::Integer(i) => *i as c_ulong,
        mlua::Value::Number(f)  => *f as c_ulong,
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// RegObject — player integer registry (pc_readglobalreg / pc_setglobalreg)
// ---------------------------------------------------------------------------
impl UserData for RegObject {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |_, this, key: String| {
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            let val = unsafe { sffi::pc_readglobalreg(this.ptr, ckey.as_ptr()) };
            Ok(val)
        });
        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (String, mlua::Value)| {
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            unsafe { sffi::pc_setglobalreg(this.ptr, ckey.as_ptr(), val_to_ulong(&val)); }
            Ok(())
        });
    }
}

// ---------------------------------------------------------------------------
// RegStringObject — player string registry
// ---------------------------------------------------------------------------
impl UserData for RegStringObject {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |_, this, key: String| {
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            let raw = unsafe { sffi::pc_readglobalregstring(this.ptr, ckey.as_ptr()) };
            let s = if raw.is_null() {
                String::new()
            } else {
                unsafe { std::ffi::CStr::from_ptr(raw).to_string_lossy().into_owned() }
            };
            Ok(s)
        });
        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (String, mlua::Value)| {
            let sval = match &val {
                mlua::Value::String(s) => s.to_str().map(|s| s.to_owned()).unwrap_or_default(),
                _ => String::new(),
            };
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            let cval = CString::new(sval).map_err(mlua::Error::external)?;
            unsafe { sffi::pc_setglobalregstring(this.ptr, ckey.as_ptr(), cval.as_ptr()); }
            Ok(())
        });
    }
}

// ---------------------------------------------------------------------------
// NpcRegObject — NPC integer registry
// ---------------------------------------------------------------------------
impl UserData for NpcRegObject {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |_, this, key: String| {
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            let val = unsafe { sffi::npc_readglobalreg_ffi(this.ptr, ckey.as_ptr()) };
            Ok(val)
        });
        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (String, mlua::Value)| {
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            unsafe { sffi::npc_setglobalreg_ffi(this.ptr, ckey.as_ptr(), val_to_int(&val)); }
            Ok(())
        });
    }
}

// ---------------------------------------------------------------------------
// MobRegObject — mob integer registry
// ---------------------------------------------------------------------------
impl UserData for MobRegObject {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |_, this, key: String| {
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            let val = unsafe { sffi::rust_mob_readglobalreg(this.ptr, ckey.as_ptr()) };
            Ok(val)
        });
        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (String, mlua::Value)| {
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            unsafe { sffi::rust_mob_setglobalreg(this.ptr, ckey.as_ptr(), val_to_int(&val)); }
            Ok(())
        });
    }
}

// ---------------------------------------------------------------------------
// MapRegObject — per-map integer registry (extracts bl.m via C helper)
// ---------------------------------------------------------------------------
impl UserData for MapRegObject {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |_, this, key: String| {
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            let val = unsafe { sffi::map_readglobalreg_sd(this.ptr, ckey.as_ptr()) };
            Ok(val)
        });
        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (String, mlua::Value)| {
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            unsafe { sffi::map_setglobalreg_sd(this.ptr, ckey.as_ptr(), val_to_int(&val)); }
            Ok(())
        });
    }
}

// ---------------------------------------------------------------------------
// GameRegObject — game-global integer registry (no self pointer needed)
// ---------------------------------------------------------------------------
impl UserData for GameRegObject {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |_, _this, key: String| {
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            let val = unsafe { sffi::map_readglobalgamereg(ckey.as_ptr()) };
            Ok(val)
        });
        methods.add_meta_method(MetaMethod::NewIndex, |_, _this, (key, val): (String, mlua::Value)| {
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            unsafe { sffi::map_setglobalgamereg(ckey.as_ptr(), val_to_int(&val)); }
            Ok(())
        });
    }
}

// ---------------------------------------------------------------------------
// QuestRegObject — player quest integer registry
// ---------------------------------------------------------------------------
impl UserData for QuestRegObject {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |_, this, key: String| {
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            let val = unsafe { sffi::pc_readquestreg(this.ptr, ckey.as_ptr()) };
            Ok(val)
        });
        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (String, mlua::Value)| {
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            unsafe { sffi::pc_setquestreg(this.ptr, ckey.as_ptr(), val_to_int(&val)); }
            Ok(())
        });
    }
}
