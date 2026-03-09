pub mod block;
pub mod util;
#[cfg(not(test))]
pub mod map_char;
#[cfg(not(test))]
pub mod map_parse;
#[cfg(not(test))]
pub mod map_server;
pub mod mob;
#[cfg(not(test))]
pub mod npc;
#[cfg(not(test))]
pub mod client;
#[cfg(not(test))]
pub mod gm_command;
#[cfg(not(test))]
pub mod pc;
#[cfg(not(test))]
pub mod scripting;
#[cfg(test)]
pub mod scripting {
    pub mod object_collect;
}
pub mod types;
