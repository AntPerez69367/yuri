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
/* --- Weather --- */
void sl_g_setweather(unsigned char region, unsigned char indoor, unsigned char weather) {
    USER *tmpsd;
    int x, i, timer;
    unsigned int t = (unsigned int)time(NULL);
    for (x = 0; x < 65535; x++) {
        if (!map_isloaded(x)) continue;
        timer = map_readglobalreg(x, "artificial_weather_timer");
        if (timer > 0 && (unsigned int)timer <= t) {
            map_setglobalreg(x, "artificial_weather_timer", 0);
            timer = 0;
        }
        if (map[x].region == region && map[x].indoor == indoor && timer == 0) {
            map[x].weather = weather;
            for (i = 1; i < fd_max; i++) {
                if (rust_session_exists(i) && (tmpsd = (USER*)rust_session_get_data(i)) &&
                    !rust_session_get_eof(i)) {
                    if (tmpsd->bl.m == x) clif_sendweather(tmpsd);
                }
            }
        }
    }
}

void sl_g_setweatherm(int m, unsigned char weather) {
    USER *tmpsd;
    int i, timer;
    unsigned int t = (unsigned int)time(NULL);
    if (!map_isloaded(m)) return;
    timer = map_readglobalreg(m, "artificial_weather_timer");
    if (timer > 0 && (unsigned int)timer <= t) {
        map_setglobalreg(m, "artificial_weather_timer", 0);
        timer = 0;
    }
    if (timer == 0) {
        map[m].weather = weather;
        for (i = 1; i < fd_max; i++) {
            if (rust_session_exists(i) && (tmpsd = (USER*)rust_session_get_data(i)) &&
                !rust_session_get_eof(i)) {
                if (tmpsd->bl.m == m) clif_sendweather(tmpsd);
            }
        }
    }
}


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
void sl_mob_addhealth(MOB *mob, int damage) {
    struct block_list *bl = map_id2bl(mob->attacker);
    if (mob->data != NULL && bl != NULL && damage > 0) {
        switch (mob->data->subtype) {
            case 0: sl_doscript_blargs("mob_ai_basic",  "on_healed", 2, &mob->bl, bl); break;
            case 1: sl_doscript_blargs("mob_ai_normal", "on_healed", 2, &mob->bl, bl); break;
            case 2: sl_doscript_blargs("mob_ai_hard",   "on_healed", 2, &mob->bl, bl); break;
            case 3: sl_doscript_blargs("mob_ai_boss",   "on_healed", 2, &mob->bl, bl); break;
            case 4: sl_doscript_blargs(mob->data->yname,"on_healed", 2, &mob->bl, bl); break;
            case 5: sl_doscript_blargs("mob_ai_ghost",  "on_healed", 2, &mob->bl, bl); break;
        }
    } else if (mob->data != NULL && damage > 0) {
        switch (mob->data->subtype) {
            case 0: sl_doscript_blargs("mob_ai_basic",  "on_healed", 1, &mob->bl); break;
            case 1: sl_doscript_blargs("mob_ai_normal", "on_healed", 1, &mob->bl); break;
            case 2: sl_doscript_blargs("mob_ai_hard",   "on_healed", 1, &mob->bl); break;
            case 3: sl_doscript_blargs("mob_ai_boss",   "on_healed", 1, &mob->bl); break;
            case 4: sl_doscript_blargs(mob->data->yname,"on_healed", 1, &mob->bl); break;
            case 5: sl_doscript_blargs("mob_ai_ghost",  "on_healed", 1, &mob->bl); break;
        }
    }
    clif_send_mob_healthscript(mob, -damage, 0);
}

/* removeHealth: set damage on attacking entity, then send the health packet. */
void sl_mob_removehealth(MOB *mob, int damage, unsigned int caster_id) {
    struct block_list *bl = NULL;
    USER *tsd = NULL;
    MOB  *tmob = NULL;
    if (caster_id > 0) {
        bl = map_id2bl(caster_id);
        mob->attacker = caster_id;
    } else {
        bl = map_id2bl(mob->attacker);
    }
    if (bl != NULL) {
        if (bl->type == BL_PC) {
            tsd = (USER *)bl;
            tsd->damage = damage;
            tsd->critchance = 0;
        } else if (bl->type == BL_MOB) {
            tmob = (MOB *)bl;
            tmob->damage = damage;
            tmob->critchance = 0;
        }
    } else {
        mob->damage = damage;
        mob->critchance = 0;
    }
    if (mob->state != MOB_DEAD)
        clif_send_mob_healthscript(mob, damage, 0);
}

/* checkThreat: return the accumulated threat amount from a specific player. */
int sl_mob_checkthreat(MOB *mob, unsigned int player_id) {
    USER *tsd = map_id2sd(player_id);
    int x;
    if (!tsd) return 0;
    for (x = 0; x < MAX_THREATCOUNT; x++)
        if (mob->threat[x].user == tsd->bl.id)
            return (int)mob->threat[x].amount;
    return 0;
}

/* setIndDmg: add individual damage from player (dmg is passed as float from Lua). */
int sl_mob_setinddmg(MOB *mob, unsigned int player_id, float dmg) {
    USER *sd = map_id2sd(player_id);
    int x;
    if (!sd) return 0;
    for (x = 0; x < MAX_THREATCOUNT; x++) {
        if (mob->dmgindtable[x][0] == sd->status.id || mob->dmgindtable[x][0] == 0) {
            mob->dmgindtable[x][0] = sd->status.id;
            mob->dmgindtable[x][1] += dmg;
            return 1;
        }
    }
    return 0;
}

/* setGrpDmg: add group damage from player. */
int sl_mob_setgrpdmg(MOB *mob, unsigned int player_id, float dmg) {
    USER *sd = map_id2sd(player_id);
    int x;
    if (!sd) return 0;
    for (x = 0; x < MAX_THREATCOUNT; x++) {
        if (mob->dmggrptable[x][0] == sd->groupid || mob->dmggrptable[x][0] == 0) {
            mob->dmggrptable[x][0] = sd->groupid;
            mob->dmggrptable[x][1] += dmg;
            return 1;
        }
    }
    return 0;
}

/* callBase: call a named event on this mob's base AI script. */
int sl_mob_callbase(MOB *mob, const char *script) {
    struct block_list *bl = map_id2bl(mob->attacker);
    if (bl != NULL)
        sl_doscript_blargs(mob->data->yname, script, 2, &mob->bl, bl);
    else
        sl_doscript_blargs(mob->data->yname, script, 2, &mob->bl, &mob->bl);
    return 1;
}

/* checkMove: return 1 if the mob can step forward in its current direction. */
int sl_mob_checkmove(MOB *mob) {
    short dx = mob->bl.x, dy = mob->bl.y;
    unsigned short m = mob->bl.m;
    char direction = mob->side;
    struct warp_list *i;
    switch (direction) {
        case 0: dy -= 1; break;
        case 1: dx += 1; break;
        case 2: dy += 1; break;
        case 3: dx -= 1; break;
    }
    if (dx < 0) dx = 0;
    if (dy < 0) dy = 0;
    if (dx >= map[m].xs) dx = map[m].xs - 1;
    if (dy >= map[m].ys) dy = map[m].ys - 1;
    for (i = map[m].warp[dx/BLOCK_SIZE + (dy/BLOCK_SIZE)*map[m].bxs]; i; i = i->next)
        if (i->x == dx && i->y == dy) return 0;
    map_foreachincell(rust_mob_move, m, dx, dy, BL_MOB, mob);
    map_foreachincell(rust_mob_move, m, dx, dy, BL_PC, mob);
    map_foreachincell(rust_mob_move, m, dx, dy, BL_NPC, mob);
    if (clif_object_canmove(m, dx, dy, direction)) return 0;
    if (clif_object_canmove_from(m, mob->bl.x, mob->bl.y, direction)) return 0;
    if (map_canmove(m, dx, dy) == 1 || mob->canmove == 1) return 0;
    return 1;
}

/* setDuration: set or clear a magic effect timer on the mob. */
void sl_mob_setduration(MOB *mob, const char *name, int time, unsigned int caster_id, int recast) {
    int id = magicdb_id(name);
    int x, alreadycast = 0, mid;
    struct block_list *bl = NULL;
    if (time < 1000 && time > 0) time = 1000;
    for (x = 0; x < MAX_MAGIC_TIMERS; x++)
        if (mob->da[x].id == id && mob->da[x].caster_id == caster_id && mob->da[x].duration > 0)
            alreadycast = 1;
    for (x = 0; x < MAX_MAGIC_TIMERS; x++) {
        mid = mob->da[x].id;
        if (mid == id && time <= 0 && mob->da[x].caster_id == caster_id && alreadycast == 1) {
            unsigned int saved_caster_id = mob->da[x].caster_id;
            mob->da[x].duration = 0; mob->da[x].id = 0; mob->da[x].caster_id = 0;
            map_foreachinarea(clif_sendanimation, mob->bl.m, mob->bl.x, mob->bl.y,
                              AREA, BL_PC, mob->da[x].animation, &mob->bl, -1);
            mob->da[x].animation = 0;
            if (saved_caster_id != mob->bl.id) bl = map_id2bl(saved_caster_id);
            if (bl) sl_doscript_blargs(magicdb_yname(mid), "uncast", 2, &mob->bl, bl);
            else    sl_doscript_blargs(magicdb_yname(mid), "uncast", 1, &mob->bl);
            return;
        } else if (mob->da[x].id == id && mob->da[x].caster_id == caster_id &&
                   (mob->da[x].duration > time || recast == 1) && alreadycast == 1) {
            mob->da[x].duration = time;
            return;
        } else if (mob->da[x].id == 0 && mob->da[x].duration == 0 && time != 0 && alreadycast != 1) {
            mob->da[x].id = id;
            mob->da[x].duration = time;
            mob->da[x].caster_id = caster_id;
            return;
        }
    }
}

/* flushDuration: clear magic timers in id range, fire uncast events. */
void sl_mob_flushduration(MOB *mob, int dis, int minid, int maxid) {
    int x, id;
    char flush;
    struct block_list *bl = NULL;
    if (maxid < minid) maxid = minid;
    for (x = 0; x < MAX_MAGIC_TIMERS; x++) {
        id = mob->da[x].id;
        if (magicdb_dispel(id) > dis) continue;
        flush = (minid <= 0) ? 1
              : (maxid <= 0) ? (id == minid)
              : (id >= minid && id <= maxid);
        if (flush) {
            mob->da[x].duration = 0;
            map_foreachinarea(clif_sendanimation, mob->bl.m, mob->bl.x, mob->bl.y,
                              AREA, BL_PC, mob->da[x].animation, &mob->bl, -1);
            mob->da[x].animation = 0; mob->da[x].id = 0;
            bl = map_id2bl(mob->da[x].caster_id);
            mob->da[x].caster_id = 0;
            if (bl) sl_doscript_blargs(magicdb_yname(id), "uncast", 2, &mob->bl, bl);
            else    sl_doscript_blargs(magicdb_yname(id), "uncast", 1, &mob->bl);
        }
    }
}

// ─── USER coroutine field accessors (for async_coro.rs) ──────────────────────
// Rust cannot safely compute C struct field offsets at compile time, so we
// expose thin wrappers that read/write USER->coref and USER->coref_container.

unsigned int  sl_user_coref(void *sd)                    { return ((USER *)sd)->coref; }
void          sl_user_set_coref(void *sd, unsigned int v){ ((USER *)sd)->coref = v; }
unsigned int  sl_user_coref_container(void *sd)          { return ((USER *)sd)->coref_container; }
void         *sl_user_map_id2sd(unsigned int id)         { return (void *)map_id2sd(id); }

/* flushDurationNoUncast: clear magic timers without firing uncast events. */
void sl_mob_flushdurationnouncast(MOB *mob, int dis, int minid, int maxid) {
    int x, id;
    char flush;
    if (maxid < minid) maxid = minid;
    for (x = 0; x < MAX_MAGIC_TIMERS; x++) {
        id = mob->da[x].id;
        if (magicdb_dispel(id) > dis) continue;
        flush = (minid <= 0) ? 1
              : (maxid <= 0) ? (id == minid)
              : (id >= minid && id <= maxid);
        if (flush) {
            mob->da[x].duration = 0; mob->da[x].caster_id = 0;
            map_foreachinarea(clif_sendanimation, mob->bl.m, mob->bl.x, mob->bl.y,
                              AREA, BL_PC, mob->da[x].animation, &mob->bl, -1);
            mob->da[x].animation = 0; mob->da[x].id = 0;
        }
    }
}

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
void sl_pc_addhealth(void *sd, int damage) {
    clif_send_pc_healthscript((USER*)sd, -damage, 0);
    clif_sendstatus((USER*)sd, SFLAG_HPMP);
}
// removeHealth: positive healthscript value damages the player (mirrors pcl_removehealth).
void sl_pc_removehealth(void *sd, int damage, int caster) {
    if (caster > 0) ((USER*)sd)->attacker = caster;
    clif_send_pc_healthscript((USER*)sd, damage, 0);
    clif_sendstatus((USER*)sd, SFLAG_HPMP);
}
void sl_pc_freeasync(void *sd)        { sl_async_freeco((USER*)sd); }
int  sl_pc_forcesave(void *sd)        { return intif_save((USER*)sd); }
void sl_pc_die(void *sd)              { pc_diescript((USER*)sd); }
void sl_pc_resurrect(void *sd)        { pc_res((USER*)sd); }
void sl_pc_showhealth(void *sd, int damage, int type) { clif_send_pc_health((USER*)sd, damage, type); }
void sl_pc_calcstat(void *sd)         { pc_calcstat((USER*)sd); }
void sl_pc_sendstatus(void *sd)       { pc_requestmp((USER*)sd); clif_sendstatus((USER*)sd, SFLAG_FULLSTATS | SFLAG_HPMP | SFLAG_XPMONEY); clif_sendupdatestatus_onequip((USER*)sd); }
int  sl_pc_status(void *sd)           { return clif_mystaytus((USER*)sd); }
void sl_pc_warp(void *sd, int m, int x, int y) { pc_warp((USER*)sd, m, x, y); }
void sl_pc_refresh(void *sd)          { pc_setpos((USER*)sd, ((USER*)sd)->bl.m, ((USER*)sd)->bl.x, ((USER*)sd)->bl.y); clif_refreshnoclick((USER*)sd); }
void sl_pc_pickup(void *sd, unsigned int id) { pc_getitemscript((USER*)sd, id); }
void sl_pc_throwitem(void *sd)        { clif_throwitem_script((USER*)sd); }
void sl_pc_forcedrop(void *sd, int id){ pc_dropitemmap((USER*)sd, id, 0); }
void sl_pc_lock(void *sd)             { clif_blockmovement((USER*)sd, 0); }
void sl_pc_unlock(void *sd)           { clif_blockmovement((USER*)sd, 1); }
void sl_pc_swing(void *sd)            { clif_parseattack((USER*)sd); }
void sl_pc_respawn(void *sd)          { clif_spawn(&((USER*)sd)->bl); }
int  sl_pc_sendhealth(void *sd, float dmgf, int critical) {
    int damage;
    if (dmgf > 0)       damage = (int)(dmgf + 0.5f);
    else if (dmgf < 0)  damage = (int)(dmgf - 0.5f);
    else                damage = 0;
    if (critical == 1)  critical = 33;
    else if (critical == 2) critical = 255;
    clif_send_pc_healthscript((USER*)sd, damage, critical);
    return 0;
}

// ─── Task 13: remaining PcObject method wrappers ─────────────────────────────

// ── Movement ─────────────────────────────────────────────────────────────────
void sl_pc_move(void *sd, int speed)        { clif_noparsewalk((USER*)sd, (char)speed); }
void sl_pc_lookat(void *sd, int id) {
    struct block_list *bl = map_id2bl(id);
    if (bl) clif_parselookat_scriptsub((USER*)sd, bl);
}
void sl_pc_minirefresh(void *sd)            { clif_refreshnoclick((USER*)sd); }
void sl_pc_refreshinventory(void *sd) {
    for (int i = 0; i < MAX_INVENTORY; i++) clif_sendadditem((USER*)sd, i);
}
void sl_pc_updateinv(void *sd)              { pc_loaditem((USER*)sd); }
void sl_pc_checkinvbod(void *sd)            { clif_checkinvbod((USER*)sd); }

// ── Equipment ────────────────────────────────────────────────────────────────
void sl_pc_equip(void *sd)                  { pc_equipscript((USER*)sd); }
void sl_pc_takeoff(void *sd)                { pc_unequipscript((USER*)sd); }
void sl_pc_deductarmor(void *sd, int v)     { clif_deductarmor((USER*)sd, v); }
void sl_pc_deductweapon(void *sd, int v)    { clif_deductweapon((USER*)sd, v); }
void sl_pc_deductdura(void *sd, int eq, int v) { clif_deductdura((USER*)sd, eq, v); }
void sl_pc_deductduraequip(void *sd)        { clif_deductduraequip((USER*)sd); }
void sl_pc_deductdurainv(void *sd, int slot, int v) {
    USER *user = (USER*)sd;
    if (slot >= 0 && slot < MAX_INVENTORY) user->status.inventory[slot].dura -= v;
}
int  sl_pc_hasequipped(void *sd, unsigned int item_id) {
    USER *user = (USER*)sd;
    for (int i = 0; i < MAX_EQUIP; i++)
        if (user->status.equip[i].id == item_id) return 1;
    return 0;
}
void sl_pc_removeitemslot(void *sd, int slot, int amount, int type) {
    pc_delitem((USER*)sd, slot, amount, type);
}
// hasitem: simple id+amount check (no engrave/owner matching for now)
int  sl_pc_hasitem(void *sd, unsigned int item_id, int amount) {
    USER *user = (USER*)sd;
    int found = 0;
    for (int i = 0; i < MAX_INVENTORY; i++)
        if (user->status.inventory[i].id == item_id) found += user->status.inventory[i].amount;
    return (found >= amount) ? found : 0;
}
int  sl_pc_hasspace(void *sd, unsigned int item_id) {
    return pc_isinvenspace((USER*)sd, (int)item_id, 0, NULL, 0, 0, 0, 0);
}

// ── Stats / level ─────────────────────────────────────────────────────────────
void sl_pc_checklevel(void *sd) { pc_checklevel((USER*)sd); }

// ── UI / display ─────────────────────────────────────────────────────────────
void sl_pc_sendminimap(void *sd)                  { clif_sendminimap((USER*)sd); }
void sl_pc_popup(void *sd, const char *msg)       { clif_popup((USER*)sd, msg); }
void sl_pc_guitext(void *sd, const char *msg)     { clif_guitextsd(msg, (USER*)sd); }
void sl_pc_sendminitext(void *sd, const char *msg){ clif_sendminitext((USER*)sd, msg); }
void sl_pc_powerboard(void *sd)                   { (void)sd; /* stub */ }
void sl_pc_showboard(void *sd, int id)            { boards_showposts((USER*)sd, id); }
void sl_pc_showpost(void *sd, int id, int post)   { boards_readpost((USER*)sd, id, post); }
void sl_pc_changeview(void *sd, int x, int y)     { clif_sendxychange((USER*)sd, x, y); }

// ── Social / network ─────────────────────────────────────────────────────────
void sl_pc_speak(void *sd, const char *msg, int len, int type) {
    clif_sendscriptsay((USER*)sd, msg, len, type);
}
int  sl_pc_sendmail(void *sd, const char *to, const char *topic, const char *msg) {
    return nmail_sendmail((USER*)sd, to, topic, msg);
}
void sl_pc_sendurl(void *sd, int type, const char *url) { clif_sendurl((USER*)sd, type, url); }
void sl_pc_swingtarget(void *sd, int id) {
    struct block_list *bl = map_id2bl(id);
    if (!bl) return;
    if      (bl->type == BL_MOB) clif_mob_damage((USER*)sd, (MOB*)bl);
    else if (bl->type == BL_PC)  clif_pc_damage((USER*)sd, (USER*)bl);
}

// ── Kill registry ─────────────────────────────────────────────────────────────
int  sl_pc_killcount(void *sd, int mob_id) {
    USER *user = (USER*)sd;
    for (int x = 0; x < MAX_KILLREG; x++)
        if (user->status.killreg[x].mob_id == (unsigned int)mob_id) return user->status.killreg[x].amount;
    return 0;
}
void sl_pc_setkillcount(void *sd, int mob_id, int amount) {
    USER *user = (USER*)sd;
    for (int x = 0; x < MAX_KILLREG; x++) {
        if (user->status.killreg[x].mob_id == (unsigned int)mob_id) { user->status.killreg[x].amount = amount; return; }
    }
    for (int x = 0; x < MAX_KILLREG; x++) {
        if (user->status.killreg[x].mob_id == 0) {
            user->status.killreg[x].mob_id = (unsigned int)mob_id;
            user->status.killreg[x].amount = amount;
            return;
        }
    }
}
void sl_pc_flushkills(void *sd, int mob_id) {
    USER *user = (USER*)sd;
    for (int x = 0; x < MAX_KILLREG; x++) {
        if (mob_id == 0 || user->status.killreg[x].mob_id == (unsigned int)mob_id) {
            user->status.killreg[x].mob_id = 0;
            user->status.killreg[x].amount = 0;
        }
    }
}
void sl_pc_flushallkills(void *sd) { sl_pc_flushkills(sd, 0); }

// ── Threat ────────────────────────────────────────────────────────────────────
void sl_pc_addthreat(void *sd, unsigned int mob_id, unsigned int amount) {
    MOB *tmob = (MOB*)map_id2mob(mob_id);
    if (!tmob) return;
    USER *user = (USER*)sd;
    tmob->lastaction = time(NULL);
    for (int x = 0; x < MAX_THREATCOUNT; x++) {
        if (tmob->threat[x].user == user->bl.id) { tmob->threat[x].amount += amount; return; }
        if (tmob->threat[x].user == 0) { tmob->threat[x].user = user->bl.id; tmob->threat[x].amount = amount; return; }
    }
}
void sl_pc_setthreat(void *sd, unsigned int mob_id, unsigned int amount) {
    MOB *tmob = (MOB*)map_id2mob(mob_id);
    if (!tmob) return;
    USER *user = (USER*)sd;
    tmob->lastaction = time(NULL);
    for (int x = 0; x < MAX_THREATCOUNT; x++) {
        if (tmob->threat[x].user == user->bl.id) { tmob->threat[x].amount = amount; return; }
        if (tmob->threat[x].user == 0) { tmob->threat[x].user = user->bl.id; tmob->threat[x].amount = amount; return; }
    }
}
void sl_pc_addthreatgeneral(void *sd, unsigned int amount) {
    /* stub — requires map_foreachinarea portability */
    (void)sd; (void)amount;
}

// ── Spell list ────────────────────────────────────────────────────────────────
int  sl_pc_hasspell(void *sd, const char *name) {
    int id = magicdb_id(name);
    if (id <= 0) return 0;
    USER *user = (USER*)sd;
    for (int i = 0; i < MAX_SPELLS; i++)
        if (user->status.skill[i] == (unsigned short)id) return 1;
    return 0;
}
void sl_pc_addspell(void *sd, int spell_id) {
    USER *user = (USER*)sd;
    for (int i = 0; i < MAX_SPELLS; i++) {
        if (user->status.skill[i] == 0) {
            user->status.skill[i] = (unsigned short)spell_id;
            pc_loadmagic(user);
            return;
        }
    }
}
void sl_pc_removespell(void *sd, int spell_id) {
    USER *user = (USER*)sd;
    for (int i = 0; i < MAX_SPELLS; i++)
        if (user->status.skill[i] == (unsigned short)spell_id) user->status.skill[i] = 0;
}

// ── Duration system ───────────────────────────────────────────────────────────
int  sl_pc_hasduration(void *sd, const char *name) {
    int id = magicdb_id(name); if (id <= 0) return 0;
    USER *user = (USER*)sd;
    for (int i = 0; i < MAX_MAGIC_TIMERS; i++)
        if (user->status.dura_aether[i].id == (unsigned short)id && user->status.dura_aether[i].duration > 0) return 1;
    return 0;
}
int  sl_pc_hasdurationid(void *sd, const char *name, int caster_id) {
    int id = magicdb_id(name); if (id <= 0) return 0;
    USER *user = (USER*)sd;
    for (int i = 0; i < MAX_MAGIC_TIMERS; i++)
        if (user->status.dura_aether[i].id == (unsigned short)id &&
            user->status.dura_aether[i].caster_id == (unsigned int)caster_id &&
            user->status.dura_aether[i].duration > 0) return 1;
    return 0;
}
int  sl_pc_getduration(void *sd, const char *name) {
    int id = magicdb_id(name); if (id <= 0) return 0;
    USER *user = (USER*)sd;
    for (int i = 0; i < MAX_MAGIC_TIMERS; i++)
        if (user->status.dura_aether[i].id == (unsigned short)id) return user->status.dura_aether[i].duration;
    return 0;
}
int  sl_pc_getdurationid(void *sd, const char *name, int caster_id) {
    int id = magicdb_id(name); if (id <= 0) return 0;
    USER *user = (USER*)sd;
    for (int i = 0; i < MAX_MAGIC_TIMERS; i++)
        if (user->status.dura_aether[i].id == (unsigned short)id &&
            user->status.dura_aether[i].caster_id == (unsigned int)caster_id)
            return user->status.dura_aether[i].duration;
    return 0;
}
int  sl_pc_durationamount(void *sd, const char *name) {
    int id = magicdb_id(name); if (id <= 0) return 0;
    USER *user = (USER*)sd; int count = 0;
    for (int i = 0; i < MAX_MAGIC_TIMERS; i++)
        if (user->status.dura_aether[i].id == (unsigned short)id && user->status.dura_aether[i].duration > 0) count++;
    return count;
}
void sl_pc_setduration(void *sd, const char *name, int time_ms, int caster_id, int recast) {
    USER *user = (USER*)sd;
    int id = magicdb_id(name); if (id <= 0) return;
    if (time_ms > 0 && time_ms < 1000) time_ms = 1000;
    int alreadycast = 0, x;
    for (x = 0; x < MAX_MAGIC_TIMERS; x++)
        if (user->status.dura_aether[x].id == (unsigned short)id &&
            user->status.dura_aether[x].caster_id == (unsigned int)caster_id &&
            user->status.dura_aether[x].duration > 0) { alreadycast = 1; break; }
    for (x = 0; x < MAX_MAGIC_TIMERS; x++) {
        if (user->status.dura_aether[x].id == (unsigned short)id && time_ms <= 0 &&
            user->status.dura_aether[x].caster_id == (unsigned int)caster_id && alreadycast) {
            clif_send_duration(user, id, time_ms, map_id2sd(caster_id));
            user->status.dura_aether[x].duration = 0; user->status.dura_aether[x].caster_id = 0;
            if (user->status.dura_aether[x].aether == 0) user->status.dura_aether[x].id = 0;
            return;
        } else if (user->status.dura_aether[x].id == (unsigned short)id &&
            user->status.dura_aether[x].caster_id == (unsigned int)caster_id &&
            user->status.dura_aether[x].aether > 0 && user->status.dura_aether[x].duration <= 0) {
            user->status.dura_aether[x].duration = time_ms;
            clif_send_duration(user, id, time_ms / 1000, map_id2sd(caster_id));
            return;
        } else if (user->status.dura_aether[x].id == (unsigned short)id &&
            user->status.dura_aether[x].caster_id == (unsigned int)caster_id &&
            (user->status.dura_aether[x].duration > time_ms || recast) && alreadycast) {
            user->status.dura_aether[x].duration = time_ms;
            clif_send_duration(user, id, time_ms / 1000, map_id2sd(caster_id));
            return;
        } else if (user->status.dura_aether[x].id == 0 && user->status.dura_aether[x].duration == 0 && time_ms != 0 && !alreadycast) {
            user->status.dura_aether[x].id = (unsigned short)id;
            user->status.dura_aether[x].duration = time_ms;
            user->status.dura_aether[x].caster_id = (unsigned int)caster_id;
            clif_send_duration(user, id, time_ms / 1000, map_id2sd(caster_id));
            return;
        }
    }
}
void sl_pc_flushduration(void *sd, int dispel_level, int min_id, int max_id) {
    USER *user = (USER*)sd; (void)dispel_level;
    for (int x = 0; x < MAX_MAGIC_TIMERS; x++) {
        int id = (int)user->status.dura_aether[x].id;
        if (id == 0 || user->status.dura_aether[x].duration <= 0) continue;
        if (min_id > 0 && id < min_id) continue;
        if (max_id > 0 && id > max_id) continue;
        clif_send_duration(user, id, 0, map_id2sd(user->status.dura_aether[x].caster_id));
        user->status.dura_aether[x].duration = 0; user->status.dura_aether[x].caster_id = 0;
        if (user->status.dura_aether[x].aether == 0) user->status.dura_aether[x].id = 0;
    }
}
void sl_pc_flushdurationnouncast(void *sd, int dispel_level, int min_id, int max_id) {
    sl_pc_flushduration(sd, dispel_level, min_id, max_id); /* same packet, skip uncast script */
}
void sl_pc_refreshdurations(void *sd) {
    USER *user = (USER*)sd;
    for (int x = 0; x < MAX_MAGIC_TIMERS; x++) {
        if (user->status.dura_aether[x].id > 0 && user->status.dura_aether[x].duration > 0)
            clif_send_duration(user, user->status.dura_aether[x].id,
                               user->status.dura_aether[x].duration / 1000,
                               map_id2sd(user->status.dura_aether[x].caster_id));
    }
}

// ── Aether system ─────────────────────────────────────────────────────────────
void sl_pc_setaether(void *sd, const char *name, int time_ms) {
    USER *user = (USER*)sd;
    int id = magicdb_id(name); if (id <= 0) return;
    if (time_ms > 0 && time_ms < 1000) time_ms = 1000;
    int alreadycast = 0, x;
    for (x = 0; x < MAX_MAGIC_TIMERS; x++)
        if (user->status.dura_aether[x].id == (unsigned short)id) { alreadycast = 1; break; }
    for (x = 0; x < MAX_MAGIC_TIMERS; x++) {
        if (user->status.dura_aether[x].id == (unsigned short)id && time_ms <= 0) {
            clif_send_aether(user, id, time_ms);
            if (user->status.dura_aether[x].duration == 0) user->status.dura_aether[x].id = 0;
            user->status.dura_aether[x].aether = 0; return;
        } else if (user->status.dura_aether[x].id == (unsigned short)id &&
            (user->status.dura_aether[x].aether > time_ms || user->status.dura_aether[x].duration > 0)) {
            user->status.dura_aether[x].aether = time_ms;
            clif_send_aether(user, id, time_ms / 1000); return;
        } else if (user->status.dura_aether[x].id == 0 && user->status.dura_aether[x].aether == 0 && time_ms != 0 && !alreadycast) {
            user->status.dura_aether[x].id = (unsigned short)id;
            user->status.dura_aether[x].aether = time_ms;
            clif_send_aether(user, id, time_ms / 1000); return;
        }
    }
}
int  sl_pc_hasaether(void *sd, const char *name) {
    int id = magicdb_id(name); if (id <= 0) return 0;
    USER *user = (USER*)sd;
    for (int i = 0; i < MAX_MAGIC_TIMERS; i++)
        if (user->status.dura_aether[i].id == (unsigned short)id && user->status.dura_aether[i].aether > 0) return 1;
    return 0;
}
int  sl_pc_getaether(void *sd, const char *name) {
    int id = magicdb_id(name); if (id <= 0) return 0;
    USER *user = (USER*)sd;
    for (int i = 0; i < MAX_MAGIC_TIMERS; i++)
        if (user->status.dura_aether[i].id == (unsigned short)id) return user->status.dura_aether[i].aether;
    return 0;
}
void sl_pc_flushaether(void *sd) {
    USER *user = (USER*)sd;
    for (int i = 0; i < MAX_MAGIC_TIMERS; i++) {
        if (user->status.dura_aether[i].aether > 0) {
            clif_send_aether(user, user->status.dura_aether[i].id, 0);
            user->status.dura_aether[i].aether = 0;
            if (user->status.dura_aether[i].duration == 0) user->status.dura_aether[i].id = 0;
        }
    }
}

// ── Clan / nation ─────────────────────────────────────────────────────────────
void sl_pc_addclan(void *sd, const char *name) {
    /* clandb_add_local was static in scripting.c — needs separate port */
    (void)sd; (void)name;
}
void sl_pc_updatepath(void *sd, int path, int mark) {
    Sql_Query(sql_handle, "UPDATE `Character` SET `ChaPthId`=%d,`ChaMark`=%d WHERE `ChaId`=%d",
              path, mark, ((USER*)sd)->status.id);
}
void sl_pc_updatecountry(void *sd, int country) {
    Sql_Query(sql_handle, "UPDATE `Character` SET `ChaNation`=%d WHERE `ChaId`=%d",
              country, ((USER*)sd)->status.id);
}

// ── Misc ──────────────────────────────────────────────────────────────────────
int  sl_pc_getcasterid(void *sd, const char *name) { (void)sd; return magicdb_id(name); }
void sl_pc_settimer(void *sd, int type, int length) {
    clif_send_timer((USER*)sd, (char)type, (unsigned int)length);
}
void sl_pc_addtime(void *sd, int v) {
    USER *user = (USER*)sd;
    user->disptimertick += v;
    clif_send_timer(user, (char)user->disptimertype, (unsigned int)user->disptimertick);
}
void sl_pc_removetime(void *sd, int v) {
    USER *user = (USER*)sd;
    user->disptimertick -= v;
    if (user->disptimertick < 0) user->disptimertick = 0;
    clif_send_timer(user, (char)user->disptimertype, (unsigned int)user->disptimertick);
}
void sl_pc_setheroshow(void *sd, int flag) {
    USER *user = (USER*)sd;
    user->status.heroes = flag;
    Sql_Query(sql_handle, "UPDATE `Character` SET `ChaHeroShow`=%d WHERE `ChaId`=%d",
              flag, user->status.id);
}

// ── Legends ───────────────────────────────────────────────────────────────────
void sl_pc_addlegend(void *sd, const char *text, const char *name, int icon, int color, unsigned int tchaid) {
    USER *user = (USER*)sd;
    for (int x = 0; x < MAX_LEGENDS; x++) {
        if (strcasecmp(user->status.legends[x].name, "") == 0 &&
            (x + 1 >= MAX_LEGENDS || strcasecmp(user->status.legends[x + 1].name, "") == 0)) {
            strncpy(user->status.legends[x].text, text ? text : "", sizeof(user->status.legends[x].text) - 1);
            user->status.legends[x].text[sizeof(user->status.legends[x].text) - 1] = '\0';
            strncpy(user->status.legends[x].name, name ? name : "", sizeof(user->status.legends[x].name) - 1);
            user->status.legends[x].name[sizeof(user->status.legends[x].name) - 1] = '\0';
            user->status.legends[x].icon   = icon;
            user->status.legends[x].color  = color;
            user->status.legends[x].tchaid = tchaid;
            return;
        }
    }
}
int sl_pc_haslegend(void *sd, const char *name) {
    USER *user = (USER*)sd;
    for (int i = 0; i < MAX_LEGENDS; i++)
        if (strcmp(user->status.legends[i].name, name ? name : "") == 0 &&
            user->status.legends[i].name[0] != '\0') return 1;
    return 0;
}
void sl_pc_removelegendbyname(void *sd, const char *name) {
    USER *user = (USER*)sd;
    for (int x = 0; x < MAX_LEGENDS; x++) {
        if (strcasecmp(user->status.legends[x].name, name ? name : "") == 0) {
            strcpy(user->status.legends[x].text, "");
            strcpy(user->status.legends[x].name, "");
            user->status.legends[x].icon = user->status.legends[x].color = user->status.legends[x].tchaid = 0;
        }
    }
    // compact: shift non-empty entries forward
    for (int x = 0; x < MAX_LEGENDS - 1; x++) {
        if (user->status.legends[x].name[0] == '\0' && user->status.legends[x + 1].name[0] != '\0') {
            strcpy(user->status.legends[x].text,  user->status.legends[x + 1].text);
            strcpy(user->status.legends[x].name,  user->status.legends[x + 1].name);
            user->status.legends[x].icon   = user->status.legends[x + 1].icon;
            user->status.legends[x].color  = user->status.legends[x + 1].color;
            user->status.legends[x].tchaid = user->status.legends[x + 1].tchaid;
            strcpy(user->status.legends[x + 1].text, "");
            strcpy(user->status.legends[x + 1].name, "");
            user->status.legends[x + 1].icon = user->status.legends[x + 1].color = user->status.legends[x + 1].tchaid = 0;
        }
    }
}
void sl_pc_removelegendbycolor(void *sd, int color) {
    USER *user = (USER*)sd;
    int count = 0;
    for (int x = 0; x < MAX_LEGENDS; x++) {
        if (user->status.legends[x].color == color) count++;
        if (x + count < MAX_LEGENDS) {
            strcpy(user->status.legends[x].text,  user->status.legends[x + count].text);
            strcpy(user->status.legends[x].name,  user->status.legends[x + count].name);
            user->status.legends[x].icon   = user->status.legends[x + count].icon;
            user->status.legends[x].color  = user->status.legends[x + count].color;
            user->status.legends[x].tchaid = user->status.legends[x + count].tchaid;
        }
    }
    // compact trailing empties
    for (int x = 0; x < MAX_LEGENDS - 1; x++) {
        if (user->status.legends[x].name[0] == '\0' && user->status.legends[x + 1].name[0] != '\0') {
            strcpy(user->status.legends[x].text,  user->status.legends[x + 1].text);
            strcpy(user->status.legends[x].name,  user->status.legends[x + 1].name);
            user->status.legends[x].icon   = user->status.legends[x + 1].icon;
            user->status.legends[x].color  = user->status.legends[x + 1].color;
            user->status.legends[x].tchaid = user->status.legends[x + 1].tchaid;
            strcpy(user->status.legends[x + 1].text, "");
            strcpy(user->status.legends[x + 1].name, "");
            user->status.legends[x + 1].icon = user->status.legends[x + 1].color = user->status.legends[x + 1].tchaid = 0;
        }
    }
}

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
void sl_g_sendside(void *bl) {
    clif_sendside((struct block_list *)bl);
}

/* sl_g_sendanimxy — broadcast an animation at (x,y) around bl's position. */
void sl_g_sendanimxy(void *bl_ptr, int anim, int x, int y, int times) {
    struct block_list *bl = (struct block_list *)bl_ptr;
    if (!bl) return;
    map_foreachinarea(clif_sendanimation_xy, bl->m, bl->x, bl->y, AREA, BL_PC, anim, times, x, y);
}

/* sl_g_delete_bl — delete a non-PC block from the world and free it. */
void sl_g_delete_bl(void *bl_ptr) {
    struct block_list *bl = (struct block_list *)bl_ptr;
    if (!bl) return;
    if (bl->type == BL_PC) return;
    map_delblock(bl);
    map_deliddb(bl);
    if (bl->id > 0) {
        clif_lookgone(bl);
        FREE(bl);
    }
}

/* sl_g_talk — make bl speak to all PCs in the surrounding area. */
void sl_g_talk(void *bl_ptr, int type, const char *msg) {
    struct block_list *bl = (struct block_list *)bl_ptr;
    if (!bl) return;
    map_foreachinarea(clif_speak, bl->m, bl->x, bl->y, AREA, BL_PC, msg, bl, type);
}

/* sl_g_getusers — collect pointers to all online USER block_lists. */
int sl_g_getusers(void **out_ptrs, int max_count) {
    USER *tsd = NULL;
    int count = 0;
    for (int i = 0; i < fd_max && count < max_count; i++) {
        if (rust_session_exists(i) && !rust_session_get_eof(i) && (tsd = rust_session_get_data(i))) {
            out_ptrs[count++] = (void *)&tsd->bl;
        }
    }
    return count;
}

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

/* sl_g_getmappvp — return map[m].pvp (0 if map not loaded). */
int sl_g_getmappvp(int m) {
    if (!map_isloaded(m)) return 0;
    return (int)map[m].pvp;
}

/* sl_g_getmaptitle — copy map[m].title into buf; returns 1 on success, 0 if not loaded. */
int sl_g_getmaptitle(int m, char *buf, int buflen) {
    if (!map_isloaded(m) || buflen <= 0) return 0;
    strncpy(buf, map[m].title, buflen - 1);
    buf[buflen - 1] = '\0';
    return 1;
}

/* sl_pc_getpk — returns 1 if sd->pvp[] contains id, else 0. Mirrors pcl_getpk. */
int sl_pc_getpk(void *sd_ptr, int id) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return 0;
    for (int x = 0; x < 20; x++) {
        if (sd->pvp[x][0] == id) return 1;
    }
    return 0;
}

/* --- PC regen overflow accumulators (float fields) --- */
int  sl_pc_vregenoverflow(void *sd) { return (int)((USER*)sd)->vregenoverflow; }
// sl_pc_set_vregenoverflow — ported to pc_accessors.rs
int  sl_pc_mregenoverflow(void *sd) { return (int)((USER*)sd)->mregenoverflow; }
// sl_pc_set_mregenoverflow — ported to pc_accessors.rs

/* --- PC group membership fields --- */
int  sl_pc_group_count(void *sd)  { return ((USER*)sd)->group_count; }
// sl_pc_set_group_count — ported to pc_accessors.rs
int  sl_pc_group_on(void *sd)     { return ((USER*)sd)->group_on; }
// sl_pc_set_group_on — ported to pc_accessors.rs
int  sl_pc_group_leader(void *sd) { return (int)((USER*)sd)->group_leader; }
// sl_pc_set_group_leader — ported to pc_accessors.rs

/* Fill `out` with group member char IDs. Returns the count written.
 * Mirrors the original scripting.c `group` table construction:
 *   - in a group: fills with groups[groupid][0..group_count-1]
 *   - solo:       fills with the player's own status.id (count = 1) */
int sl_pc_getgroup(void *sd, unsigned int *out, int max) {
    USER *user = (USER*)sd;
    if (user->group_count > 0) {
        int n = user->group_count < max ? user->group_count : max;
        for (int i = 0; i < n; i++)
            out[i] = groups[user->groupid][i];
        return n;
    }
    if (max > 0) out[0] = user->status.id;
    return 1;
}

/* ---- Shared block-object methods (Task 5) ---- */

/* sendAnimation — broadcast spell/skill animation to players in AREA around bl.
 * clif_sendanimation(struct block_list *bl, va_list): reads anim, target-bl, times. */
void sl_g_sendanimation(void *bl_ptr, int anim, int times) {
    struct block_list *bl = (struct block_list *)bl_ptr;
    if (!bl) return;
    map_foreachinarea(clif_sendanimation, bl->m, bl->x, bl->y, AREA, BL_PC,
                      anim, bl, times);
}

/* playSound — play a sound effect at bl's position.
 * clif_playsound(struct block_list *, int) */
void sl_g_playsound(void *bl_ptr, int sound) {
    struct block_list *bl = (struct block_list *)bl_ptr;
    if (!bl) return;
    clif_playsound(bl, sound);
}

/* sendAction — broadcast action animation to players in AREA around bl.
 * clif_sendaction(struct block_list *, int type, int time, int sound) */
void sl_g_sendaction(void *bl_ptr, int action, int speed) {
    struct block_list *bl = (struct block_list *)bl_ptr;
    if (!bl) return;
    clif_sendaction(bl, action, speed, 0);
}

/* msg — send a colored message to a specific player (by ID).
 * clif_sendmsg(USER *, int color, const char *msg).
 * target==0: broadcast not implemented (matches bll_talkcolor in scripting.c). */
void sl_g_msg(void *bl_ptr, int color, const char *msg, int target) {
    struct block_list *bl = (struct block_list *)bl_ptr;
    if (!bl || !msg) return;
    if (target != 0) {
        USER *tsd = map_id2sd((unsigned int)target);
        if (tsd) clif_sendmsg(tsd, color, msg);
    }
}

/* delete a floor item from the world (mirrors bll_delete for BL_ITEM).
 * Removes from spatial grid, ID DB, and sends lookgone to nearby players.
 * Does NOT free memory — the Lua object may still read fields after delete. */
void sl_fl_delete(void *bl_ptr) {
    struct block_list *bl = (struct block_list *)bl_ptr;
    if (!bl || bl->type == BL_PC) return;
    map_delblock(bl);
    map_deliddb(bl);
    if (bl->id > 0) {
        clif_lookgone(bl);
    }
}

/* dropItem — drop item at bl's position using mobdb_dropitem.
 * owner==0 means no specific owner USER*. */
void sl_g_dropitem(void *bl_ptr, int item_id, int amount, int owner) {
    struct block_list *bl = (struct block_list *)bl_ptr;
    if (!bl) return;
    USER *sd  = (owner != 0) ? map_id2sd((unsigned int)owner) : NULL;
    int dura  = itemdb_dura((unsigned int)item_id);
    int prot  = itemdb_protected((unsigned int)item_id);
    mobdb_dropitem(bl->id, (unsigned int)item_id, (unsigned int)amount,
                   dura, prot, 0, bl->m, bl->x, bl->y, sd);
}

/* dropItemXY — drop item at specified map coordinates. */
void sl_g_dropitemxy(void *bl_ptr, int item_id, int amount, int m, int x, int y, int owner) {
    (void)bl_ptr;
    USER *sd  = (owner != 0) ? map_id2sd((unsigned int)owner) : NULL;
    int dura  = itemdb_dura((unsigned int)item_id);
    int prot  = itemdb_protected((unsigned int)item_id);
    mobdb_dropitem(0, (unsigned int)item_id, (unsigned int)amount,
                   dura, prot, 0, m, x, y, sd);
}

/* objectCanMove — 1 if cell (x,y) is passable from side, 0 if blocked.
 * clif_object_canmove(int m, int x, int y, int side) returns non-zero when BLOCKED. */
int sl_g_objectcanmove(void *bl_ptr, int x, int y, int side) {
    struct block_list *bl = (struct block_list *)bl_ptr;
    if (!bl) return 0;
    return clif_object_canmove(bl->m, x, y, side) ? 0 : 1;
}

/* objectCanMoveFrom — 1 if block can move FROM (x,y) facing side, 0 if blocked. */
int sl_g_objectcanmovefrom(void *bl_ptr, int x, int y, int side) {
    struct block_list *bl = (struct block_list *)bl_ptr;
    if (!bl) return 0;
    return clif_object_canmove_from(bl->m, x, y, side) ? 0 : 1;
}

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
void sl_g_sendparcel(void *bl_ptr, int receiver, int sender,
                     int item, int amount, int owner,
                     const char *engrave, int npcflag) {
    (void)bl_ptr;
    int pos    = -1;
    int newest = -1;
    int x;
    size_t engrave_len = engrave ? strlen(engrave) : 0;
    char *escape = (char *)malloc(engrave_len * 2 + 1);
    if (!escape) return;

    SqlStmt *stmt = SqlStmt_Malloc(sql_handle);
    if (!stmt) { Sql_ShowDebug(sql_handle); free(escape); return; }

    if (SQL_ERROR == SqlStmt_Prepare(stmt,
            "SELECT `ParPosition` FROM `Parcels` WHERE `ParChaIdDestination` = '%u'",
            (unsigned int)receiver) ||
        SQL_ERROR == SqlStmt_Execute(stmt) ||
        SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_INT, &pos, 0, NULL, NULL)) {
        SqlStmt_ShowDebug(stmt);
        SqlStmt_Free(stmt);
        free(escape);
        return;
    }

    if (SqlStmt_NumRows(stmt) > 0) {
        for (x = 0; x < SqlStmt_NumRows(stmt) && SQL_SUCCESS == SqlStmt_NextRow(stmt); x++) {
            if (pos > newest) newest = pos;
        }
    }
    newest += 1;
    SqlStmt_Free(stmt);

    Sql_EscapeString(sql_handle, escape, engrave ? engrave : "");

    if (SQL_ERROR == Sql_Query(sql_handle,
            "INSERT INTO `Parcels` (`ParChaIdDestination`, `ParSender`, `ParItmId`,"
            " `ParAmount`, `ParChaIdOwner`, `ParEngrave`, `ParPosition`, `ParNpc`,"
            " `ParCustomLook`, `ParCustomLookColor`, `ParCustomIcon`, `ParCustomIconColor`,"
            " `ParProtected`, `ParItmDura`) VALUES"
            " ('%u','%u','%u','%u','%u','%s','%d','%d',0,0,0,0,%d,%u)",
            (unsigned int)receiver, (unsigned int)sender, (unsigned int)item,
            (unsigned int)amount, (unsigned int)owner, escape, newest, npcflag,
            itemdb_protected((unsigned int)item),
            (unsigned int)itemdb_dura((unsigned int)item))) {
        Sql_ShowDebug(sql_handle);
    }
    free(escape);
}

/* throwBlock — send throw animation from bl's position to (x,y).
 * Mirrors bll_throw in scripting.c; uses raw clif_send with SAMEAREA. */
void sl_g_throwblock(void *bl_ptr, int x, int y, int icon, int color, int action) {
    struct block_list *bl = (struct block_list *)bl_ptr;
    if (!bl) return;
    unsigned char buf[255];
    WBUFB(buf, 0)  = 0xAA;
    WBUFW(buf, 1)  = SWAP16(0x1B);
    WBUFB(buf, 3)  = 0x16;
    WBUFB(buf, 4)  = 0x03;
    WBUFL(buf, 5)  = SWAP32(bl->id);
    WBUFW(buf, 9)  = SWAP16(icon + 49152);
    WBUFB(buf, 11) = (unsigned char)color;
    WBUFL(buf, 12) = 0;
    WBUFW(buf, 16) = SWAP16(bl->x);
    WBUFW(buf, 18) = SWAP16(bl->y);
    WBUFW(buf, 20) = SWAP16(x);
    WBUFW(buf, 22) = SWAP16(y);
    WBUFL(buf, 24) = 0;
    WBUFB(buf, 28) = (unsigned char)action;
    WBUFB(buf, 29) = 0x00;
    clif_send(buf, 30, bl, SAMEAREA);
}

/* delFromIDDB — remove block from map id database.
 * map_deliddb(struct block_list *) */
void sl_g_deliddb(void *bl_ptr) {
    struct block_list *bl = (struct block_list *)bl_ptr;
    if (!bl) return;
    map_deliddb(bl);
}

/* addPermanentSpawn — no-op (matches bll_permspawn in scripting.c) */
void sl_g_addpermanentspawn(void *bl_ptr) {
    (void)bl_ptr;
}

/* ---- PC non-dialog methods (Task 7) ---- */

/* --- Inventory --- */

/* addItem — build a struct item and call pc_additem.
 * dura==0 uses itemdb default; owner==0 means no owner; engrave may be NULL. */
void sl_pc_additem(void *sd_ptr, unsigned int id, unsigned int amount,
                   int dura, unsigned int owner, const char *engrave) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    struct item fl;
    memset(&fl, 0, sizeof(fl));
    fl.id     = id;
    fl.amount = amount;
    fl.owner  = owner;
    fl.dura   = dura ? (unsigned int)dura : (unsigned int)itemdb_dura(id);
    fl.protected = (unsigned int)itemdb_protected(id);
    if (engrave && engrave[0])
        strncpy(fl.real_name, engrave, sizeof(fl.real_name) - 1);
    pc_additem(sd, &fl);
}

/* getInventoryItem — return pointer into sd->status.inventory[slot].
 * Returns NULL if sd is NULL, slot is out of range, or slot is empty (id == 0). */
void *sl_pc_getinventoryitem(void *sd_ptr, int slot) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return NULL;
    if (slot < 0 || slot >= MAX_INVENTORY) return NULL;
    if (sd->status.inventory[slot].id == 0) return NULL;
    return &sd->status.inventory[slot];
}

/* getEquippedItem — return pointer into sd->status.equip[slot].
 * Returns NULL if sd is NULL, slot is out of range, or slot is empty. */
void *sl_pc_getequippeditem_sd(void *sd_ptr, int slot) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return NULL;
    if (slot < 0 || slot >= MAX_EQUIP) return NULL;
    if (sd->status.equip[slot].id == 0) return NULL;
    return &sd->status.equip[slot];
}

/* removeItem — remove items matching id/amount/engrave/owner from inventory.
 * Mirrors pcl_removeinventoryitem: iterates inventory and calls pc_delitem. */
void sl_pc_removeitem(void *sd_ptr, unsigned int id, unsigned int amount,
                      int type, unsigned int owner, const char *engrave) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    if (!engrave) engrave = "";
    int x;
    /* First pass: prefer partial stacks */
    for (x = 0; x < sd->status.maxinv && amount > 0; x++) {
        if (sd->status.inventory[x].id != id) continue;
        if (owner && sd->status.inventory[x].owner != owner) continue;
        if (strcasecmp(sd->status.inventory[x].real_name, engrave) != 0) continue;
        unsigned int avail = sd->status.inventory[x].amount;
        if (avail == 0) continue;
        unsigned int take = (avail < amount) ? avail : amount;
        pc_delitem(sd, x, (int)take, type);
        amount -= take;
    }
}

/* removeItemDura — remove items matching id with full durability.
 * Mirrors pcl_removeitemdura logic. */
void sl_pc_removeitemdura(void *sd_ptr, unsigned int id, unsigned int amount,
                          int type) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    int x;
    for (x = 0; x < sd->status.maxinv && amount > 0; x++) {
        if (sd->status.inventory[x].id != id) continue;
        if (sd->status.inventory[x].dura != (unsigned int)itemdb_dura(id))
            continue;
        unsigned int avail = sd->status.inventory[x].amount;
        if (avail == 0) continue;
        unsigned int take = (avail < amount) ? avail : amount;
        pc_delitem(sd, x, (int)take, type);
        amount -= take;
    }
}

/* hasItemDura — return remaining amount needed (0 = has enough full-dura items).
 * Mirrors pcl_hasitemdura: negative return means surplus. */
int sl_pc_hasitemdura(void *sd_ptr, unsigned int id, unsigned int amount) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return (int)amount;
    int x;
    unsigned int dura = (unsigned int)itemdb_dura(id);
    for (x = 0; x < sd->status.maxinv && amount > 0; x++) {
        if (sd->status.inventory[x].id != id) continue;
        if (sd->status.inventory[x].dura != dura) continue;
        if (sd->status.inventory[x].amount == 0) continue;
        if (sd->status.inventory[x].amount >= amount)
            return 0;
        amount -= sd->status.inventory[x].amount;
    }
    return (int)amount;
}

/* --- Bank --- */

/* checkBankItemId — return item_id at bank slot, 0 if empty. */
int sl_pc_checkbankitems(void *sd_ptr, int slot) {
    USER *sd = (USER *)sd_ptr;
    if (!sd || slot < 0 || slot >= MAX_BANK_SLOTS) return 0;
    return (int)sd->status.banks[slot].item_id;
}

/* checkBankAmount — return amount at bank slot. */
int sl_pc_checkbankamounts(void *sd_ptr, int slot) {
    USER *sd = (USER *)sd_ptr;
    if (!sd || slot < 0 || slot >= MAX_BANK_SLOTS) return 0;
    return (int)sd->status.banks[slot].amount;
}

/* checkBankOwner — return owner char-id at bank slot. */
int sl_pc_checkbankowners(void *sd_ptr, int slot) {
    USER *sd = (USER *)sd_ptr;
    if (!sd || slot < 0 || slot >= MAX_BANK_SLOTS) return 0;
    return (int)sd->status.banks[slot].owner;
}

/* checkBankEngrave — return engrave string at bank slot. */
const char *sl_pc_checkbankengraves(void *sd_ptr, int slot) {
    USER *sd = (USER *)sd_ptr;
    if (!sd || slot < 0 || slot >= MAX_BANK_SLOTS) return "";
    return sd->status.banks[slot].real_name;
}

/* bankDeposit — add item/amount/owner/engrave to a matching or empty bank slot.
 * Mirrors pcl_bankdeposit (simplified: no custom look/icon). */
void sl_pc_bankdeposit(void *sd_ptr, unsigned int item, unsigned int amount,
                       unsigned int owner, const char *engrave) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    if (!engrave) engrave = "";
    int x, deposit = -1;
    for (x = 0; x < MAX_BANK_SLOTS; x++) {
        if (sd->status.banks[x].item_id == item &&
            sd->status.banks[x].owner   == owner &&
            !strcasecmp(sd->status.banks[x].real_name, engrave)) {
            deposit = x;
            break;
        }
    }
    if (deposit != -1) {
        sd->status.banks[deposit].amount += amount;
    } else {
        for (x = 0; x < MAX_BANK_SLOTS; x++) {
            if (sd->status.banks[x].item_id == 0) {
                sd->status.banks[x].item_id = item;
                sd->status.banks[x].amount  = amount;
                sd->status.banks[x].owner   = owner;
                strncpy(sd->status.banks[x].real_name, engrave,
                        sizeof(sd->status.banks[x].real_name) - 1);
                break;
            }
        }
    }
}

/* bankWithdraw — reduce amount from matching bank slot. Removes slot when empty.
 * Mirrors pcl_bankwithdraw (simplified). */
void sl_pc_bankwithdraw(void *sd_ptr, unsigned int item, unsigned int amount,
                        unsigned int owner, const char *engrave) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    if (!engrave) engrave = "";
    int x, deposit = -1;
    for (x = 0; x < MAX_BANK_SLOTS; x++) {
        if (sd->status.banks[x].item_id == item &&
            sd->status.banks[x].owner   == owner &&
            !strcasecmp(sd->status.banks[x].real_name, engrave)) {
            deposit = x;
            break;
        }
    }
    if (deposit == -1) return;
    if (sd->status.banks[deposit].amount <= amount) {
        memset(&sd->status.banks[deposit], 0, sizeof(sd->status.banks[deposit]));
    } else {
        sd->status.banks[deposit].amount -= amount;
    }
}

/* bankCheckAmount — return amount of matching item in bank (all matching slots summed). */
int sl_pc_bankcheckamount(void *sd_ptr, unsigned int item, unsigned int amount,
                          unsigned int owner, const char *engrave) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return 0;
    if (!engrave) engrave = "";
    unsigned int total = 0;
    int x;
    for (x = 0; x < MAX_BANK_SLOTS; x++) {
        if (sd->status.banks[x].item_id == item &&
            sd->status.banks[x].owner   == owner &&
            !strcasecmp(sd->status.banks[x].real_name, engrave)) {
            total += sd->status.banks[x].amount;
        }
    }
    (void)amount; /* caller compares total against their own threshold */
    return (int)total;
}

/* --- Clan bank — no separate struct; no-ops until a clan_bank struct exists --- */
/* sl_pc_clanbankdeposit: no-op — clan bank lives in a separate SQL table, not in sd. */
void sl_pc_clanbankdeposit(void *sd_ptr, unsigned int item, unsigned int amount,
                           unsigned int owner, const char *engrave) {
    (void)sd_ptr; (void)item; (void)amount; (void)owner; (void)engrave;
    /* Clan bank is SQL-backed; deposit logic is in pcl_clanbankdeposit in scripting.c */
}

void sl_pc_clanbankwithdraw(void *sd_ptr, unsigned int item, unsigned int amount,
                            unsigned int owner, const char *engrave) {
    (void)sd_ptr; (void)item; (void)amount; (void)owner; (void)engrave;
    /* No-op: clan bank is SQL-backed */
}

/* sl_pc_getclanitems — return item_id at clan bank slot.
 * Mirrors pcl_getclanbankitems: clan->clanbanks[slot].item_id. */
int sl_pc_getclanitems(void *sd_ptr, int slot) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return 0;
    struct clan_data *clan = clandb_search((int)sd->status.clan);
    if (!clan || !clan->clanbanks) return 0;
    if (slot < 0 || slot >= 255) return 0;
    return (int)clan->clanbanks[slot].item_id;
}

/* sl_pc_getclanamounts — return amount at clan bank slot. */
int sl_pc_getclanamounts(void *sd_ptr, int slot) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return 0;
    struct clan_data *clan = clandb_search((int)sd->status.clan);
    if (!clan || !clan->clanbanks) return 0;
    if (slot < 0 || slot >= 255) return 0;
    return (int)clan->clanbanks[slot].amount;
}

/* sl_pc_checkclankitemamounts — total amount of a given item across all clan bank slots.
 * The `amount` parameter is unused (caller compares the returned total). */
int sl_pc_checkclankitemamounts(void *sd_ptr, int item, int amount) {
    USER *sd = (USER *)sd_ptr;
    (void)amount;
    if (!sd) return 0;
    struct clan_data *clan = clandb_search((int)sd->status.clan);
    if (!clan || !clan->clanbanks) return 0;
    unsigned int total = 0;
    int x;
    for (x = 0; x < 255; x++) {
        if ((int)clan->clanbanks[x].item_id == item)
            total += clan->clanbanks[x].amount;
    }
    return (int)total;
}

/* --- Spell lists --- */

/* getAllDurations — fill out_names[0..max) with magicdb ynames of active durations.
 * Returns count written. */
int sl_pc_getalldurations(void *sd_ptr, const char **out_names, int max) {
    USER *sd = (USER *)sd_ptr;
    if (!sd || !out_names) return 0;
    int i, count = 0;
    for (i = 0; i < MAX_MAGIC_TIMERS && count < max; i++) {
        if (sd->status.dura_aether[i].id > 0 &&
            sd->status.dura_aether[i].duration > 0) {
            out_names[count++] = magicdb_yname(sd->status.dura_aether[i].id);
        }
    }
    return count;
}

/* getSpells — fill out_ids[0..max) with spell IDs from sd->status.skill[].
 * Returns count written. Mirrors pcl_getspells. */
int sl_pc_getspells(void *sd_ptr, int *out_ids, int max) {
    USER *sd = (USER *)sd_ptr;
    if (!sd || !out_ids) return 0;
    int x, count = 0;
    for (x = 0; x < MAX_SPELLS && count < max; x++) {
        if (sd->status.skill[x])
            out_ids[count++] = (int)sd->status.skill[x];
    }
    return count;
}

/* getSpellNames — fill out_names[0..max) with magicdb names of known spells.
 * Mirrors pcl_getspellname. */
int sl_pc_getspellnames(void *sd_ptr, const char **out_names, int max) {
    USER *sd = (USER *)sd_ptr;
    if (!sd || !out_names) return 0;
    int x, count = 0;
    for (x = 0; x < MAX_SPELLS && count < max; x++) {
        if (sd->status.skill[x])
            out_names[count++] = magicdb_name(sd->status.skill[x]);
    }
    return count;
}

/* getUnknownSpells — no-op: requires SQL query (pcl_getunknownspells in scripting.c).
 * Returns 0; caller should use the scripting.c version for real behaviour. */
int sl_pc_getunknownspells(void *sd_ptr, int *out_ids, int max) {
    (void)sd_ptr; (void)out_ids; (void)max;
    /* SQL-backed; not implementable without lua_State. */
    return 0;
}

/* --- Legends --- */

/* getLegend — return the text of the first legend matching name, or NULL. */
const char *sl_pc_getlegend(void *sd_ptr, const char *name) {
    USER *sd = (USER *)sd_ptr;
    if (!sd || !name) return NULL;
    int x;
    for (x = 0; x < MAX_LEGENDS; x++) {
        if (!strcasecmp(sd->status.legends[x].name, name))
            return sd->status.legends[x].text;
    }
    return NULL;
}

/* --- Combat --- */

/* giveXP — call pc_givexp with the global xp_rate multiplier. */
void sl_pc_givexp(void *sd_ptr, unsigned int amount) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    pc_givexp(sd, amount, xp_rate);
}

/* updateState — broadcast state packet to nearby players. Mirrors bll_updatestate for PCs. */
void sl_pc_updatestate(void *sd_ptr) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    fprintf(stderr, "[DBG sl_compat] updatestate: id=%u state=%d m=%d x=%d y=%d\n",
            sd->status.id, sd->status.state, sd->bl.m, sd->bl.x, sd->bl.y);
    map_foreachinarea(clif_updatestate, sd->bl.m, sd->bl.x, sd->bl.y, AREA,
                      BL_PC, sd);
}

/* addMagic (addMana alias) — add to sd->status.mp; send status.
 * No dedicated C function; mirrors inline logic from scripting.c. */
void sl_pc_addmagic(void *sd_ptr, int amount) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    sd->status.mp = (unsigned int)((int)sd->status.mp + amount);
    clif_sendstatus(sd, SFLAG_HPMP);
}

/* addManaExtend — no separate "extend" function exists; same as addMagic. */
void sl_pc_addmanaextend(void *sd_ptr, int amount) {
    sl_pc_addmagic(sd_ptr, amount);
}

/* setTimeValues — shift timevalues[] ring buffer, prepend newval.
 * Mirrors pcl_settimevalues. */
void sl_pc_settimevalues(void *sd_ptr, unsigned int newval) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    int i, n = (int)(sizeof(sd->timevalues) / sizeof(sd->timevalues[0]));
    for (i = n - 1; i > 0; i--)
        sd->timevalues[i] = sd->timevalues[i - 1];
    sd->timevalues[0] = newval;
}

/* setPK — record id in sd->pvp[]; mirrors pcl_setpk. */
void sl_pc_setpk(void *sd_ptr, int id) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    int x, exist = -1;
    for (x = 0; x < 20; x++) {
        if ((int)sd->pvp[x][0] == id) { exist = x; break; }
    }
    if (exist != -1) {
        sd->pvp[exist][1] = (unsigned int)time(NULL);
    } else {
        for (x = 0; x < 20; x++) {
            if (!sd->pvp[x][0]) {
                sd->pvp[x][0] = (unsigned int)id;
                sd->pvp[x][1] = (unsigned int)time(NULL);
                clif_getchararea(sd);
                break;
            }
        }
    }
}

/* activeSpells — return 1 if named duration is active on sd, else 0.
 * Mirrors pcl_hasduration logic. */
int sl_pc_activespells(void *sd_ptr, const char *name) {
    USER *sd = (USER *)sd_ptr;
    if (!sd || !name) return 0;
    int id = magicdb_id(name);
    int x;
    for (x = 0; x < MAX_MAGIC_TIMERS; x++) {
        if (sd->status.dura_aether[x].id == (unsigned short)id &&
            sd->status.dura_aether[x].duration > 0)
            return 1;
    }
    return 0;
}

/* getEquippedDura — return durability of equipped item at slot matching id.
 * Returns -1 if not found. */
int sl_pc_getequippeddura(void *sd_ptr, unsigned int id, int slot) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return -1;
    if (slot >= 0 && slot < MAX_EQUIP) {
        if (sd->status.equip[slot].id == id)
            return (int)sd->status.equip[slot].dura;
    } else {
        int x;
        for (x = 0; x < MAX_EQUIP; x++) {
            if (sd->status.equip[x].id == id)
                return (int)sd->status.equip[x].dura;
        }
    }
    return -1;
}

/* addHealth (extend variant) — send health packet without triggering combat scripts.
 * Mirrors pcl_addhealth: negative damage = heal. */
void sl_pc_addhealth_extend(void *sd_ptr, int amount) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    clif_send_pc_healthscript(sd, -amount, 0);
    clif_sendstatus(sd, SFLAG_HPMP);
}

/* removeHealth (extend variant) — reduce HP by damage, send packet. */
void sl_pc_removehealth_extend(void *sd_ptr, int damage) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    if (sd->status.state != PC_DIE) {
        clif_send_pc_healthscript(sd, damage, 0);
        clif_sendstatus(sd, SFLAG_HPMP);
    }
}

/* addHealth2 — heal sd by amount; mirrors pcl_addhealth (type unused). */
void sl_pc_addhealth2(void *sd_ptr, int amount, int type) {
    (void)type;
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    struct block_list *bl = map_id2bl(sd->attacker);
    if (bl && amount > 0)
        sl_doscript_blargs("player_combat", "on_healed", 2, &sd->bl, bl);
    else if (amount > 0)
        sl_doscript_blargs("player_combat", "on_healed", 1, &sd->bl);
    clif_send_pc_healthscript(sd, -amount, 0);
    clif_sendstatus(sd, SFLAG_HPMP);
}

/* removeHealthNoDmgNum — reduce HP without displaying a damage number.
 * No dedicated C function; use clif_send_pc_health which takes type. */
void sl_pc_removehealth_nodmgnum(void *sd_ptr, int damage, int type) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    if (sd->status.state != PC_DIE)
        clif_send_pc_health(sd, damage, type);
}

/* --- Economy --- */

/* addGold — add gold to sd->status.money and send status update. */
void sl_pc_addgold(void *sd_ptr, int amount) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    sd->status.money = (unsigned int)((int)sd->status.money + amount);
    clif_sendstatus(sd, SFLAG_XPMONEY);
}

/* removeGold — subtract gold from sd->status.money (floor at 0). */
void sl_pc_removegold(void *sd_ptr, int amount) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    if ((int)sd->status.money < amount)
        sd->status.money = 0;
    else
        sd->status.money -= (unsigned int)amount;
    clif_sendstatus(sd, SFLAG_XPMONEY);
}

/* logBuySell — no-op: logging is commented out in pcl_logbuysell in scripting.c. */
void sl_pc_logbuysell(void *sd_ptr, unsigned int item, unsigned int amount,
                      unsigned int gold, int flag) {
    (void)sd_ptr; (void)item; (void)amount; (void)gold; (void)flag;
    /* Body intentionally empty — matches pcl_logbuysell which is a commented-out no-op. */
}

/* --- Ranged --- */

/* calcThrow — no dedicated C function; no-op placeholder. */
void sl_pc_calcthrow(void *sd_ptr) {
    (void)sd_ptr;
    /* No pc_calcthrow exists in pc.h */
}

/* calcRangedDamage — no dedicated C function; no-op placeholder. */
int sl_pc_calcrangeddamage(void *sd_ptr, void *bl_ptr) {
    (void)sd_ptr; (void)bl_ptr;
    return 0;
}

/* calcRangedHit — no dedicated C function; no-op placeholder. */
int sl_pc_calcrangedhit(void *sd_ptr, void *bl_ptr) {
    (void)sd_ptr; (void)bl_ptr;
    return 0;
}

/* --- Misc --- */

/* gmMsg — send a GM broadcast message to a specific player.
 * Uses clif_sendmsg (color 0 = system message). */
void sl_pc_gmmsg(void *sd_ptr, const char *msg) {
    USER *sd = (USER *)sd_ptr;
    if (!sd || !msg) return;
    clif_sendmsg(sd, 0, msg);
}

/* talkSelf — send a colored message visible only to the player themselves. */
void sl_pc_talkself(void *sd_ptr, int color, const char *msg) {
    USER *sd = (USER *)sd_ptr;
    if (!sd || !msg) return;
    clif_sendmsg(sd, color, msg);
}

/* broadcastSd — broadcast msg to all PCs on map m.
 * Uses clif_broadcast (same map); bl_ptr ignored. */
void sl_pc_broadcast_sd(void *sd_ptr, const char *msg, int m) {
    (void)sd_ptr;
    if (!msg) return;
    clif_broadcast(msg, m);
}

/* killRank — return kill count for mob_id from sd->status.killreg[].
 * Mirrors pcl_killcount. */
int sl_pc_killrank(void *sd_ptr, int mob_id) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return 0;
    int x;
    for (x = 0; x < MAX_KILLREG; x++) {
        if ((int)sd->status.killreg[x].mob_id == mob_id)
            return (int)sd->status.killreg[x].amount;
    }
    return 0;
}

/* getParcel — no-op: parcel retrieval requires SQL + lua_State (pcl_getparcel).
 * Returns NULL; caller should invoke the Lua-layer function instead. */
void *sl_pc_getparcel(void *sd_ptr) {
    (void)sd_ptr;
    return NULL;
}

/* getParcelList — no-op: requires SQL + lua_State. Returns 0. */
int sl_pc_getparcellist(void *sd_ptr, void **out, int max) {
    (void)sd_ptr; (void)out; (void)max;
    return 0;
}

/* removeParcel — delete a parcel row from Parcels by position.
 * Mirrors pcl_removeparcel (position key only). */
void sl_pc_removeparcel(void *sd_ptr, int sender, unsigned int item,
                        unsigned int amount, int pos, unsigned int owner,
                        const char *engrave, int npcflag) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    (void)sender; (void)item; (void)amount; (void)owner; (void)engrave; (void)npcflag;
    if (SQL_ERROR == Sql_Query(sql_handle,
            "DELETE FROM `Parcels` WHERE `ParChaIdDestination` = '%u' AND"
            " `ParPosition` = '%d'",
            sd->status.id, pos)) {
        Sql_ShowDebug(sql_handle);
    }
    Sql_FreeResult(sql_handle);
}

/* expireItem — expire timed items in inventory and equipped slots.
 * Mirrors pcl_expireitem. */
void sl_pc_expireitem(void *sd_ptr) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    unsigned int t = (unsigned int)time(NULL);
    char msg[255];
    int x, eqdel = 0;

    for (x = 0; x < sd->status.maxinv; x++) {
        if (!sd->status.inventory[x].id) continue;
        if ((sd->status.inventory[x].time > 0 && sd->status.inventory[x].time < t) ||
            ((unsigned int)itemdb_time(sd->status.inventory[x].id) > 0 &&
             (unsigned int)itemdb_time(sd->status.inventory[x].id) < t)) {
            snprintf(msg, sizeof(msg),
                "Your %s has expired! Please visit the cash shop to purchase another.",
                itemdb_name(sd->status.inventory[x].id));
            pc_delitem(sd, x, 1, 8);
            clif_sendminitext(sd, msg);
        }
    }

    for (x = 0; x < sd->status.maxinv; x++) {
        if (!sd->status.inventory[x].id) { eqdel = x; break; }
    }

    for (x = 0; x < MAX_EQUIP; x++) {
        if (!sd->status.equip[x].id) continue;
        if ((sd->status.equip[x].time > 0 && sd->status.equip[x].time < t) ||
            ((unsigned int)itemdb_time(sd->status.equip[x].id) > 0 &&
             (unsigned int)itemdb_time(sd->status.equip[x].id) < t)) {
            snprintf(msg, sizeof(msg),
                "Your %s has expired! Please visit the cash shop to purchase another.",
                itemdb_name(sd->status.equip[x].id));
            pc_unequip(sd, x);
            pc_delitem(sd, eqdel, 1, 8);
            clif_sendminitext(sd, msg);
        }
    }
}

/* addGuide / delGuide — commented out in scripting.c; no-ops here. */
void sl_pc_addguide(void *sd_ptr, int guide) {
    (void)sd_ptr; (void)guide;
    /* pcl_addguide is commented out in scripting.c */
}
void sl_pc_delguide(void *sd_ptr, int guide) {
    (void)sd_ptr; (void)guide;
    /* pcl_delguide is commented out in scripting.c */
}

/* getCreationItems — return item id from sd's creation packet at slot len.
 * Mirrors pcl_getcreationitems: reads RFIFOB(fd, len)-1 as inventory index. */
int sl_pc_getcreationitems(void *sd_ptr, int len, unsigned int *out) {
    USER *sd = (USER *)sd_ptr;
    if (!sd || !out) return 0;
    int curitem = RFIFOB(sd->fd, len) - 1;
    if (curitem >= 0 && curitem < sd->status.maxinv && sd->status.inventory[curitem].id) {
        *out = sd->status.inventory[curitem].id;
        return 1;
    }
    return 0;
}

/* getCreationAmounts — return amount for creation slot.
 * Mirrors pcl_getcreationamounts: stackable items use RFIFOB, non-stackable return 1. */
int sl_pc_getcreationamounts(void *sd_ptr, int len, unsigned int item_id) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return 0;
    if (itemdb_type((int)item_id) < 3 || itemdb_type((int)item_id) > 17)
        return (int)RFIFOB(sd->fd, len);
    return 1;
}

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

/* input — mirrors pcl_input.
 * Sends a text-input dialog to the client.
 * clif_input(sd, npc_id, prompt, item_name) */
void sl_pc_input_send(void *sd_ptr, const char *msg) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    clif_input(sd, sd->last_click, msg, "");
}

/* dialog — mirrors pcl_dialog.
 * Sends a dialog box (with optional previous/next buttons).
 * clif_scriptmes(sd, npc_id, msg, previous, next)
 * The task spec lists a "graphics" array, but the underlying packet only
 * supports previous/next flags; encode previous=graphics[0], next=graphics[1]. */
void sl_pc_dialog_send(void *sd_ptr, const char *msg, int *graphics, int ngraphics) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    int previous = (graphics && ngraphics > 0) ? graphics[0] : 0;
    int next     = (graphics && ngraphics > 1) ? graphics[1] : 0;
    clif_scriptmes(sd, sd->last_click, msg, previous, next);
}

/* dialogseq — mirrors pcl_inputseq.
 * Sends a sequenced dialog box with a menu embedded (clif_inputseq).
 * clif_inputseq(sd, npc_id, title, subtitle, body, menu_opts, n, previous, next)
 * can_continue maps to the "next" flag. */
void sl_pc_dialogseq_send(void *sd_ptr, const char **entries, int n, int can_continue) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    /* entries layout expected by caller: [0]=title, [1]=subtitle, [2]=body,
     * [3..n-1]=menu options.  Remaining entries become the menu options array. */
    const char *title    = (n > 0 && entries) ? entries[0] : "";
    const char *subtitle = (n > 1 && entries) ? entries[1] : "";
    const char *body     = (n > 2 && entries) ? entries[2] : "";
    int nopts = (n > 3) ? n - 3 : 0;
    const char **opts = (nopts > 0) ? (entries + 3) : NULL;
    clif_inputseq(sd, sd->last_click, title, subtitle, body,
                  opts, nopts, 0, can_continue);
}

/* Build a 1-indexed wrapper array: buf[0]=NULL, buf[1..n]=options[0..n-1].
 * clif_scriptmenu / clif_scriptmenuseq loop `for (x = 1; x < size + 1; x++)`
 * so they expect options at indices 1..n, not 0..n-1. */
#define MENU_1IDX(options, n, buf) \
    const char *(buf)[(n) + 1]; \
    (buf)[0] = NULL; \
    for (int _i = 0; _i < (n); _i++) (buf)[_i + 1] = (options)[_i]

/* menu — mirrors pcl_menu.
 * Sends a menu to the client via the seq-menu packet.
 * clif_scriptmenuseq(sd, npc_id, topic, options[], n, previous, next) */
void sl_pc_menu_send(void *sd_ptr, const char *msg, const char **options, int n) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    MENU_1IDX(options, n, buf);
    clif_scriptmenuseq(sd, sd->last_click, msg, buf, n, 0, 0);
}

/* menuseq — mirrors pcl_menuseq (same packet as menu, different Lua name).
 * clif_scriptmenuseq(sd, npc_id, topic, options[], n, previous, next) */
void sl_pc_menuseq_send(void *sd_ptr, const char *msg, const char **options, int n) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    MENU_1IDX(options, n, buf);
    clif_scriptmenuseq(sd, sd->last_click, msg, buf, n, 0, 0);
}

/* menustring — uses the non-seq menu packet (clif_scriptmenu).
 * clif_scriptmenu(sd, npc_id, topic, options[], n)
 * Note: clif_scriptmenu takes non-const char* arrays; cast away const. */
void sl_pc_menustring_send(void *sd_ptr, const char *msg, const char **options, int n) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    MENU_1IDX(options, n, buf);
    /* clif_scriptmenu (0x2F) was never handled by the client; use
     * clif_scriptmenuseq (0x30) instead — the client responds via 0x3A/case-0x02,
     * which reaches resume_menuseq.  previous=0, next=0 for a plain string menu. */
    clif_scriptmenuseq(sd, sd->last_click, msg, buf, n, 0, 0);
}

/* menustring2 — no corresponding clif_ function exists in the original codebase.
 * No-op: packet variant was never implemented on the server. */
void sl_pc_menustring2_send(void *sd_ptr, const char *msg, const char **options, int n) {
    (void)sd_ptr; (void)msg; (void)options; (void)n;
    /* no matching clif_ function */
}

/* buy — mirrors pcl_buy.
 * Constructs an item[] array from raw ids/displaynames/buytexts and calls
 * clif_buydialog(sd, npc_id, dialog, item[], price[], count).
 *
 * items[i]      — item id (resolved via itemdb_id-equivalent; passed as id directly)
 * values[i]     — prices
 * displaynames[i] — written into item[i].real_name
 * buytext[i]    — written into item[i].buytext
 */
void sl_pc_buy_send(void *sd_ptr, const char *msg, int *items, int *values,
                    const char **displaynames, const char **buytext, int n) {
    USER *sd = (USER *)sd_ptr;
    if (!sd || n <= 0) return;

    struct item *ilist = (struct item *)calloc((size_t)n, sizeof(struct item));
    if (!ilist) return;

    for (int i = 0; i < n; i++) {
        ilist[i].id = (unsigned int)items[i];
        if (displaynames && displaynames[i])
            strncpy(ilist[i].real_name, displaynames[i], sizeof(ilist[i].real_name) - 1);
        if (buytext && buytext[i])
            strncpy((char *)ilist[i].buytext, buytext[i], sizeof(ilist[i].buytext) - 1);
    }

    clif_buydialog(sd, (unsigned int)sd->last_click, msg, ilist, values, n);
    free(ilist);
}

/* buydialog — simplified buy dialog: only item ids, no prices/display-names.
 * Sends a buy dialog with item ids; prices array left as NULL. */
void sl_pc_buydialog_send(void *sd_ptr, const char *msg, int *items, int n) {
    USER *sd = (USER *)sd_ptr;
    if (!sd || n <= 0) return;

    struct item *ilist = (struct item *)calloc((size_t)n, sizeof(struct item));
    if (!ilist) return;

    for (int i = 0; i < n; i++)
        ilist[i].id = (unsigned int)items[i];

    clif_buydialog(sd, (unsigned int)sd->last_click, msg, ilist, NULL, n);
    free(ilist);
}

/* buyextend — extended buy dialog with separate prices and max amounts.
 * No distinct clif_ function exists; falls back to clif_buydialog with prices. */
void sl_pc_buyextend_send(void *sd_ptr, const char *msg, int *items, int *prices,
                          int *maxamounts, int n) {
    USER *sd = (USER *)sd_ptr;
    if (!sd || n <= 0) return;
    (void)maxamounts; /* max amounts are enforced server-side, not in the packet */

    struct item *ilist = (struct item *)calloc((size_t)n, sizeof(struct item));
    if (!ilist) return;

    for (int i = 0; i < n; i++)
        ilist[i].id = (unsigned int)items[i];

    clif_buydialog(sd, (unsigned int)sd->last_click, msg, ilist, prices, n);
    free(ilist);
}

/* sell — mirrors pcl_sell.
 * Finds inventory slots for each item id, then calls clif_selldialog.
 * clif_selldialog(sd, npc_id, dialog, slot_indices[], count) */
void sl_pc_sell_send(void *sd_ptr, const char *msg, int *items, int n) {
    USER *sd = (USER *)sd_ptr;
    if (!sd || n <= 0) return;

    int slots[MAX_INVENTORY];
    int count = 0;

    for (int j = 0; j < n && count < MAX_INVENTORY; j++) {
        unsigned int item_id = (unsigned int)items[j];
        for (int x = 0; x < sd->status.maxinv && count < MAX_INVENTORY; x++) {
            if (sd->status.inventory[x].id == item_id) {
                slots[count++] = x;
            }
        }
    }
    clif_selldialog(sd, (unsigned int)sd->last_click, msg, slots, count);
}

/* sell2 — variant of sell; no distinct packet, same clif_selldialog call. */
void sl_pc_sell2_send(void *sd_ptr, const char *msg, int *items, int n) {
    sl_pc_sell_send(sd_ptr, msg, items, n);
}

/* sellextend — extended sell dialog; same packet as sell. */
void sl_pc_sellextend_send(void *sd_ptr, const char *msg, int *items, int n) {
    sl_pc_sell_send(sd_ptr, msg, items, n);
}

/* showbank / clan bank / bank-add / bank-money — no corresponding clif_
 * function exists in the original codebase (pcl_bank was a no-op stub).
 * All bank-show variants are no-ops. */
void sl_pc_showbank_send(void *sd_ptr, const char *msg) {
    (void)sd_ptr; (void)msg;
    /* pcl_bank in scripting.c was an empty stub; no clif_ send function exists */
}
void sl_pc_showbankadd_send(void *sd_ptr) {
    (void)sd_ptr;
}
void sl_pc_bankaddmoney_send(void *sd_ptr) {
    (void)sd_ptr;
}
void sl_pc_bankwithdrawmoney_send(void *sd_ptr) {
    (void)sd_ptr;
}
void sl_pc_clanshowbank_send(void *sd_ptr, const char *msg) {
    (void)sd_ptr; (void)msg;
}
void sl_pc_clanshowbankadd_send(void *sd_ptr) {
    (void)sd_ptr;
}
void sl_pc_clanbankaddmoney_send(void *sd_ptr) {
    (void)sd_ptr;
}
void sl_pc_clanbankwithdrawmoney_send(void *sd_ptr) {
    (void)sd_ptr;
}
void sl_pc_clanviewbank_send(void *sd_ptr) {
    (void)sd_ptr;
}

/* repairextend / repairall — no corresponding clif_ function in original codebase.
 * No-ops: repair dialog was never implemented server-side. */
void sl_pc_repairextend_send(void *sd_ptr) {
    (void)sd_ptr;
    /* no clif_ repair dialog function exists in this codebase */
}
void sl_pc_repairall_send(void *sd_ptr, void *npc_bl) {
    (void)sd_ptr; (void)npc_bl;
    /* no clif_ repair dialog function exists in this codebase */
}

// ─── Accessors for Rust client dispatcher — ported to src/game/scripting/pc_accessors.rs ───
// sl_pc_fd, sl_pc_chat_timer, sl_pc_set_chat_timer, sl_pc_attacked, sl_pc_set_attacked,
// sl_pc_loaded, sl_pc_inventory_id — all ported to pc_accessors.rs.
int  sl_map_spell(int m)                       { return map_isloaded(m) ? map[m].spell : 0; }

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

char *clif_getName(unsigned int id) {
  static char name[16];
  memset(name, 0, 16);

  SqlStmt *stmt;
  stmt = SqlStmt_Malloc(sql_handle);
  if (stmt == NULL) { SqlStmt_ShowDebug(stmt); }

  if (SQL_ERROR == SqlStmt_Prepare(stmt, "SELECT `ChaName` FROM `Character` WHERE `ChaId` = '%u'", id) ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_STRING, &name, sizeof(name), NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
  }

  if (SQL_SUCCESS == SqlStmt_NextRow(stmt)) {}

  SqlStmt_Free(stmt);
  return &name[0];
}

int clif_Hacker(char *name, const char *reason) {
  char StringBuffer[1024];
  printf(CL_MAGENTA "%s " CL_NORMAL "possibly hacking" CL_BOLD "%s" CL_NORMAL "\n", name, reason);
  sprintf(StringBuffer, "%s possibly hacking: %s", name, reason);
  clif_broadcasttogm(StringBuffer, -1);
  return 0;
}

int clif_sendurl(USER *sd, int type, const char *url) {
  if (!sd) return 0;

  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOB(sd->fd, 3) = 0x66;
  WFIFOB(sd->fd, 4) = 0x03;
  WFIFOB(sd->fd, 5) = type;
  WFIFOW(sd->fd, 6) = SWAP16(strlen(url));
  memcpy(WFIFOP(sd->fd, 8), url, strlen(url));
  WFIFOW(sd->fd, 1) = SWAP16(strlen(url) + 8);
  WFIFOSET(sd->fd, encrypt(sd->fd));

  return 0;
}

int clif_sendprofile(USER *sd) {
  if (!sd) return 0;

  int len = 0;
  char url[255];
  sprintf(url, "https://www.website.com/users");

  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOB(sd->fd, 3) = 0x62;
  WFIFOB(sd->fd, 5) = 0x04;
  WFIFOB(sd->fd, 6) = strlen(url);
  memcpy(WFIFOP(sd->fd, 7), url, strlen(url));
  len += strlen(url) + 7;
  WFIFOW(sd->fd, 1) = SWAP16(len);
  WFIFOSET(sd->fd, encrypt(sd->fd));

  return 0;
}

int clif_sendboard(USER *sd) {
  int len = 0;

  char url1[] = "https://www.website.com/boards";
  char url2[] = "https://www.website.com/boards";
  char url3[] = "?abc123";

  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOB(sd->fd, 3) = 0x62;
  WFIFOB(sd->fd, 5) = 0x00;

  len += 6;

  WFIFOB(sd->fd, len) = strlen(url1);
  memcpy(WFIFOP(sd->fd, len + 1), url1, strlen(url1));
  len += strlen(url1) + 1;

  WFIFOB(sd->fd, len) = strlen(url2);
  memcpy(WFIFOP(sd->fd, len + 1), url2, strlen(url2));
  len += strlen(url2) + 1;

  WFIFOB(sd->fd, len) = strlen(url3);
  memcpy(WFIFOP(sd->fd, len + 1), url3, strlen(url3));
  len += strlen(url3) + 1;

  WFIFOW(sd->fd, 1) = SWAP16(len);
  WFIFOSET(sd->fd, encrypt(sd->fd));

  return 0;
}

int CheckProximity(struct point one, struct point two, int radius) {
  int ret = 0;
  if (one.m == two.m)
    if (abs(one.x - two.x) <= radius && abs(one.y - two.y) <= radius) ret = 1;
  return ret;
}

int clif_accept2(int fd, char *name, int name_len) {
  char n[32];

  if (name_len <= 0 || name_len > 16) {
    rust_session_set_eof(fd, 11);
    return 0;
  }

  if (rust_should_shutdown()) {
    rust_session_set_eof(fd, 1);
    return 0;
  }
  memset(n, 0, 16);
  memcpy(n, name, name_len);

  int id = 0;

  SqlStmt *stmt;
  stmt = SqlStmt_Malloc(sql_handle);
  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    return -1;
  }

  if (SQL_ERROR == SqlStmt_Prepare(stmt, "SELECT `ChaId` FROM `Character` WHERE `ChaName` = '%s'", n) ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_UINT, &id, 0, NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return -1;
  }

  if (SQL_SUCCESS == SqlStmt_NextRow(stmt)) {
    SqlStmt_Free(stmt);
  }

  intif_load(fd, id, n);
  return 0;
}

int clif_timeout(int fd) {
  USER *sd = NULL;
  int a, b, c, d;

  if (fd == char_fd) return 0;
  if (fd <= 1) return 0;
  if (!rust_session_exists(fd)) return 0;
  if (!rust_session_get_data(fd)) rust_session_set_eof(fd, 12);

  nullpo_ret(0, sd = (USER *)rust_session_get_data(fd));
  a = b = c = d = rust_session_get_client_ip(fd);
  a &= 0xff;
  b = (b >> 8) & 0xff;
  c = (c >> 16) & 0xff;
  d = (d >> 24) & 0xff;

  printf("\033[1;32m%s \033[0m(IP: \033[1;40m%u.%u.%u.%u\033[0m) timed out!\n",
         sd->status.name, a, b, c, d);
  rust_session_set_eof(fd, 1);
  return 0;
}

int clif_popup(USER *sd, const char *buf) {
  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  WFIFOHEAD(sd->fd, strlen(buf) + 5 + 3);
  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOW(sd->fd, 1) = SWAP16(strlen(buf) + 5);
  WFIFOB(sd->fd, 3) = 0x0A;
  WFIFOB(sd->fd, 4) = 0x03;
  WFIFOB(sd->fd, 5) = 0x08;
  WFIFOW(sd->fd, 6) = SWAP16(strlen(buf));
  strcpy(WFIFOP(sd->fd, 8), buf);
  WFIFOSET(sd->fd, encrypt(sd->fd));
  return 0;
}

int clif_paperpopup(USER *sd, const char *buf, int width, int height) {
  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  WFIFOHEAD(sd->fd, strlen(buf) + 11 + 3);
  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOW(sd->fd, 1) = SWAP16(strlen(buf) + 11);
  WFIFOB(sd->fd, 3) = 0x35;
  WFIFOB(sd->fd, 5) = 0;
  WFIFOB(sd->fd, 6) = width;
  WFIFOB(sd->fd, 7) = height;
  WFIFOB(sd->fd, 8) = 0;
  WFIFOW(sd->fd, 9) = SWAP16(strlen(buf));
  strcpy(WFIFOP(sd->fd, 11), buf);
  WFIFOSET(sd->fd, encrypt(sd->fd));
  return 0;
}

int clif_paperpopupwrite(USER *sd, const char *buf, int width, int height, int invslot) {
  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  WFIFOHEAD(sd->fd, strlen(buf) + 11 + 3);
  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOW(sd->fd, 1) = SWAP16(strlen(buf) + 11);
  WFIFOB(sd->fd, 3) = 0x1B;
  WFIFOB(sd->fd, 5) = invslot;
  WFIFOB(sd->fd, 6) = 0;
  WFIFOB(sd->fd, 7) = width;
  WFIFOB(sd->fd, 8) = height;
  WFIFOW(sd->fd, 9) = SWAP16(strlen(buf));
  strcpy(WFIFOP(sd->fd, 11), buf);
  WFIFOSET(sd->fd, encrypt(sd->fd));
  return 0;
}

int clif_paperpopupwrite_save(USER *sd) {
  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  char input[300];
  memset(input, 0, 300);
  memcpy(input, RFIFOP(sd->fd, 8), SWAP16(RFIFOW(sd->fd, 6)));
  unsigned int slot = RFIFOB(sd->fd, 5);

  if (strcmp(sd->status.inventory[slot].note, input) != 0) {
    memcpy(sd->status.inventory[slot].note, input, 300);
  }
  return 0;
}

int stringTruncate(char *buffer, int maxLength) {
  if (!buffer || maxLength <= 0 || strlen(buffer) == maxLength) return 0;
  buffer[maxLength] = '\0';
  return 0;
}

int clif_transfer(USER *sd, int serverid, int m, int x, int y) {
  int len = 0;
  int dest_port;
  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  if (serverid == 0) dest_port = 2001;
  if (serverid == 1) dest_port = 2002;
  if (serverid == 2) dest_port = 2003;

  WFIFOHEAD(sd->fd, 255);
  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOB(sd->fd, 3) = 0x03;
  WFIFOL(sd->fd, 4) = SWAP32(map_ip);
  WFIFOW(sd->fd, 8) = SWAP16(dest_port);
  WFIFOB(sd->fd, 10) = 0x16;
  WFIFOW(sd->fd, 11) = SWAP16(9);
  strcpy(WFIFOP(sd->fd, 13), xor_key);
  len = 11;
  WFIFOB(sd->fd, len + 11) = strlen(sd->status.name);
  strcpy(WFIFOP(sd->fd, len + 12), sd->status.name);
  len += strlen(sd->status.name) + 1;
  len += 4;
  WFIFOB(sd->fd, 10) = len;
  WFIFOW(sd->fd, 1) = SWAP16(len + 8);
  WFIFOSET(sd->fd, len + 11);

  return 0;
}

int clif_transfer_test(USER *sd, int m, int x, int y) {
  int len = 0;
  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  char map_ipaddress_s[] = "192.88.99.100";
  unsigned int map_ipaddress = inet_addr(map_ipaddress_s);

  WFIFOHEAD(sd->fd, 255);
  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOB(sd->fd, 3) = 0x03;
  WFIFOL(sd->fd, 4) = SWAP32(map_ipaddress);
  WFIFOW(sd->fd, 8) = SWAP16(2001);
  WFIFOB(sd->fd, 10) = 0x16;
  WFIFOW(sd->fd, 11) = SWAP16(9);
  strcpy(WFIFOP(sd->fd, 13), xor_key);
  len = 11;
  WFIFOB(sd->fd, len + 11) = strlen("FAKEUSERNAME");
  strcpy(WFIFOP(sd->fd, len + 12), "FAKEUSERNAME");
  len += strlen("FAKEUSERNAME") + 1;
  len += 4;
  WFIFOB(sd->fd, 10) = len;
  WFIFOW(sd->fd, 1) = SWAP16(len + 8);
  WFIFOSET(sd->fd, len + 11);

  return 0;
}

int clif_sendBoardQuestionaire(USER *sd, struct board_questionaire *q, int count) {
  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  WFIFOHEAD(sd->fd, 65535);
  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOB(sd->fd, 3) = 0x31;
  WFIFOB(sd->fd, 5) = 0x09;
  WFIFOB(sd->fd, 6) = count;
  int len = 7;
  for (int i = 0; i < count; i++) {
    WFIFOB(sd->fd, len) = strlen(q[i].header);
    len += 1;
    strcpy(WFIFOP(sd->fd, len), q[i].header);
    len += strlen(q[i].header);
    WFIFOB(sd->fd, len) = 1;
    WFIFOB(sd->fd, len + 1) = 2;
    len += 2;
    WFIFOB(sd->fd, len) = q[i].inputLines;
    len += 1;
    WFIFOB(sd->fd, len) = strlen(q[i].question);
    len += 1;
    strcpy(WFIFOP(sd->fd, len), q[i].question);
    len += strlen(q[i].question);
    WFIFOB(sd->fd, len) = 1;
    len += 1;
  }

  WFIFOB(sd->fd, len) = 0;
  WFIFOB(sd->fd, len + 1) = 0x6B;
  len += 2;

  WFIFOW(sd->fd, 1) = SWAP16(len + 3);
  WFIFOSET(sd->fd, encrypt(sd->fd));
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

int clif_addtokillreg(USER *sd, int mob) {
  USER *tsd = NULL;
  int x;
  nullpo_ret(0, sd);
  for (x = 0; x < sd->group_count; x++) {
    tsd = map_id2sd(groups[sd->groupid][x]);
    if (!tsd) continue;
    if (tsd->bl.m == sd->bl.m) {
      addtokillreg(tsd, mob);
    }
  }
  return 0;
}

int clif_sendheartbeat(int id, int none) {
  USER *sd = map_id2sd((unsigned int)id);
  nullpo_ret(1, sd);

  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  WFIFOHEAD(sd->fd, 7);
  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOW(sd->fd, 1) = SWAP16(0x07);
  WFIFOB(sd->fd, 3) = 0x3B;
  WFIFOB(sd->fd, 5) = 0x5F;
  WFIFOB(sd->fd, 6) = 0x0A;
  WFIFOSET(sd->fd, encrypt(sd->fd));

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

int clif_getequiptype(int val) {
  int type = 0;

  switch (val) {
    case EQ_WEAP:    type = 1;  break;
    case EQ_ARMOR:   type = 2;  break;
    case EQ_SHIELD:  type = 3;  break;
    case EQ_HELM:    type = 4;  break;
    case EQ_NECKLACE: type = 6; break;
    case EQ_LEFT:    type = 7;  break;
    case EQ_RIGHT:   type = 8;  break;
    case EQ_BOOTS:   type = 13; break;
    case EQ_MANTLE:  type = 14; break;
    case EQ_COAT:    type = 16; break;
    case EQ_SUBLEFT:  type = 20; break;
    case EQ_SUBRIGHT: type = 21; break;
    case EQ_FACEACC: type = 22; break;
    case EQ_CROWN:   type = 23; break;
    default: return 0; break;
  }

  return type;
}

static short crctable[256] = {
    0x0000, 0x1021, 0x2042, 0x3063, 0x4084, 0x50A5, 0x60C6, 0x70E7, 0x8108,
    0x9129, 0xA14A, 0xB16B, 0xC18C, 0xD1AD, 0xE1CE, 0xF1EF, 0x1231, 0x0210,
    0x3273, 0x2252, 0x52B5, 0x4294, 0x72F7, 0x62D6, 0x9339, 0x8318, 0xB37B,
    0xA35A, 0xD3BD, 0xC39C, 0xF3FF, 0xE3DE, 0x2462, 0x3443, 0x0420, 0x1401,
    0x64E6, 0x74C7, 0x44A4, 0x5485, 0xA56A, 0xB54B, 0x8528, 0x9509, 0xE5EE,
    0xF5CF, 0xC5AC, 0xD58D, 0x3653, 0x2672, 0x1611, 0x0630, 0x76D7, 0x66F6,
    0x5695, 0x46B4, 0xB75B, 0xA77A, 0x9719, 0x8738, 0xF7DF, 0xE7FE, 0xD79D,
    0xC7BC, 0x48C4, 0x58E5, 0x6886, 0x78A7, 0x0840, 0x1861, 0x2802, 0x3823,
    0xC9CC, 0xD9ED, 0xE98E, 0xF9AF, 0x8948, 0x9969, 0xA90A, 0xB92B, 0x5AF5,
    0x4AD4, 0x7AB7, 0x6A96, 0x1A71, 0x0A50, 0x3A33, 0x2A12, 0xDBFD, 0xCBDC,
    0xFBBF, 0xEB9E, 0x9B79, 0x8B58, 0xBB3B, 0xAB1A, 0x6CA6, 0x7C87, 0x4CE4,
    0x5CC5, 0x2C22, 0x3C03, 0x0C60, 0x1C41, 0xEDAE, 0xFD8F, 0xCDEC, 0xDDCD,
    0xAD2A, 0xBD0B, 0x8D68, 0x9D49, 0x7E97, 0x6EB6, 0x5ED5, 0x4EF4, 0x3E13,
    0x2E32, 0x1E51, 0x0E70, 0xFF9F, 0xEFBE, 0xDFDD, 0xCFFC, 0xBF1B, 0xAF3A,
    0x9F59, 0x8F78, 0x9188, 0x81A9, 0xB1CA, 0xA1EB, 0xD10C, 0xC12D, 0xF14E,
    0xE16F, 0x1080, 0x00A1, 0x30C2, 0x20E3, 0x5004, 0x4025, 0x7046, 0x6067,
    0x83B9, 0x9398, 0xA3FB, 0xB3DA, 0xC33D, 0xD31C, 0xE37F, 0xF35E, 0x02B1,
    0x1290, 0x22F3, 0x32D2, 0x4235, 0x5214, 0x6277, 0x7256, 0xB5EA, 0xA5CB,
    0x95A8, 0x8589, 0xF56E, 0xE54F, 0xD52C, 0xC50D, 0x34E2, 0x24C3, 0x14A0,
    0x0481, 0x7466, 0x6447, 0x5424, 0x4405, 0xA7DB, 0xB7FA, 0x8799, 0x97B8,
    0xE75F, 0xF77E, 0xC71D, 0xD73C, 0x26D3, 0x36F2, 0x0691, 0x16B0, 0x6657,
    0x7676, 0x4615, 0x5634, 0xD94C, 0xC96D, 0xF90E, 0xE92F, 0x99C8, 0x89E9,
    0xB98A, 0xA9AB, 0x5844, 0x4865, 0x7806, 0x6827, 0x18C0, 0x08E1, 0x3882,
    0x28A3, 0xCB7D, 0xDB5C, 0xEB3F, 0xFB1E, 0x8BF9, 0x9BD8, 0xABBB, 0xBB9A,
    0x4A75, 0x5A54, 0x6A37, 0x7A16, 0x0AF1, 0x1AD0, 0x2AB3, 0x3A92, 0xFD2E,
    0xED0F, 0xDD6C, 0xCD4D, 0xBDAA, 0xAD8B, 0x9DE8, 0x8DC9, 0x7C26, 0x6C07,
    0x5C64, 0x4C45, 0x3CA2, 0x2C83, 0x1CE0, 0x0CC1, 0xEF1F, 0xFF3E, 0xCF5D,
    0xDF7C, 0xAF9B, 0xBFBA, 0x8FD9, 0x9FF8, 0x6E17, 0x7E36, 0x4E55, 0x5E74,
    0x2E93, 0x3EB2, 0x0ED1, 0x1EF0};

short nexCRCC(short *buf, int len) {
  unsigned short crc, temp;

  crc = 0;
  while (len != 0) {
    crc = (crctable[crc >> 8] ^ (crc << 8)) ^ buf[0];
    temp = crctable[crc >> 8] ^ buf[1];
    crc = ((temp << 8) ^ crctable[(crc & 0xFF) ^ (temp >> 8)]) ^ buf[2];
    buf += 3;
    len -= 6;
  }
  return (crc);
}

int clif_debug(unsigned char *stringthing, int len) {
  int i;

  for (i = 0; i < len; i++) printf("%02X ", stringthing[i]);
  printf("\n");

  for (i = 0; i < len; i++) {
    if (stringthing[i] <= 32 || stringthing[i] > 126) printf("   ");
    else printf("%02X ", stringthing[i]);
  }

  printf("\n");
  return 0;
}

int clif_user_list(USER *sd) {
  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  if (!char_fd) return 0;
  WFIFOHEAD(char_fd, 4);
  WFIFOW(char_fd, 0) = 0x300B;
  WFIFOW(char_fd, 2) = sd->fd;
  WFIFOSET(char_fd, 4);

  return 0;
}

void clif_delay(int milliseconds) {
  clock_t start_time = clock();
  while (clock() < start_time + milliseconds) ;
}

int clif_quit(USER *sd) {
  map_delblock(&sd->bl);
  clif_lookgone(&sd->bl);
  return 0;
}

unsigned int clif_getlvlxp(int level) {
  double constant = 0.2;
  float xprequired = pow((level / constant), 2);
  return (unsigned int)(xprequired + 0.5);
}

int clif_show_ghost(USER *sd, USER *tsd) {
  if (!sd->status.gm_level) {
    if (!map[sd->bl.m].show_ghosts && tsd->status.state == 1 &&
        sd->bl.id != tsd->bl.id) {
      if (map[sd->bl.m].pvp) {
        if (sd->status.state == 1 && sd->optFlags & optFlag_ghosts) return 1;
        else return 0;
      } else return 1;
    }
  }

  return 1;
}

int clif_getitemarea(USER *sd) {
  return 0;
}

int clif_sendweather(USER *sd) {
  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  WFIFOHEAD(sd->fd, 6);
  WFIFOHEADER(sd->fd, 0x1F, 3);
  WFIFOB(sd->fd, 5) = 0;
  if (sd->status.settingFlags & FLAG_WEATHER)
    WFIFOB(sd->fd, 5) = map[sd->bl.m].weather;
  WFIFOSET(sd->fd, encrypt(sd->fd));
  return 0;
}

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

int clif_sendmob_side(MOB *mob) {
  unsigned char buf[16];
  WBUFB(buf, 0) = 0xAA;
  WBUFB(buf, 1) = 0x00;
  WBUFB(buf, 2) = 0x07;
  WBUFB(buf, 3) = 0x11;
  WBUFB(buf, 4) = 0x03;
  WBUFL(buf, 5) = SWAP32(mob->bl.id);
  WBUFB(buf, 9) = mob->side;
  clif_send(buf, 16, &mob->bl, AREA_WOS);
  return 0;
}

int clif_runfloor_sub(struct block_list *bl, va_list ap) {
  NPC *nd = NULL;
  USER *sd = NULL;

  nullpo_ret(0, nd = (NPC *)bl);
  nullpo_ret(0, sd = va_arg(ap, USER *));

  if (bl->subtype != FLOOR) return 0;

  sl_async_freeco(sd);
  sl_doscript_blargs(nd->name, "click2", 2, &sd->bl, &nd->bl);
  return 0;
}

int clif_parsedropitem(USER *sd) {
  char RegStr[] = "goldbardupe";
  int DupeTimes = pc_readglobalreg(sd, RegStr);
  if (DupeTimes) { return 0; }

  if (sd->status.gm_level == 0) {
    if (sd->status.state == 3) {
      clif_sendminitext(sd, "You cannot do that while riding a mount.");
      return 0;
    }
    if (sd->status.state == 1) {
      clif_sendminitext(sd, "Spirits can't do that.");
      return 0;
    }
  }

  sd->fakeDrop = 0;

  int id = RFIFOB(sd->fd, 5) - 1;
  int all = RFIFOB(sd->fd, 6);
  if (id >= sd->status.maxinv) return 0;
  if (sd->status.inventory[id].id) {
    if (itemdb_droppable(sd->status.inventory[id].id)) {
      clif_sendminitext(sd, "You can't drop this item.");
      return 0;
    }
  }

  clif_sendaction(&sd->bl, 5, 20, 0);

  sd->invslot = id;

  sl_doscript_blargs(itemdb_yname(sd->status.inventory[id].id), "on_drop", 1, &sd->bl);

  for (int x = 0; x < MAX_MAGIC_TIMERS; x++) {
    if (sd->status.dura_aether[x].id > 0 && sd->status.dura_aether[x].duration > 0) {
      sl_doscript_blargs(magicdb_yname(sd->status.dura_aether[x].id), "on_drop_while_cast", 1, &sd->bl);
    }
  }

  for (int x = 0; x < MAX_MAGIC_TIMERS; x++) {
    if (sd->status.dura_aether[x].id > 0 && sd->status.dura_aether[x].aether > 0) {
      sl_doscript_blargs(magicdb_yname(sd->status.dura_aether[x].id), "on_drop_while_aether", 1, &sd->bl);
    }
  }

  if (sd->fakeDrop) return 0;

  pc_dropitemmap(sd, id, all);

  return 0;
}

int clif_mapmsgnum(USER *sd, int id) {
  int msgnum = 0;
  switch (id) {
    case EQ_HELM:     msgnum = MAP_EQHELM;     break;
    case EQ_WEAP:     msgnum = MAP_EQWEAP;     break;
    case EQ_ARMOR:    msgnum = MAP_EQARMOR;    break;
    case EQ_SHIELD:   msgnum = MAP_EQSHIELD;   break;
    case EQ_RIGHT:    msgnum = MAP_EQRIGHT;    break;
    case EQ_LEFT:     msgnum = MAP_EQLEFT;     break;
    case EQ_SUBLEFT:  msgnum = MAP_EQSUBLEFT;  break;
    case EQ_SUBRIGHT: msgnum = MAP_EQSUBRIGHT; break;
    case EQ_FACEACC:  msgnum = MAP_EQFACEACC;  break;
    case EQ_CROWN:    msgnum = MAP_EQCROWN;    break;
    case EQ_BOOTS:    msgnum = MAP_EQBOOTS;    break;
    case EQ_MANTLE:   msgnum = MAP_EQMANTLE;   break;
    case EQ_COAT:     msgnum = MAP_EQCOAT;     break;
    case EQ_NECKLACE: msgnum = MAP_EQNECKLACE; break;
    default: return -1; break;
  }
  return msgnum;
}

int clif_destroyold(USER *sd) {
  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  WFIFOHEAD(sd->fd, 6);
  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOW(sd->fd, 1) = SWAP16(3);
  WFIFOB(sd->fd, 3) = 0x58;
  WFIFOB(sd->fd, 4) = 0x03;
  WFIFOB(sd->fd, 5) = 0x00;
  WFIFOSET(sd->fd, encrypt(sd->fd));
  return 0;
}

int clif_refreshnoclick(USER *sd) {
  clif_sendmapinfo(sd);
  clif_sendxynoclick(sd);
  clif_mob_look_start(sd);
  map_foreachinarea(clif_object_look_sub, sd->bl.m, sd->bl.x, sd->bl.y, SAMEAREA, BL_ALL, LOOK_GET, sd);
  clif_mob_look_close(sd);
  clif_destroyold(sd);
  clif_sendchararea(sd);
  clif_getchararea(sd);

  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  WFIFOHEAD(sd->fd, 5);
  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOW(sd->fd, 1) = SWAP16(2);
  WFIFOB(sd->fd, 3) = 0x22;
  WFIFOB(sd->fd, 4) = 0x03;
  set_packet_indexes((unsigned char *)WFIFOP(sd->fd, 0));
  WFIFOSET(sd->fd, 5 + 3);

  if (!map[sd->bl.m].canGroup) {
    char buff[256];
    sd->status.settingFlags ^= FLAG_GROUP;

    if (sd->status.settingFlags & FLAG_GROUP) {
      // not enabled
    } else {
      if (sd->group_count > 0) {
        clif_leavegroup(sd);
      }
      sprintf(buff, "Join a group     :OFF");
      clif_sendstatus(sd, 0);
      clif_sendminitext(sd, buff);
    }
  }

  return 0;
}

int clif_sendupdatestatus(USER *sd) {
  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  WFIFOHEAD(sd->fd, 33);
  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOB(sd->fd, 1) = 0x00;
  WFIFOB(sd->fd, 2) = 0x1C;
  WFIFOB(sd->fd, 3) = 0x08;
  WFIFOB(sd->fd, 5) = 0x38;
  WFIFOL(sd->fd, 6) = SWAP32(sd->status.hp);
  WFIFOL(sd->fd, 10) = SWAP32(sd->status.mp);
  WFIFOL(sd->fd, 14) = SWAP32(sd->status.exp);
  WFIFOL(sd->fd, 18) = SWAP32(sd->status.money);
  WFIFOL(sd->fd, 22) = 0x00;
  WFIFOB(sd->fd, 26) = 0x00;
  WFIFOB(sd->fd, 27) = 0x00;
  WFIFOB(sd->fd, 28) = sd->blind;
  WFIFOB(sd->fd, 29) = sd->drunk;
  WFIFOB(sd->fd, 30) = 0x00;
  WFIFOB(sd->fd, 31) = 0x73;
  WFIFOB(sd->fd, 32) = 0x35;
  WFIFOSET(sd->fd, encrypt(sd->fd));
  return 0;
}

int clif_sendupdatestatus2(USER *sd) {
  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  float percentage = clif_getXPBarPercent(sd);

  WFIFOHEAD(sd->fd, 25);
  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOB(sd->fd, 3) = 0x08;
  WFIFOB(sd->fd, 5) = 0x18;
  WFIFOL(sd->fd, 6) = SWAP32(sd->status.exp);
  WFIFOL(sd->fd, 10) = SWAP32(sd->status.money);
  WFIFOB(sd->fd, 14) = (int)percentage;
  WFIFOB(sd->fd, 15) = sd->drunk;
  WFIFOB(sd->fd, 16) = sd->blind;
  WFIFOB(sd->fd, 17) = 0x00;
  WFIFOB(sd->fd, 18) = 0x00;
  WFIFOB(sd->fd, 19) = 0x00;
  WFIFOB(sd->fd, 20) = sd->flags;
  WFIFOB(sd->fd, 21) = 0x01;
  WFIFOL(sd->fd, 22) = SWAP32(sd->status.settingFlags);
  WFIFOSET(sd->fd, encrypt(sd->fd));
  return 0;
}

int clif_getLevelTNL(USER *sd) {
  int tnl = 0;

  int path = sd->status.class;
  int level = sd->status.level;
  if (path > 5) path = classdb_path(path);

  if (level < 99) tnl = classdb_level(path, level) - sd->status.exp;

  return tnl;
}

float clif_getXPBarPercent(USER *sd) {
  float percentage;

  int path = sd->status.class;
  int level = sd->status.level;
  int expInLevel = 0;
  int tnl = 0;

  if (path > 5) path = classdb_path(path);

  path = sd->status.class;
  level = sd->status.level;
  if (path > 5) path = classdb_path(path);
  if (level < 99) {
    expInLevel = classdb_level(path, level);
    expInLevel -= classdb_level(path, level - 1);
    tnl = classdb_level(path, level) - (sd->status.exp);
    percentage = (((float)(expInLevel - tnl)) / (expInLevel)) * 100;

    if (!sd->underLevelFlag && sd->status.exp < classdb_level(path, level - 1))
      sd->underLevelFlag = sd->status.level;

    if (sd->underLevelFlag != sd->status.level)
      sd->underLevelFlag = 0;

    if (sd->underLevelFlag)
      percentage = ((float)sd->status.exp / classdb_level(path, level)) * 100;
  } else {
    percentage = ((float)sd->status.exp / 4294967295) * 100;
  }

  return percentage;
}

int clif_sendupdatestatus_onkill(USER *sd) {
  int tnl = clif_getLevelTNL(sd);
  nullpo_ret(0, sd);
  float percentage = clif_getXPBarPercent(sd);

  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  WFIFOHEAD(sd->fd, 33);
  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOB(sd->fd, 1) = 0x00;
  WFIFOB(sd->fd, 2) = 0x1C;
  WFIFOB(sd->fd, 3) = 0x08;
  WFIFOB(sd->fd, 5) = 0x19;
  WFIFOL(sd->fd, 6) = SWAP32(sd->status.exp);
  WFIFOL(sd->fd, 10) = SWAP32(sd->status.money);
  WFIFOB(sd->fd, 14) = (int)percentage;
  WFIFOB(sd->fd, 15) = sd->drunk;
  WFIFOB(sd->fd, 16) = sd->blind;
  WFIFOB(sd->fd, 17) = 0;
  WFIFOB(sd->fd, 18) = 0;
  WFIFOB(sd->fd, 19) = 0;
  WFIFOB(sd->fd, 20) = sd->flags;
  WFIFOB(sd->fd, 21) = 0;
  WFIFOL(sd->fd, 22) = SWAP32(sd->status.settingFlags);
  WFIFOL(sd->fd, 26) = SWAP32(tnl);
  WFIFOB(sd->fd, 30) = sd->armor;
  WFIFOB(sd->fd, 31) = sd->dam;
  WFIFOB(sd->fd, 32) = sd->hit;
  WFIFOSET(sd->fd, encrypt(sd->fd));

  return 0;
}

int clif_sendupdatestatus_onequip(USER *sd) {
  int tnl = clif_getLevelTNL(sd);
  nullpo_ret(0, sd);
  float percentage = clif_getXPBarPercent(sd);

  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  WFIFOHEAD(sd->fd, 62);
  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOB(sd->fd, 1) = 0x00;
  WFIFOB(sd->fd, 2) = 65;
  WFIFOB(sd->fd, 3) = 0x08;
  WFIFOB(sd->fd, 5) = 89;
  WFIFOB(sd->fd, 6) = 0x00;
  WFIFOB(sd->fd, 7) = sd->status.country;
  WFIFOB(sd->fd, 8) = sd->status.totem;
  WFIFOB(sd->fd, 9) = 0x00;
  WFIFOB(sd->fd, 10) = sd->status.level;
  WFIFOL(sd->fd, 11) = SWAP32(sd->max_hp);
  WFIFOL(sd->fd, 15) = SWAP32(sd->max_mp);
  WFIFOB(sd->fd, 19) = sd->might;
  WFIFOB(sd->fd, 20) = sd->will;
  WFIFOB(sd->fd, 21) = 0x03;
  WFIFOB(sd->fd, 22) = 0x03;
  WFIFOB(sd->fd, 23) = sd->grace;
  WFIFOB(sd->fd, 24) = 0;
  WFIFOB(sd->fd, 25) = 0;
  WFIFOB(sd->fd, 26) = 0;
  WFIFOB(sd->fd, 27) = 0;
  WFIFOB(sd->fd, 28) = 0;
  WFIFOB(sd->fd, 29) = 0;
  WFIFOB(sd->fd, 30) = 0;
  WFIFOB(sd->fd, 31) = 0;
  WFIFOB(sd->fd, 32) = 0;
  WFIFOB(sd->fd, 33) = 0;
  WFIFOB(sd->fd, 34) = sd->status.maxinv;
  WFIFOL(sd->fd, 35) = SWAP32(sd->status.exp);
  WFIFOL(sd->fd, 39) = SWAP32(sd->status.money);
  WFIFOB(sd->fd, 43) = (int)percentage;
  WFIFOB(sd->fd, 44) = sd->drunk;
  WFIFOB(sd->fd, 45) = sd->blind;
  WFIFOB(sd->fd, 46) = 0x00;
  WFIFOB(sd->fd, 47) = 0x00;
  WFIFOB(sd->fd, 48) = 0x00;
  WFIFOB(sd->fd, 49) = sd->flags;
  WFIFOB(sd->fd, 50) = 0x00;
  WFIFOL(sd->fd, 51) = SWAP32(sd->status.settingFlags);
  WFIFOL(sd->fd, 55) = SWAP32(tnl);
  WFIFOB(sd->fd, 59) = sd->armor;
  WFIFOB(sd->fd, 60) = sd->dam;
  WFIFOB(sd->fd, 61) = sd->hit;
  WFIFOSET(sd->fd, encrypt(sd->fd));

  return 0;
}

int clif_sendupdatestatus_onunequip(USER *sd) {
  int tnl = clif_getLevelTNL(sd);
  nullpo_ret(0, sd);
  float percentage = clif_getXPBarPercent(sd);

  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  WFIFOHEAD(sd->fd, 52);
  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOB(sd->fd, 1) = 0x00;
  WFIFOB(sd->fd, 2) = 55;
  WFIFOB(sd->fd, 3) = 0x08;
  WFIFOB(sd->fd, 5) = 88;
  WFIFOB(sd->fd, 6) = 0x00;
  WFIFOB(sd->fd, 7) = 20;
  WFIFOB(sd->fd, 8) = 0x00;
  WFIFOB(sd->fd, 9) = 0x00;
  WFIFOB(sd->fd, 10) = 0x00;
  WFIFOL(sd->fd, 11) = sd->status.hp;
  WFIFOL(sd->fd, 15) = sd->status.mp;
  WFIFOB(sd->fd, 19) = 0;
  WFIFOB(sd->fd, 20) = 0;
  WFIFOB(sd->fd, 21) = 0;
  WFIFOB(sd->fd, 22) = 0;
  WFIFOB(sd->fd, 23) = 0;
  WFIFOB(sd->fd, 24) = 0;
  WFIFOB(sd->fd, 25) = 0;
  WFIFOB(sd->fd, 26) = sd->armor;
  WFIFOB(sd->fd, 27) = 0;
  WFIFOB(sd->fd, 28) = 0;
  WFIFOB(sd->fd, 29) = 0;
  WFIFOB(sd->fd, 30) = 0;
  WFIFOB(sd->fd, 31) = 0;
  WFIFOB(sd->fd, 32) = 0;
  WFIFOB(sd->fd, 33) = 0;
  WFIFOB(sd->fd, 34) = 0;
  WFIFOL(sd->fd, 35) = SWAP32(sd->status.exp);
  WFIFOL(sd->fd, 39) = SWAP32(sd->status.money);
  WFIFOB(sd->fd, 43) = (int)percentage;
  WFIFOB(sd->fd, 44) = sd->drunk;
  WFIFOB(sd->fd, 45) = sd->blind;
  WFIFOB(sd->fd, 46) = 0x00;
  WFIFOB(sd->fd, 47) = 0x00;
  WFIFOB(sd->fd, 48) = 0x00;
  WFIFOB(sd->fd, 49) = sd->flags;
  WFIFOL(sd->fd, 50) = tnl;
  WFIFOSET(sd->fd, encrypt(sd->fd));

  return 0;
}

int clif_parselookat_sub(struct block_list *bl, va_list ap) {
  USER *sd = NULL;
  nullpo_ret(0, bl);
  nullpo_ret(0, sd = va_arg(ap, USER *));
  sl_doscript_blargs("onLook", NULL, 2, &sd->bl, bl);
  return 0;
}

int clif_parselookat_scriptsub(USER *sd, struct block_list *bl) {
  /* Body is commented out — was once active but fully dead code */
  return 0;
}

int clif_parselookat_2(USER *sd) {
  int dx = sd->bl.x;
  int dy = sd->bl.y;

  switch (sd->status.side) {
    case 0: dy--; break;
    case 1: dx++; break;
    case 2: dy++; break;
    case 3: dx--; break;
  }

  map_foreachincell(clif_parselookat_sub, sd->bl.m, dx, dy, BL_PC, sd);
  map_foreachincell(clif_parselookat_sub, sd->bl.m, dx, dy, BL_MOB, sd);
  map_foreachincell(clif_parselookat_sub, sd->bl.m, dx, dy, BL_ITEM, sd);
  map_foreachincell(clif_parselookat_sub, sd->bl.m, dx, dy, BL_NPC, sd);
  return 0;
}

int clif_parselookat(USER *sd) {
  int x = 0, y = 0;

  x = SWAP16(RFIFOW(sd->fd, 5));
  y = SWAP16(RFIFOW(sd->fd, 7));

  map_foreachincell(clif_parselookat_sub, sd->bl.m, x, y, BL_PC, sd);
  map_foreachincell(clif_parselookat_sub, sd->bl.m, x, y, BL_MOB, sd);
  map_foreachincell(clif_parselookat_sub, sd->bl.m, x, y, BL_ITEM, sd);
  map_foreachincell(clif_parselookat_sub, sd->bl.m, x, y, BL_NPC, sd);
  return 0;
}

int clif_parsechangepos(USER *sd) {
  if (!RFIFOB(sd->fd, 5)) {
    pc_changeitem(sd, RFIFOB(sd->fd, 6) - 1, RFIFOB(sd->fd, 7) - 1);
  } else {
    clif_sendminitext(sd, "You are busy.");
  }
  return 0;
}

int clif_parseviewchange(USER *sd) {
  int dx = 0, dy = 0;
  int x0, y0, x1, y1, direction = 0;

  direction = RFIFOB(sd->fd, 5);
  dx = RFIFOB(sd->fd, 6);
  dy = RFIFOB(sd->fd, 7);
  x0 = SWAP16(RFIFOW(sd->fd, 8));
  y0 = SWAP16(RFIFOW(sd->fd, 10));
  x1 = RFIFOB(sd->fd, 12);
  y1 = RFIFOB(sd->fd, 13);

  if (sd->status.state == 3) {
    clif_sendminitext(sd, "You cannot do that while riding a mount.");
    return 0;
  }

  switch (direction) {
    case 0: dy++;  break;
    case 1: dx--;  break;
    case 2: dy--;  break;
    case 3: dx++;  break;
    default: break;
  }

  clif_sendxychange(sd, dx, dy);
  clif_mob_look_start(sd);
  map_foreachinblock(clif_object_look_sub, sd->bl.m, x0, y0, x0 + (x1 - 1), y0 + (y1 - 1), BL_ALL, LOOK_GET, sd);
  clif_mob_look_close(sd);
  map_foreachinblock(clif_charlook_sub, sd->bl.m, x0, y0, x0 + (x1 - 1), y0 + (y1 - 1), BL_PC, LOOK_GET, sd);
  map_foreachinblock(clif_cnpclook_sub, sd->bl.m, x0, y0, x0 + (x1 - 1), y0 + (y1 - 1), BL_NPC, LOOK_GET, sd);
  map_foreachinblock(clif_cmoblook_sub, sd->bl.m, x0, y0, x0 + (x1 - 1), y0 + (y1 - 1), BL_MOB, LOOK_GET, sd);
  map_foreachinblock(clif_charlook_sub, sd->bl.m, x0, y0, x0 + (x1 - 1), y0 + (y1 - 1), BL_PC, LOOK_SEND, sd);

  return 0;
}

int clif_parsefriends(USER *sd, char *friendList, int len) {
  int i = 0;
  int j = 0;
  char friends[20][16];
  char escape[16];
  int friendCount = 0;
  SqlStmt *stmt = SqlStmt_Malloc(sql_handle);

  if (stmt == NULL) { SqlStmt_ShowDebug(stmt); return 0; }

  memset(friends, 0, sizeof(char) * 20 * 16);

  do {
    j = 0;
    if (friendList[i] == 0x0C) {
      do {
        i = i + 1;
        friends[friendCount][j] = friendList[i];
        j = j + 1;
      } while (friendList[i] != 0x00);
      friendCount = friendCount + 1;
    }
    i = i + 1;
  } while (i < len);

  if (SQL_ERROR == SqlStmt_Prepare(stmt, "SELECT * FROM `Friends` WHERE `FndChaId` = %d", sd->status.id) ||
      SQL_ERROR == SqlStmt_Execute(stmt)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return 0;
  }

  if (SqlStmt_NumRows(stmt) == 0) {
    if (SQL_ERROR == Sql_Query(sql_handle, "INSERT INTO `Friends` (`FndChaId`) VALUES (%d)", sd->status.id))
      Sql_ShowDebug(sql_handle);
  }

  for (i = 0; i < 20; i++) {
    Sql_EscapeString(sql_handle, escape, friends[i]);
    if (SQL_ERROR == Sql_Query(sql_handle,
                               "UPDATE `Friends` SET `FndChaName%d` = '%s' WHERE `FndChaId` = '%u'",
                               i + 1, escape, sd->status.id))
      Sql_ShowDebug(sql_handle);
  }

  SqlStmt_Free(stmt);
  return 0;
}

int clif_changeprofile(USER *sd) {
  sd->profilepic_size = SWAP16(RFIFOW(sd->fd, 5)) + 2;
  sd->profile_size = RFIFOB(sd->fd, 5 + sd->profilepic_size) + 1;
  memcpy(sd->profilepic_data, RFIFOP(sd->fd, 5), sd->profilepic_size);
  memcpy(sd->profile_data, RFIFOP(sd->fd, 5 + sd->profilepic_size), sd->profile_size);
  return 0;
}

int check_packet_size(int fd, int len) {
  if ((size_t)RFIFOREST(fd) > (size_t)len) {
    if (RFIFOB(fd, len) != 0xAA) {
      rust_session_set_eof(fd, 1);
      return 1;
    }
  }
  return 0;
}

int canusepowerboards(USER *sd) {
  if (sd->status.gm_level) return 1;
  if (!pc_readglobalreg(sd, "carnagehost")) return 0;
  if (sd->bl.m >= 2001 && sd->bl.m <= 2099) return 1;
  return 0;
}

int clif_stoptimers(USER *sd) {
  for (int x = 0; x < MAX_MAGIC_TIMERS; x++) {
    if (sd->status.dura_aether[x].dura_timer) {
      timer_remove(sd->status.dura_aether[x].dura_timer);
    }
    if (sd->status.dura_aether[x].aether_timer) {
      timer_remove(sd->status.dura_aether[x].aether_timer);
    }
  }
  return 0;
}

int clif_handle_disconnect(USER *sd) {
  USER *tsd = NULL;
  if (sd->exchange.target) {
    tsd = map_id2sd(sd->exchange.target);
    clif_exchange_close(sd);

    if (tsd && tsd->exchange.target == sd->bl.id) {
      clif_exchange_message(tsd, "Exchange cancelled.", 4, 0);
      clif_exchange_close(tsd);
    }
  }

  pc_stoptimer(sd);
  sl_async_freeco(sd);

  clif_leavegroup(sd);
  clif_stoptimers(sd);

  sl_doscript_blargs("logout", NULL, 1, &sd->bl);
  intif_savequit(sd);
  clif_quit(sd);
  map_deliddb(&sd->bl);

  if (SQL_ERROR == Sql_Query(sql_handle,
              "UPDATE `Character` SET `ChaOnline` = '0' WHERE `ChaId` = '%u'",
              sd->status.id))
    Sql_ShowDebug(sql_handle);

  printf("[map] [handle_disconnect] name=%s\n", sd->status.name);
  return 0;
}

int clif_handle_missingobject(USER *sd) {
  struct block_list *bl = NULL;
  bl = map_id2bl(SWAP32(RFIFOL(sd->fd, 5)));

  if (bl) {
    if (bl->type == BL_PC) {
      clif_charspecific(sd->status.id, SWAP32(RFIFOL(sd->fd, 5)));
      clif_charspecific(SWAP32(RFIFOL(sd->fd, 5)), sd->status.id);
    } else {
      clif_object_look_specific(sd, SWAP32(RFIFOL(sd->fd, 5)));
    }
  }
  return 0;
}

int clif_handle_menuinput(USER *sd) {
  int npcinf;
  npcinf = RFIFOB(sd->fd, 5);

  if (!hasCoref(sd)) return 0;

  switch (npcinf) {
    case 0: sl_async_freeco(sd); break;
    case 1: clif_parsemenu(sd); break;
    case 2: clif_parsebuy(sd); break;
    case 3: clif_parseinput(sd); break;
    case 4: clif_parsesell(sd); break;
    default: sl_async_freeco(sd); break;
  }

  return 0;
}

int clif_handle_powerboards(USER *sd) {
  USER *tsd = NULL;

  tsd = map_id2sd(SWAP32(RFIFOL(sd->fd, 11)));
  if (tsd)
    sd->pbColor = RFIFOB(sd->fd, 15);
  else
    sd->pbColor = 0;

  if (tsd != NULL)
    sl_doscript_blargs("powerBoard", NULL, 2, &sd->bl, &tsd->bl);
  else
    sl_doscript_blargs("powerBoard", NULL, 2, &sd->bl, 0);

  return 0;
}

int clif_handle_boards(USER *sd) {
  int postcolor;
  switch (RFIFOB(sd->fd, 5)) {
    case 1:
      sd->bcount = 0;
      sd->board_popup = 0;
      clif_showboards(sd);
      break;
    case 2:
      if (RFIFOB(sd->fd, 8) == 127) sd->bcount = 0;
      boards_showposts(sd, SWAP16(RFIFOW(sd->fd, 6)));
      break;
    case 3:
      boards_readpost(sd, SWAP16(RFIFOW(sd->fd, 6)), SWAP16(RFIFOW(sd->fd, 8)));
      break;
    case 4:
      boards_post(sd, SWAP16(RFIFOW(sd->fd, 6)));
      break;
    case 5:
      boards_delete(sd, SWAP16(RFIFOW(sd->fd, 6)));
      break;
    case 6:
      if (sd->status.level >= 10)
        nmail_write(sd);
      else
        clif_sendminitext(sd, "You must be at least level 10 to view/send nmail.");
      break;
    case 7:
      if (sd->status.gm_level) {
        postcolor = map_getpostcolor(SWAP16(RFIFOW(sd->fd, 6)), SWAP16(RFIFOW(sd->fd, 8)));
        postcolor ^= 1;
        map_changepostcolor(SWAP16(RFIFOW(sd->fd, 6)), SWAP16(RFIFOW(sd->fd, 8)), postcolor);
        nmail_sendmessage(sd, "Post updated.", 6, 0);
      }
      break;
    case 8:
      sl_doscript_blargs(boarddb_yname(SWAP16(RFIFOW(sd->fd, 6))), "write", 1, &sd->bl);
    case 9:
      sd->bcount = 0;
      boards_showposts(sd, 0);
      break;
  }
  return 0;
}

int clif_print_disconnect(int fd) {
  if (rust_session_get_eof(fd) == 4) return 0;

  printf(CL_NORMAL "(Reason: " CL_GREEN);
  switch (rust_session_get_eof(fd)) {
    case 0x00:
    case 0x01: printf("NORMAL_EOF"); break;
    case 0x02: printf("SOCKET_SEND_ERROR"); break;
    case 0x03: printf("SOCKET_RECV_ERROR"); break;
    case 0x04: printf("ZERO_RECV_ERROR(NORMAL)"); break;
    case 0x05: printf("MISSING_WDATA"); break;
    case 0x06: printf("WDATA_REALLOC"); break;
    case 0x07: printf("NO_MMO_DATA"); break;
    case 0x08: printf("SESSIONDATA_EXISTS"); break;
    case 0x09: printf("PLAYER_CONNECTING"); break;
    case 0x0A: printf("INVALID_EXCHANGE"); break;
    case 0x0B: printf("ACCEPT_NAMELEN_ERROR"); break;
    case 0x0C: printf("PLAYER_TIMEOUT"); break;
    case 0x0D: printf("INVALID_PACKET_HEADER"); break;
    case 0x0E: printf("WPE_HACK"); break;
    default:   printf("UNKNOWN"); break;
  }
  printf(CL_NORMAL ")\n");
  return 0;
}

unsigned int metacrc(char *file) {
  FILE *fp = NULL;

  unsigned int checksum = 0;
  unsigned int size;
  Bytef fileinf[196608];
  fp = fopen(file, "rb");
  if (!fp) return 0;
  fseek(fp, 0, SEEK_END);
  size = ftell(fp);
  fseek(fp, 0, SEEK_SET);
  fread(fileinf, 1, size, fp);
  fclose(fp);
  checksum = crc32(checksum, fileinf, size);

  return checksum;
}

int send_metafile(USER *sd, char *file) {
  int len = 0;
  unsigned int checksum = 0;
  uLongf clen = 0;
  Bytef *ubuf;
  Bytef *cbuf;
  unsigned int ulen = 0;
  char filebuf[255];
  unsigned int retval;
  FILE *fp = NULL;

  sprintf(filebuf, "%s%s", meta_dir, file);
  checksum = metacrc(filebuf);

  fp = fopen(filebuf, "rb");
  if (!fp) return 0;

  fseek(fp, 0, SEEK_END);
  ulen = ftell(fp);
  fseek(fp, 0, SEEK_SET);
  ubuf = calloc(ulen + 1, sizeof(Bytef));
  clen = compressBound(ulen);
  cbuf = calloc(clen + 1, sizeof(Bytef));
  fread(ubuf, 1, ulen, fp);
  fclose(fp);

  retval = compress(cbuf, &clen, ubuf, ulen);

  if (retval != 0) printf("Error retval=%d\n", retval);
  WFIFOHEAD(sd->fd, 65535 * 2);
  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOB(sd->fd, 3) = 0x6F;
  WFIFOB(sd->fd, 5) = 0;
  WFIFOB(sd->fd, 6) = strlen(file);
  strcpy(WFIFOP(sd->fd, 7), file);
  len += strlen(file) + 1;
  WFIFOL(sd->fd, len + 6) = SWAP32(checksum);
  len += 4;
  WFIFOW(sd->fd, len + 6) = SWAP16(clen);
  len += 2;
  memcpy(WFIFOP(sd->fd, len + 6), cbuf, clen);
  len += clen;
  WFIFOB(sd->fd, len + 6) = 0;
  len += 1;
  WFIFOW(sd->fd, 1) = SWAP16(len + 3);
  set_packet_indexes((unsigned char *)WFIFOP(sd->fd, 0));
  tk_crypt_static((unsigned char *)WFIFOP(sd->fd, 0));
  WFIFOSET(sd->fd, len + 6 + 3);

  free(cbuf);
  free(ubuf);
  return 0;
}

int send_meta(USER *sd) {
  char temp[255];
  memset(temp, 0, 255);
  memcpy(temp, RFIFOP(sd->fd, 7), RFIFOB(sd->fd, 6));
  send_metafile(sd, temp);
  return 0;
}

int send_metalist(USER *sd) {
  int len = 0;
  unsigned int checksum;
  char filebuf[6000];
  int x;

  WFIFOHEAD(sd->fd, 65535 * 2);
  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOB(sd->fd, 3) = 0x6F;
  WFIFOB(sd->fd, 5) = 1;
  WFIFOW(sd->fd, 6) = SWAP16(metamax);
  len += 2;
  for (x = 0; x < metamax; x++) {
    WFIFOB(sd->fd, (len + 6)) = strlen(meta_file[x]);
    memcpy(WFIFOP(sd->fd, len + 7), meta_file[x], strlen(meta_file[x]));
    len += strlen(meta_file[x]) + 1;
    sprintf(filebuf, "%s%s", meta_dir, meta_file[x]);
    checksum = metacrc(filebuf);
    WFIFOL(sd->fd, len + 6) = SWAP32(checksum);
    len += 4;
  }

  WFIFOW(sd->fd, 1) = SWAP16(len + 4);
  set_packet_indexes((unsigned char *)WFIFOP(sd->fd, 0));
  tk_crypt_static((unsigned char *)WFIFOP(sd->fd, 0));
  WFIFOSET(sd->fd, len + 7 + 3);

  return 0;
}

int clif_handle_obstruction(USER *sd) {
  int xold = 0, yold = 0, nx = 0, ny = 0;
  sd->canmove = 0;
  xold = SWAP16(RFIFOW(sd->fd, 5));
  yold = SWAP16(RFIFOW(sd->fd, 7));
  nx = xold;
  ny = yold;

  switch (RFIFOB(sd->fd, 9)) {
    case 0: ny = yold - 1; break;
    case 1: nx = xold + 1; break;
    case 2: ny = yold + 1; break;
    case 3: nx = xold - 1; break;
  }

  sd->bl.x = nx;
  sd->bl.y = ny;
  clif_sendxy(sd);
  return 0;
}

int clif_sendtest(USER *sd) {
  static int number;

  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  WFIFOHEAD(sd->fd, 7);
  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOB(sd->fd, 1) = 0x00;
  WFIFOB(sd->fd, 2) = 0x04;
  WFIFOB(sd->fd, 3) = 0x63;
  WFIFOB(sd->fd, 4) = 0x03;
  WFIFOB(sd->fd, 5) = number;
  WFIFOB(sd->fd, 6) = 0;
  WFIFOSET(sd->fd, encrypt(sd->fd));
  number++;

  return 0;
}

int clif_parsemenu(USER *sd) {
  int selection;
  selection = SWAP16(RFIFOW(sd->fd, 10));
  sl_resumemenu(selection, sd);
  return 0;
}

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

int clif_isregistered(unsigned int id) {
  int accountid = 0;

  SqlStmt *stmt = SqlStmt_Malloc(sql_handle);

  if (stmt == NULL) { SqlStmt_ShowDebug(stmt); return 0; }

  if (SQL_ERROR == SqlStmt_Prepare(stmt,
                       "SELECT `AccountId` FROM `Accounts` WHERE "
                       "`AccountCharId1` = '%u' OR `AccountCharId2` = '%u' OR "
                       "`AccountCharId3` = '%u' OR `AccountCharId4` = '%u' OR "
                       "`AccountCharId5` = '%u' OR `AccountCharId6` = '%u'",
                       id, id, id, id, id, id) ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_UINT, &accountid, 0, NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return 0;
  }

  if (SQL_SUCCESS != SqlStmt_NextRow(stmt)) {}

  return accountid;
}

char *clif_getaccountemail(unsigned int id) {
  char *email;
  CALLOC(email, char, 255);
  memset(email, 0, 255);

  int acctid = clif_isregistered(id);
  if (acctid == 0) return 0;

  SqlStmt *stmt = SqlStmt_Malloc(sql_handle);

  if (stmt == NULL) { SqlStmt_ShowDebug(stmt); return 0; }

  if (SQL_ERROR == SqlStmt_Prepare(stmt,
              "SELECT `AccountEmail` FROM `Accounts` WHERE `AccountId` = '%u'",
              acctid) ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_STRING, &email[0], 255, NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return 0;
  }

  if (SQL_SUCCESS != SqlStmt_NextRow(stmt)) {}

  return &email[0];
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

int clif_object_canmove(int m, int x, int y, int side) {
  int object = read_obj(m, x, y);
  unsigned char flag = objectFlags[object];

  switch (side) {
    case 0: if (flag & OBJ_UP)    return 1; break;
    case 1: if (flag & OBJ_RIGHT) return 1; break;
    case 2: if (flag & OBJ_DOWN)  return 1; break;
    case 3: if (flag & OBJ_LEFT)  return 1; break;
  }

  return 0;
}

int clif_object_canmove_from(int m, int x, int y, int side) {
  int object = read_obj(m, x, y);
  unsigned char flag = objectFlags[object];

  switch (side) {
    case 0: if (flag & OBJ_DOWN)  return 1; break;
    case 1: if (flag & OBJ_LEFT)  return 1; break;
    case 2: if (flag & OBJ_UP)    return 1; break;
    case 3: if (flag & OBJ_RIGHT) return 1; break;
  }

  return 0;
}

int clif_changestatus(USER *sd, int type) {
  int oldm, oldx, oldy;
  char buff[256];

  switch (type) {
    case 0x00:
      if (RFIFOB(sd->fd, 7) == 1) {
        if (sd->status.state == 0) {
          clif_findmount(sd);
          if (sd->status.state == 0)
            clif_sendminitext(sd, "Good try, but there is nothing here that you can ride.");
        } else if (sd->status.state == 1) {
          clif_sendminitext(sd, "Spirits can't do that.");
        } else if (sd->status.state == 2) {
          clif_sendminitext(sd, "Good try, but there is nothing here that you can ride.");
        } else if (sd->status.state == 3) {
          sl_doscript_blargs("onDismount", NULL, 1, &sd->bl);
        } else if (sd->status.state == 4) {
          clif_sendminitext(sd, "You cannot do that while transformed.");
        }
      }
      break;
    case 0x01:
      sd->status.settingFlags ^= FLAG_WHISPER;
      if (sd->status.settingFlags & FLAG_WHISPER) {
        clif_sendminitext(sd, "Listen to whisper:ON");
      } else {
        clif_sendminitext(sd, "Listen to whisper:OFF");
      }
      clif_sendstatus(sd, 0);
      break;
    case 0x02:
      sd->status.settingFlags ^= FLAG_GROUP;
      if (sd->status.settingFlags & FLAG_GROUP) {
        sprintf(buff, "Join a group     :ON");
      } else {
        if (sd->group_count > 0) { clif_leavegroup(sd); }
        sprintf(buff, "Join a group     :OFF");
      }
      clif_sendstatus(sd, 0);
      clif_sendminitext(sd, buff);
      break;
    case 0x03:
      sd->status.settingFlags ^= FLAG_SHOUT;
      if (sd->status.settingFlags & FLAG_SHOUT) {
        clif_sendminitext(sd, "Listen to shout  :ON");
      } else {
        clif_sendminitext(sd, "Listen to shout  :OFF");
      }
      clif_sendstatus(sd, 0);
      break;
    case 0x04:
      sd->status.settingFlags ^= FLAG_ADVICE;
      if (sd->status.settingFlags & FLAG_ADVICE) {
        clif_sendminitext(sd, "Listen to advice :ON");
      } else {
        clif_sendminitext(sd, "Listen to advice :OFF");
      }
      clif_sendstatus(sd, 0);
      break;
    case 0x05:
      sd->status.settingFlags ^= FLAG_MAGIC;
      if (sd->status.settingFlags & FLAG_MAGIC) {
        clif_sendminitext(sd, "Believe in magic :ON");
      } else {
        clif_sendminitext(sd, "Believe in magic :OFF");
      }
      clif_sendstatus(sd, 0);
      break;
    case 0x06:
      sd->status.settingFlags ^= FLAG_WEATHER;
      if (sd->status.settingFlags & FLAG_WEATHER) {
        sprintf(buff, "Weather change   :ON");
      } else {
        sprintf(buff, "Weather change   :OFF");
      }
      clif_sendminitext(sd, buff);
      clif_sendweather(sd);
      clif_sendstatus(sd, 0);
      break;
    case 0x07:
      oldm = sd->bl.m;
      oldx = sd->bl.x;
      oldy = sd->bl.y;
      sd->status.settingFlags ^= FLAG_REALM;
      clif_quit(sd);
      clif_sendmapinfo(sd);
      pc_setpos(sd, oldm, oldx, oldy);
      clif_sendmapinfo(sd);
      clif_spawn(sd);
      clif_mob_look_start(sd);
      map_foreachinarea(clif_object_look_sub, sd->bl.m, sd->bl.x, sd->bl.y, SAMEAREA, BL_ALL, LOOK_GET, sd);
      clif_mob_look_close(sd);
      clif_destroyold(sd);
      clif_sendchararea(sd);
      clif_getchararea(sd);
      if (sd->status.settingFlags & FLAG_REALM) {
        clif_sendminitext(sd, "Realm-centered   :ON");
      } else {
        clif_sendminitext(sd, "Realm-centered   :OFF");
      }
      clif_sendstatus(sd, 0);
      break;
    case 0x08:
      sd->status.settingFlags ^= FLAG_EXCHANGE;
      if (sd->status.settingFlags & FLAG_EXCHANGE) {
        sprintf(buff, "Exchange         :ON");
      } else {
        sprintf(buff, "Exchange         :OFF");
      }
      clif_sendstatus(sd, 0);
      clif_sendminitext(sd, buff);
      break;
    case 0x09:
      sd->status.settingFlags ^= FLAG_FASTMOVE;
      if (sd->status.settingFlags & FLAG_FASTMOVE) {
        clif_sendminitext(sd, "Fast Move        :ON");
      } else {
        clif_sendminitext(sd, "Fast Move        :OFF");
      }
      clif_sendstatus(sd, 0);
      break;
    case 10:
      sd->status.clan_chat = (sd->status.clan_chat + 1) % 2;
      if (sd->status.clan_chat) {
        clif_sendminitext(sd, "Clan whisper     :ON");
      } else {
        clif_sendminitext(sd, "Clan whisper     :OFF");
      }
      break;
    case 13:
      if (RFIFOB(sd->fd, 4) == 3) return 0;
      sd->status.settingFlags ^= FLAG_SOUND;
      if (sd->status.settingFlags & FLAG_SOUND) {
        sprintf(buff, "Hear sounds      :ON");
      } else {
        sprintf(buff, "Hear sounds      :OFF");
      }
      clif_sendminitext(sd, buff);
      clif_sendstatus(sd, 0);
      break;
    case 14:
      sd->status.settingFlags ^= FLAG_HELM;
      if (sd->status.settingFlags & FLAG_HELM) {
        clif_sendminitext(sd, "Show Helmet      :ON");
        pc_setglobalreg(sd, "show_helmet", 1);
      } else {
        clif_sendminitext(sd, "Show Helmet      :OFF");
        pc_setglobalreg(sd, "show_helmet", 0);
      }
      clif_sendstatus(sd, 0);
      clif_sendchararea(sd);
      clif_getchararea(sd);
      break;
    case 15:
      sd->status.settingFlags ^= FLAG_NECKLACE;
      if (sd->status.settingFlags & FLAG_NECKLACE) {
        clif_sendminitext(sd, "Show Necklace      :ON");
        pc_setglobalreg(sd, "show_necklace", 1);
      } else {
        clif_sendminitext(sd, "Show Necklace      :OFF");
        pc_setglobalreg(sd, "show_necklace", 0);
      }
      clif_sendstatus(sd, 0);
      clif_sendchararea(sd);
      clif_getchararea(sd);
      break;
    default: break;
  }

  return 0;
}

int clif_postitem(USER *sd) {
  int slot = RFIFOB(sd->fd, 5) - 1;

  int x = 0;
  int y = 0;

  if (sd->status.side == 0) { x = sd->bl.x; y = sd->bl.y - 1; }
  if (sd->status.side == 1) { x = sd->bl.x + 1; y = sd->bl.y; }
  if (sd->status.side == 2) { x = sd->bl.x; y = sd->bl.y + 1; }
  if (sd->status.side == 3) { x = sd->bl.x - 1; y = sd->bl.y; }

  if (x < 0 || y < 0) return 0;

  int obj = read_obj(sd->bl.m, x, y);

  if (obj == 1619 || obj == 1620) {
    if (sd->status.inventory[slot].amount > 1)
      clif_input(sd, sd->last_click, "How many would you like to post?", "");
  }

  sd->invslot = slot;

  return 0;
}

int clif_pushback(USER *sd) {
  switch (sd->status.side) {
    case 0: pc_warp(sd, sd->bl.m, sd->bl.x, sd->bl.y + 2); break;
    case 1: pc_warp(sd, sd->bl.m, sd->bl.x - 2, sd->bl.y); break;
    case 2: pc_warp(sd, sd->bl.m, sd->bl.x, sd->bl.y - 2); break;
    case 3: pc_warp(sd, sd->bl.m, sd->bl.x + 2, sd->bl.y); break;
  }

  return 0;
}

int clif_cancelafk(USER *sd) {
  nullpo_ret(0, sd);
  sd->afktime = 0;
  sd->afk = 0;
  return 0;
}

int clif_send(const unsigned char *buf, int len, struct block_list *bl, int type) {
  USER *sd = NULL;
  USER *tsd = NULL;
  int i;

  switch (type) {
    case ALL_CLIENT:
    case SAMESRV:
      for (i = 0; i < fd_max; i++) {
        if (rust_session_exists(i) && (sd = rust_session_get_data(i))) {
          if (bl->type == BL_PC) tsd = (USER *)bl;
          if (tsd && RBUFB(buf, 3) == 0x0D && !clif_isignore(tsd, sd)) continue;
          WFIFOHEAD(i, len + 3);
          memcpy(WFIFOP(i, 0), buf, len);
          WFIFOSET(i, encrypt(i));
        }
      }
      break;
    case SAMEMAP:
      for (i = 0; i < fd_max; i++) {
        if (rust_session_exists(i) && (sd = rust_session_get_data(i)) && sd->bl.m == bl->m) {
          if (bl->type == BL_PC) tsd = (USER *)bl;
          if (tsd && RBUFB(buf, 3) == 0x0D && !clif_isignore(tsd, sd)) continue;
          WFIFOHEAD(i, len + 3);
          memcpy(WFIFOP(i, 0), buf, len);
          WFIFOSET(i, encrypt(i));
        }
      }
      break;
    case SAMEMAP_WOS:
      for (i = 0; i < fd_max; i++) {
        if (rust_session_exists(i) && (sd = rust_session_get_data(i)) &&
            sd->bl.m == bl->m && sd != (USER *)bl) {
          if (bl->type == BL_PC) tsd = (USER *)bl;
          if (tsd && RBUFB(buf, 3) == 0x0D && !clif_isignore(tsd, sd)) continue;
          WFIFOHEAD(i, len + 3);
          memcpy(WFIFOP(i, 0), buf, len);
          WFIFOSET(i, encrypt(i));
        }
      }
      break;
    case AREA:
    case AREA_WOS:
      map_foreachinarea(clif_send_sub, bl->m, bl->x, bl->y, AREA, BL_PC, buf, len, bl, type);
      break;
    case SAMEAREA:
    case SAMEAREA_WOS:
      map_foreachinarea(clif_send_sub, bl->m, bl->x, bl->y, SAMEAREA, BL_PC, buf, len, bl, type);
      break;
    case CORNER:
      map_foreachinarea(clif_send_sub, bl->m, bl->x, bl->y, CORNER, BL_PC, buf, len, bl, type);
      break;
    case SELF:
      sd = (USER *)bl;
      WFIFOHEAD(sd->fd, len + 3);
      memcpy(WFIFOP(sd->fd, 0), buf, len);
      WFIFOSET(sd->fd, encrypt(sd->fd));
      break;
  }
  return 0;
}

int clif_sendtogm(unsigned char *buf, int len, struct block_list *bl, int type) {
  USER *sd = NULL;
  int i;

  switch (type) {
    case ALL_CLIENT:
    case SAMESRV:
      for (i = 0; i < fd_max; i++) {
        if (rust_session_exists(i) && (sd = rust_session_get_data(i))) {
          WFIFOHEAD(i, len + 3);
          memcpy(WFIFOP(i, 0), buf, len);
          WFIFOSET(i, encrypt(i));
        }
      }
      break;
    case SAMEMAP:
      for (i = 0; i < fd_max; i++) {
        if (rust_session_exists(i) && (sd = rust_session_get_data(i)) && sd->bl.m == bl->m) {
          WFIFOHEAD(i, len + 3);
          memcpy(WFIFOP(i, 0), buf, len);
          WFIFOSET(i, encrypt(i));
        }
      }
      break;
    case SAMEMAP_WOS:
      for (i = 0; i < fd_max; i++) {
        if (rust_session_exists(i) && (sd = rust_session_get_data(i)) &&
            sd->bl.m == bl->m && sd != (USER *)bl) {
          WFIFOHEAD(i, len + 3);
          memcpy(WFIFOP(i, 0), buf, len);
          WFIFOSET(i, encrypt(i));
        }
      }
      break;
    case AREA:
    case AREA_WOS:
      map_foreachinarea(clif_send_sub, bl->m, bl->x, bl->y, AREA, BL_PC, buf, len, bl, type);
      break;
    case SAMEAREA:
    case SAMEAREA_WOS:
      map_foreachinarea(clif_send_sub, bl->m, bl->x, bl->y, SAMEAREA, BL_PC, buf, len, bl, type);
      break;
    case CORNER:
      map_foreachinarea(clif_send_sub, bl->m, bl->x, bl->y, CORNER, BL_PC, buf, len, bl, type);
      break;
    case SELF:
      sd = (USER *)bl;
      WFIFOHEAD(sd->fd, len + 3);
      memcpy(WFIFOP(sd->fd, 0), buf, len);
      WFIFOSET(sd->fd, encrypt(sd->fd));
      break;
  }
  return 0;
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

int clif_npc_move(struct block_list *bl, va_list ap) {
  unsigned char *buf;
  USER *sd = NULL;
  NPC *nd = NULL;

  va_arg(ap, int);
  nullpo_ret(0, sd = (USER *)bl);
  nullpo_ret(0, nd = va_arg(ap, NPC *));

  CALLOC(buf, unsigned char, 32);
  WBUFB(buf, 0) = 0xAA;
  WBUFB(buf, 1) = 0x00;
  WBUFB(buf, 2) = 0x0C;
  WBUFB(buf, 3) = 0x0C;
  WBUFL(buf, 5) = SWAP32(nd->bl.id);
  WBUFW(buf, 9) = SWAP16(nd->bl.bx);
  WBUFW(buf, 11) = SWAP16(nd->bl.by);
  WBUFB(buf, 13) = nd->side;
  WBUFB(buf, 14) = 0x00;

  clif_send(buf, 32, &nd->bl, AREA_WOS);
  FREE(buf);
  return 0;
}

int clif_mob_move(struct block_list *bl, va_list ap) {
  int type;
  USER *sd = NULL;
  MOB *mob = NULL;
  type = va_arg(ap, int);

  if (type == LOOK_GET) {
    nullpo_ret(0, sd = va_arg(ap, USER *));
    nullpo_ret(0, mob = (MOB *)bl);
    if (mob->state == MOB_DEAD) return 0;
  } else {
    nullpo_ret(0, sd = (USER *)bl);
    nullpo_ret(0, mob = va_arg(ap, MOB *));
    if (mob->state == MOB_DEAD) return 0;
  }

  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  WFIFOHEAD(sd->fd, 14);
  WFIFOHEADER(sd->fd, 0x0C, 11);
  WFIFOL(sd->fd, 5) = SWAP32(mob->bl.id);
  WFIFOW(sd->fd, 9) = SWAP16(mob->bx);
  WFIFOW(sd->fd, 11) = SWAP16(mob->by);
  WFIFOB(sd->fd, 13) = mob->side;
  WFIFOSET(sd->fd, encrypt(sd->fd));
  return 0;
}

// ---------------------------------------------------------------------------
// encrypt / decrypt — moved from c_src/net_crypt.c
// All crypto primitives live in Rust (src/network/crypt.rs); these wrappers
// remain in C because they access FIFO buffers and USER->EncHash.
// ---------------------------------------------------------------------------
int encrypt(int fd) {
  USER *sd = rust_session_get_data(fd);

  if (sd == NULL) {
    printf("[encrypt] sd is NULL for fd=%d\n", fd);
    fflush(stdout);
    return 1;
  }

  unsigned char *buf = (unsigned char *)WFIFOP(fd, 0);
  if (!buf) {
    printf("[encrypt] WFIFOP returned NULL for fd=%d\n", fd);
    fflush(stdout);
    return 1;
  }

  set_packet_indexes(buf);

  if (is_key_server(buf[3])) {
    char key[10];
    generate_key2(buf, sd->EncHash, key, 0);
    tk_crypt_dynamic(buf, key);
  } else {
    tk_crypt_static(buf);
  }
  int pkt_len = (int)SWAP16(*(unsigned short *)(buf + 1)) + 3;
  return pkt_len;
}

int decrypt(int fd) {
  USER *sd = (USER *)rust_session_get_data(fd);

  if (sd == NULL) return 1;

  if (is_key_client(RFIFOB(fd, 3))) {
    char key[10];
    generate_key2((unsigned char *)RFIFOP(fd, 0), sd->EncHash, key, 1);
    tk_crypt_dynamic((unsigned char *)RFIFOP(fd, 0), key);
  } else {
    tk_crypt_static((unsigned char *)RFIFOP(fd, 0));
  }
  return 0;
}

// ---------------------------------------------------------------------------
// createdb_start — moved from c_src/creation_db.c
// Item creation is driven entirely by Lua ("itemCreation" script).
// The old SQL-backed create_db was removed — no DB table exists.
// ---------------------------------------------------------------------------
int createdb_start(USER *sd) {
  int item_c = RFIFOB(sd->fd, 5);
  int item[10], item_amount[10];
  int len = 6;
  int x;
  int curitem;

  for (x = 0; x < item_c; x++) {
    curitem = RFIFOB(sd->fd, len) - 1;
    item[x] = sd->status.inventory[curitem].id;

    if (itemdb_stackamount(item[x]) > 1) {
      item_amount[x] = RFIFOB(sd->fd, len + 1);
      len += 2;
    } else {
      item_amount[x] = 1;
      len += 1;
    }
  }
  sd->creation_works = 0;
  sd->creation_item = 0;
  sd->creation_itemamount = 0;

  printf("creation system executed by: %s\n", sd->status.name);

  lua_newtable(sl_gstate);

  int j, k;

  for (j = 0, k = 1; j < item_c; j++, k += 2) {
    lua_pushnumber(sl_gstate, item[j]);
    lua_rawseti(sl_gstate, -2, k);

    lua_pushnumber(sl_gstate, item_amount[j]);
    lua_rawseti(sl_gstate, -2, k + 1);
  }

  lua_setglobal(sl_gstate, "creationItems");
  lua_settop(sl_gstate, 0);

  sl_async_freeco(sd);
  sl_doscript_blargs("itemCreation", NULL, 1, &sd->bl);

  return 0;
}
