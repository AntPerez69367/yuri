// build.rs — link external system libraries required by the map_server binary.
//
// All C source files (c_deps/ and c_src/) have been eliminated as part of the
// C→Rust migration. No cc::Build compile step is needed. We only emit link
// directives for the external system libraries that Rust code calls into.
//
// Migration history:
//   config.c     removed — globals ported to src/ffi/config_globals.rs
//   map_server.c removed — globals/functions ported to src/game/map_server.rs
//   sl_compat.c  removed (Task 1.11) — globals moved to Rust
//   map_server_stubs.c removed (Task 2.3) — ported to src/game/map_server.rs
//   rust_shims.c removed (Task 3.1)
//   rust_shims_map.c removed (Task 3.2)
//   c_deps/timer.c   removed (Task 4.1) — replaced by src/ffi/timer.rs
//   c_deps/rndm.c    removed (Tasks 4.2-4.8) — replaced by rand::random::<u32>()
//   c_deps/db_mysql.c removed (Tasks 4.2-4.8) — sql_handle dropped; all SQL via sqlx
//   c_deps/showmsg.c removed (Tasks 4.2-4.8) — no callers
//   c_deps/strlib.c  removed (Tasks 4.2-4.8) — no callers

fn main() {
    // LuaJIT — required by mlua; not available as a Rust crate.
    println!("cargo:rustc-link-lib=luajit-5.1");
    // LuaJIT runtime dependencies on Linux.
    println!("cargo:rustc-link-lib=dl");
    println!("cargo:rustc-link-lib=m");

    // Re-run triggers
    println!("cargo:rerun-if-changed=build.rs");
}
