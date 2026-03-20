use std::fs;
use std::process::Command;
use tealr::TypeWalker;
use yuri::game::lua::entity::item::LuaItem;
use yuri::game::lua::entity::mob::LuaMob;
use yuri::game::lua::entity::npc::LuaNpc;
use yuri::game::lua::entity::player::LuaPlayer;

fn main() {
    let walker = TypeWalker::new()
        .process_type::<LuaPlayer>()
        .process_type::<LuaMob>()
        .process_type::<LuaNpc>()
        .process_type::<LuaItem>();

    let json = walker.to_json_pretty().expect("failed to serialize type info");

    // Write JSON for reference
    fs::create_dir_all("docs").expect("failed to create docs dir");
    fs::write("docs/lua-api.json", &json).expect("failed to write docs/lua-api.json");

    // tealr_doc_gen expects {name}.json in the working directory
    fs::write("yuri-lua-api.json", &json).expect("failed to write yuri-lua-api.json");

    // Clean and regenerate HTML
    let _ = fs::remove_dir_all("docs/lua-api-html");

    let status = Command::new("tealr_doc_gen")
        .arg("run")
        .status();

    // Clean up temp file
    let _ = fs::remove_file("yuri-lua-api.json");

    match status {
        Ok(s) if s.success() => {
            println!("Lua API docs generated:");
            println!("  JSON: docs/lua-api.json");
            println!("  HTML: docs/lua-api-html/");
        }
        Ok(s) => {
            eprintln!("tealr_doc_gen exited with {s}");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Failed to run tealr_doc_gen: {e}");
            eprintln!("Install with: cargo install tealr_doc_gen");
            println!("JSON written to docs/lua-api.json");
        }
    }
}
