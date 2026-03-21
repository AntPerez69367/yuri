use std::ffi::CString;
use mlua::{MetaMethod, UserData, UserDataMethods};

use crate::game::pc::MapSessionData;
use crate::game::pc::{
    pc_readglobalreg, pc_setglobalreg, pc_readglobalregstring, pc_setglobalregstring,
    pc_readquestreg, pc_setquestreg,
};
use crate::game::npc::{npc_readglobalreg, npc_setglobalreg};
use crate::game::mob::{mob_readglobalreg, mob_setglobalreg};
use crate::game::map_server::map_readglobalgamereg;
use crate::game::scripting::map_globals;

pub struct RegObject       { pub ptr: *mut std::ffi::c_void }
pub struct RegStringObject { pub ptr: *mut std::ffi::c_void }
pub struct NpcRegObject    { pub ptr: *mut std::ffi::c_void }
pub struct MobRegObject    { pub ptr: *mut std::ffi::c_void }
pub struct MapRegObject    { pub ptr: *mut std::ffi::c_void }
pub struct GameRegObject   { pub ptr: *mut std::ffi::c_void }
pub struct QuestRegObject  { pub ptr: *mut std::ffi::c_void }

// SAFETY: These objects wrap raw C pointers that are owned and managed by the
// Lua scripting runtime, which runs entirely on a single dedicated thread.
// The underlying C registry functions (pc_readglobalreg, npc_setglobalreg,
// etc.) are called only while holding the Lua lock and are never aliased from
// another thread. Transferring these objects across threads is therefore safe
// because no concurrent access can occur while they are in transit and they
// carry no interior mutability beyond the C-side registry data.
unsafe impl Send for RegObject {}
unsafe impl Send for RegStringObject {}
unsafe impl Send for NpcRegObject {}
unsafe impl Send for MobRegObject {}
unsafe impl Send for MapRegObject {}
unsafe impl Send for GameRegObject {}
unsafe impl Send for QuestRegObject {}

fn val_to_int(v: &mlua::Value) -> Result<i32, mlua::Error> {
    match v {
        mlua::Value::Integer(i) => {
            i32::try_from(*i).map_err(|_| {
                mlua::Error::external(format!("integer value {} out of range for i32", i))
            })
        }
        mlua::Value::Number(f) => {
            if !f.is_finite() {
                return Err(mlua::Error::external(format!(
                    "float value {} is not finite; expected integer for registry",
                    f
                )));
            }
            if f.fract() != 0.0 {
                return Err(mlua::Error::external(format!(
                    "float value {} is not a whole number; expected integer for registry",
                    f
                )));
            }
            if *f < i32::MIN as f64 || *f > i32::MAX as f64 {
                return Err(mlua::Error::external(format!(
                    "float value {} out of range for i32",
                    f
                )));
            }
            Ok(*f as i32)
        }
        other => Err(mlua::Error::external(format!(
            "expected integer for registry value, got {}",
            other.type_name()
        ))),
    }
}

fn val_to_ulong(v: &mlua::Value) -> Result<u64, mlua::Error> {
    match v {
        mlua::Value::Integer(i) => {
            u64::try_from(*i).map_err(|_| {
                mlua::Error::external(format!("integer value {} out of range for u64", i))
            })
        }
        mlua::Value::Number(f) => {
            if !f.is_finite() {
                return Err(mlua::Error::external(format!(
                    "float value {} is not finite; expected integer for registry",
                    f
                )));
            }
            if f.fract() != 0.0 {
                return Err(mlua::Error::external(format!(
                    "float value {} is not a whole number; expected integer for registry",
                    f
                )));
            }
            let t = f.trunc();
            if t < 0.0 {
                return Err(mlua::Error::external(format!(
                    "float value {} is negative; expected non-negative integer for registry",
                    f
                )));
            }
            // Use u128 intermediate to avoid f64 precision loss at the high end of u64 range.
            let bits = t as u128;
            if bits > u64::MAX as u128 {
                return Err(mlua::Error::external(format!(
                    "float value {} overflows u64",
                    f
                )));
            }
            Ok(bits as u64)
        }
        other => Err(mlua::Error::external(format!(
            "expected integer for registry value, got {}",
            other.type_name()
        ))),
    }
}

// ---------------------------------------------------------------------------
// RegObject — player integer registry (pc_readglobalreg / pc_setglobalreg)
// ---------------------------------------------------------------------------
impl UserData for RegObject {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |_, this, key: String| {
            if this.ptr.is_null() {
                return Err(mlua::Error::external("RegObject: ptr is null"));
            }
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            let val = unsafe { pc_readglobalreg(this.ptr as *mut MapSessionData, ckey.as_ptr()) };
            Ok(val)
        });
        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (String, mlua::Value)| {
            if this.ptr.is_null() {
                return Err(mlua::Error::external("RegObject: ptr is null"));
            }
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            unsafe { pc_setglobalreg(this.ptr as *mut MapSessionData, ckey.as_ptr(), val_to_ulong(&val)?); }
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
            if this.ptr.is_null() {
                return Err(mlua::Error::external("RegStringObject: ptr is null"));
            }
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            let raw = unsafe { pc_readglobalregstring(this.ptr as *mut MapSessionData, ckey.as_ptr()) };
            let s = if raw.is_null() {
                String::new()
            } else {
                unsafe { std::ffi::CStr::from_ptr(raw).to_string_lossy().into_owned() }
            };
            Ok(s)
        });
        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (String, mlua::Value)| {
            if this.ptr.is_null() {
                return Err(mlua::Error::external("RegStringObject: ptr is null"));
            }
            let sval = match &val {
                mlua::Value::String(s) => s.to_string_lossy(),
                other => return Err(mlua::Error::external(format!(
                    "expected string for registry value, got {}",
                    other.type_name()
                ))),
            };
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            let cval = CString::new(sval).map_err(mlua::Error::external)?;
            unsafe { pc_setglobalregstring(this.ptr as *mut MapSessionData, ckey.as_ptr(), cval.as_ptr()); }
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
            if this.ptr.is_null() {
                return Err(mlua::Error::external("NpcRegObject: ptr is null"));
            }
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            let val = unsafe { npc_readglobalreg(this.ptr as *mut _, ckey.as_ptr()) };
            Ok(val)
        });
        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (String, mlua::Value)| {
            if this.ptr.is_null() {
                return Err(mlua::Error::external("NpcRegObject: ptr is null"));
            }
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            unsafe { npc_setglobalreg(this.ptr as *mut _, ckey.as_ptr(), val_to_int(&val)?); }
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
            if this.ptr.is_null() {
                return Err(mlua::Error::external("MobRegObject: ptr is null"));
            }
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            let val = unsafe { mob_readglobalreg(this.ptr as *mut _, ckey.as_ptr()) };
            Ok(val)
        });
        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (String, mlua::Value)| {
            if this.ptr.is_null() {
                return Err(mlua::Error::external("MobRegObject: ptr is null"));
            }
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            unsafe { mob_setglobalreg(this.ptr as *mut _, ckey.as_ptr(), val_to_int(&val)?); }
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
            if this.ptr.is_null() {
                return Err(mlua::Error::external("MapRegObject: ptr is null"));
            }
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            let val = unsafe { map_globals::map_readglobalreg_sd(this.ptr as *mut MapSessionData, ckey.as_ptr()) };
            Ok(val)
        });
        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (String, mlua::Value)| {
            if this.ptr.is_null() {
                return Err(mlua::Error::external("MapRegObject: ptr is null"));
            }
            let val_i = val_to_int(&val)?;
            // Extract the map index synchronously from the session data pointer.
            let m = unsafe {
                let sd = this.ptr as *const crate::game::pc::MapSessionData;
                (*sd).m as i32
            };
            crate::database::blocking_run_async(async move {
                unsafe { crate::game::map_server::map_setglobalreg_str(m, key, val_i).await; }
            });
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
            Ok(map_readglobalgamereg(&key))
        });
        methods.add_meta_method(MetaMethod::NewIndex, |_, _this, (key, val): (String, mlua::Value)| {
            let val_i = val_to_int(&val)?;
            crate::database::blocking_run_async(async move {
                crate::game::map_server::map_setglobalgamereg_str(key, val_i).await;
            });
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
            if this.ptr.is_null() {
                return Err(mlua::Error::external("QuestRegObject: ptr is null"));
            }
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            let val = unsafe { pc_readquestreg(this.ptr as *mut MapSessionData, ckey.as_ptr()) };
            Ok(val)
        });
        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (String, mlua::Value)| {
            if this.ptr.is_null() {
                return Err(mlua::Error::external("QuestRegObject: ptr is null"));
            }
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            unsafe { pc_setquestreg(this.ptr as *mut MapSessionData, ckey.as_ptr(), val_to_int(&val)?); }
            Ok(())
        });
    }
}
