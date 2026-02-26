#pragma once

#include <stdarg.h>
#include <stdbool.h>
#include <lua.h>
#include <luajit.h>

#include "class_db.h"
#include "map_server.h"

/* Rust-provided non-variadic entry points */
extern void  rust_sl_init(void);
extern void  rust_sl_fixmem(void);
extern int   rust_sl_reload(void);
extern int   rust_sl_luasize(void *user);
extern int   rust_sl_doscript_blargs_vec(const char *root, const char *method,
                                          int nargs, struct block_list **args);
extern int   rust_sl_doscript_strings_vec(const char *root, const char *method,
                                           int nargs, const char **args);
extern int   rust_sl_doscript_stackargs(const char *root, const char *method, int nargs);
extern int   rust_sl_updatepeople(struct block_list *bl, void *ap);
extern void  rust_sl_resumemenu(unsigned int id, void *sd);
extern void  rust_sl_resumemenuseq(unsigned int id, int choice, void *sd);
extern void  rust_sl_resumeinputseq(unsigned int id, char *input, void *sd);
extern void  rust_sl_resumedialog(unsigned int id, void *sd);
extern void  rust_sl_resumebuy(char *items, void *sd);
extern void  rust_sl_resumeinput(char *tag, char *input, void *sd);
extern void  rust_sl_resumesell(unsigned int id, void *sd);
extern void  rust_sl_async_freeco(void *user);
extern void  rust_sl_exec(void *user, char *code);
extern void *sl_gstate;

#define sl_init()          rust_sl_init()
#define sl_fixmem()        rust_sl_fixmem()
#define sl_luasize(u)      rust_sl_luasize(u)

static inline int sl_reload(lua_State *L) {
    (void)L; return rust_sl_reload();
}

extern int   sl_doscript_blargs(char *root, const char *method, int nargs, ...);
extern int   sl_doscript_strings(char *root, const char *method, int nargs, ...);

#define sl_doscript_stackargs(r,m,n)   rust_sl_doscript_stackargs(r,m,n)
extern int   sl_updatepeople(struct block_list *bl, void *ap);
#define sl_resumemenu(id, sd)          rust_sl_resumemenu(id, sd)
#define sl_resumemenuseq(id,ch,sd)     rust_sl_resumemenuseq(id,ch,sd)
#define sl_resumeinputseq(id,inp,sd)   rust_sl_resumeinputseq(id,inp,sd)
#define sl_resumedialog(id, sd)        rust_sl_resumedialog(id, sd)
#define sl_resumebuy(items, sd)        rust_sl_resumebuy(items, sd)
#define sl_resumeinput(tag, inp, sd)   rust_sl_resumeinput(tag, inp, sd)
#define sl_resumesell(id, sd)          rust_sl_resumesell(id, sd)
#define sl_async_freeco(u)             rust_sl_async_freeco(u)
#define sl_doscript_simple(root,method,bl) sl_doscript_blargs(root, method, 1, bl)
#define sl_runfunc(r,bl)               /* no-op until Phase 3 */
#define sl_exec(u,c)                   rust_sl_exec(u,c)

/* pcl_* / mobl_* / npcl_* / fll_* declarations removed: scripting.c is now Rust */
