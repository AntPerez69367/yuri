use std::ffi::{CString, c_int, c_ulong};
use std::os::raw::c_void;
use mlua::{MetaMethod, UserData, UserDataMethods};

use crate::game::scripting::ffi as sffi;
use crate::game::scripting::map_globals;

pub struct RegObject       { pub ptr: *mut c_void }
pub struct RegStringObject { pub ptr: *mut c_void }
pub struct NpcRegObject    { pub ptr: *mut c_void }
pub struct MobRegObject    { pub ptr: *mut c_void }
pub struct MapRegObject    { pub ptr: *mut c_void }
pub struct GameRegObject   { pub ptr: *mut c_void }
pub struct QuestRegObject  { pub ptr: *mut c_void }

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

fn val_to_int(v: &mlua::Value) -> Result<c_int, mlua::Error> {
    match v {
        mlua::Value::Integer(i) => {
            c_int::try_from(*i).map_err(|_| {
                mlua::Error::external(format!("integer value {} out of range for c_int", i))
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
            if *f < c_int::MIN as f64 || *f > c_int::MAX as f64 {
                return Err(mlua::Error::external(format!(
                    "float value {} out of range for c_int",
                    f
                )));
            }
            Ok(*f as c_int)
        }
        other => Err(mlua::Error::external(format!(
            "expected integer for registry value, got {}",
            other.type_name()
        ))),
    }
}

fn val_to_ulong(v: &mlua::Value) -> Result<c_ulong, mlua::Error> {
    match v {
        mlua::Value::Integer(i) => {
            c_ulong::try_from(*i).map_err(|_| {
                mlua::Error::external(format!("integer value {} out of range for c_ulong", i))
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
            // Use u128 intermediate to avoid f64 precision loss at the high end of c_ulong range.
            let bits = t as u128;
            if bits > c_ulong::MAX as u128 {
                return Err(mlua::Error::external(format!(
                    "float value {} overflows c_ulong",
                    f
                )));
            }
            Ok(bits as c_ulong)
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
            let val = unsafe { sffi::rust_pc_readglobalreg(this.ptr, ckey.as_ptr()) };
            Ok(val)
        });
        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (String, mlua::Value)| {
            if this.ptr.is_null() {
                return Err(mlua::Error::external("RegObject: ptr is null"));
            }
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            unsafe { sffi::rust_pc_setglobalreg(this.ptr, ckey.as_ptr(), val_to_ulong(&val)?); }
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
            let raw = unsafe { sffi::rust_pc_readglobalregstring(this.ptr, ckey.as_ptr()) };
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
            unsafe { sffi::rust_pc_setglobalregstring(this.ptr, ckey.as_ptr(), cval.as_ptr()); }
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
            let val = unsafe { sffi::npc_readglobalreg_ffi(this.ptr, ckey.as_ptr()) };
            Ok(val)
        });
        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (String, mlua::Value)| {
            if this.ptr.is_null() {
                return Err(mlua::Error::external("NpcRegObject: ptr is null"));
            }
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            unsafe { sffi::npc_setglobalreg_ffi(this.ptr, ckey.as_ptr(), val_to_int(&val)?); }
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
            let val = unsafe { sffi::rust_mob_readglobalreg(this.ptr, ckey.as_ptr()) };
            Ok(val)
        });
        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (String, mlua::Value)| {
            if this.ptr.is_null() {
                return Err(mlua::Error::external("MobRegObject: ptr is null"));
            }
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            unsafe { sffi::rust_mob_setglobalreg(this.ptr, ckey.as_ptr(), val_to_int(&val)?); }
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
            let val = unsafe { map_globals::map_readglobalreg_sd(this.ptr, ckey.as_ptr()) };
            Ok(val)
        });
        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (String, mlua::Value)| {
            if this.ptr.is_null() {
                return Err(mlua::Error::external("MapRegObject: ptr is null"));
            }
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            unsafe { map_globals::map_setglobalreg_sd(this.ptr, ckey.as_ptr(), val_to_int(&val)?); }
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
            unsafe { sffi::map_setglobalgamereg(ckey.as_ptr(), val_to_int(&val)?); }
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
            let val = unsafe { sffi::rust_pc_readquestreg(this.ptr, ckey.as_ptr()) };
            Ok(val)
        });
        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (String, mlua::Value)| {
            if this.ptr.is_null() {
                return Err(mlua::Error::external("QuestRegObject: ptr is null"));
            }
            let ckey = CString::new(key).map_err(mlua::Error::external)?;
            unsafe { sffi::rust_pc_setquestreg(this.ptr, ckey.as_ptr(), val_to_int(&val)?); }
            Ok(())
        });
    }
}
