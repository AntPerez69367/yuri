//! extern "C" stubs for C functions called by scripting method bodies.
//! Replace each group as the corresponding Rust module is ported.

use std::os::raw::c_int;

pub const BL_PC:  c_int = 0x01;
pub const BL_MOB: c_int = 0x02;
pub const BL_NPC: c_int = 0x04;

extern "C" {
    // pc_* stubs added in Phase 6 as method bodies are written.
    // clif_* stubs added as method bodies are written.
    // mob_* stubs added in Phase 5 as method bodies are written.
}
