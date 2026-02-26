#pragma once

#include "map_server.h"

enum { MOB_ALIVE, MOB_DEAD, MOB_PARA, MOB_BLIND, MOB_HIT, MOB_ESCAPE };
enum { MOB_NORMAL, MOB_AGGRESSIVE, MOB_STATIONARY };

extern unsigned int MOB_SPAWN_START;
extern unsigned int MOB_SPAWN_MAX;
extern unsigned int MOB_ONETIME_START;
extern unsigned int MOB_ONETIME_MAX;
extern unsigned int MIN_TIMER;

// ─── mob_db sub-module — implemented in Rust (src/database/mob_db.rs) ─────────
struct MobDbData;
struct MobDbData* rust_mobdb_search(unsigned int id);
struct MobDbData* rust_mobdb_searchexist(unsigned int id);
struct MobDbData* rust_mobdb_searchname(const char* s);
int rust_mobdb_init(void);
void rust_mobdb_term(void);
int rust_mobdb_id(const char* s);
int rust_mobdb_level(unsigned int id);
unsigned int rust_mobdb_experience(unsigned int id);

static inline struct mobdb_data* mobdb_search(unsigned int id)      { return (struct mobdb_data*)rust_mobdb_search(id); }
static inline struct mobdb_data* mobdb_searchexist(unsigned int id) { return (struct mobdb_data*)rust_mobdb_searchexist(id); }
static inline struct mobdb_data* mobdb_searchname(const char* s)    { return (struct mobdb_data*)rust_mobdb_searchname(s); }
static inline int mobdb_id(const char* s)                           { return rust_mobdb_id(s); }
static inline int mobdb_level(unsigned int id)                      { return (int)rust_mobdb_level(id); }
static inline unsigned int mobdb_experience(unsigned int id)        { return rust_mobdb_experience(id); }
static inline void mobdb_term(void)                                 { rust_mobdb_term(); }

// ─── mob game logic — implemented in Rust (src/game/mob.rs) ──────────────────
int rust_mobspawn_read(void);
int rust_mob_timer_spawns(int, int);
int rust_mob_respawn_getstats(MOB*);
int rust_mob_warp(MOB*, int, int, int);
unsigned int* rust_mobspawn_onetime(unsigned int, int, int, int, int, int, int,
                                    unsigned int, unsigned int);
int rust_mob_readglobalreg(MOB*, const char*);
int rust_mob_setglobalreg(MOB*, const char*, int);
int rust_mob_drops(MOB*, void* /*USER**/);
int rust_mob_handle_sub(MOB*);
int rust_kill_mob(MOB*);
int rust_mob_calcstat(MOB*);

static inline int mobspawn_read(void)                   { return rust_mobspawn_read(); }
static inline int mob_timer_spawns(int id, int n)       { return rust_mob_timer_spawns(id, n); }
static inline int mob_respawn_getstats(MOB* m)          { return rust_mob_respawn_getstats(m); }
static inline int mob_warp(MOB* m, int a, int b, int c) { return rust_mob_warp(m, a, b, c); }
static inline unsigned int* mobspawn_onetime(unsigned int id, int m, int x, int y,
    int t, int s, int e, unsigned int r, unsigned int o) {
  return rust_mobspawn_onetime(id, m, x, y, t, s, e, r, o);
}
static inline int mob_readglobalreg(MOB* m, const char* r) { return rust_mob_readglobalreg(m, r); }
static inline int mob_setglobalreg(MOB* m, const char* r, int v) { return rust_mob_setglobalreg(m, r, v); }
static inline int mobdb_drops(MOB* m, USER* sd)         { return rust_mob_drops(m, (void*)sd); }
static inline int mob_handle_sub(MOB* m)                { rust_mob_handle_sub(m); return 0; }
static inline int kill_mob(MOB* m)                      { return rust_kill_mob(m); }
static inline int mob_calcstat(MOB* m)                  { return rust_mob_calcstat(m); }

// ─── C helpers that stay in mob.c ────────────────────────────────────────────
int mobdb_init(void);
int mob_find_target(struct block_list*, va_list);
int mob_attack(MOB*, int);
int mob_calc_critical(MOB*, USER*);
int mob_move(struct block_list*, va_list);
int mobdb_dropitem(unsigned int, unsigned int, int, int, int, int, int, int,
                   int, USER*);
void mob_free_helper(MOB*);

// ─── mob_respawn_nousers — inline wrapper (no public Rust FFI needed) ─────────
// Called only from mob game logic; Rust exposes it via rust_mob_respawn_getstats chain.
// Remaining callers go through mob_respawn_getstats or mob_respawn.
int rust_mob_respawn_nousers(MOB*);
static inline int mob_respawn_nousers(MOB* m) { return rust_mob_respawn_nousers(m); }
int rust_mob_respawn(MOB*);
static inline int mob_respawn(MOB* m) { return rust_mob_respawn(m); }

// Ported movement / timer functions (Rust implementations)
int rust_mob_flushmagic(MOB*);
int rust_move_mob(MOB*);
int rust_move_mob_ignore_object(MOB*);
int rust_moveghost_mob(MOB*);
int rust_move_mob_intent(MOB*, struct block_list*);

static inline int mob_flushmagic(MOB* m)          { return rust_mob_flushmagic(m); }
static inline int move_mob(MOB* m)                { return rust_move_mob(m); }
static inline int move_mob_ignore_object(MOB* m)  { return rust_move_mob_ignore_object(m); }
static inline int moveghost_mob(MOB* m)           { return rust_moveghost_mob(m); }
static inline int move_mob_intent(MOB* m, struct block_list* b) { return rust_move_mob_intent(m, b); }

// Stubs — no longer called from C (timer tick handled by rust_mob_timer_spawns)
static inline int mob_duratimer(MOB* m)           { (void)m; return 0; }
static inline int mob_secondduratimer(MOB* m)     { (void)m; return 0; }
static inline int mob_thirdduratimer(MOB* m)      { (void)m; return 0; }
static inline int mob_fourthduratimer(MOB* m)     { (void)m; return 0; }
static inline int mob_move2(MOB* m, int x, int y, int s) { (void)m;(void)x;(void)y;(void)s; return 0; }
