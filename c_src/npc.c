// npc.c — stripped to C-only helper accessors needed by npc_move_sub (Rust).
// All NPC game logic has been ported to src/game/npc.rs.
// TODO: move these helpers to mob.c / pc.c when those modules are ported.

#include "mob.h"
#include "map_server.h"

// Called from Rust npc_move_sub to check if a MOB block_list entry is dead.
int npc_helper_mob_is_dead(struct block_list *bl) {
  MOB *mob = (MOB *)bl;
  return (mob && mob->state == MOB_DEAD) ? 1 : 0;
}

// Called from Rust npc_move_sub to check if a PC block_list entry should be
// skipped (dead, invisible, or GM-level >= 50).
int npc_helper_pc_is_skip(struct block_list *bl, struct block_list *npc_bl) {
  USER *sd;
  NPC *nd;
  if (!bl || !npc_bl) return 0;
  sd = (USER *)bl;
  nd = (NPC *)npc_bl;
  if ((map[nd->bl.m].show_ghosts && sd->status.state == PC_DIE) ||
      sd->status.state == -1 || sd->status.gm_level >= 50) {
    return 1;
  }
  return 0;
}
