/* sl_compat.c — real C symbols for scripting dispatch.
 *
 * These provide linkable symbols so Rust extern "C" declarations in
 * npc.rs / mob.rs can resolve at link time.  The static inline versions
 * in scripting.h are compiled away and never produce symbols.
 */
#include <arpa/inet.h>
#include <math.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <zlib.h>
#include "scripting.h"
#include "board_db.h"
#include "core.h"
#include "net_crypt.h"
#include "session.h"
#include "map_parse.h"
#include "map_server.h"
#include "clan_db.h"
#include "config.h"
#include "db_mysql.h"
#include "mob.h"
#include "npc.h"
#include "pc.h"
#include "item_db.h"
#include "class_db.h"
#include "map_char.h"
#include "magic_db.h"
#include "showmsg.h"
#include "timer.h"

/* -------------------------------------------------------------------------
 * sl_map_isloaded — thin C wrapper around the map_isloaded macro.
 * Called from Rust (src/game/map_char.rs) which cannot expand C macros.
 */
int sl_map_isloaded(int m) { return map_isloaded(m); }

/* -------------------------------------------------------------------------
 * Functions previously in scripting.c that are referenced as callbacks or
 * used by sl_g_* wrappers.
 * --------------------------------------------------------------------- */

int sl_throw(struct block_list *bl, va_list ap) {
    USER *sd = NULL;
    nullpo_ret(0, sd = (USER *)bl);
    char *buf = va_arg(ap, char *);
    int len = va_arg(ap, int);
    if (!rust_session_exists(sd->fd) || rust_session_get_eof(sd->fd)) {
        rust_session_set_eof(sd->fd, 8);
        return 0;
    }
    WFIFOHEAD(sd->fd, len);
    memcpy(WFIFOP(sd->fd, 0), buf, len);
    WFIFOSET(sd->fd, encrypt(sd->fd));
    return 0;
}

/* sl_updatepeople is provided as a Rust #[no_mangle] symbol in ffi/scripting.rs */

int sl_doscript_blargs(const char *root, const char *method, int nargs, ...) {
    struct block_list *args[16] = {0};
    va_list ap; va_start(ap, nargs);
    for (int i = 0; i < nargs && i < 16; i++)
        args[i] = va_arg(ap, struct block_list *);
    va_end(ap);
    return rust_sl_doscript_blargs_vec(root, method, nargs, args);
}

int sl_doscript_strings(const char *root, const char *method, int nargs, ...) {
    const char *args[16] = {0};
    va_list ap; va_start(ap, nargs);
    for (int i = 0; i < nargs && i < 16; i++)
        args[i] = va_arg(ap, const char *);
    va_end(ap);
    return rust_sl_doscript_strings_vec(root, method, nargs, args);
}

/* Map registry helpers — extract map index from USER* so Rust can call without
 * knowing the block_list struct layout. */
int map_readglobalreg_sd(void *sd, const char *attrname) {
    return map_readglobalreg(((USER *)sd)->bl.m, attrname);
}

int map_setglobalreg_sd(void *sd, const char *attrname, int val) {
    return map_setglobalreg(((USER *)sd)->bl.m, attrname, val);
}

/* -------------------------------------------------------------------------
 * sl_globals — typed wrappers used by globals.rs Lua bindings.
 * --------------------------------------------------------------------- */

/* --- Real-time helpers --- */
// sl_g_setweather — ported to src/game/scripting/map_globals.rs
// sl_g_setweatherm — ported to src/game/scripting/map_globals.rs


/* --- SetMap (load map file + configure) --- */
int sl_g_setmap(int m, const char *mapfile, const char *title,
                int bgm, int bgmtype, int pvp, int spell,
                unsigned char light, int weather,
                int sweeptime, int cantalk, int show_ghosts,
                int region, int indoor, int warpout,
                int bind, int reqlvl, int reqvita, int reqmana) {
    unsigned short buff;
    unsigned int pos = 0;
    int i, old_blockcount;
    FILE *fp;
    if (!mapfile) return -1;
    fp = fopen(mapfile, "rb");
    if (!fp) { printf("MAP_ERR: Map file not found (%s).\n", mapfile); return -1; }
    old_blockcount = map[m].bxs * map[m].bys;
    if (title) { strncpy(map[m].title, title, sizeof(map[m].title) - 1); map[m].title[sizeof(map[m].title) - 1] = '\0'; }
    map[m].bgm = bgm; map[m].bgmtype = bgmtype;
    map[m].pvp = pvp; map[m].spell = spell;
    map[m].light = light; map[m].weather = weather;
    map[m].sweeptime = sweeptime; map[m].cantalk = cantalk;
    map[m].show_ghosts = show_ghosts; map[m].region = region;
    map[m].indoor = indoor; map[m].warpout = warpout;
    map[m].bind = bind; map[m].reqlvl = reqlvl;
    map[m].reqvita = reqvita; map[m].reqmana = reqmana;
    fread(&buff, 2, 1, fp); map[m].xs = SWAP16(buff);
    fread(&buff, 2, 1, fp); map[m].ys = SWAP16(buff);
    if (map_isloaded(m)) {
        REALLOC(map[m].tile, unsigned short, map[m].xs * map[m].ys);
        REALLOC(map[m].obj,  unsigned short, map[m].xs * map[m].ys);
        REALLOC(map[m].map,  unsigned char,  map[m].xs * map[m].ys);
        REALLOC(map[m].pass, unsigned short, map[m].xs * map[m].ys);
    } else {
        CALLOC(map[m].tile, unsigned short, map[m].xs * map[m].ys);
        CALLOC(map[m].obj,  unsigned short, map[m].xs * map[m].ys);
        CALLOC(map[m].map,  unsigned char,  map[m].xs * map[m].ys);
        CALLOC(map[m].pass, unsigned short, map[m].xs * map[m].ys);
    }
    map[m].bxs = (map[m].xs + BLOCK_SIZE - 1) / BLOCK_SIZE;
    map[m].bys = (map[m].ys + BLOCK_SIZE - 1) / BLOCK_SIZE;
    if (map_isloaded(m)) {
        int new_blockcount = map[m].bxs * map[m].bys;
        FREE(map[m].warp);
        CALLOC(map[m].warp,       struct warp_list *,  new_blockcount);
        if (old_blockcount > new_blockcount) {
            for (i = new_blockcount; i < old_blockcount; i++) {
                map[m].block[i] = NULL; map[m].block_mob[i] = NULL;
            }
        }
        REALLOC(map[m].block,     struct block_list *, new_blockcount);
        REALLOC(map[m].block_mob, struct block_list *, new_blockcount);
        if (new_blockcount > old_blockcount) {
            for (i = old_blockcount; i < new_blockcount; i++) {
                map[m].block[i] = NULL; map[m].block_mob[i] = NULL;
            }
        }
    } else {
        CALLOC(map[m].warp,       struct warp_list *,  map[m].bxs * map[m].bys);
        CALLOC(map[m].block,      struct block_list *, map[m].bxs * map[m].bys);
        CALLOC(map[m].block_mob,  struct block_list *, map[m].bxs * map[m].bys);
        CALLOC(map[m].registry,   struct global_reg,   1000);
    }
    {
        size_t total = (size_t)map[m].xs * map[m].ys;
        while (pos < total) {
            if (fread(&buff, 2, 1, fp) != 1) break;
            map[m].tile[pos] = SWAP16(buff);
            if (fread(&buff, 2, 1, fp) != 1) break;
            map[m].pass[pos] = SWAP16(buff);
            if (fread(&buff, 2, 1, fp) != 1) break;
            map[m].obj[pos]  = SWAP16(buff);
            pos++;
        }
    }
    fclose(fp);
    map_loadregistry(m);
    map_foreachinarea((int(*)(struct block_list*, va_list))sl_updatepeople, m, 0, 0, SAMEMAP, BL_PC);
    return 0;
}

/* --- Throw packet --- */
void sl_g_throw(int id, int m, int x, int y, int x2, int y2,
                int icon, int color, int action) {
    char buf[255];
    WBUFB(buf, 0) = 0xAA;
    WBUFW(buf, 1) = SWAP16(0x1B);
    WBUFB(buf, 3) = 0x16;
    WBUFB(buf, 4) = 0x03;
    WBUFL(buf, 5) = SWAP32(id);
    WBUFW(buf, 9) = SWAP16(icon + 49152);
    WBUFB(buf, 11) = color;
    WBUFL(buf, 12) = 0;
    WBUFW(buf, 16) = SWAP16(x); WBUFW(buf, 18) = SWAP16(x);
    WBUFW(buf, 20) = SWAP16(x2); WBUFW(buf, 22) = SWAP16(y2);
    WBUFL(buf, 24) = 0; WBUFB(buf, 28) = action; WBUFB(buf, 29) = 0;
    map_foreachinarea(sl_throw, m, x, y, SAMEAREA, BL_PC, buf, 30);
}

/* --- sendMeta --- */
void sl_g_sendmeta(void) {
    USER *tsd;
    int i;
    for (i = 0; i < fd_max; i++) {
        if (rust_session_exists(i) && !rust_session_get_eof(i) &&
            (tsd = (USER*)rust_session_get_data(i)))
            send_metalist(tsd);
    }
}

// sl_g_getxpforlevel ported to Rust — see globals.rs getXPforLevel

/* -------------------------------------------------------------------------
 * Mob scripting helpers — called from scripting/types/mob.rs.
 * These access MOB and USER fields that Rust cannot safely mirror.
 * --------------------------------------------------------------------- */

/* addHealth: heal mob and dispatch on_healed to the appropriate AI script. */
// ported to Rust — see src/game/mob.rs (sl_mob_addhealth)
// ported to Rust — see src/game/mob.rs (sl_mob_removehealth)
// ported to Rust — see src/game/mob.rs (sl_mob_checkthreat)
// ported to Rust — see src/game/mob.rs (sl_mob_setinddmg)
// ported to Rust — see src/game/mob.rs (sl_mob_setgrpdmg)
// ported to Rust — see src/game/mob.rs (sl_mob_callbase)
// ported to Rust — see src/game/mob.rs (sl_mob_checkmove)
// ported to Rust — see src/game/mob.rs (sl_mob_setduration)
// ported to Rust — see src/game/mob.rs (sl_mob_flushduration)
// ported to Rust — see src/game/mob.rs (sl_mob_flushdurationnouncast)

// ─── USER coroutine field accessors (for async_coro.rs) ──────────────────────
// Rust cannot safely compute C struct field offsets at compile time, so we
// expose thin wrappers that read/write USER->coref and USER->coref_container.

// sl_user_coref — ported to pc_accessors.rs
// sl_user_set_coref — ported to pc_accessors.rs
// sl_user_coref_container — ported to pc_accessors.rs
// sl_user_map_id2sd — ported to pc_accessors.rs

// ═══════════════════════════════════════════════════════════════════════════
// PC (USER) field accessors and method wrappers for pc.rs
// All functions take void* to avoid requiring Rust to know USER layout.
// ═══════════════════════════════════════════════════════════════════════════


// ─── Read: computed / indirect fields — ported to src/game/scripting/pc_accessors.rs ───

// ─── Write: direct field setters — ported to src/game/scripting/pc_accessors.rs ───
// ─── Write: GFX setters — ported to src/game/scripting/pc_accessors.rs ──────────
// sl_pc_set_gfx_name, sl_pc_set_name, sl_pc_set_title, sl_pc_set_clan_title,
// sl_pc_set_afkmessage, sl_pc_set_speech — ported to pc_accessors.rs (bounded_copy).

// ─── Method wrappers ─────────────────────────────────────────────────────────
// Simple methods: thin C wrappers around C game functions.

// addHealth: negative healthscript value heals the player (mirrors pcl_addhealth).
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_addhealth)
// removeHealth: positive healthscript value damages the player (mirrors pcl_removehealth).
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_removehealth)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_freeasync)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_forcesave)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_die)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_resurrect)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_showhealth)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_calcstat)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_sendstatus)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_status)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_warp)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_refresh)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_pickup)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_throwitem)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_forcedrop)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_lock)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_unlock)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_swing)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_respawn)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_sendhealth)

// ─── Task 13: remaining PcObject method wrappers ─────────────────────────────

// ── Movement ─────────────────────────────────────────────────────────────────
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_move)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_lookat)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_minirefresh)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_refreshinventory)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_updateinv)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_checkinvbod)

// ── Equipment ────────────────────────────────────────────────────────────────
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_equip)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_takeoff)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_deductarmor)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_deductweapon)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_deductdura)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_deductduraequip)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_deductdurainv)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_hasequipped)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_removeitemslot)
// hasitem: simple id+amount check (no engrave/owner matching for now)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_hasitem)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_hasspace)

// ── Stats / level ─────────────────────────────────────────────────────────────
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_checklevel)

// ── UI / display ─────────────────────────────────────────────────────────────
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_sendminimap)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_popup)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_guitext)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_sendminitext)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_powerboard)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_showboard)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_showpost)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_changeview)

// ── Social / network ─────────────────────────────────────────────────────────
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_speak)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_sendmail)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_sendurl)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_swingtarget)

// ── Kill registry ─────────────────────────────────────────────────────────────
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_killcount)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_setkillcount)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_flushkills)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_flushallkills)

// ── Threat ────────────────────────────────────────────────────────────────────
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_addthreat)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_setthreat)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_addthreatgeneral)

// ── Spell list ────────────────────────────────────────────────────────────────
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_hasspell)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_addspell)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_removespell)

// ── Duration system ───────────────────────────────────────────────────────────
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_hasduration)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_hasdurationid)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_getduration)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_getdurationid)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_durationamount)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_setduration)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_flushduration)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_flushdurationnouncast)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_refreshdurations)

// ── Aether system ─────────────────────────────────────────────────────────────
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_setaether)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_hasaether)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_getaether)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_flushaether)

// ── Clan / nation ─────────────────────────────────────────────────────────────
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_addclan)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_updatepath)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_updatecountry)

// ── Misc ──────────────────────────────────────────────────────────────────────
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_getcasterid)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_settimer)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_addtime)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_removetime)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_setheroshow)

// ── Legends ───────────────────────────────────────────────────────────────────
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_addlegend)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_haslegend)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_removelegendbyname)
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_removelegendbycolor)

/* -------------------------------------------------------------------------
 * NPC scripting helpers — callable from Rust npc.rs __index methods.
 * -------------------------------------------------------------------------
 */

/* Callback that fills a flat pointer array from map_foreachincell. */
struct sl_bl_collect { struct block_list **ptrs; int count; int max; };
static int sl_getobjectscell_cb(struct block_list *bl, va_list ap) {
    struct sl_bl_collect *col = va_arg(ap, struct sl_bl_collect *);
    if (col->count < col->max) col->ptrs[col->count++] = bl;
    return 0;
}

/* sl_g_getobjectscell — collect up to max_count BL pointers in a cell.
 * Returns the number of pointers stored in out_ptrs.
 * Mirrors bll_getobjects_cell from scripting.c. */
int sl_g_getobjectscell(int m, int x, int y, int type, void **out_ptrs, int max_count) {
    struct sl_bl_collect col = { (struct block_list **)out_ptrs, 0, max_count };
    map_foreachincell(sl_getobjectscell_cb, m, x, y, type, &col);
    return col.count;
}

/* sl_g_getobjectsinmap — collect up to max_count BL pointers across an entire map.
 * Returns the number of pointers stored in out_ptrs.
 * Mirrors bll_getobjects_map from scripting.c. */
int sl_g_getobjectsinmap(int m, int type, void **out_ptrs, int max_count) {
    struct sl_bl_collect col = { (struct block_list **)out_ptrs, 0, max_count };
    map_foreachinarea(sl_getobjectscell_cb, m, 0, 0, SAMEMAP, type, &col);
    return col.count;
}

/* sl_g_addnpc — allocate and register a scripted NPC.
 * Mirrors bll_addnpc from scripting.c.
 * npc_yname may be NULL; falls back to "nothing". */
void sl_g_addnpc(const char *name, int m, int x, int y, int subtype,
                 int timer, int duration, int owner, int movetime,
                 const char *npc_yname) {
    struct npc_data *nd;
    CALLOC(nd, struct npc_data, 1);
    strncpy(nd->name,     name,                              sizeof(nd->name)     - 1);
    strncpy(nd->npc_name, npc_yname ? npc_yname : "nothing", sizeof(nd->npc_name) - 1);
    nd->bl.type        = BL_NPC;
    nd->bl.subtype     = subtype;
    nd->bl.m           = m;
    nd->bl.x           = x;
    nd->bl.y           = y;
    nd->bl.graphic_id  = 0;
    nd->bl.graphic_color = 0;
    nd->bl.id          = npc_get_new_npctempid();
    nd->bl.next        = NULL;
    nd->bl.prev        = NULL;
    nd->actiontime     = timer;
    nd->duration       = duration;
    nd->owner          = owner;
    nd->movetime       = movetime;
    map_addblock(&nd->bl);
    map_addiddb(&nd->bl);
    sl_doscript_blargs(nd->name, "on_spawn", 1, &nd->bl);
}

/* sl_g_sendside — send a side-update packet for bl to nearby players. */
// sl_g_sendside — ported to src/game/scripting/map_globals.rs

/* sl_g_sendanimxy — broadcast an animation at (x,y) around bl's position. */
void sl_g_sendanimxy(void *bl_ptr, int anim, int x, int y, int times) {
    struct block_list *bl = (struct block_list *)bl_ptr;
    if (!bl) return;
    map_foreachinarea(clif_sendanimation_xy, bl->m, bl->x, bl->y, AREA, BL_PC, anim, times, x, y);
}

// sl_g_delete_bl — ported to src/game/scripting/map_globals.rs

/* sl_g_talk — make bl speak to all PCs in the surrounding area. */
void sl_g_talk(void *bl_ptr, int type, const char *msg) {
    struct block_list *bl = (struct block_list *)bl_ptr;
    if (!bl) return;
    map_foreachinarea(clif_speak, bl->m, bl->x, bl->y, AREA, BL_PC, msg, bl, type);
}

// sl_g_getusers — ported to src/game/scripting/map_globals.rs

/* sl_g_getobjectscellwithtraps — like sl_g_getobjectscell but includes trap NPCs. */
int sl_g_getobjectscellwithtraps(int m, int x, int y, int type, void **out_ptrs, int max_count) {
    struct sl_bl_collect col = { (struct block_list **)out_ptrs, 0, max_count };
    map_foreachincellwithtraps(sl_getobjectscell_cb, m, x, y, type, &col);
    return col.count;
}

/* Callback that skips dead mobs and stealthed/state==1 PCs (mirrors bll_getaliveobjects_helper). */
static int sl_getaliveobjectscell_cb(struct block_list *bl, va_list ap) {
    struct sl_bl_collect *col = va_arg(ap, struct sl_bl_collect *);
    if (bl->type == BL_MOB) {
        MOB *mob = (MOB *)bl;
        if (mob->state == MOB_DEAD) return 0;
    }
    if (bl->type == BL_PC) {
        USER *ptr = (USER *)bl;
        if (ptr && ((ptr->optFlags & optFlag_stealth) || ptr->status.state == 1)) return 0;
    }
    if (col->count < col->max) col->ptrs[col->count++] = bl;
    return 0;
}

/* sl_g_getaliveobjectscell — like sl_g_getobjectscell but skips dead mobs and invisible PCs. */
int sl_g_getaliveobjectscell(int m, int x, int y, int type, void **out_ptrs, int max_count) {
    struct sl_bl_collect col = { (struct block_list **)out_ptrs, 0, max_count };
    map_foreachincell(sl_getaliveobjectscell_cb, m, x, y, type, &col);
    return col.count;
}

/* sl_g_getobjectsarea — collect BL pointers within AREA range of bl's position.
 * Mirrors bll_getobjects_area from scripting.c. */
int sl_g_getobjectsarea(void *bl_ptr, int type, void **out_ptrs, int max_count) {
    struct block_list *bl = (struct block_list *)bl_ptr;
    struct sl_bl_collect col = { (struct block_list **)out_ptrs, 0, max_count };
    map_foreachinarea(sl_getobjectscell_cb, bl->m, bl->x, bl->y, AREA, type, &col);
    return col.count;
}

/* sl_g_getaliveobjectsarea — like sl_g_getobjectsarea but skips dead/invisible. */
int sl_g_getaliveobjectsarea(void *bl_ptr, int type, void **out_ptrs, int max_count) {
    struct block_list *bl = (struct block_list *)bl_ptr;
    struct sl_bl_collect col = { (struct block_list **)out_ptrs, 0, max_count };
    map_foreachinarea(sl_getaliveobjectscell_cb, bl->m, bl->x, bl->y, AREA, type, &col);
    return col.count;
}

/* sl_g_getobjectssamemap — collect BL pointers across the whole map.
 * Mirrors bll_getobjects_samemap from scripting.c. */
int sl_g_getobjectssamemap(void *bl_ptr, int type, void **out_ptrs, int max_count) {
    struct block_list *bl = (struct block_list *)bl_ptr;
    struct sl_bl_collect col = { (struct block_list **)out_ptrs, 0, max_count };
    map_foreachinarea(sl_getobjectscell_cb, bl->m, bl->x, bl->y, SAMEMAP, type, &col);
    return col.count;
}

/* sl_g_getaliveobjectssamemap — like sl_g_getobjectssamemap but skips dead/invisible. */
int sl_g_getaliveobjectssamemap(void *bl_ptr, int type, void **out_ptrs, int max_count) {
    struct block_list *bl = (struct block_list *)bl_ptr;
    struct sl_bl_collect col = { (struct block_list **)out_ptrs, 0, max_count };
    map_foreachinarea(sl_getaliveobjectscell_cb, bl->m, bl->x, bl->y, SAMEMAP, type, &col);
    return col.count;
}

// sl_g_getmappvp — ported to src/game/scripting/map_globals.rs
// sl_g_getmaptitle — ported to src/game/scripting/map_globals.rs

/* sl_pc_getpk — returns 1 if sd->pvp[] contains id, else 0. Mirrors pcl_getpk. */
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_getpk)

/* --- PC regen overflow accumulators (float fields) --- */
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_vregenoverflow)
// sl_pc_set_vregenoverflow — ported to pc_accessors.rs
// ported to Rust — see src/game/scripting/pc_accessors.rs (sl_pc_mregenoverflow)
// sl_pc_set_mregenoverflow — ported to pc_accessors.rs

// sl_pc_group_count — ported to pc_accessors.rs
// sl_pc_set_group_count — ported to pc_accessors.rs
// sl_pc_group_on — ported to pc_accessors.rs
// sl_pc_set_group_on — ported to pc_accessors.rs
// sl_pc_group_leader — ported to pc_accessors.rs
// sl_pc_set_group_leader — ported to pc_accessors.rs
// sl_pc_getgroup — ported to pc_accessors.rs

/* ---- Shared block-object methods (Task 5) ---- */

/* sendAnimation — broadcast spell/skill animation to players in AREA around bl.
 * clif_sendanimation(struct block_list *bl, va_list): reads anim, target-bl, times. */
void sl_g_sendanimation(void *bl_ptr, int anim, int times) {
    struct block_list *bl = (struct block_list *)bl_ptr;
    if (!bl) return;
    map_foreachinarea(clif_sendanimation, bl->m, bl->x, bl->y, AREA, BL_PC,
                      anim, bl, times);
}

// sl_g_playsound — ported to src/game/scripting/map_globals.rs

// sl_g_sendaction — ported to src/game/scripting/map_globals.rs

// sl_g_msg — ported to src/game/scripting/map_globals.rs
// sl_fl_delete — ported to src/game/scripting/map_globals.rs

// sl_g_dropitem — ported to src/game/scripting/map_globals.rs
// sl_g_dropitemxy — ported to src/game/scripting/map_globals.rs

// sl_g_objectcanmove — ported to src/game/scripting/map_globals.rs
// sl_g_objectcanmovefrom — ported to src/game/scripting/map_globals.rs

/* repeatAnimation — send animation to all PCs in AREA (mirrors bll_repeatanimation).
 * duration is in ms; divide by 1000 for the wire format. */
void sl_g_repeatanimation(void *bl_ptr, int anim, int duration) {
    struct block_list *bl = (struct block_list *)bl_ptr;
    if (!bl) return;
    if (duration > 0) duration /= 1000;
    map_foreachinarea(clif_sendanimation, bl->m, bl->x, bl->y, AREA, BL_PC,
                      anim, bl, duration);
}

/* selfAnimation — send animation from bl to the single player at target_id.
 * Uses foreachincell on the target's own cell so the va_list handler sees
 * exactly one BL_PC (the target sd). */
void sl_g_selfanimation(void *bl_ptr, int target_id, int anim, int times) {
    struct block_list *bl = (struct block_list *)bl_ptr;
    if (!bl) return;
    USER *sd = map_id2sd((unsigned int)target_id);
    if (!sd) return;
    map_foreachincell(clif_sendanimation, sd->bl.m, sd->bl.x, sd->bl.y, BL_PC,
                      anim, bl, times);
}

/* selfAnimationXY — send XY animation to a single player at target_id.
 * clif_sendanimation_xy reads: anim, times, x, y from va_list. */
void sl_g_selfanimationxy(void *bl_ptr, int target_id, int anim, int x, int y, int times) {
    (void)bl_ptr;
    USER *sd = map_id2sd((unsigned int)target_id);
    if (!sd) return;
    map_foreachincell(clif_sendanimation_xy, sd->bl.m, sd->bl.x, sd->bl.y, BL_PC,
                      anim, times, x, y);
}

/* sendParcel — insert a parcel into the Parcels table for receiver.
 * Mirrors bll_sendparcel in scripting.c; custom look/icon fields default to 0. */
// sl_g_sendparcel — ported to src/game/scripting/map_globals.rs

// sl_g_throwblock — ported to src/game/scripting/map_globals.rs

// sl_g_deliddb — ported to src/game/scripting/map_globals.rs
// sl_g_addpermanentspawn — ported to src/game/scripting/map_globals.rs

/* ---- PC non-dialog methods (Task 7) ---- */

/* --- Inventory --- */

// sl_pc_additem — ported to src/game/scripting/pc_accessors.rs
// sl_pc_getinventoryitem — ported to src/game/scripting/pc_accessors.rs
// sl_pc_getequippeditem_sd — ported to src/game/scripting/pc_accessors.rs
// sl_pc_removeitem — ported to src/game/scripting/pc_accessors.rs
// sl_pc_removeitemdura — ported to src/game/scripting/pc_accessors.rs
// sl_pc_hasitemdura — ported to src/game/scripting/pc_accessors.rs

/* --- Bank --- */

// sl_pc_checkbankitems — ported to src/game/scripting/pc_accessors.rs
// sl_pc_checkbankamounts — ported to src/game/scripting/pc_accessors.rs
// sl_pc_checkbankowners — ported to src/game/scripting/pc_accessors.rs
// sl_pc_checkbankengraves — ported to src/game/scripting/pc_accessors.rs
// sl_pc_bankdeposit — ported to src/game/scripting/pc_accessors.rs
// sl_pc_bankwithdraw — ported to src/game/scripting/pc_accessors.rs
// sl_pc_bankcheckamount — ported to src/game/scripting/pc_accessors.rs
// sl_pc_clanbankdeposit — ported to src/game/scripting/pc_accessors.rs (no-op)
// sl_pc_clanbankwithdraw — ported to src/game/scripting/pc_accessors.rs (no-op)

// sl_pc_getclanitems — ported to src/game/scripting/pc_accessors.rs
// sl_pc_getclanamounts — ported to src/game/scripting/pc_accessors.rs
// sl_pc_checkclankitemamounts — ported to src/game/scripting/pc_accessors.rs

/* --- Spell lists --- */

// sl_pc_getalldurations — ported to src/game/scripting/pc_accessors.rs
// sl_pc_getspells — ported to src/game/scripting/pc_accessors.rs
// sl_pc_getspellnames — ported to src/game/scripting/pc_accessors.rs

// sl_pc_getunknownspells — ported to src/game/scripting/pc_accessors.rs (no-op)

/* --- Legends --- */

// sl_pc_getlegend — ported to src/game/scripting/pc_accessors.rs

/* --- Combat --- */

// sl_pc_givexp — ported to src/game/scripting/pc_accessors.rs

/* updateState — broadcast state packet to nearby players. Mirrors bll_updatestate for PCs. */
void sl_pc_updatestate(void *sd_ptr) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    fprintf(stderr, "[DBG sl_compat] updatestate: id=%u state=%d m=%d x=%d y=%d\n",
            sd->status.id, sd->status.state, sd->bl.m, sd->bl.x, sd->bl.y);
    map_foreachinarea(clif_updatestate, sd->bl.m, sd->bl.x, sd->bl.y, AREA,
                      BL_PC, sd);
}

// sl_pc_addmagic — ported to pc_accessors.rs
// sl_pc_addmanaextend — ported to pc_accessors.rs
// sl_pc_settimevalues — ported to pc_accessors.rs

// sl_pc_setpk — ported to src/game/scripting/pc_accessors.rs

// sl_pc_activespells — ported to src/game/scripting/pc_accessors.rs

// sl_pc_getequippeddura — ported to pc_accessors.rs
// sl_pc_addhealth_extend — ported to pc_accessors.rs
// sl_pc_removehealth_extend — ported to pc_accessors.rs

// sl_pc_addhealth2 — ported to src/game/scripting/pc_accessors.rs

// sl_pc_removehealth_nodmgnum — ported to src/game/scripting/pc_accessors.rs

/* --- Economy --- */

// sl_pc_addgold — ported to pc_accessors.rs
// sl_pc_removegold — ported to pc_accessors.rs
// sl_pc_logbuysell — ported to pc_accessors.rs (no-op)
// sl_pc_calcthrow — ported to pc_accessors.rs (no-op)
// sl_pc_calcrangeddamage — ported to pc_accessors.rs (no-op)
// sl_pc_calcrangedhit — ported to pc_accessors.rs (no-op)

/* --- Misc --- */

// sl_pc_gmmsg — ported to src/game/scripting/pc_accessors.rs
// sl_pc_talkself — ported to src/game/scripting/pc_accessors.rs
// sl_pc_broadcast_sd — ported to src/game/scripting/pc_accessors.rs

// sl_pc_killrank — ported to src/game/scripting/pc_accessors.rs

// sl_pc_getparcel — ported to src/game/scripting/pc_accessors.rs (no-op)
// sl_pc_getparcellist — ported to src/game/scripting/pc_accessors.rs (no-op)

// sl_pc_removeparcel — ported to src/game/scripting/pc_accessors.rs

// sl_pc_expireitem — ported to src/game/scripting/pc_accessors.rs

// sl_pc_addguide — ported to pc_accessors.rs (no-op)
// sl_pc_delguide — ported to pc_accessors.rs (no-op)

// sl_pc_getcreationitems — ported to src/game/scripting/pc_accessors.rs

// sl_pc_getcreationamounts — ported to src/game/scripting/pc_accessors.rs

/* =========================================================================
 * Task 9 — Async dialog send helpers
 *
 * Each function sends the network packet for one dialog type but does NOT
 * yield the coroutine.  Yielding is Task 10's responsibility.
 *
 * Sources:
 *   pcl_input      → clif_input(sd, sd->last_click, dialog, "")
 *   pcl_dialog     → clif_scriptmes(sd, sd->last_click, msg, previous, next)
 *   pcl_inputseq   → clif_inputseq(sd, sd->last_click, t1,t2,t3, opts, n, prev, next)
 *   pcl_menu       → clif_scriptmenuseq(sd, sd->last_click, msg, opts, n, prev, next)
 *   pcl_menuseq    → clif_scriptmenuseq (same function, different Lua method name)
 *   pcl_buy        → clif_buydialog(sd, sd->last_click, dialog, item[], price[], n)
 *   pcl_sell       → clif_selldialog(sd, sd->last_click, dialog, slot[], count)
 *   clif_scriptmenu → used for menustring variant (non-seq menu)
 *   bank / repair  → no corresponding clif_ function exists; implemented as no-ops.
 * ========================================================================= */

// sl_pc_input_send — ported to src/game/scripting/pc_accessors.rs

// sl_pc_dialog_send — ported to src/game/scripting/pc_accessors.rs

// sl_pc_dialogseq_send — ported to src/game/scripting/pc_accessors.rs

// sl_pc_menu_send — ported to src/game/scripting/pc_accessors.rs
// sl_pc_menuseq_send — ported to src/game/scripting/pc_accessors.rs
// sl_pc_menustring_send — ported to src/game/scripting/pc_accessors.rs
// sl_pc_menustring2_send — ported to src/game/scripting/pc_accessors.rs (no-op)

// sl_pc_buy_send — ported to src/game/scripting/pc_accessors.rs
// sl_pc_buydialog_send — ported to src/game/scripting/pc_accessors.rs
// sl_pc_buyextend_send — ported to src/game/scripting/pc_accessors.rs
// sl_pc_sell_send — ported to src/game/scripting/pc_accessors.rs
// sl_pc_sell2_send — ported to src/game/scripting/pc_accessors.rs
// sl_pc_sellextend_send — ported to src/game/scripting/pc_accessors.rs
// sl_pc_showbank_send — ported to src/game/scripting/pc_accessors.rs (no-op)
// sl_pc_showbankadd_send — ported to src/game/scripting/pc_accessors.rs (no-op)
// sl_pc_bankaddmoney_send — ported to src/game/scripting/pc_accessors.rs (no-op)
// sl_pc_bankwithdrawmoney_send — ported to src/game/scripting/pc_accessors.rs (no-op)
// sl_pc_clanshowbank_send — ported to src/game/scripting/pc_accessors.rs (no-op)
// sl_pc_clanshowbankadd_send — ported to src/game/scripting/pc_accessors.rs (no-op)
// sl_pc_clanbankaddmoney_send — ported to src/game/scripting/pc_accessors.rs (no-op)
// sl_pc_clanbankwithdrawmoney_send — ported to src/game/scripting/pc_accessors.rs (no-op)
// sl_pc_clanviewbank_send — ported to src/game/scripting/pc_accessors.rs (no-op)
// sl_pc_repairextend_send — ported to src/game/scripting/pc_accessors.rs (no-op)
// sl_pc_repairall_send — ported to src/game/scripting/pc_accessors.rs (no-op)
// ─── Accessors for Rust client dispatcher — ported to src/game/scripting/pc_accessors.rs ───
// sl_pc_fd, sl_pc_chat_timer, sl_pc_set_chat_timer, sl_pc_attacked, sl_pc_set_attacked,
// sl_pc_loaded, sl_pc_inventory_id — all ported to pc_accessors.rs.
// sl_map_spell — ported to pc_accessors.rs

/* =========================================================================
 * Functions moved from c_src/map_parse.c — map_parse.c deleted.
 * All these functions were active C implementations in map_parse.c.
 * They are moved here (sl_compat.c) because they use C macros, C types
 * (USER*, MOB*, NPC*, SqlStmt*) and C APIs that cannot be used from stable Rust.
 * ========================================================================= */

/* File-scope globals from map_parse.c */
unsigned int groups[MAX_GROUPS][MAX_GROUP_MEMBERS];
int val[32];

int flags[16] = {1,   2,   4,    8,    16,   32,   64,    128,
                 256, 512, 1024, 2048, 4096, 8192, 16386, 32768};

int getclifslotfromequiptype(int equipType) {
  int type;

  switch (equipType) {
    case EQ_WEAP:    type = 0x01; break;
    case EQ_ARMOR:   type = 0x02; break;
    case EQ_SHIELD:  type = 0x03; break;
    case EQ_HELM:    type = 0x04; break;
    case EQ_NECKLACE: type = 0x06; break;
    case EQ_LEFT:    type = 0x07; break;
    case EQ_RIGHT:   type = 0x08; break;
    case EQ_BOOTS:   type = 13;   break;
    case EQ_MANTLE:  type = 14;   break;
    case EQ_COAT:    type = 16;   break;
    case EQ_SUBLEFT:  type = 20;  break;
    case EQ_SUBRIGHT: type = 21;  break;
    case EQ_FACEACC: type = 22;   break;
    case EQ_CROWN:   type = 23;   break;
    default:         type = 0;
  }

  return type;
}

char *replace_str(char *str, char *orig, char *rep) {
  static char buffer[4096];
  char *p;

  if (!(p = strstr(str, orig))) return str;

  strncpy(buffer, str, p - str);
  buffer[p - str] = '\0';
  sprintf(buffer + (p - str), "%s%s", rep, p + strlen(orig));

  return buffer;
}

int CheckProximity(struct point one, struct point two, int radius) {
  int ret = 0;
  if (one.m == two.m)
    if (abs(one.x - two.x) <= radius && abs(one.y - two.y) <= radius) ret = 1;
  return ret;
}

int stringTruncate(char *buffer, int maxLength) {
  if (!buffer || maxLength <= 0 || strlen(buffer) == maxLength) return 0;
  buffer[maxLength] = '\0';
  return 0;
}

int addtokillreg(USER *sd, int mob) {
  for (int x = 0; x < MAX_KILLREG; x++) {
    if (sd->status.killreg[x].mob_id == mob) {
      sd->status.killreg[x].amount++;
      return 0;
    }
  }

  for (int x = 0; x < MAX_KILLREG; x++) {
    if (sd->status.killreg[x].mob_id == 0) {
      sd->status.killreg[x].mob_id = mob;
      sd->status.killreg[x].amount = 1;
      return 0;
    }
  }

  return 0;
}

int pc_sendpong(int id, int none) {
  USER *sd = map_id2sd((unsigned int)id);
  nullpo_ret(1, sd);

  if (sd) {
    if (!rust_session_exists(sd->fd)) {
      rust_session_set_eof(sd->fd, 8);
      return 0;
    }

    WFIFOHEAD(sd->fd, 10);
    WFIFOB(sd->fd, 0) = 0xAA;
    WFIFOW(sd->fd, 1) = SWAP16(0x09);
    WFIFOB(sd->fd, 3) = 0x68;
    WFIFOL(sd->fd, 5) = SWAP32(gettick());
    WFIFOB(sd->fd, 9) = 0x00;
    WFIFOSET(sd->fd, encrypt(sd->fd));

    sd->LastPingTick = gettick();
  }

  return 0;
}

/* clif_getequiptype — ported to src/game/client/visual.rs (Task 16) */

// nexCRCC and crctable removed — nex_crcc ported to src/game/map_parse/movement.rs
/* clif_debug — ported to src/game/client/visual.rs (Task 16) */

/* clif_user_list — ported to src/game/client/visual.rs (Task 16) */

/* clif_getlvlxp — ported to src/game/client/visual.rs (Task 16) */

/* clif_show_ghost — ported to src/game/client/visual.rs (Task 16) */

/* clif_getitemarea — ported to src/game/client/visual.rs (Task 16) */

/* clif_sendweather — ported to src/game/client/visual.rs (Task 16) */

int checkevent_claim(int eventid, int fd, USER *sd) {
  int claim = 0;

  SqlStmt *stmt;
  stmt = SqlStmt_Malloc(sql_handle);
  if (stmt == NULL) { SqlStmt_ShowDebug(stmt); return 0; }
  if (SQL_ERROR == SqlStmt_Prepare(stmt,
                                   "SELECT `EventClaim` FROM `RankingScores` "
                                   "WHERE `EventId` = '%u' AND `ChaId` = '%u'",
                                   eventid, sd->status.id) ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_UINT, &claim, 0, NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return claim;
  }

  if (SQL_SUCCESS != SqlStmt_NextRow(stmt)) {
    claim = 2;
  }

  SqlStmt_Free(stmt);
  return claim;
}

void dateevent_block(int pos, int eventid, int fd, USER *sd) {
  WFIFOB(fd, pos) = 0;
  WFIFOB(fd, pos + 1) = eventid;
  WFIFOB(fd, pos + 2) = 142;
  WFIFOB(fd, pos + 3) = 227;
  retrieveEventDates(eventid, pos, fd);
  WFIFOB(fd, pos + 20) = checkevent_claim(eventid, fd, sd);
}

void filler_block(int pos, int eventid, int fd, USER *sd) {
  int player_score = checkPlayerScore(eventid, sd);
  int player_rank = checkPlayerRank(eventid, sd);

  WFIFOB(fd, pos + 1) = RFIFOB(fd, 7);
  WFIFOB(fd, pos + 2) = 142;
  WFIFOB(fd, pos + 3) = 227;
  WFIFOB(fd, pos + 4) = 1;
  clif_intcheck(player_rank, pos + 8, fd);
  clif_intcheck(player_score, pos + 12, fd);
  WFIFOB(fd, pos + 13) = checkevent_claim(eventid, fd, sd);
}

int gettotalscores(int eventid) {
  int scores;

  SqlStmt *stmt;
  stmt = SqlStmt_Malloc(sql_handle);
  if (stmt == NULL) { SqlStmt_ShowDebug(stmt); return 0; }
  if (SQL_ERROR == SqlStmt_Prepare(stmt,
              "SELECT `ChaId` FROM `RankingScores` WHERE `EventId` = '%u'",
              eventid) ||
      SQL_ERROR == SqlStmt_Execute(stmt)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return 0;
  }
  scores = SqlStmt_NumRows(stmt);
  SqlStmt_Free(stmt);

  return scores;
}

int getevents() {
  int events;
  SqlStmt *stmt;
  stmt = SqlStmt_Malloc(sql_handle);
  if (stmt == NULL) { SqlStmt_ShowDebug(stmt); return 0; }
  if (SQL_ERROR == SqlStmt_Prepare(stmt, "SELECT `EventId` FROM `RankingEvents`") ||
      SQL_ERROR == SqlStmt_Execute(stmt)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return 0;
  }
  events = SqlStmt_NumRows(stmt);
  SqlStmt_Free(stmt);
  return events;
}

int getevent_name(int pos, int fd, USER *sd) {
  char name[40];
  char buf[40];
  int i = 0;

  SqlStmt *stmt;
  stmt = SqlStmt_Malloc(sql_handle);
  if (stmt == NULL) { SqlStmt_ShowDebug(stmt); return 0; }

  if (SQL_ERROR == SqlStmt_Prepare(stmt, "SELECT `EventName` FROM `RankingEvents`") ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_STRING, &name, sizeof(name), NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return 0;
  }

  for (i = 0; (i < SqlStmt_NumRows(stmt)) && (SQL_SUCCESS == SqlStmt_NextRow(stmt)); i++) {
    dateevent_block(pos, i, fd, sd);
    pos += 21;
    sprintf(buf, "%s", name);
    WFIFOB(fd, pos) = strlen(buf);
    pos++;
    strncpy(WFIFOP(fd, pos), buf, strlen(buf));
    pos += strlen(buf);
  }

  return pos;
}

int getevent_playerscores(int eventid, int totalscores, int pos, int fd) {
  char name[16];
  int score;
  int rank;
  char buf[40];
  int offset = RFIFOB(fd, 17) - 10;
  int i = 0;

  SqlStmt *stmt;
  stmt = SqlStmt_Malloc(sql_handle);
  if (stmt == NULL) { SqlStmt_ShowDebug(stmt); return 0; }

  if (totalscores > 10) {
    SqlStmt_Prepare(stmt,
        "SELECT `ChaName`, `Score`, `Rank` FROM `RankingScores` WHERE "
        "`EventId` = '%u' ORDER BY `Rank` ASC LIMIT 10 OFFSET %u",
        eventid, offset);
  } else {
    SqlStmt_Prepare(stmt,
                    "SELECT `ChaName`, `Score`, `Rank` FROM `RankingScores` "
                    "WHERE `EventId` = '%u' ORDER BY `Rank` ASC LIMIT 10",
                    eventid);
  }

  if (SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_STRING, &name, sizeof(name), NULL, NULL) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 1, SQLDT_INT, &score, 0, NULL, NULL) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 2, SQLDT_INT, &rank, 0, NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return 0;
  }

  if (SqlStmt_NumRows(stmt) < 10) {
    WFIFOB(fd, pos - 1) = SqlStmt_NumRows(stmt);
  }

  for (i = 0; (i < SqlStmt_NumRows(stmt)) && (SQL_SUCCESS == SqlStmt_NextRow(stmt)); i++) {
    sprintf(buf, "%s", name);
    WFIFOB(fd, pos) = strlen(buf);
    pos++;
    strncpy(WFIFOP(fd, pos), buf, strlen(buf));
    pos += strlen(buf);
    pos += 3;
    WFIFOB(fd, pos) = rank;
    pos += 4;
    clif_intcheck(score, pos, fd);
    pos++;
  }

  return pos;
}

int clif_parseranking(USER *sd, int fd) {
  WFIFOHEAD(fd, 0);
  WFIFOB(fd, 0) = 0xAA;
  WFIFOB(fd, 1) = 0x02;
  WFIFOB(fd, 3) = 0x7D;
  WFIFOB(fd, 5) = 0x03;
  WFIFOB(fd, 6) = 0;

  int i = 0;
  for (i = 8; i < 1500; i++) { WFIFOB(fd, i) = 0x00; }
  WFIFOB(fd, 7) = getevents();
  int chosen_event = RFIFOB(fd, 7);

  updateRanks(chosen_event);

  int pos = 8;
  pos = getevent_name(pos, fd, sd);
  filler_block(pos, chosen_event, fd, sd);
  pos += 15;
  WFIFOB(fd, pos) = 10;
  int totalscores = gettotalscores(chosen_event);
  pos++;
  pos = getevent_playerscores(chosen_event, totalscores, pos, fd);
  pos += 3;
  WFIFOB(fd, pos) = totalscores;
  pos += 1;

  WFIFOB(fd, 2) = pos - 3;
  WFIFOSET(fd, encrypt(fd));

  return 0;
}

int canusepowerboards(USER *sd) {
  if (sd->status.gm_level) return 1;
  if (!pc_readglobalreg(sd, "carnagehost")) return 0;
  if (sd->bl.m >= 2001 && sd->bl.m <= 2099) return 1;
  return 0;
}

int send_metalist(USER *sd);

int clif_updatestate(struct block_list *bl, va_list ap) {
  char buf[64];
  USER *sd = NULL;
  USER *src_sd = NULL;
  int len = 0;

  nullpo_ret(0, sd = va_arg(ap, USER *));
  nullpo_ret(0, src_sd = (USER *)bl);

  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  WFIFOHEAD(src_sd->fd, 512);
  WFIFOB(src_sd->fd, 0) = 0xAA;
  WFIFOB(src_sd->fd, 3) = 0x1D;
  WFIFOL(src_sd->fd, 5) = SWAP32(sd->bl.id);

  if (sd->status.state == 4) {
    WFIFOB(src_sd->fd, 9) = 1;
    WFIFOB(src_sd->fd, 10) = 15;
    WFIFOB(src_sd->fd, 11) = sd->status.state;
    WFIFOW(src_sd->fd, 12) = SWAP16(sd->disguise + 32768);
    WFIFOB(src_sd->fd, 14) = sd->disguise_color;

    sprintf(buf, "%s", sd->status.name);

    WFIFOB(src_sd->fd, 16) = strlen(buf);
    len += strlen(sd->status.name) + 1;
    strcpy(WFIFOP(src_sd->fd, 17), buf);

    WFIFOW(src_sd->fd, 1) = SWAP16(len + 13);
    WFIFOSET(src_sd->fd, encrypt(src_sd->fd));
  } else {
    WFIFOW(src_sd->fd, 9) = SWAP16(sd->status.sex);

    if ((sd->status.state == 2 || (sd->optFlags & optFlag_stealth)) &&
        sd->bl.id != src_sd->bl.id &&
        (src_sd->status.gm_level || clif_isingroup(src_sd, sd) ||
         (sd->gfx.dye == src_sd->gfx.dye && sd->gfx.dye != 0 && src_sd->gfx.dye != 0))) {
      WFIFOB(src_sd->fd, 11) = 5;
    } else {
      WFIFOB(src_sd->fd, 11) = sd->status.state;
    }

    if ((sd->optFlags & optFlag_stealth) && !sd->status.state && !src_sd->status.gm_level)
      WFIFOB(src_sd->fd, 11) = 2;

    if (sd->status.state == 3) {
      WFIFOW(src_sd->fd, 12) = SWAP16(sd->disguise);
    } else {
      WFIFOW(src_sd->fd, 12) = SWAP16(0);
    }

    WFIFOB(src_sd->fd, 14) = sd->speed;
    WFIFOB(src_sd->fd, 15) = 0;
    WFIFOB(src_sd->fd, 16) = sd->status.face;
    WFIFOB(src_sd->fd, 17) = sd->status.hair;
    WFIFOB(src_sd->fd, 18) = sd->status.hair_color;
    WFIFOB(src_sd->fd, 19) = sd->status.face_color;
    WFIFOB(src_sd->fd, 20) = sd->status.skin_color;

    if (!pc_isequip(sd, EQ_ARMOR)) {
      WFIFOW(src_sd->fd, 21) = SWAP16(sd->status.sex);
    } else {
      if (sd->status.equip[EQ_ARMOR].customLook != 0) {
        WFIFOW(src_sd->fd, 21) = SWAP16(sd->status.equip[EQ_ARMOR].customLook);
      } else {
        WFIFOW(src_sd->fd, 21) = SWAP16(itemdb_look(pc_isequip(sd, EQ_ARMOR)));
      }
      if (sd->status.armor_color > 0) {
        WFIFOB(src_sd->fd, 23) = sd->status.armor_color;
      } else {
        if (sd->status.equip[EQ_ARMOR].customLook != 0) {
          WFIFOB(src_sd->fd, 23) = sd->status.equip[EQ_ARMOR].customLookColor;
        } else {
          WFIFOB(src_sd->fd, 23) = itemdb_lookcolor(pc_isequip(sd, EQ_ARMOR));
        }
      }
    }

    if (pc_isequip(sd, EQ_COAT)) {
      WFIFOW(src_sd->fd, 21) = SWAP16(itemdb_look(pc_isequip(sd, EQ_COAT)));
      if (sd->status.armor_color > 0) {
        WFIFOB(src_sd->fd, 23) = sd->status.armor_color;
      } else {
        WFIFOB(src_sd->fd, 23) = itemdb_lookcolor(pc_isequip(sd, EQ_COAT));
      }
    }

    if (!pc_isequip(sd, EQ_WEAP)) {
      WFIFOW(src_sd->fd, 24) = 0xFFFF;
      WFIFOB(src_sd->fd, 26) = 0x0;
    } else {
      if (sd->status.equip[EQ_WEAP].customLook != 0) {
        WFIFOW(src_sd->fd, 24) = SWAP16(sd->status.equip[EQ_WEAP].customLook);
        WFIFOB(src_sd->fd, 26) = sd->status.equip[EQ_WEAP].customLookColor;
      } else {
        WFIFOW(src_sd->fd, 24) = SWAP16(itemdb_look(pc_isequip(sd, EQ_WEAP)));
        WFIFOB(src_sd->fd, 26) = itemdb_lookcolor(pc_isequip(sd, EQ_WEAP));
      }
    }

    if (!pc_isequip(sd, EQ_SHIELD)) {
      WFIFOW(src_sd->fd, 27) = 0xFFFF;
      WFIFOB(src_sd->fd, 29) = 0;
    } else {
      if (sd->status.equip[EQ_SHIELD].customLook != 0) {
        WFIFOW(src_sd->fd, 27) = SWAP16(sd->status.equip[EQ_SHIELD].customLook);
        WFIFOB(src_sd->fd, 29) = sd->status.equip[EQ_SHIELD].customLookColor;
      } else {
        WFIFOW(src_sd->fd, 27) = SWAP16(itemdb_look(pc_isequip(sd, EQ_SHIELD)));
        WFIFOB(src_sd->fd, 29) = itemdb_lookcolor(pc_isequip(sd, EQ_SHIELD));
      }
    }

    if (!pc_isequip(sd, EQ_HELM) || !(sd->status.settingFlags & FLAG_HELM) ||
        (itemdb_look(pc_isequip(sd, EQ_HELM)) == -1)) {
      WFIFOB(src_sd->fd, 30) = 0;
      WFIFOW(src_sd->fd, 31) = 0xFFFF;
    } else {
      WFIFOB(src_sd->fd, 30) = 1;
      if (sd->status.equip[EQ_HELM].customLook != 0) {
        WFIFOB(src_sd->fd, 31) = sd->status.equip[EQ_HELM].customLook;
        WFIFOB(src_sd->fd, 32) = sd->status.equip[EQ_HELM].customLookColor;
      } else {
        WFIFOB(src_sd->fd, 31) = itemdb_look(pc_isequip(sd, EQ_HELM));
        WFIFOB(src_sd->fd, 32) = itemdb_lookcolor(pc_isequip(sd, EQ_HELM));
      }
    }

    if (!pc_isequip(sd, EQ_FACEACC)) {
      WFIFOW(src_sd->fd, 33) = 0xFFFF;
      WFIFOB(src_sd->fd, 35) = 0x0;
    } else {
      WFIFOW(src_sd->fd, 33) = SWAP16(itemdb_look(pc_isequip(sd, EQ_FACEACC)));
      WFIFOB(src_sd->fd, 35) = itemdb_lookcolor(pc_isequip(sd, EQ_FACEACC));
    }

    if (!pc_isequip(sd, EQ_CROWN)) {
      WFIFOW(src_sd->fd, 36) = 0xFFFF;
      WFIFOB(src_sd->fd, 38) = 0x0;
    } else {
      WFIFOB(src_sd->fd, 30) = 0;
      if (sd->status.equip[EQ_CROWN].customLook != 0) {
        WFIFOW(src_sd->fd, 36) = SWAP16(sd->status.equip[EQ_CROWN].customLook);
        WFIFOB(src_sd->fd, 38) = sd->status.equip[EQ_CROWN].customLookColor;
      } else {
        WFIFOW(src_sd->fd, 36) = SWAP16(itemdb_look(pc_isequip(sd, EQ_CROWN)));
        WFIFOB(src_sd->fd, 38) = itemdb_lookcolor(pc_isequip(sd, EQ_CROWN));
      }
    }

    if (!pc_isequip(sd, EQ_FACEACCTWO)) {
      WFIFOW(src_sd->fd, 39) = 0xFFFF;
      WFIFOB(src_sd->fd, 41) = 0x0;
    } else {
      WFIFOW(src_sd->fd, 39) = SWAP16(itemdb_look(pc_isequip(sd, EQ_FACEACCTWO)));
      WFIFOB(src_sd->fd, 41) = itemdb_lookcolor(pc_isequip(sd, EQ_FACEACCTWO));
    }

    if (!pc_isequip(sd, EQ_MANTLE)) {
      WFIFOW(src_sd->fd, 42) = 0xFFFF;
      WFIFOB(src_sd->fd, 44) = 0xFF;
    } else {
      WFIFOW(src_sd->fd, 42) = SWAP16(itemdb_look(pc_isequip(sd, EQ_MANTLE)));
      WFIFOB(src_sd->fd, 44) = itemdb_lookcolor(pc_isequip(sd, EQ_MANTLE));
    }

    if (!pc_isequip(sd, EQ_NECKLACE) || !(sd->status.settingFlags & FLAG_NECKLACE) ||
        (itemdb_look(pc_isequip(sd, EQ_NECKLACE)) == -1)) {
      WFIFOW(src_sd->fd, 45) = 0xFFFF;
      WFIFOB(src_sd->fd, 47) = 0x0;
    } else {
      WFIFOW(src_sd->fd, 45) = SWAP16(itemdb_look(pc_isequip(sd, EQ_NECKLACE)));
      WFIFOB(src_sd->fd, 47) = itemdb_lookcolor(pc_isequip(sd, EQ_NECKLACE));
    }

    if (!pc_isequip(sd, EQ_BOOTS)) {
      WFIFOW(src_sd->fd, 48) = SWAP16(sd->status.sex);
      WFIFOB(src_sd->fd, 50) = 0x0;
    } else {
      if (sd->status.equip[EQ_BOOTS].customLook != 0) {
        WFIFOW(src_sd->fd, 48) = SWAP16(sd->status.equip[EQ_BOOTS].customLook);
        WFIFOB(src_sd->fd, 50) = sd->status.equip[EQ_BOOTS].customLookColor;
      } else {
        WFIFOW(src_sd->fd, 48) = SWAP16(itemdb_look(pc_isequip(sd, EQ_BOOTS)));
        WFIFOB(src_sd->fd, 50) = itemdb_lookcolor(pc_isequip(sd, EQ_BOOTS));
      }
    }

    WFIFOB(src_sd->fd, 51) = 0;
    WFIFOB(src_sd->fd, 52) = 128;
    WFIFOB(src_sd->fd, 53) = 0;

    if (sd->gfx.dye != 0 && src_sd->gfx.dye != 0 &&
        src_sd->gfx.dye != sd->gfx.dye && sd->status.state == 2) {
      WFIFOB(src_sd->fd, 51) = 0;
    } else {
      if (sd->gfx.dye)
        WFIFOB(src_sd->fd, 51) = sd->gfx.titleColor;
      else
        WFIFOB(src_sd->fd, 51) = 0;
    }

    sprintf(buf, "%s", sd->status.name);
    len = strlen(buf);

    if (src_sd->status.clan == sd->status.clan) {
      if (src_sd->status.clan > 0) {
        if (src_sd->status.id != sd->status.id) {
          WFIFOB(src_sd->fd, 53) = 3;
        }
      }
    }

    if (clif_isingroup(src_sd, sd)) {
      if (sd->status.id != src_sd->status.id) {
        WFIFOB(src_sd->fd, 53) = 2;
      }
    }

    if ((sd->status.state != 5) && (sd->status.state != 2)) {
      WFIFOB(src_sd->fd, 54) = len;
      strcpy(WFIFOP(src_sd->fd, 55), buf);
    } else {
      WFIFOB(src_sd->fd, 54) = 0;
      len = 0;
    }

    if ((sd->status.gm_level && sd->gfx.toggle) || sd->clone) {
      WFIFOB(src_sd->fd, 16) = sd->gfx.face;
      WFIFOB(src_sd->fd, 17) = sd->gfx.hair;
      WFIFOB(src_sd->fd, 18) = sd->gfx.chair;
      WFIFOB(src_sd->fd, 19) = sd->gfx.cface;
      WFIFOB(src_sd->fd, 20) = sd->gfx.cskin;
      WFIFOW(src_sd->fd, 21) = SWAP16(sd->gfx.armor);
      if (sd->gfx.dye > 0) {
        WFIFOB(src_sd->fd, 23) = sd->gfx.dye;
      } else {
        WFIFOB(src_sd->fd, 23) = sd->gfx.carmor;
      }
      WFIFOW(src_sd->fd, 24) = SWAP16(sd->gfx.weapon);
      WFIFOB(src_sd->fd, 26) = sd->gfx.cweapon;
      WFIFOW(src_sd->fd, 27) = SWAP16(sd->gfx.shield);
      WFIFOB(src_sd->fd, 29) = sd->gfx.cshield;

      if (sd->gfx.helm < 255) {
        WFIFOB(src_sd->fd, 30) = 1;
      } else if (sd->gfx.crown < 65535) {
        WFIFOB(src_sd->fd, 30) = 0xFF;
      } else {
        WFIFOB(src_sd->fd, 30) = 0;
      }

      WFIFOB(src_sd->fd, 31) = sd->gfx.helm;
      WFIFOB(src_sd->fd, 32) = sd->gfx.chelm;
      WFIFOW(src_sd->fd, 33) = SWAP16(sd->gfx.faceAcc);
      WFIFOB(src_sd->fd, 35) = sd->gfx.cfaceAcc;
      WFIFOW(src_sd->fd, 36) = SWAP16(sd->gfx.crown);
      WFIFOB(src_sd->fd, 38) = sd->gfx.ccrown;
      WFIFOW(src_sd->fd, 39) = SWAP16(sd->gfx.faceAccT);
      WFIFOB(src_sd->fd, 41) = sd->gfx.cfaceAccT;
      WFIFOW(src_sd->fd, 42) = SWAP16(sd->gfx.mantle);
      WFIFOB(src_sd->fd, 44) = sd->gfx.cmantle;
      WFIFOW(src_sd->fd, 45) = SWAP16(sd->gfx.necklace);
      WFIFOB(src_sd->fd, 47) = sd->gfx.cnecklace;
      WFIFOW(src_sd->fd, 48) = SWAP16(sd->gfx.boots);
      WFIFOB(src_sd->fd, 50) = sd->gfx.cboots;

      len = strlen(sd->gfx.name);
      if ((sd->status.state != 2) && (sd->status.state != 5) &&
          strcasecmp(sd->gfx.name, "")) {
        WFIFOB(src_sd->fd, 52) = len;
        strcpy(WFIFOP(src_sd->fd, 53), sd->gfx.name);
      } else {
        WFIFOB(src_sd->fd, 52) = 0;
        len = 1;
      }
    }

    WFIFOW(src_sd->fd, 1) = SWAP16(len + 55 + 3);
    WFIFOSET(src_sd->fd, encrypt(src_sd->fd));
  }

  if (map[sd->bl.m].show_ghosts) {
    if (sd->status.state == 1 && (src_sd->bl.id != sd->bl.id)) {
      if (src_sd->status.state != 1 && !(src_sd->optFlags & optFlag_ghosts)) {
        WFIFOB(src_sd->fd, 0) = 0xAA;
        WFIFOB(src_sd->fd, 1) = 0x00;
        WFIFOB(src_sd->fd, 2) = 0x06;
        WFIFOB(src_sd->fd, 3) = 0x0E;
        WFIFOB(src_sd->fd, 4) = 0x03;
        WFIFOL(src_sd->fd, 5) = SWAP32(sd->bl.id);
        WFIFOSET(src_sd->fd, encrypt(src_sd->fd));
        return 0;
      } else {
        clif_charspecific(src_sd->bl.id, sd->bl.id);
      }
    }
  }

  return 0;
}

int clif_showboards(USER *sd) {
  int len;
  int x, i;
  int b_count;

  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  WFIFOHEAD(sd->fd, 65535);
  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOB(sd->fd, 3) = 0x31;
  WFIFOB(sd->fd, 4) = 3;
  WFIFOB(sd->fd, 5) = 1;
  WFIFOB(sd->fd, 6) = 13;
  strcpy(WFIFOP(sd->fd, 7), "YuriBoards");
  len = 15;
  b_count = 0;
  for (i = 0; i < 256; i++) {
    for (x = 0; x < 256; x++) {
      if (boarddb_sort(x) == i && boarddb_level(x) <= sd->status.level &&
          boarddb_gmlevel(x) <= sd->status.gm_level &&
          (boarddb_path(x) == sd->status.class || boarddb_path(x) == 0) &&
          (boarddb_clan(x) == sd->status.clan || boarddb_clan(x) == 0)) {
        WFIFOW(sd->fd, len + 6) = SWAP16(x);
        WFIFOB(sd->fd, len + 8) = strlen(boarddb_name(x));
        strcpy(WFIFOP(sd->fd, len + 9), boarddb_name(x));
        len += strlen(boarddb_name(x)) + 3;
        b_count += 1;
        break;
      }
    }
  }
  WFIFOB(sd->fd, 20) = b_count;
  WFIFOW(sd->fd, 1) = SWAP16(len + 3);
  WFIFOSET(sd->fd, encrypt(sd->fd));
  return 0;
}

int clif_clickonplayer(USER *sd, struct block_list *bl) {
  USER *tsd = NULL;
  int len = 0;
  char equip_status[65535];
  char buff[256];
  char buf[255];
  int x, count = 0, equip_len = 0;
  char *nameof = NULL;

  tsd = map_id2sd(bl->id);
  equip_status[0] = '\0';

  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  WFIFOHEAD(sd->fd, 65535);
  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOB(sd->fd, 3) = 0x34;

  if (strlen(tsd->status.title) > 0) {
    WFIFOB(sd->fd, 5) = strlen(tsd->status.title);
    strcpy(WFIFOP(sd->fd, 6), tsd->status.title);
    len += strlen(tsd->status.title) + 1;
  } else {
    WFIFOB(sd->fd, 5) = 0;
    len += 1;
  }

  if (tsd->status.clan > 0) {
    WFIFOB(sd->fd, len + 5) = strlen(clandb_name(tsd->status.clan));
    strcpy(WFIFOP(sd->fd, len + 6), clandb_name(tsd->status.clan));
    len += strlen(clandb_name(tsd->status.clan)) + 1;
  } else {
    WFIFOB(sd->fd, len + 5) = 0;
    len += 1;
  }

  if (strlen(tsd->status.clan_title) > 0) {
    WFIFOB(sd->fd, len + 5) = strlen(tsd->status.clan_title);
    strcpy(WFIFOP(sd->fd, len + 6), tsd->status.clan_title);
    len += strlen(tsd->status.clan_title) + 1;
  } else {
    WFIFOB(sd->fd, len + 5) = 0;
    len += 1;
  }

  if (classdb_name(tsd->status.class, tsd->status.mark)) {
    WFIFOB(sd->fd, len + 5) = strlen(classdb_name(tsd->status.class, tsd->status.mark));
    strcpy(WFIFOP(sd->fd, len + 6), classdb_name(tsd->status.class, tsd->status.mark));
    len += strlen(classdb_name(tsd->status.class, tsd->status.mark)) + 1;
  } else {
    WFIFOB(sd->fd, len + 5) = 0;
    len += 1;
  }

  WFIFOB(sd->fd, len + 5) = strlen(tsd->status.name);
  strcpy(WFIFOP(sd->fd, len + 6), tsd->status.name);
  len += strlen(tsd->status.name);

  WFIFOW(sd->fd, len + 6) = SWAP16(tsd->status.sex);
  WFIFOB(sd->fd, len + 8) = tsd->status.state;
  WFIFOW(sd->fd, len + 9) = SWAP16(0);
  WFIFOB(sd->fd, len + 11) = tsd->speed;

  if (tsd->status.state == 3) {
    WFIFOW(sd->fd, len + 9) = SWAP16(tsd->disguise);
  } else if (tsd->status.state == 4) {
    WFIFOW(sd->fd, len + 9) = SWAP16(tsd->disguise + 32768);
    WFIFOB(sd->fd, len + 11) = tsd->disguise_color;
  }

  WFIFOB(sd->fd, len + 12) = 0;
  WFIFOB(sd->fd, len + 13) = tsd->status.face;
  WFIFOB(sd->fd, len + 14) = tsd->status.hair;
  WFIFOB(sd->fd, len + 15) = tsd->status.hair_color;
  WFIFOB(sd->fd, len + 16) = tsd->status.face_color;
  WFIFOB(sd->fd, len + 17) = tsd->status.skin_color;

  len += 14;

  if (!pc_isequip(tsd, EQ_ARMOR)) {
    WFIFOW(sd->fd, len + 4) = SWAP16(tsd->status.sex);
  } else {
    if (tsd->status.equip[EQ_ARMOR].customLook != 0) {
      WFIFOW(sd->fd, len + 4) = SWAP16(tsd->status.equip[EQ_ARMOR].customLook);
    } else {
      WFIFOW(sd->fd, len + 4) = SWAP16(itemdb_look(pc_isequip(tsd, EQ_ARMOR)));
    }
    if (tsd->status.armor_color > 0) {
      WFIFOB(sd->fd, len + 6) = tsd->status.armor_color;
    } else {
      if (tsd->status.equip[EQ_ARMOR].customLook != 0) {
        WFIFOB(sd->fd, len + 6) = tsd->status.equip[EQ_ARMOR].customLookColor;
      } else {
        WFIFOB(sd->fd, len + 6) = itemdb_lookcolor(pc_isequip(tsd, EQ_ARMOR));
      }
    }
  }
  if (pc_isequip(tsd, EQ_COAT)) {
    WFIFOW(sd->fd, len + 4) = SWAP16(itemdb_look(pc_isequip(tsd, EQ_COAT)));
    if (tsd->status.armor_color > 0) {
      WFIFOB(sd->fd, len + 6) = tsd->status.armor_color;
    } else {
      WFIFOB(sd->fd, len + 6) = itemdb_lookcolor(pc_isequip(tsd, EQ_COAT));
    }
  }

  len += 3;
  if (!pc_isequip(tsd, EQ_WEAP)) {
    WFIFOW(sd->fd, len + 4) = 0xFFFF;
    WFIFOB(sd->fd, len + 6) = 0;
  } else {
    if (tsd->status.equip[EQ_WEAP].customLook != 0) {
      WFIFOW(sd->fd, len + 4) = SWAP16(tsd->status.equip[EQ_WEAP].customLook);
      WFIFOB(sd->fd, len + 6) = tsd->status.equip[EQ_WEAP].customLookColor;
    } else {
      WFIFOW(sd->fd, len + 4) = SWAP16(itemdb_look(pc_isequip(tsd, EQ_WEAP)));
      WFIFOB(sd->fd, len + 6) = itemdb_lookcolor(pc_isequip(tsd, EQ_WEAP));
    }
  }
  len += 3;
  if (!pc_isequip(tsd, EQ_SHIELD)) {
    WFIFOW(sd->fd, len + 4) = 0xFFFF;
    WFIFOB(sd->fd, len + 6) = 0;
  } else {
    if (tsd->status.equip[EQ_SHIELD].customLook != 0) {
      WFIFOW(sd->fd, len + 4) = SWAP16(tsd->status.equip[EQ_SHIELD].customLook);
      WFIFOB(sd->fd, len + 6) = tsd->status.equip[EQ_SHIELD].customLookColor;
    } else {
      WFIFOW(sd->fd, len + 4) = SWAP16(itemdb_look(pc_isequip(tsd, EQ_SHIELD)));
      WFIFOB(sd->fd, len + 6) = itemdb_lookcolor(pc_isequip(tsd, EQ_SHIELD));
    }
  }
  len += 3;
  if (!pc_isequip(tsd, EQ_HELM) || !(tsd->status.settingFlags & FLAG_HELM) ||
      (itemdb_look(pc_isequip(tsd, EQ_HELM)) == -1)) {
    WFIFOB(sd->fd, len + 4) = 0;
    WFIFOW(sd->fd, len + 5) = 0xFFFF;
  } else {
    WFIFOB(sd->fd, len + 4) = 1;
    if (tsd->status.equip[EQ_HELM].customLook != 0) {
      WFIFOB(sd->fd, len + 5) = tsd->status.equip[EQ_HELM].customLook;
      WFIFOB(sd->fd, len + 6) = tsd->status.equip[EQ_HELM].customLookColor;
    } else {
      WFIFOB(sd->fd, len + 5) = itemdb_look(pc_isequip(tsd, EQ_HELM));
      WFIFOB(sd->fd, len + 6) = itemdb_lookcolor(pc_isequip(tsd, EQ_HELM));
    }
  }
  len += 3;
  if (!pc_isequip(tsd, EQ_FACEACC)) {
    WFIFOW(sd->fd, len + 4) = 0xFFFF;
    WFIFOB(sd->fd, len + 6) = 0;
  } else {
    WFIFOW(sd->fd, len + 4) = SWAP16(itemdb_look(pc_isequip(tsd, EQ_FACEACC)));
    WFIFOB(sd->fd, len + 6) = itemdb_lookcolor(pc_isequip(tsd, EQ_FACEACC));
  }
  len += 3;
  if (!pc_isequip(tsd, EQ_CROWN)) {
    WFIFOW(sd->fd, len + 4) = 0xFFFF;
    WFIFOB(sd->fd, len + 6) = 0;
  } else {
    WFIFOB(sd->fd, len) = 0;
    if (tsd->status.equip[EQ_CROWN].customLook != 0) {
      WFIFOW(sd->fd, len + 4) = SWAP16(tsd->status.equip[EQ_CROWN].customLook);
      WFIFOB(sd->fd, len + 6) = tsd->status.equip[EQ_CROWN].customLookColor;
    } else {
      WFIFOW(sd->fd, len + 4) = SWAP16(itemdb_look(pc_isequip(tsd, EQ_CROWN)));
      WFIFOB(sd->fd, len + 6) = itemdb_lookcolor(pc_isequip(tsd, EQ_CROWN));
    }
  }
  len += 3;

  if (!pc_isequip(tsd, EQ_FACEACCTWO)) {
    WFIFOW(sd->fd, len + 4) = 0xFFFF;
    WFIFOB(sd->fd, len + 6) = 0;
  } else {
    WFIFOW(sd->fd, len + 4) = SWAP16(itemdb_look(pc_isequip(tsd, EQ_FACEACCTWO)));
    WFIFOB(sd->fd, len + 6) = itemdb_lookcolor(pc_isequip(tsd, EQ_FACEACCTWO));
  }
  len += 3;

  if (!pc_isequip(tsd, EQ_MANTLE)) {
    WFIFOW(sd->fd, len + 4) = 0xFFFF;
    WFIFOB(sd->fd, len + 6) = 0xFF;
  } else {
    WFIFOW(sd->fd, len + 4) = SWAP16(itemdb_look(pc_isequip(tsd, EQ_MANTLE)));
    WFIFOB(sd->fd, len + 6) = itemdb_lookcolor(pc_isequip(tsd, EQ_MANTLE));
  }
  len += 3;

  if (!pc_isequip(tsd, EQ_NECKLACE) || !(tsd->status.settingFlags & FLAG_NECKLACE) ||
      (itemdb_look(pc_isequip(tsd, EQ_NECKLACE)) == -1)) {
    WFIFOW(sd->fd, len + 4) = 0xFFFF;
    WFIFOB(sd->fd, len + 6) = 0;
  } else {
    WFIFOW(sd->fd, len + 4) = SWAP16(itemdb_look(pc_isequip(tsd, EQ_NECKLACE)));
    WFIFOB(sd->fd, len + 6) = itemdb_lookcolor(pc_isequip(tsd, EQ_NECKLACE));
  }
  len += 3;

  if (!pc_isequip(tsd, EQ_BOOTS)) {
    WFIFOW(sd->fd, len + 4) = SWAP16(tsd->status.sex);
    WFIFOB(sd->fd, len + 6) = 0;
  } else {
    if (tsd->status.equip[EQ_BOOTS].customLook != 0) {
      WFIFOW(sd->fd, len + 4) = SWAP16(tsd->status.equip[EQ_BOOTS].customLook);
      WFIFOB(sd->fd, len + 6) = tsd->status.equip[EQ_BOOTS].customLookColor;
    } else {
      WFIFOW(sd->fd, len + 4) = SWAP16(itemdb_look(pc_isequip(tsd, EQ_BOOTS)));
      WFIFOB(sd->fd, len + 6) = itemdb_lookcolor(pc_isequip(tsd, EQ_BOOTS));
    }
  }
  len += 3;

  for (x = 0; x < 14; x++) {
    if (tsd->status.equip[x].id > 0) {
      if (tsd->status.equip[x].customIcon != 0) {
        WFIFOW(sd->fd, len + 6) = SWAP16(tsd->status.equip[x].customIcon + 49152);
        WFIFOB(sd->fd, len + 8) = tsd->status.equip[x].customIconColor;
      } else {
        WFIFOW(sd->fd, len + 6) = SWAP16(itemdb_icon(tsd->status.equip[x].id));
        WFIFOB(sd->fd, len + 8) = itemdb_iconcolor(tsd->status.equip[x].id);
      }

      len += 3;

      if (strlen(tsd->status.equip[x].real_name)) {
        sprintf(buf, "%s", tsd->status.equip[x].real_name);
      } else {
        sprintf(buf, "%s", itemdb_name(tsd->status.equip[x].id));
      }

      WFIFOB(sd->fd, len + 6) = strlen(buf);
      strcpy(WFIFOP(sd->fd, len + 7), buf);
      len += strlen(buf) + 1;
      WFIFOB(sd->fd, len + 6) = strlen(itemdb_name(tsd->status.equip[x].id));
      strcpy(WFIFOP(sd->fd, len + 7), itemdb_name(tsd->status.equip[x].id));
      len += strlen(itemdb_name(tsd->status.equip[x].id)) + 1;
      WFIFOL(sd->fd, len + 6) = SWAP32(tsd->status.equip[x].dura);
      len += 5;
    } else {
      WFIFOW(sd->fd, len + 6) = SWAP16(0);
      WFIFOB(sd->fd, len + 8) = 0;
      WFIFOB(sd->fd, len + 9) = 0;
      WFIFOB(sd->fd, len + 10) = 0;
      WFIFOL(sd->fd, len + 11) = SWAP32(0);
      len += 10;
    }

    if (tsd->status.equip[x].id > 0 &&
        (itemdb_type(tsd->status.equip[x].id) >= 3) &&
        (itemdb_type(tsd->status.equip[x].id) <= 16)) {
      if (strlen(tsd->status.equip[x].real_name)) {
        nameof = tsd->status.equip[x].real_name;
      } else {
        nameof = itemdb_name(tsd->status.equip[x].id);
      }

      sprintf(buff, map_msg[clif_mapmsgnum(tsd, x)].message, nameof);
      strcat(equip_status, buff);
      strcat(equip_status, "\x0A");
    }
  }

  if (strlen(equip_status) == 0) { strcat(equip_status, "No items equipped."); }

  equip_len = strlen(equip_status);
  if (equip_len > 255) equip_len = 255;
  WFIFOB(sd->fd, len + 6) = equip_len;
  strcpy(WFIFOP(sd->fd, len + 7), equip_status);
  len += equip_len + 1;

  WFIFOL(sd->fd, len + 6) = SWAP32(bl->id);
  len += 4;

  if (tsd->status.settingFlags & FLAG_GROUP) {
    WFIFOB(sd->fd, len + 6) = 1;
  } else {
    WFIFOB(sd->fd, len + 6) = 0;
  }

  if (tsd->status.settingFlags & FLAG_EXCHANGE) {
    WFIFOB(sd->fd, len + 7) = 1;
  } else {
    WFIFOB(sd->fd, len + 7) = 0;
  }

  WFIFOB(sd->fd, len + 8) = 2 - tsd->status.sex;
  len += 3;
  WFIFOW(sd->fd, len + 6) = 0;
  len += 2;

  memcpy(WFIFOP(sd->fd, len + 6), tsd->profilepic_data, tsd->profilepic_size);
  len += tsd->profilepic_size;
  memcpy(WFIFOP(sd->fd, len + 6), tsd->profile_data, tsd->profile_size);
  len += tsd->profile_size;

  for (x = 0; x < MAX_LEGENDS; x++) {
    if (strlen(tsd->status.legends[x].text) && strlen(tsd->status.legends[x].name)) {
      count++;
    }
  }

  WFIFOW(sd->fd, len + 6) = SWAP16(count);
  len += 2;

  for (x = 0; x < MAX_LEGENDS; x++) {
    if (strlen(tsd->status.legends[x].text) && strlen(tsd->status.legends[x].name)) {
      WFIFOB(sd->fd, len + 6) = tsd->status.legends[x].icon;
      WFIFOB(sd->fd, len + 7) = tsd->status.legends[x].color;

      if (tsd->status.legends[x].tchaid > 0) {
        char *name = clif_getName(tsd->status.legends[x].tchaid);
        char *bff = replace_str(tsd->status.legends[x].text, "$player", name);

        WFIFOB(sd->fd, len + 8) = strlen(bff);
        memcpy(WFIFOP(sd->fd, len + 9), bff, strlen(bff));
        len += strlen(bff) + 3;
      } else {
        WFIFOB(sd->fd, len + 8) = strlen(tsd->status.legends[x].text);
        memcpy(WFIFOP(sd->fd, len + 9), tsd->status.legends[x].text, strlen(tsd->status.legends[x].text));
        len += strlen(tsd->status.legends[x].text) + 3;
      }
    }
  }

  WFIFOB(sd->fd, len + 6) = 3 - tsd->status.sex;

  if (clif_isregistered(tsd->status.id) > 0)
    WFIFOB(sd->fd, len + 7) = 1;
  else
    WFIFOB(sd->fd, len + 7) = 0;

  len += 5;

  WFIFOW(sd->fd, 1) = SWAP16(len + 3);
  WFIFOSET(sd->fd, encrypt(sd->fd));
  sl_doscript_blargs("onClick", NULL, 2, &sd->bl, &tsd->bl);
  return 0;
}

int clif_object_canmove_from(int m, int x, int y, int side);

/* Non-variadic trampoline so Rust can invoke clif_send_sub via map_foreachinarea
 * without touching va_list. area_type selects the search area (AREA/SAMEAREA/CORNER);
 * type is passed as the last vararg and read by clif_send_sub.
 * External linkage (no static) so Rust can call it via extern "C". */
void clif_send_area(int m, int x, int y, int area_type, int type,
                    const unsigned char *buf, int len, struct block_list *src_bl) {
  map_foreachinarea(clif_send_sub, m, x, y, area_type, BL_PC, buf, len, src_bl, type);
}

int clif_send_sub(struct block_list *bl, va_list ap) {
  unsigned char *buf = NULL;
  int len;
  struct block_list *src_bl = NULL;
  int type;
  USER *sd = NULL;
  USER *tsd = NULL;

  nullpo_ret(0, ap);
  nullpo_ret(0, sd = (USER *)bl);

  buf = va_arg(ap, unsigned char *);
  len = va_arg(ap, int);
  nullpo_ret(0, src_bl = va_arg(ap, struct block_list *));
  if (src_bl->type == BL_PC) tsd = (USER *)src_bl;

  if (tsd) {
    if ((tsd->optFlags & optFlag_stealth) && !sd->status.gm_level &&
        sd->status.id != tsd->status.id) {
      return 0;
    }

    if (map[tsd->bl.m].show_ghosts && tsd->status.state == 1 &&
        tsd->bl.id != sd->bl.id && sd->status.state != 1 &&
        !(sd->optFlags & optFlag_ghosts)) {
      return 0;
    }
  }

  if (sd && tsd) {
    if (RBUFB(buf, 3) == 0x0D && !clif_isignore(tsd, sd)) return 0;
  }

  type = va_arg(ap, int);

  switch (type) {
    case AREA_WOS:
    case SAMEAREA_WOS:
      if (bl == src_bl) return 0;
      break;
  }

  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  if (RBUFB(buf, 3) == 0x0D && RBUFB(buf, 5) >= 10) {
    if (pc_readglobalreg(sd, "chann_en") >= 1 && RBUFB(buf, 5) == 10) {
      WBUFB(buf, 5) = 0;
      WFIFOHEAD(sd->fd, len + 3);
      if (isActive(sd) && WFIFOP(sd->fd, 0) != (char *)buf)
        memcpy(WFIFOP(sd->fd, 0), buf, len);
      if (sd) WFIFOSET(sd->fd, encrypt(sd->fd));
      WBUFB(buf, 5) = 10;
    } else if (pc_readglobalreg(sd, "chann_es") >= 1 && RBUFB(buf, 5) == 11) {
      WBUFB(buf, 5) = 0;
      WFIFOHEAD(sd->fd, len + 3);
      if (isActive(sd) && WFIFOP(sd->fd, 0) != (char *)buf)
        memcpy(WFIFOP(sd->fd, 0), buf, len);
      if (sd) WFIFOSET(sd->fd, encrypt(sd->fd));
      WBUFB(buf, 5) = 11;
    } else if (pc_readglobalreg(sd, "chann_fr") >= 1 && RBUFB(buf, 5) == 12) {
      WBUFB(buf, 5) = 0;
      WFIFOHEAD(sd->fd, len + 3);
      if (isActive(sd) && WFIFOP(sd->fd, 0) != (char *)buf)
        memcpy(WFIFOP(sd->fd, 0), buf, len);
      if (sd) WFIFOSET(sd->fd, encrypt(sd->fd));
      WBUFB(buf, 5) = 12;
    } else if (pc_readglobalreg(sd, "chann_cn") >= 1 && RBUFB(buf, 5) == 13) {
      WBUFB(buf, 5) = 0;
      WFIFOHEAD(sd->fd, len + 3);
      if (isActive(sd) && WFIFOP(sd->fd, 0) != (char *)buf)
        memcpy(WFIFOP(sd->fd, 0), buf, len);
      if (sd) WFIFOSET(sd->fd, encrypt(sd->fd));
      WBUFB(buf, 5) = 13;
    } else if (pc_readglobalreg(sd, "chann_pt") >= 1 && RBUFB(buf, 5) == 14) {
      WBUFB(buf, 5) = 0;
      WFIFOHEAD(sd->fd, len + 3);
      if (isActive(sd) && WFIFOP(sd->fd, 0) != (char *)buf)
        memcpy(WFIFOP(sd->fd, 0), buf, len);
      if (sd) WFIFOSET(sd->fd, encrypt(sd->fd));
      WBUFB(buf, 5) = 14;
    } else if (pc_readglobalreg(sd, "chann_id") >= 1 && RBUFB(buf, 5) == 15) {
      WBUFB(buf, 5) = 0;
      WFIFOHEAD(sd->fd, len + 3);
      if (isActive(sd) && WFIFOP(sd->fd, 0) != (char *)buf)
        memcpy(WFIFOP(sd->fd, 0), buf, len);
      if (sd) WFIFOSET(sd->fd, encrypt(sd->fd));
      WBUFB(buf, 5) = 15;
    }
  } else {
    WFIFOHEAD(sd->fd, len + 3);
    if (isActive(sd) && WFIFOP(sd->fd, 0) != (char *)buf)
      memcpy(WFIFOP(sd->fd, 0), buf, len);
    if (sd) WFIFOSET(sd->fd, encrypt(sd->fd));
  }

  return 0;
}

// ---------------------------------------------------------------------------
// encrypt / decrypt — moved from c_src/net_crypt.c
// All crypto primitives live in Rust (src/network/crypt.rs); these wrappers
// remain in C because they access FIFO buffers and USER->EncHash.
// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// createdb_start — ported to src/game/client/handlers.rs
