//! extern "C" stubs for C functions called by scripting method bodies.
//! Replace each group as the corresponding Rust module is ported.

use std::ffi::{c_char, c_int, c_ulong};
use std::os::raw::c_void;

pub const BL_PC:  c_int = 0x01;
pub const BL_MOB: c_int = 0x02;
pub const BL_NPC: c_int = 0x04;

extern "C" {
    // --- Phase 2: registry types ---

    // Player (USER*) integer registries
    pub fn pc_readglobalreg(sd: *mut c_void, attrname: *const c_char) -> c_int;
    pub fn pc_setglobalreg(sd: *mut c_void, attrname: *const c_char, val: c_ulong) -> c_int;
    pub fn pc_readnpcintreg(sd: *mut c_void, attrname: *const c_char) -> c_int;
    pub fn pc_setnpcintreg(sd: *mut c_void, attrname: *const c_char, val: c_int) -> c_int;
    pub fn pc_readquestreg(sd: *mut c_void, attrname: *const c_char) -> c_int;
    pub fn pc_setquestreg(sd: *mut c_void, attrname: *const c_char, val: c_int) -> c_int;

    // Player string registry
    pub fn pc_readglobalregstring(sd: *mut c_void, attrname: *const c_char) -> *const c_char;
    pub fn pc_setglobalregstring(sd: *mut c_void, attrname: *const c_char, val: *const c_char) -> c_int;

    // NPC integer registry (via static-inline wrapper in npc.h → npc_*_ffi symbols)
    pub fn npc_readglobalreg_ffi(nd: *mut c_void, attrname: *const c_char) -> c_int;
    pub fn npc_setglobalreg_ffi(nd: *mut c_void, attrname: *const c_char, val: c_int) -> c_int;

    // Mob registries — already #[no_mangle] Rust functions in ffi/mob.rs
    pub fn rust_mob_readglobalreg(mob: *mut c_void, attrname: *const c_char) -> c_int;
    pub fn rust_mob_setglobalreg(mob: *mut c_void, attrname: *const c_char, val: c_int) -> c_int;

    // Map registries — helpers in sl_compat.c extract bl.m from USER*
    pub fn map_readglobalreg_sd(sd: *mut c_void, attrname: *const c_char) -> c_int;
    pub fn map_setglobalreg_sd(sd: *mut c_void, attrname: *const c_char, val: c_int) -> c_int;

    // Game-global registries (no self pointer)
    pub fn map_readglobalgamereg(attrname: *const c_char) -> c_int;
    pub fn map_setglobalgamereg(attrname: *const c_char, val: c_int) -> c_int;

    // pc_* stubs added in Phase 6 as method bodies are written.
    // clif_* stubs added as method bodies are written.
    // mob_* stubs added in Phase 5 as method bodies are written.
}
