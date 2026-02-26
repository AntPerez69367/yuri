#pragma once

#include "map_server.h"

// npc.c has been ported to Rust (src/game/npc.rs).
// Functions below are extern declarations resolving to libyuri.a symbols.

// Called from map_server.rs via extern "C" — keep exact names
extern int npc_init();
extern int warp_init();
extern int npc_runtimers(int, int);

// npc_id global (read from Rust via extern)
extern unsigned int npc_id;

// npc_move_sub: defined in Rust (ffi/npc.rs) with #[no_mangle], declared here for
// any C code that passes it as a function pointer
int npc_move_sub(struct block_list *, va_list);

// Called from scripting.c / map_parse.c — inline wrappers to _ffi variants
static inline unsigned int npc_get_new_npctempid()                            { extern unsigned int npc_get_new_npctempid_ffi(); return npc_get_new_npctempid_ffi(); }
static inline int npc_idlower(int id)                                         { extern int npc_idlower_ffi(int); return npc_idlower_ffi(id); }
static inline int npc_readglobalreg(NPC *nd, const char *reg)                 { extern int npc_readglobalreg_ffi(NPC*, const char*); return npc_readglobalreg_ffi(nd, reg); }
static inline int npc_setglobalreg(NPC *nd, const char *r, int v)             { extern int npc_setglobalreg_ffi(NPC*, const char*, int); return npc_setglobalreg_ffi(nd, r, v); }
static inline int npc_warp(NPC *nd, int m, int x, int y)                      { extern int npc_warp_ffi(NPC*, int, int, int); return npc_warp_ffi(nd, m, x, y); }
static inline int npc_move(NPC *nd)                                           { extern int npc_move_ffi(NPC*); return npc_move_ffi(nd); }
static inline int npc_action(NPC *nd)                                         { extern int npc_action_ffi(NPC*); return npc_action_ffi(nd); }
static inline int npc_movetime(NPC *nd)                                       { extern int npc_movetime_ffi(NPC*); return npc_movetime_ffi(nd); }
static inline int npc_duration(NPC *nd)                                       { extern int npc_duration_ffi(NPC*); return npc_duration_ffi(nd); }
static inline int npc_src_clear()                                             { extern int npc_src_clear_ffi(); return npc_src_clear_ffi(); }
static inline int npc_src_add(const char *f)                                  { extern int npc_src_add_ffi(const char*); return npc_src_add_ffi(f); }
static inline int npc_warp_add(const char *f)                                 { extern int npc_warp_add_ffi(const char*); return npc_warp_add_ffi(f); }

// Helper accessors for npc_move_sub (used from Rust until mob.rs/pc.rs land)
int npc_helper_mob_is_dead(struct block_list *bl);
int npc_helper_pc_is_skip(struct block_list *bl, struct block_list *npc_bl);
