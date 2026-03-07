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

    // Compile config.c (common_nocore) — config globals referenced by Rust FFI.
    let mut config_build = cc::Build::new();
    config_build
        .file("c_src/config.c")
        .include("c_src")
        .include("c_deps")
        .include("/usr/include/mysql");
    for flag in base_flags {
        config_build.flag(flag);
    }
    config_build.compile("config_c");

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

    // Compile map_game C files (sl_compat.c, map_server_stubs.c) — game logic that
    // Rust map_server links against. Needs LuaJIT and MySQL headers.
    // map_server.c has been deleted; its globals (sql_handle, char_fd, map_fd,
    // userlist, auth_n) now live in src/game/map_server.rs as #[no_mangle] statics.
    // map_server_stubs.c contains the remaining live C functions (Phase 3 TODO items).
    let mut map_game_build = cc::Build::new();
    map_game_build
        .files(&["c_src/map_server_stubs.c", "c_src/sl_compat.c", "c_src/rust_shims.c", "c_src/rust_shims_map.c"])
        .include("c_src")
        .include("c_deps")
        .include("/usr/include/mysql")
        .include("/usr/include/luajit-2.1");
    for flag in base_flags {
        map_game_build.flag(flag);
    }
    map_game_build.compile("map_game_c");

    // Link the cc-compiled C archives in a group to handle circular dependencies
    // between the three C archives and libyuri (the Rust rlib, linked automatically
    // by cargo — we don't need to reference libyuri.a explicitly here since cargo
    // uses the rlib for Rust→C linkage when building a binary).
    //
    // cc::Build::compile() already emits `cargo:rustc-link-lib=static=<name>` for
    // each archive, which causes them to be linked. We wrap them in --start-group
    // /--end-group to handle any circular refs within the C archives themselves.
    println!("cargo:rustc-link-arg-bin=map_server=-Wl,--start-group");
    println!("cargo:rustc-link-arg-bin=map_server=-Wl,{}/libmap_game_c.a", out_dir);
    println!("cargo:rustc-link-arg-bin=map_server=-Wl,{}/libconfig_c.a", out_dir);
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
    println!("cargo:rerun-if-changed=c_src/config.c");
    println!("cargo:rerun-if-changed=c_src/map_server_stubs.c");
    println!("cargo:rerun-if-changed=c_src/sl_compat.c");
    println!("cargo:rerun-if-changed=c_src/rust_shims.c");
    println!("cargo:rerun-if-changed=c_src/rust_shims_map.c");
    println!("cargo:rerun-if-changed=c_deps/db_mysql.c");
    println!("cargo:rerun-if-changed=c_deps/db.c");
    println!("cargo:rerun-if-changed=c_deps/ers.c");
    println!("cargo:rerun-if-changed=c_deps/md5calc.c");
    println!("cargo:rerun-if-changed=c_deps/rndm.c");
    println!("cargo:rerun-if-changed=c_deps/showmsg.c");
    println!("cargo:rerun-if-changed=c_deps/strlib.c");
    println!("cargo:rerun-if-changed=c_deps/timer.c");
}
