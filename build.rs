// build.rs — link directives for the map_server binary.
//
// map_server calls C game-logic functions (npc_init, clif_parse, etc.) that
// live in libmap_game.a, built by cmake before cargo runs.
//
// libmap_game.a, libcommon.a, and libyuri.a have circular symbol dependencies
// (same pattern documented in MEMORY.md). We use --start-group/--end-group.

fn main() {
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let bin_dir = std::path::Path::new("bin");
    let target_dir = std::path::PathBuf::from(format!("target/{}", profile));

    // Only emit map_game link directives when cmake has produced the library.
    // This lets `cargo check` / `cargo test` work in pure-Rust contexts.
    if bin_dir.join("libmap_game.a").exists() && bin_dir.join("libcommon_nocore.a").exists() {
        // Group: libmap_game.a ↔ libcommon_nocore.a ↔ libyuri.a ↔ libdeps.a (circular deps).
        // We use common_nocore (no core.c) to avoid a duplicate main() symbol
        // — core.c defines main() for C executables; Rust provides its own main().
        println!("cargo:rustc-link-arg=-Wl,--start-group");
        println!("cargo:rustc-link-arg=-Wl,bin/libmap_game.a");
        println!("cargo:rustc-link-arg=-Wl,bin/libcommon_nocore.a");
        println!("cargo:rustc-link-arg=-Wl,bin/libdeps.a");
        println!(
            "cargo:rustc-link-arg=-Wl,{}/libyuri.a",
            target_dir.display()
        );
        println!("cargo:rustc-link-arg=-Wl,--end-group");

        // External deps required by map_game and deps
        println!("cargo:rustc-link-lib=luajit-5.1");
        println!("cargo:rustc-link-lib=mysqlclient");
        println!("cargo:rustc-link-lib=z");
        println!("cargo:rustc-link-lib=m");
        println!("cargo:rustc-link-lib=dl");
        println!("cargo:rustc-link-lib=pthread");
    }

    println!("cargo:rerun-if-changed=bin/libmap_game.a");
    println!("cargo:rerun-if-changed=bin/libcommon_nocore.a");
    println!("cargo:rerun-if-changed=build.rs");
}
