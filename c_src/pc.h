#pragma once

#include <stdarg.h>

#include "map_server.h"
#include "mmo.h"

/* pc game logic — implemented in Rust (src/game/pc.rs) */

/* ── timer callbacks ───────────────────────────────────────────────────────── */
int rust_pc_item_timer(int id, int none);
int rust_pc_savetimer(int id, int none);
int rust_pc_castusetimer(int id, int none);
int rust_pc_afktimer(int id, int none);
int rust_pc_timer(int id, int none);
int rust_pc_scripttimer(int id, int none);
int rust_pc_atkspeed(int id, int none);
int rust_pc_disptimertick(int id, int none);
int rust_pc_sendpong(int id, int none);  /* timer callback; C still defines pc_sendpong in map_parse.c */

static inline int pc_item_timer(int id, int n)      { return rust_pc_item_timer(id, n); }
static inline int pc_timer(int id, int n)           { return rust_pc_timer(id, n); }
static inline int pc_scripttimer(int id, int n)     { return rust_pc_scripttimer(id, n); }
static inline int pc_atkspeed(int id, int n)        { return rust_pc_atkspeed(id, n); }
static inline int pc_disptimertick(int id, int n)   { return rust_pc_disptimertick(id, n); }

/* ── duration timers (bl_* timer callbacks) ────────────────────────────────── */
int rust_bl_duratimer(int id, int none);
int rust_bl_secondduratimer(int id, int none);
int rust_bl_thirdduratimer(int id, int none);
int rust_bl_fourthduratimer(int id, int none);
int rust_bl_fifthduratimer(int id, int none);
int rust_bl_aethertimer(int id, int none);

static inline int bl_duratimer(int id, int n)       { return rust_bl_duratimer(id, n); }
static inline int bl_secondduratimer(int id, int n) { return rust_bl_secondduratimer(id, n); }
static inline int bl_thirdduratimer(int id, int n)  { return rust_bl_thirdduratimer(id, n); }
static inline int bl_fourthduratimer(int id, int n) { return rust_bl_fourthduratimer(id, n); }
static inline int bl_fifthduratimer(int id, int n)  { return rust_bl_fifthduratimer(id, n); }
static inline int bl_aethertimer(int id, int n)     { return rust_bl_aethertimer(id, n); }

/* ── timer start/stop ──────────────────────────────────────────────────────── */
int rust_pc_starttimer(USER *sd);
int rust_pc_stoptimer(USER *sd);

static inline int pc_starttimer(USER *sd) { return rust_pc_starttimer(sd); }
static inline int pc_stoptimer(USER *sd)  { return rust_pc_stoptimer(sd); }

/* ── mp / level / exp ──────────────────────────────────────────────────────── */
int rust_pc_requestmp(USER *sd);
int rust_pc_checklevel(USER *sd);
int rust_pc_givexp(USER *sd, unsigned int exp, unsigned int xprate);

static inline int pc_requestmp(USER *sd)                             { return rust_pc_requestmp(sd); }
static inline int pc_checklevel(USER *sd)                            { return rust_pc_checklevel(sd); }
static inline int pc_givexp(USER *sd, unsigned int e, unsigned int r){ return rust_pc_givexp(sd, e, r); }

/* ── stat calculation ──────────────────────────────────────────────────────── */
int rust_pc_calcstat(USER *sd);
float rust_pc_calcdamage(USER *sd);
int rust_pc_calcdam(USER *sd);

static inline int   pc_calcstat(USER *sd)   { return rust_pc_calcstat(sd); }
static inline float pc_calcdamage(USER *sd) { return rust_pc_calcdamage(sd); }
static inline int   pc_calcdam(USER *sd)    { return rust_pc_calcdam(sd); }

/* ── registry (local) ──────────────────────────────────────────────────────── */
int   rust_pc_readreg(USER *sd, int reg);
int   rust_pc_setreg(USER *sd, int reg, int val);
char *rust_pc_readregstr(USER *sd, int reg);
int   rust_pc_setregstr(USER *sd, int reg, char *str);

static inline int   pc_readreg(USER *sd, int r)          { return rust_pc_readreg(sd, r); }
static inline int   pc_setreg(USER *sd, int r, int v)    { return rust_pc_setreg(sd, r, v); }
static inline char *pc_readregstr(USER *sd, int r)       { return rust_pc_readregstr(sd, r); }
static inline int   pc_setregstr(USER *sd, int r, char *s){ return rust_pc_setregstr(sd, r, s); }

/* ── registry (global string) ──────────────────────────────────────────────── */
char *rust_pc_readglobalregstring(USER *sd, const char *reg);
int   rust_pc_setglobalregstring(USER *sd, const char *reg, const char *val);

static inline char *pc_readglobalregstring(USER *sd, const char *r)         { return rust_pc_readglobalregstring(sd, r); }
static inline int   pc_setglobalregstring(USER *sd, const char *r, const char *v){ return rust_pc_setglobalregstring(sd, r, v); }

/* ── registry (global int) ─────────────────────────────────────────────────── */
int rust_pc_readglobalreg(USER *sd, const char *reg);
int rust_pc_setglobalreg(USER *sd, const char *reg, unsigned long val);

static inline int pc_readglobalreg(USER *sd, const char *r)            { return rust_pc_readglobalreg(sd, r); }
static inline int pc_setglobalreg(USER *sd, const char *r, unsigned long v){ return rust_pc_setglobalreg(sd, r, v); }

/* ── params ────────────────────────────────────────────────────────────────── */
int rust_pc_readparam(USER *sd, int type);
int rust_pc_setparam(USER *sd, int type, int val);

static inline int pc_readparam(USER *sd, int t)        { return rust_pc_readparam(sd, t); }
static inline int pc_setparam(USER *sd, int t, int v)  { return rust_pc_setparam(sd, t, v); }

/* ── account registry ──────────────────────────────────────────────────────── */
int rust_pc_readacctreg(USER *sd, const char *reg);
int rust_pc_setacctreg(USER *sd, const char *reg, int val);
int rust_pc_saveacctregistry(USER *sd, int flag);

static inline int pc_readacctreg(USER *sd, const char *r)       { return rust_pc_readacctreg(sd, r); }
static inline int pc_setacctreg(USER *sd, const char *r, int v) { return rust_pc_setacctreg(sd, r, v); }
static inline int pc_saveacctregistry(USER *sd, int f)          { return rust_pc_saveacctregistry(sd, f); }

/* ── npc / quest registry ──────────────────────────────────────────────────── */
int rust_pc_readnpcintreg(USER *sd, const char *reg);
int rust_pc_setnpcintreg(USER *sd, const char *reg, int val);
int rust_pc_readquestreg(USER *sd, const char *reg);
int rust_pc_setquestreg(USER *sd, const char *reg, int val);

static inline int pc_readnpcintreg(USER *sd, const char *r)        { return rust_pc_readnpcintreg(sd, r); }
static inline int pc_setnpcintreg(USER *sd, const char *r, int v)  { return rust_pc_setnpcintreg(sd, r, v); }
static inline int pc_readquestreg(USER *sd, const char *r)         { return rust_pc_readquestreg(sd, r); }
static inline int pc_setquestreg(USER *sd, const char *r, int v)   { return rust_pc_setquestreg(sd, r, v); }

/* ── inventory space checks ────────────────────────────────────────────────── */
int rust_pc_isinvenspace(USER *sd, int id, int owner, const char *engrave,
                         unsigned int custom_look, unsigned int custom_look_color,
                         unsigned int custom_icon, unsigned int custom_icon_color);
int rust_pc_isinvenitemspace(USER *sd, int num, int id, int owner, char *engrave);

static inline int pc_isinvenspace(USER *sd, int id, int owner, const char *engrave,
                                  unsigned int cl, unsigned int clc,
                                  unsigned int ci, unsigned int cic) {
    return rust_pc_isinvenspace(sd, id, owner, engrave, cl, clc, ci, cic);
}
static inline int pc_isinvenitemspace(USER *sd, int num, int id, int owner, char *engrave) {
    return rust_pc_isinvenitemspace(sd, num, id, owner, engrave);
}

/* ── floor-item helpers ────────────────────────────────────────────────────── */
int rust_pc_dropitemfull(USER *sd, struct item *fl2);
int rust_pc_addtocurrent2(struct block_list *bl, ...);
int rust_pc_addtocurrent(struct block_list *bl, ...);
int rust_pc_npc_drop(struct block_list *bl, ...);

static inline int pc_dropitemfull(USER *sd, struct item *fl2)          { return rust_pc_dropitemfull(sd, fl2); }
static inline int pc_addtocurrent(struct block_list *bl, va_list ap)   { return rust_pc_addtocurrent(bl, ap); }

/* ── item add/del/use ──────────────────────────────────────────────────────── */
int rust_pc_additem(USER *sd, struct item *fl);
int rust_pc_additemnolog(USER *sd, struct item *fl);
int rust_pc_delitem(USER *sd, int id, int amount, int type);
int rust_pc_dropitemmap(USER *sd, int id, int type);
int rust_pc_changeitem(USER *sd, int id1, int id2);
int rust_pc_useitem(USER *sd, int id);
int rust_pc_getitemmap(USER *sd, int id);
int rust_pc_getitemsaround(USER *sd);
int rust_pc_handle_item(int a, int b);
int rust_pc_handle_item_sub(struct block_list *bl, ...);
int rust_pc_runfloor_sub(USER *sd);

static inline int pc_additem(USER *sd, struct item *fl)        { return rust_pc_additem(sd, fl); }
static inline int pc_additemnolog(USER *sd, struct item *fl)   { return rust_pc_additemnolog(sd, fl); }
static inline int pc_delitem(USER *sd, int id, int amt, int t) { return rust_pc_delitem(sd, id, amt, t); }
static inline int pc_dropitemmap(USER *sd, int id, int t)      { return rust_pc_dropitemmap(sd, id, t); }
static inline int pc_changeitem(USER *sd, int id1, int id2)    { return rust_pc_changeitem(sd, id1, id2); }
static inline int pc_useitem(USER *sd, int id)                 { return rust_pc_useitem(sd, id); }
static inline int pc_getitemmap(USER *sd, int id)              { return rust_pc_getitemmap(sd, id); }
static inline int pc_getitemsaround(USER *sd)                  { return rust_pc_getitemsaround(sd); }
static inline int pc_handle_item(int a, int b)                 { return rust_pc_handle_item(a, b); }
static inline int pc_handle_item_sub(struct block_list *bl, va_list ap) { return rust_pc_handle_item_sub(bl, ap); }
static inline int pc_runfloor_sub(USER *sd)                    { return rust_pc_runfloor_sub(sd); }

/* ── load item/equip display ───────────────────────────────────────────────── */
int rust_pc_loaditem(USER *sd);
int rust_pc_loadequip(USER *sd);
int rust_pc_loaditemrealname(USER *sd);
int rust_pc_loadequiprealname(USER *sd);

static inline int pc_loaditem(USER *sd)         { return rust_pc_loaditem(sd); }
static inline int pc_loadequip(USER *sd)        { return rust_pc_loadequip(sd); }
static inline int pc_loaditemrealname(USER *sd) { return rust_pc_loaditemrealname(sd); }
static inline int pc_loadequiprealname(USER *sd){ return rust_pc_loadequiprealname(sd); }

/* ── equip ─────────────────────────────────────────────────────────────────── */
int rust_pc_isequip(USER *sd, int type);
int rust_pc_canequipitem(USER *sd, int id);
int rust_pc_canequipstats(USER *sd, unsigned int id);
int rust_pc_equipitem(USER *sd, int id);
int rust_pc_equipscript(USER *sd);
int rust_pc_unequip(USER *sd, int type);
int rust_pc_unequipscript(USER *sd);
int rust_pc_getitemscript(USER *sd, int id);

static inline int pc_isequip(USER *sd, int t)          { return rust_pc_isequip(sd, t); }
static inline int pc_canequipitem(USER *sd, int id)    { return rust_pc_canequipitem(sd, id); }
static inline int pc_canequipstats(USER *sd, unsigned int id) { return rust_pc_canequipstats(sd, id); }
static inline int pc_equipitem(USER *sd, int id)       { return rust_pc_equipitem(sd, id); }
static inline int pc_equipscript(USER *sd)             { return rust_pc_equipscript(sd); }
static inline int pc_unequip(USER *sd, int t)          { return rust_pc_unequip(sd, t); }
static inline int pc_unequipscript(USER *sd)           { return rust_pc_unequipscript(sd); }
static inline int pc_getitemscript(USER *sd, int id)   { return rust_pc_getitemscript(sd, id); }

/* ── position / warp ───────────────────────────────────────────────────────── */
int rust_pc_setpos(USER *sd, int m, int x, int y);
int rust_pc_warp(USER *sd, int m, int x, int y);

static inline int pc_setpos(USER *sd, int m, int x, int y) { return rust_pc_setpos(sd, m, x, y); }
static inline int pc_warp(USER *sd, int m, int x, int y)   { return rust_pc_warp(sd, m, x, y); }

/* ── magic / aether ────────────────────────────────────────────────────────── */
int rust_pc_loadmagic(USER *sd);
int rust_pc_magic_startup(USER *sd);
int rust_pc_reload_aether(USER *sd);

static inline int pc_loadmagic(USER *sd)     { return rust_pc_loadmagic(sd); }
static inline int pc_magic_startup(USER *sd) { return rust_pc_magic_startup(sd); }
static inline int pc_reload_aether(USER *sd) { return rust_pc_reload_aether(sd); }

/* ── death / resurrection / combat state ──────────────────────────────────── */
int rust_pc_die(USER *sd);
int rust_pc_diescript(USER *sd);
int rust_pc_res(USER *sd);
int rust_pc_uncast(USER *sd);
int rust_pc_checkformail(USER *sd);

static inline int pc_die(USER *sd)         { return rust_pc_die(sd); }
static inline int pc_diescript(USER *sd)   { return rust_pc_diescript(sd); }
static inline int pc_res(USER *sd)         { return rust_pc_res(sd); }
static inline int pc_uncast(USER *sd)      { return rust_pc_uncast(sd); }
static inline int pc_checkformail(USER *sd){ return rust_pc_checkformail(sd); }

/* ── C helpers that stay in C (pc.c / map_parse.c) ──────────────────────────── */
int pc_heal(USER *sd, int hp, int mp, int caster);
int pc_healing(int id, int none);
int pc_sendpong(int id, int none);  /* defined in map_parse.c */
