// build.rs — compile C game-logic files and link them into the map_server binary.
//
// Previously, a separate cmake step built libmap_game.a and libcommon_nocore.a
// into bin/. Now the cc crate compiles those C files directly as part of the
// Cargo build, eliminating the cmake dependency entirely.
//
// libmap_game_c (compiled here) and libyuri.a have circular symbol dependencies.
// We use --start-group/--end-group to allow multiple linker passes.

fn main() {
    // OUT_DIR is where cc places the compiled .a files (e.g. target/debug/build/yuri-.../out/)
    let out_dir = std::env::var("OUT_DIR").unwrap();

    // Tell cargo to search OUT_DIR for our cc-compiled archives
    println!("cargo:rustc-link-search=native={}", out_dir);

    // Common C flags
    let base_flags = &[
        "-std=gnu17",
        "-g3",
        "-DDEBUG",
        "-DFD_SETSIZE=1024",
        "-fno-stack-protector",
        // Suppress warnings that are harmless but noisy during the migration
        "-Wno-implicit-function-declaration",
        "-Wno-unused-variable",
        "-Wno-return-type",
    ];

    // config.c removed — all globals ported to src/ffi/config_globals.rs as #[no_mangle] statics.

    // Compile c_deps/*.c (db, timer, showmsg, strlib, etc.)
    let mut deps_build = cc::Build::new();
    deps_build
        .files(&[
            "c_deps/db_mysql.c",
            "c_deps/db.c",
            "c_deps/ers.c",
            "c_deps/md5calc.c",
            "c_deps/rndm.c",
            "c_deps/showmsg.c",
            "c_deps/strlib.c",
            "c_deps/timer.c",
        ])
        .include("c_src")
        .include("c_deps")
        .include("/usr/include/mysql");
    for flag in base_flags {
        deps_build.flag(flag);
    }
    deps_build.compile("deps_c");

    // rust_shims.c deleted (Task 3.1): all shim symbols removed; Rust callers now
    // use #[link_name = "rust_*"] to reach the real Rust implementations directly.
    // rust_shims_map.c deleted (Task 3.2): sl_intif_save / sl_intif_savequit
    // ported to Rust in src/ffi/map_char.rs as rust_sl_intif_save / rust_sl_intif_savequit.
    //
    // map_server.c has been deleted; its globals (sql_handle, char_fd, map_fd,
    // userlist, auth_n) now live in src/game/map_server.rs as #[no_mangle] statics.
    // sl_compat.c has been deleted; its globals (groups[]) moved to Rust (Task 1.11).
    // map_server_stubs.c has been deleted (Task 2.3); all its functions and globals
    // (map_reload, map_reset_timer, groups[], log_fd, map_max, map_ip_s, log_ip_s,
    //  oldHour, oldMinute, cronjobtimer, bl_list_count, mobsearch_db) now live in
    //  src/game/map_server.rs as #[no_mangle] statics/functions.
    //
    // All C source in c_src/ has been eliminated. The map_game_c archive is no
    // longer needed; remove the linker group args below if this section is empty.
    // (We keep the compile step absent — nothing to compile.)

    // Link the cc-compiled C archives in a group to handle circular dependencies
    // between the three C archives and libyuri (the Rust rlib, linked automatically
    // by cargo — we don't need to reference libyuri.a explicitly here since cargo
    // uses the rlib for Rust→C linkage when building a binary).
    //
    // cc::Build::compile() already emits `cargo:rustc-link-lib=static=<name>` for
    // each archive, which causes them to be linked. We wrap them in --start-group
    // /--end-group to handle any circular refs within the C archives themselves.
    // Note: libmap_game_c.a removed (Tasks 3.1+3.2) — only libdeps_c.a remains.
    println!("cargo:rustc-link-arg-bin=map_server=-Wl,--start-group");
    println!("cargo:rustc-link-arg-bin=map_server=-Wl,{}/libdeps_c.a", out_dir);
    println!("cargo:rustc-link-arg-bin=map_server=-Wl,--end-group");

    // External deps required by the C game libraries.
    println!("cargo:rustc-link-lib=luajit-5.1");
    println!("cargo:rustc-link-lib=mysqlclient");
    println!("cargo:rustc-link-lib=z");
    println!("cargo:rustc-link-lib=m");
    println!("cargo:rustc-link-lib=dl");
    println!("cargo:rustc-link-lib=pthread");

    // Re-run triggers
    println!("cargo:rerun-if-changed=build.rs");
    // map_server_stubs.c deleted (Task 2.3) — trigger removed.
    // rust_shims.c deleted (Task 3.1) — trigger removed.
    // rust_shims_map.c deleted (Task 3.2) — trigger removed.
    println!("cargo:rerun-if-changed=c_deps/db_mysql.c");
    println!("cargo:rerun-if-changed=c_deps/db.c");
    println!("cargo:rerun-if-changed=c_deps/ers.c");
    println!("cargo:rerun-if-changed=c_deps/md5calc.c");
    println!("cargo:rerun-if-changed=c_deps/rndm.c");
    println!("cargo:rerun-if-changed=c_deps/showmsg.c");
    println!("cargo:rerun-if-changed=c_deps/strlib.c");
    println!("cargo:rerun-if-changed=c_deps/timer.c");
}
