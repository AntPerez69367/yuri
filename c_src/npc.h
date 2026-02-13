#pragma once

#include "map_server.h"

struct npc_src_list {
  struct npc_src_list *next;
  char *file;
};

int npc_src_clear();
int npc_src_add(const char *);
int npc_init();
extern unsigned int npc_id;
unsigned int npc_get_new_npctempid();
int npc_idlower(int);
int npc_runtimers(int, int);
int npc_action(NPC *);
int npc_duration(NPC *);
int npc_move_sub(struct block_list *, va_list);
int npc_move(NPC *);
int npc_movetime(NPC *nd);
int npc_warp(NPC *, int, int, int);
int npc_warp_add(const char *);
int npc_readglobalreg(NPC *, const char *);
int npc_setglobalreg(NPC *, const char *, int);
int warp_init();