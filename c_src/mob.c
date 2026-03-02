// mob.c — C helpers that require C ABI linkage.
// All game logic has been ported to src/game/mob.rs.
// This file provides:
//   mob_free_helper     — FREE() wrapper for Rust
//   mob_null            — no-op va_list callback
//   mobdb_init          — calls rust_mobdb_init() + rust_mobspawn_read()
#include "mob.h"

#include <stdlib.h>

#include "core.h"
#include "map_server.h"

// Note: all global variables (mob_id, MOB_SPAWN_MAX, etc.) are now owned
// by Rust (src/game/mob.rs) via #[export_name]. Do not redeclare them here.

void mob_free_helper(MOB *m) { FREE(m); }

int mob_null(struct block_list *bl, va_list ap) { (void)bl; (void)ap; return 0; }

int mobdb_init() {
  if (rust_mobdb_init() != 0) return -1;
  rust_mobspawn_read();
  return 0;
}
