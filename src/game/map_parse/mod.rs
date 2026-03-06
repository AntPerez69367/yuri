//! Port of `c_src/map_parse.c` — client packet handlers and send helpers.
//!
//! Submodule layout:
//!   packet       — FIFO helpers + clif_send routing layer
//!   player_state — sendstatus, sendxy, sendid, sendmapinfo (login sequence)
//!   visual       — object look/spawn system (clif_*look*, clif_spawn)
//!   movement     — parsewalk, chararea, sendmapdata
//!   combat       — parseattack, magic, dura system
//!   chat         — parsesay, parsewisp, broadcast
//!   dialogs      — scriptmes, scriptmenu, buydialog, selldialog, input
//!   items        — parseuseitem, parseunequip, parsewield, throwitem
//!   trading      — clif_exchange_* family
//!   groups       — party/group status, add, update, leave
//!   events       — rankings, reward parcels

pub mod packet;
pub mod player_state;
pub mod visual;
pub mod movement;
pub mod combat;
pub mod chat;
pub mod dialogs;
pub mod items;
pub mod trading;
pub mod groups;
pub mod events;
