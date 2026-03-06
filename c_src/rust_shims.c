/*
 * rust_shims.c — C shim wrappers for symbols that Rust extern "C" blocks reference
 * but that are macros or static-inline functions in the C headers.
 *
 * Compile WITHOUT including item_db.h or pc.h to avoid the static-inline
 * redefinition conflicts. Use only forward declarations for what we need.
 *
 * For read_pass we need the MapData layout; we include map_server.h but
 * #undef the read_pass macro immediately after.
 */

/*
 * NOTE: We intentionally do NOT include mmo.h / item_db.h / pc.h / scripting.h
 * here because those headers contain static-inline definitions of the same
 * function names we're defining below, causing redefinition errors.
 * Instead, we use void* for opaque pointer args and forward-declare the
 * rust_* functions we delegate to.
 */

/* ── Forward declarations: rust_* symbols from libyuri.a ────────────────── */

extern void  rust_sl_async_freeco(void *user);
extern char *rust_itemdb_text(unsigned int id);
extern int   rust_itemdb_icon(unsigned int id);
extern int   rust_itemdb_iconcolor(unsigned int id);
extern int   rust_itemdb_exchangeable(unsigned int id);
extern int   rust_pc_additemnolog(void *sd, void *fl);
extern char *rust_itemdb_name(unsigned int id);
extern int   rust_itemdb_type(unsigned int id);
extern int   rust_itemdb_dura(unsigned int id);
extern int   rust_itemdb_droppable(unsigned int id);
extern int   rust_pc_additem(void *sd, void *fl);
extern int   rust_pc_delitem(void *sd, int id, int amount, int type);
extern int   rust_pc_readglobalreg(void *sd, const char *reg);
extern int   rust_pc_isinvenspace(void *sd, int id, int owner, const char *engrave,
                                  unsigned int cl, unsigned int clc,
                                  unsigned int ci, unsigned int cic);
extern char *rust_classdb_name(int id, int rank);

/* ── item_db wrappers ────────────────────────────────────────────────────── */
char *itemdb_text(unsigned int id)         { return rust_itemdb_text(id); }
int   itemdb_icon(unsigned int id)         { return rust_itemdb_icon(id); }
int   itemdb_iconcolor(unsigned int id)    { return rust_itemdb_iconcolor(id); }
int   itemdb_exchangeable(unsigned int id) { return rust_itemdb_exchangeable(id); }

/* ── pc wrappers ─────────────────────────────────────────────────────────── */
int pc_additemnolog(void *sd, void *fl)    { return rust_pc_additemnolog(sd, fl); }
int pc_additem(void *sd, void *fl)         { return rust_pc_additem(sd, fl); }
int pc_delitem(void *sd, int id, int amt, int t) { return rust_pc_delitem(sd, id, amt, t); }
int pc_readglobalreg(void *sd, const char *reg) { return rust_pc_readglobalreg(sd, reg); }
int pc_isinvenspace(void *sd, int id, int owner, const char *engrave,
                    unsigned int cl, unsigned int clc, unsigned int ci, unsigned int cic) {
    return rust_pc_isinvenspace(sd, id, owner, engrave, cl, clc, ci, cic);
}

/* ── item_db wrappers (part 2) ───────────────────────────────────────────── */
char *itemdb_name(unsigned int id)         { return rust_itemdb_name(id); }
int   itemdb_type(unsigned int id)         { return rust_itemdb_type(id); }
int   itemdb_dura(unsigned int id)         { return rust_itemdb_dura(id); }
int   itemdb_droppable(unsigned int id)    { return rust_itemdb_droppable(id); }

/* ── class_db wrappers ───────────────────────────────────────────────────── */
char *classdb_name(int id, int rank)       { return rust_classdb_name(id, rank); }

/* ── scripting macros ────────────────────────────────────────────────────── */

/* sl_doscript_simple: C macro → sl_doscript_blargs(root, method, 1, bl)
 * The macro is used by C code directly; this shim is needed by Rust extern "C"
 * callers (e.g. src/game/map_parse/combat.rs) that link against the symbol. */
extern int sl_doscript_blargs(const char *root, const char *method, int nargs, ...);
int sl_doscript_simple(const char *root, const char *method, void *bl) {
    return sl_doscript_blargs(root, method, 1, bl);
}

/* sl_async_freeco: C macro → rust_sl_async_freeco(u)
 * The macro is used by C code directly; this shim is needed by Rust extern "C"
 * callers (e.g. src/game/pc.rs, src/game/map_parse/chat.rs) that link against
 * the symbol name "sl_async_freeco" rather than "rust_sl_async_freeco". */
void sl_async_freeco(void *user) {
    rust_sl_async_freeco(user);
}

/* ── read_pass: C macro → map[m].pass[(x) + (y)*map[m].xs] ─────────────── */
/*
 * read_pass is a macro in map_server.h. We need the MapData struct to
 * implement it. Include map_server.h here (it's safe because it doesn't
 * define any of the functions above), then #undef read_pass and define
 * the real function.
 *
 * map_server.h includes mmo.h which includes item_db.h (with static inlines
 * for itemdb_text etc.), but those are already defined above in this TU
 * as non-static. GCC allows a prior non-static definition to coexist with a
 * later static-inline declaration in the same TU — actually it does NOT.
 *
 * Workaround: define ITEM_DB_NO_INLINE before including mmo.h to suppress
 * the static-inline bodies. But item_db.h doesn't have that guard.
 *
 * Alternative: compute map[m].pass directly using a minimal struct layout
 * without including map_server.h. But that's brittle.
 *
 * Safest alternative: separate the read_pass shim into a different TU that
 * includes the full headers without defining any of the above functions.
 * We achieve this by compiling rust_shims_map.c separately (see build.rs).
 */
/* read_pass is provided in rust_shims_map.c to avoid header conflicts. */
