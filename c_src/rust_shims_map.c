/*
 * rust_shims_map.c — trampolines for static-inline map_char.h functions.
 *
 * Rust cannot call static-inline C functions directly; these non-inline
 * wrappers provide linkable symbols for the Rust game modules.
 *
 * read_pass was previously provided here; it is now inlined in Rust
 * (src/game/map_parse/items.rs and movement.rs) and removed.
 */

#include "map_server.h"
#include "map_char.h"

/* sl_intif_savequit: trampoline for the static-inline intif_savequit in map_char.h.
 * Caller: src/game/client/handlers.rs */
int sl_intif_savequit(USER *sd) { return intif_savequit(sd); }

/* sl_intif_save: trampoline for the static-inline intif_save in map_char.h.
 * Callers: src/game/scripting/pc_accessors.rs, src/game/map_server.rs */
int sl_intif_save(void *sd) { return intif_save((USER *)sd); }
