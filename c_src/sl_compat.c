/* sl_compat.c — real C symbols for scripting dispatch.
 *
 * These provide linkable symbols so Rust extern "C" declarations in
 * npc.rs / mob.rs can resolve at link time.  The static inline versions
 * in scripting.h are compiled away and never produce symbols.
 */
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include "scripting.h"
#include "core.h"
#include "net_crypt.h"
#include "session.h"
#include "map_parse.h"
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

int sl_doscript_blargs(char *root, const char *method, int nargs, ...) {
    struct block_list *args[16] = {0};
    va_list ap; va_start(ap, nargs);
    for (int i = 0; i < nargs && i < 16; i++)
        args[i] = va_arg(ap, struct block_list *);
    va_end(ap);
    return rust_sl_doscript_blargs_vec(root, method, nargs, args);
}

int sl_doscript_strings(char *root, const char *method, int nargs, ...) {
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
void sl_g_realtime(int *day, int *hour, int *minute, int *second) {
    time_t now = time(NULL);
    struct tm *t = localtime(&now);
    *day    = t->tm_wday;
    *hour   = t->tm_hour;
    *minute = t->tm_min;
    *second = t->tm_sec;
}

/* --- Warp helpers --- */
int sl_g_getwarp(int m, int x, int y) {
    struct warp_list *i;
    if (!map_isloaded(m)) return 0;
    if (x < 0) x = 0;
    if (y < 0) y = 0;
    if (x >= map[m].xs) x = map[m].xs - 1;
    if (y >= map[m].ys) y = map[m].ys - 1;
    for (i = map[m].warp[x/BLOCK_SIZE + (y/BLOCK_SIZE)*map[m].bxs]; i; i = i->next)
        if (i->x == x && i->y == y) return 1;
    return 0;
}

int sl_g_setwarps(int mm, int mx, int my, int tm_m, int tx, int ty) {
    struct warp_list *war;
    if (!map_isloaded(mm) || !map_isloaded(tm_m)) return 0;
    CALLOC(war, struct warp_list, 1);
    war->x = mx; war->y = my; war->tm = tm_m; war->tx = tx; war->ty = ty;
    war->next = map[mm].warp[(mx/BLOCK_SIZE) + (my/BLOCK_SIZE)*map[mm].bxs];
    if (war->next) war->next->prev = war;
    map[mm].warp[(mx/BLOCK_SIZE) + (my/BLOCK_SIZE)*map[mm].bxs] = war;
    return 1;
}

/* --- Weather --- */
int sl_g_getweather(unsigned char region, unsigned char indoor) {
    int x;
    for (x = 0; x < 65535; x++)
        if (map[x].region == region && map[x].indoor == indoor)
            return map[x].weather;
    return 0;
}

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

/* --- Light setter --- */
void sl_g_setlight(unsigned char region, unsigned char indoor, unsigned char light) {
    int x;
    for (x = 0; x < 65535; x++) {
        if (map_isloaded(x) && map[x].region == region && map[x].indoor == indoor)
            if (map[x].light == 0) map[x].light = light;
    }
}

/* --- SaveMap --- */
int sl_g_savemap(int m, const char *path) {
    FILE *fp;
    short val;
    int x, y;
    if (!path) return 0;
    fp = fopen(path, "wb");
    if (!fp) return 0;
    val = SWAP16(map[m].xs); fwrite(&val, 2, 1, fp);
    val = SWAP16(map[m].ys); fwrite(&val, 2, 1, fp);
    for (y = 0; y < map[m].ys; y++) {
        for (x = 0; x < map[m].xs; x++) {
            int pos = y * map[m].xs + x;
            val = SWAP16(map[m].tile[pos]); fwrite(&val, 2, 1, fp);
            val = SWAP16(map[m].pass[pos]); fwrite(&val, 2, 1, fp);
            val = SWAP16(map[m].obj[pos]);  fwrite(&val, 2, 1, fp);
        }
    }
    fclose(fp);
    return 1;
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
    int i, blockcount;
    FILE *fp;
    if (!mapfile) return -1;
    fp = fopen(mapfile, "rb");
    if (!fp) { printf("MAP_ERR: Map file not found (%s).\n", mapfile); return -1; }
    blockcount = map[m].bxs * map[m].bys;
    if (title) strcpy(map[m].title, title);
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
        FREE(map[m].warp);
        CALLOC(map[m].warp,       struct warp_list *,  map[m].bxs * map[m].bys);
        REALLOC(map[m].block,     struct block_list *, map[m].bxs * map[m].bys);
        REALLOC(map[m].block_mob, struct block_list *, map[m].bxs * map[m].bys);
        if (map[m].bxs * map[m].bys > blockcount) {
            for (i = blockcount; i < map[m].bxs * map[m].bys; i++) {
                map[m].block[i] = NULL; map[m].block_mob[i] = NULL;
            }
        }
    } else {
        CALLOC(map[m].warp,       struct warp_list *,  map[m].bxs * map[m].bys);
        CALLOC(map[m].block,      struct block_list *, map[m].bxs * map[m].bys);
        CALLOC(map[m].block_mob,  struct block_list *, map[m].bxs * map[m].bys);
        CALLOC(map[m].registry,   struct global_reg,   1000);
    }
    while (!feof(fp)) {
        fread(&buff, 2, 1, fp); map[m].tile[pos] = SWAP16(buff);
        fread(&buff, 2, 1, fp); map[m].pass[pos] = SWAP16(buff);
        fread(&buff, 2, 1, fp); map[m].obj[pos]  = SWAP16(buff);
        if (++pos >= (unsigned int)(map[m].xs * map[m].ys)) break;
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

/* --- addMob (SQL) --- */
int sl_g_addmob(int m, int x, int y, int mobid) {
    if (!map_isloaded(m)) return 0;
    if (SQL_ERROR == Sql_Query(sql_handle,
        "INSERT INTO `Spawns%d` (`SpnMapId`,`SpnX`,`SpnY`,`SpnMobId`,"
        "`SpnLastDeath`,`SpnStartTime`,`SpnEndTime`,`SpnMobIdReplace`) "
        "VALUES(%d,%d,%d,%d,0,25,25,0)",
        serverid, m, x, y, mobid)) {
        Sql_ShowDebug(sql_handle); return 0;
    }
    return 1;
}

/* --- checkOnline --- */
int sl_g_checkonline_id(int id) {
    unsigned int cha_id = 0;
    int result = 0;
    SqlStmt *stmt = SqlStmt_Malloc(sql_handle);
    if (!stmt) return 0;
    if (SQL_ERROR == SqlStmt_Prepare(stmt,
        "SELECT `ChaId` FROM `Character` WHERE `ChaOnline`='1' AND `ChaId`='%u'",
        (unsigned)id) ||
        SQL_ERROR == SqlStmt_Execute(stmt) ||
        SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_UINT, &cha_id, 0, NULL, NULL)) {
        SqlStmt_ShowDebug(stmt); SqlStmt_Free(stmt); return 0;
    }
    result = (SQL_SUCCESS == SqlStmt_NextRow(stmt)) ? 1 : 0;
    SqlStmt_Free(stmt);
    return result;
}

int sl_g_checkonline_name(const char *name) {
    unsigned int cha_id = 0;
    int result = 0;
    char esc[128] = {0};
    SqlStmt *stmt = SqlStmt_Malloc(sql_handle);
    if (!stmt) return 0;
    Sql_EscapeStringLen(sql_handle, esc, name, strnlen(name, 64));
    if (SQL_ERROR == SqlStmt_Prepare(stmt,
        "SELECT `ChaId` FROM `Character` WHERE `ChaOnline`='1' AND `ChaName`='%s'",
        esc) ||
        SQL_ERROR == SqlStmt_Execute(stmt) ||
        SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_UINT, &cha_id, 0, NULL, NULL)) {
        SqlStmt_ShowDebug(stmt); SqlStmt_Free(stmt); return 0;
    }
    result = (SQL_SUCCESS == SqlStmt_NextRow(stmt)) ? 1 : 0;
    SqlStmt_Free(stmt);
    return result;
}

/* --- getOfflineID --- */
int sl_g_getofflineid(const char *name) {
    unsigned int id = 0;
    char esc[128] = {0};
    SqlStmt *stmt = SqlStmt_Malloc(sql_handle);
    if (!stmt) return 0;
    Sql_EscapeStringLen(sql_handle, esc, name, strnlen(name, 64));
    if (SQL_ERROR == SqlStmt_Prepare(stmt,
        "SELECT `ChaId` FROM `Character` WHERE `ChaName`='%s'", esc) ||
        SQL_ERROR == SqlStmt_Execute(stmt) ||
        SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_UINT, &id, 0, NULL, NULL)) {
        SqlStmt_ShowDebug(stmt); SqlStmt_Free(stmt); return 0;
    }
    SqlStmt_NextRow(stmt);
    SqlStmt_Free(stmt);
    return (int)id;
}

/* --- MapModifiers --- */
int sl_g_addmapmodifier(unsigned int mapid, const char *modifier, int value) {
    char esc[255];
    Sql_EscapeString(sql_handle, esc, modifier);
    if (SQL_ERROR == Sql_Query(sql_handle,
        "INSERT INTO `MapModifiers` (`ModMapId`,`ModModifier`,`ModValue`) "
        "VALUES('%u','%s','%d')", mapid, esc, value)) {
        Sql_ShowDebug(sql_handle); return 0;
    }
    return 1;
}

int sl_g_removemapmodifier(unsigned int mapid, const char *modifier) {
    char esc[255];
    Sql_EscapeString(sql_handle, esc, modifier);
    if (SQL_ERROR == Sql_Query(sql_handle,
        "DELETE FROM `MapModifiers` WHERE `ModMapId`='%u' AND `ModModifier`='%s'",
        mapid, esc)) {
        Sql_ShowDebug(sql_handle); return 0;
    }
    return 1;
}

int sl_g_removemapmodifierid(unsigned int mapid) {
    if (SQL_ERROR == Sql_Query(sql_handle,
        "DELETE FROM `MapModifiers` WHERE `ModMapId`='%u'", mapid)) {
        Sql_ShowDebug(sql_handle); return 0;
    }
    return 1;
}

int sl_g_getfreemapmodifierid(void) {
    unsigned int mapid = 0;
    SqlStmt *stmt = SqlStmt_Malloc(sql_handle);
    if (!stmt) return 0;
    if (SQL_ERROR == SqlStmt_Prepare(stmt, "SELECT MAX(`ModMapId`) FROM `MapModifiers`") ||
        SQL_ERROR == SqlStmt_Execute(stmt) ||
        SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_UINT, &mapid, 0, NULL, NULL)) {
        SqlStmt_ShowDebug(stmt); SqlStmt_Free(stmt); return 0;
    }
    SqlStmt_NextRow(stmt);
    SqlStmt_Free(stmt);
    return (int)mapid + 1;
}

/* --- WisdomStar --- */
float sl_g_getwisdomstarmultiplier(void) {
    float mult = 0.0f;
    SqlStmt *stmt = SqlStmt_Malloc(sql_handle);
    if (!stmt) return 0.0f;
    if (SQL_ERROR == SqlStmt_Prepare(stmt, "SELECT `WSMultiplier` FROM `WisdomStar`") ||
        SQL_ERROR == SqlStmt_Execute(stmt) ||
        SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_FLOAT, &mult, 0, NULL, NULL)) {
        SqlStmt_ShowDebug(stmt); SqlStmt_Free(stmt); return 0.0f;
    }
    SqlStmt_NextRow(stmt);
    SqlStmt_Free(stmt);
    return mult;
}

void sl_g_setwisdomstarmultiplier(float mult, int value) {
    Sql_Query(sql_handle,
        "UPDATE `WisdomStar` SET `WSMultiplier`='%f',`WSValue`='%d'", mult, value);
}

/* --- KanDonationPoints --- */
int sl_g_getkandonationpoints(void) {
    unsigned int val = 0;
    SqlStmt *stmt = SqlStmt_Malloc(sql_handle);
    if (!stmt) return 0;
    if (SQL_ERROR == SqlStmt_Prepare(stmt, "SELECT `KDPPoints` FROM `KanDonationPool`") ||
        SQL_ERROR == SqlStmt_Execute(stmt) ||
        SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_UINT, &val, 0, NULL, NULL)) {
        SqlStmt_ShowDebug(stmt); SqlStmt_Free(stmt); return 0;
    }
    SqlStmt_NextRow(stmt);
    SqlStmt_Free(stmt);
    return (int)val;
}

void sl_g_setkandonationpoints(int val) {
    Sql_Query(sql_handle, "UPDATE `KanDonationPool` SET `KDPPoints`='%d'", val);
}

void sl_g_addkandonationpoints(int val) {
    Sql_Query(sql_handle,
        "UPDATE `KanDonationPool` SET `KDPPoints`=`KDPPoints`+'%d'", val);
}

/* --- ClanTribute --- */
unsigned int sl_g_getclantribute(int clan) {
    unsigned int val = 0;
    SqlStmt *stmt = SqlStmt_Malloc(sql_handle);
    if (!stmt) return 0;
    if (SQL_ERROR == SqlStmt_Prepare(stmt,
        "SELECT `ClnTribute` FROM `Clans` WHERE `ClnId`='%i'", clan) ||
        SQL_ERROR == SqlStmt_Execute(stmt) ||
        SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_UINT, &val, 0, NULL, NULL)) {
        SqlStmt_ShowDebug(stmt); SqlStmt_Free(stmt); return 0;
    }
    SqlStmt_NextRow(stmt);
    SqlStmt_Free(stmt);
    return val;
}

void sl_g_setclantribute(int clan, unsigned int val) {
    Sql_Query(sql_handle,
        "UPDATE `Clans` SET `ClnTribute`='%u' WHERE `ClnId`='%i'", val, clan);
}

void sl_g_addclantribute(int clan, unsigned int val) {
    Sql_Query(sql_handle,
        "UPDATE `Clans` SET `ClnTribute`=`ClnTribute`+'%u' WHERE `ClnId`='%i'", val, clan);
}

/* --- ClanName --- */
int sl_g_getclanname(int clan, char *buf, int buflen) {
    char name[64] = {0};
    SqlStmt *stmt = SqlStmt_Malloc(sql_handle);
    if (!stmt) return 0;
    if (SQL_ERROR == SqlStmt_Prepare(stmt,
        "SELECT `ClnName` FROM `Clans` WHERE `ClnId`='%i'", clan) ||
        SQL_ERROR == SqlStmt_Execute(stmt) ||
        SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_STRING, &name, sizeof(name), NULL, NULL)) {
        SqlStmt_ShowDebug(stmt); SqlStmt_Free(stmt); return 0;
    }
    if (SQL_SUCCESS == SqlStmt_NextRow(stmt)) {
        strncpy(buf, name, (size_t)(buflen - 1));
        buf[buflen - 1] = '\0';
        SqlStmt_Free(stmt);
        return 1;
    }
    SqlStmt_Free(stmt);
    return 0;
}

void sl_g_setclanname(int clan, const char *name) {
    char esc[128];
    struct ClanData *db;
    Sql_EscapeString(sql_handle, esc, name);
    Sql_Query(sql_handle,
        "UPDATE `Clans` SET `ClnName`='%s' WHERE `ClnId`='%i'", esc, clan);
    db = rust_clandb_searchexist(clan);
    if (db) strncpy(db->name, name, sizeof(db->name) - 1);
}

/* --- ClanBankSlots --- */
int sl_g_getclanbankslots(int clan) {
    int val = 0;
    SqlStmt *stmt = SqlStmt_Malloc(sql_handle);
    if (!stmt) return 0;
    if (SQL_ERROR == SqlStmt_Prepare(stmt,
        "SELECT `ClnBankSlots` FROM `Clans` WHERE `ClnId`='%i'", clan) ||
        SQL_ERROR == SqlStmt_Execute(stmt) ||
        SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_INT, &val, 0, NULL, NULL)) {
        SqlStmt_ShowDebug(stmt); SqlStmt_Free(stmt); return 0;
    }
    SqlStmt_NextRow(stmt);
    SqlStmt_Free(stmt);
    return val;
}

void sl_g_setclanbankslots(int clan, int val) {
    Sql_Query(sql_handle,
        "UPDATE `Clans` SET `ClnBankSlots`='%i' WHERE `ClnId`='%i'", val, clan);
}

/* --- ClanMember --- */
int sl_g_removeclanmember(int id) {
    USER *sd = map_id2sd((unsigned int)id);
    if (sd) {
        sd->status.clan = 0;
        strcpy(sd->status.clan_title, "");
        sd->status.clanRank = 0;
        clif_mystaytus(sd);
    }
    if (SQL_ERROR == Sql_Query(sql_handle,
        "UPDATE `Character` SET `ChaClnId`='0',`ChaClanTitle`='',`ChaClnRank`='0'"
        " WHERE `ChaId`='%u'", (unsigned)id)) {
        Sql_ShowDebug(sql_handle); Sql_FreeResult(sql_handle); return 0;
    }
    Sql_FreeResult(sql_handle);
    return 1;
}

int sl_g_addclanmember(int id, int clan) {
    USER *sd = map_id2sd((unsigned int)id);
    if (sd) {
        sd->status.clan = clan;
        strcpy(sd->status.clan_title, "");
        sd->status.clanRank = 1;
        clif_mystaytus(sd);
    }
    if (SQL_ERROR == Sql_Query(sql_handle,
        "UPDATE `Character` SET `ChaClnId`='%u',`ChaClanTitle`='',`ChaClnRank`='1'"
        " WHERE `ChaId`='%u'", (unsigned)clan, (unsigned)id)) {
        Sql_ShowDebug(sql_handle); Sql_FreeResult(sql_handle); return 0;
    }
    Sql_FreeResult(sql_handle);
    return 1;
}

int sl_g_updateclanmemberrank(int id, int rank) {
    USER *sd = map_id2sd((unsigned int)id);
    if (sd) sd->status.clanRank = rank;
    if (SQL_ERROR == Sql_Query(sql_handle,
        "UPDATE `Character` SET `ChaClnRank`='%u' WHERE `ChaId`='%u'",
        (unsigned)rank, (unsigned)id)) {
        Sql_ShowDebug(sql_handle); Sql_FreeResult(sql_handle); return 0;
    }
    Sql_FreeResult(sql_handle);
    return 1;
}

int sl_g_updateclanmembertitle(int id, const char *title) {
    char esc[128];
    USER *sd = map_id2sd((unsigned int)id);
    if (sd) {
        strncpy(sd->status.clan_title, title, sizeof(sd->status.clan_title) - 1);
        clif_mystaytus(sd);
    }
    Sql_EscapeString(sql_handle, esc, title);
    if (SQL_ERROR == Sql_Query(sql_handle,
        "UPDATE `Character` SET `ChaClanTitle`='%s' WHERE `ChaId`='%u'",
        esc, (unsigned)id)) {
        Sql_ShowDebug(sql_handle); Sql_FreeResult(sql_handle); return 0;
    }
    Sql_FreeResult(sql_handle);
    return 1;
}

/* --- PathMember --- */
int sl_g_removepathember(int id) {
    USER *sd = map_id2sd((unsigned int)id);
    if (sd) {
        sd->status.class = classdb_path(sd->status.class);
        sd->status.classRank = 0;
        clif_mystaytus(sd);
        if (SQL_ERROR == Sql_Query(sql_handle,
            "UPDATE `Character` SET `ChaPthId`='%u',`ChaPthRank`='0' WHERE `ChaId`='%u'",
            (unsigned)sd->status.class, (unsigned)id)) {
            Sql_ShowDebug(sql_handle); Sql_FreeResult(sql_handle); return 0;
        }
        Sql_FreeResult(sql_handle);
        return 1;
    } else {
        unsigned char pth = 0;
        SqlStmt *stmt = SqlStmt_Malloc(sql_handle);
        if (!stmt) return 0;
        if (SQL_ERROR == SqlStmt_Prepare(stmt,
            "SELECT `ChaPthId` FROM `Character` WHERE `ChaId`='%u'", (unsigned)id) ||
            SQL_ERROR == SqlStmt_Execute(stmt) ||
            SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_UCHAR, &pth, 0, NULL, NULL)) {
            SqlStmt_ShowDebug(stmt); SqlStmt_Free(stmt); return 0;
        }
        SqlStmt_NextRow(stmt);
        SqlStmt_Free(stmt);
        pth = (unsigned char)classdb_path(pth);
        if (SQL_ERROR == Sql_Query(sql_handle,
            "UPDATE `Character` SET `ChaPthId`='%u',`ChaPthRank`='0' WHERE `ChaId`='%u'",
            (unsigned)pth, (unsigned)id)) {
            Sql_ShowDebug(sql_handle); Sql_FreeResult(sql_handle); return 0;
        }
        Sql_FreeResult(sql_handle);
        return 1;
    }
}

int sl_g_addpathember(int id, int cls) {
    USER *sd = map_id2sd((unsigned int)id);
    if (sd) { sd->status.class = cls; sd->status.classRank = 0; clif_mystaytus(sd); }
    if (SQL_ERROR == Sql_Query(sql_handle,
        "UPDATE `Character` SET `ChaPthId`='%u',`ChaPthRank`='0' WHERE `ChaId`='%u'",
        (unsigned)cls, (unsigned)id)) {
        Sql_ShowDebug(sql_handle); Sql_FreeResult(sql_handle); return 0;
    }
    Sql_FreeResult(sql_handle);
    return 1;
}

/* --- XP for level --- */
unsigned int sl_g_getxpforlevel(int path, int level) {
    if (path > 5) path = classdb_path(path);
    return classdb_level(path, level);
}

/* -------------------------------------------------------------------------
 * Mob scripting helpers — called from scripting/types/mob.rs.
 * These access MOB and USER fields that Rust cannot safely mirror.
 * --------------------------------------------------------------------- */

/* addHealth: heal mob and dispatch on_healed to the appropriate AI script. */
void sl_mob_addhealth(MOB *mob, int damage) {
    struct block_list *bl = map_id2bl(mob->attacker);
    if (bl != NULL && damage > 0) {
        switch (mob->data->subtype) {
            case 0: sl_doscript_blargs("mob_ai_basic",  "on_healed", 2, &mob->bl, bl); break;
            case 1: sl_doscript_blargs("mob_ai_normal", "on_healed", 2, &mob->bl, bl); break;
            case 2: sl_doscript_blargs("mob_ai_hard",   "on_healed", 2, &mob->bl, bl); break;
            case 3: sl_doscript_blargs("mob_ai_boss",   "on_healed", 2, &mob->bl, bl); break;
            case 4: sl_doscript_blargs(mob->data->yname,"on_healed", 2, &mob->bl, bl); break;
            case 5: sl_doscript_blargs("mob_ai_ghost",  "on_healed", 2, &mob->bl, bl); break;
        }
    } else if (damage > 0) {
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
    if (dx >= map[m].xs) dx = map[m].xs - 1;
    if (dy >= map[m].ys) dy = map[m].ys - 1;
    for (i = map[m].warp[dx/BLOCK_SIZE + (dy/BLOCK_SIZE)*map[m].bxs]; i; i = i->next)
        if (i->x == dx && i->y == dy) return 0;
    map_foreachincell(mob_move, m, dx, dy, BL_MOB, mob);
    map_foreachincell(mob_move, m, dx, dy, BL_PC, mob);
    map_foreachincell(mob_move, m, dx, dy, BL_NPC, mob);
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
            mob->da[x].duration = 0; mob->da[x].id = 0; mob->da[x].caster_id = 0;
            map_foreachinarea(clif_sendanimation, mob->bl.m, mob->bl.x, mob->bl.y,
                              AREA, BL_PC, mob->da[x].animation, &mob->bl, -1);
            mob->da[x].animation = 0;
            if (mob->da[x].caster_id != mob->bl.id) bl = map_id2bl(mob->da[x].caster_id);
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

// ─── Read: bl / map fields (from block_list embedded in USER) ────────────────
int  sl_pc_bl_id(void *sd)   { return ((USER*)sd)->bl.id; }
int  sl_pc_bl_m(void *sd)    { return ((USER*)sd)->bl.m; }
int  sl_pc_bl_x(void *sd)    { return ((USER*)sd)->bl.x; }
int  sl_pc_bl_y(void *sd)    { return ((USER*)sd)->bl.y; }
int  sl_pc_bl_type(void *sd) { return ((USER*)sd)->bl.type; }

// ─── Read: status fields ─────────────────────────────────────────────────────
int  sl_pc_status_id(void *sd)        { return ((USER*)sd)->status.id; }
int  sl_pc_status_hp(void *sd)        { return ((USER*)sd)->status.hp; }
int  sl_pc_status_mp(void *sd)        { return ((USER*)sd)->status.mp; }
int  sl_pc_status_level(void *sd)     { return ((USER*)sd)->status.level; }
int  sl_pc_status_exp(void *sd)       { return (int)((USER*)sd)->status.exp; }
int  sl_pc_status_expsoldmagic(void *sd)  { return ((USER*)sd)->status.expsoldmagic; }
int  sl_pc_status_expsoldhealth(void *sd) { return ((USER*)sd)->status.expsoldhealth; }
int  sl_pc_status_expsoldstats(void *sd)  { return ((USER*)sd)->status.expsoldstats; }
int  sl_pc_status_class(void *sd)     { return ((USER*)sd)->status.class; }
int  sl_pc_status_totem(void *sd)     { return ((USER*)sd)->status.totem; }
int  sl_pc_status_tier(void *sd)      { return ((USER*)sd)->status.tier; }
int  sl_pc_status_mark(void *sd)      { return ((USER*)sd)->status.mark; }
int  sl_pc_status_country(void *sd)   { return ((USER*)sd)->status.country; }
int  sl_pc_status_clan(void *sd)      { return ((USER*)sd)->status.clan; }
int  sl_pc_status_gm_level(void *sd)  { return ((USER*)sd)->status.gm_level; }
int  sl_pc_status_sex(void *sd)       { return ((USER*)sd)->status.sex; }
int  sl_pc_status_side(void *sd)      { return ((USER*)sd)->status.side; }
int  sl_pc_status_state(void *sd)     { return ((USER*)sd)->status.state; }
int  sl_pc_status_face(void *sd)      { return ((USER*)sd)->status.face; }
int  sl_pc_status_hair(void *sd)      { return ((USER*)sd)->status.hair; }
int  sl_pc_status_hair_color(void *sd)  { return ((USER*)sd)->status.hair_color; }
int  sl_pc_status_face_color(void *sd)  { return ((USER*)sd)->status.face_color; }
int  sl_pc_status_armor_color(void *sd) { return ((USER*)sd)->status.armor_color; }
int  sl_pc_status_skin_color(void *sd)  { return ((USER*)sd)->status.skin_color; }
int  sl_pc_status_basehp(void *sd)    { return ((USER*)sd)->status.basehp; }
int  sl_pc_status_basemp(void *sd)    { return ((USER*)sd)->status.basemp; }
int  sl_pc_status_money(void *sd)     { return ((USER*)sd)->status.money; }
int  sl_pc_status_bankmoney(void *sd) { return ((USER*)sd)->status.bankmoney; }
int  sl_pc_status_maxslots(void *sd)  { return ((USER*)sd)->status.maxslots; }
int  sl_pc_status_maxinv(void *sd)    { return ((USER*)sd)->status.maxinv; }
int  sl_pc_status_partner(void *sd)   { return ((USER*)sd)->status.partner; }
int  sl_pc_status_pk(void *sd)        { return ((USER*)sd)->status.pk; }
int  sl_pc_status_killedby(void *sd)  { return ((USER*)sd)->status.killedby; }
int  sl_pc_status_killspk(void *sd)   { return ((USER*)sd)->status.killspk; }
int  sl_pc_status_pkduration(void *sd){ return ((USER*)sd)->status.pkduration; }
int  sl_pc_status_basegrace(void *sd) { return ((USER*)sd)->status.basegrace; }
int  sl_pc_status_basemight(void *sd) { return ((USER*)sd)->status.basemight; }
int  sl_pc_status_basewill(void *sd)  { return ((USER*)sd)->status.basewill; }
int  sl_pc_status_basearmor(void *sd) { return ((USER*)sd)->status.basearmor; }
int  sl_pc_status_tutor(void *sd)     { return ((USER*)sd)->status.tutor; }
int  sl_pc_status_karma(void *sd)     { return ((USER*)sd)->status.karma; }
int  sl_pc_status_alignment(void *sd) { return ((USER*)sd)->status.alignment; }
int  sl_pc_status_classRank(void *sd) { return ((USER*)sd)->status.classRank; }
int  sl_pc_status_clanRank(void *sd)  { return ((USER*)sd)->status.clanRank; }
int  sl_pc_status_novice_chat(void *sd) { return ((USER*)sd)->status.novice_chat; }
int  sl_pc_status_subpath_chat(void *sd){ return ((USER*)sd)->status.subpath_chat; }
int  sl_pc_status_clan_chat(void *sd)  { return ((USER*)sd)->status.clan_chat; }
int  sl_pc_status_miniMapToggle(void *sd){ return ((USER*)sd)->status.miniMapToggle; }
int  sl_pc_status_heroes(void *sd)    { return ((USER*)sd)->status.heroes; }
int  sl_pc_status_mute(void *sd)      { return ((USER*)sd)->status.mute; }
int  sl_pc_status_settingFlags(void *sd){ return (int)((USER*)sd)->status.settingFlags; }
int  sl_pc_status_killspvp(void *sd)  { return ((USER*)sd)->killspvp; }
int  sl_pc_status_profile_vitastats(void *sd)  { return ((USER*)sd)->status.profile_vitastats; }
int  sl_pc_status_profile_equiplist(void *sd)  { return ((USER*)sd)->status.profile_equiplist; }
int  sl_pc_status_profile_legends(void *sd)    { return ((USER*)sd)->status.profile_legends; }
int  sl_pc_status_profile_spells(void *sd)     { return ((USER*)sd)->status.profile_spells; }
int  sl_pc_status_profile_inventory(void *sd)  { return ((USER*)sd)->status.profile_inventory; }
int  sl_pc_status_profile_bankitems(void *sd)  { return ((USER*)sd)->status.profile_bankitems; }
const char* sl_pc_status_name(void *sd)      { return ((USER*)sd)->status.name; }
const char* sl_pc_status_title(void *sd)     { return ((USER*)sd)->status.title; }
const char* sl_pc_status_clan_title(void *sd){ return ((USER*)sd)->status.clan_title; }
const char* sl_pc_status_afkmessage(void *sd){ return ((USER*)sd)->status.afkmessage; }
const char* sl_pc_status_f1name(void *sd)    { return ((USER*)sd)->status.f1name; }

// ─── Read: direct USER fields ─────────────────────────────────────────────────
int  sl_pc_npc_g(void *sd)        { return ((USER*)sd)->npc_g; }
int  sl_pc_npc_gc(void *sd)       { return ((USER*)sd)->npc_gc; }
int  sl_pc_groupid(void *sd)      { return ((USER*)sd)->groupid; }
int  sl_pc_time(void *sd)         { return ((USER*)sd)->time; }
int  sl_pc_fakeDrop(void *sd)     { return ((USER*)sd)->fakeDrop; }
int  sl_pc_max_hp(void *sd)       { return ((USER*)sd)->max_hp; }
int  sl_pc_max_mp(void *sd)       { return ((USER*)sd)->max_mp; }
int  sl_pc_lastvita(void *sd)     { return ((USER*)sd)->lastvita; }
int  sl_pc_rage(void *sd)         { return ((USER*)sd)->rage; }
int  sl_pc_polearm(void *sd)      { return ((USER*)sd)->polearm; }
int  sl_pc_last_click(void *sd)   { return ((USER*)sd)->last_click; }
int  sl_pc_grace(void *sd)        { return ((USER*)sd)->grace; }
int  sl_pc_might(void *sd)        { return ((USER*)sd)->might; }
int  sl_pc_will(void *sd)         { return ((USER*)sd)->will; }
int  sl_pc_armor(void *sd)        { return ((USER*)sd)->armor; }
int  sl_pc_dam(void *sd)          { return ((USER*)sd)->dam; }
int  sl_pc_hit(void *sd)          { return ((USER*)sd)->hit; }
int  sl_pc_miss(void *sd)         { return ((USER*)sd)->miss; }
int  sl_pc_sleep(void *sd)        { return ((USER*)sd)->sleep; }
int  sl_pc_attack_speed(void *sd) { return ((USER*)sd)->attack_speed; }
int  sl_pc_enchanted(void *sd)    { return ((USER*)sd)->enchanted; }
int  sl_pc_confused(void *sd)     { return ((USER*)sd)->confused; }
int  sl_pc_target(void *sd)       { return ((USER*)sd)->target; }
int  sl_pc_deduction(void *sd)    { return ((USER*)sd)->deduction; }
int  sl_pc_speed(void *sd)        { return ((USER*)sd)->speed; }
int  sl_pc_disguise(void *sd)     { return ((USER*)sd)->disguise; }
int  sl_pc_disguise_color(void *sd){ return ((USER*)sd)->disguise_color; }
int  sl_pc_attacker(void *sd)     { return ((USER*)sd)->attacker; }
int  sl_pc_invis(void *sd)        { return ((USER*)sd)->invis; }
int  sl_pc_damage(void *sd)       { return ((USER*)sd)->damage; }
int  sl_pc_crit(void *sd)         { return ((USER*)sd)->crit; }
int  sl_pc_critchance(void *sd)   { return ((USER*)sd)->critchance; }
int  sl_pc_critmult(void *sd)     { return ((USER*)sd)->critmult; }
int  sl_pc_rangeTarget(void *sd)  { return ((USER*)sd)->rangeTarget; }
int  sl_pc_exchange_gold(void *sd){ return ((USER*)sd)->exchange.gold; }
int  sl_pc_exchange_count(void *sd){ return ((USER*)sd)->exchange.item_count; }
int  sl_pc_bod_count(void *sd)    { return ((USER*)sd)->boditems.bod_count; }
int  sl_pc_paralyzed(void *sd)    { return ((USER*)sd)->paralyzed; }
int  sl_pc_blind(void *sd)        { return ((USER*)sd)->blind; }
int  sl_pc_drunk(void *sd)        { return ((USER*)sd)->drunk; }
int  sl_pc_board(void *sd)        { return ((USER*)sd)->board; }
int  sl_pc_board_candel(void *sd) { return ((USER*)sd)->board_candel; }
int  sl_pc_board_canwrite(void *sd){ return ((USER*)sd)->board_canwrite; }
int  sl_pc_boardshow(void *sd)    { return ((USER*)sd)->boardshow; }
int  sl_pc_boardnameval(void *sd) { return ((USER*)sd)->boardnameval; }
int  sl_pc_msPing(void *sd)       { return ((USER*)sd)->msPing; }
int  sl_pc_pbColor(void *sd)      { return ((USER*)sd)->pbColor; }
int  sl_pc_coref(void *sd)        { return (int)((USER*)sd)->coref; }
int  sl_pc_optFlags(void *sd)     { return (int)((USER*)sd)->optFlags; }
int  sl_pc_snare(void *sd)        { return ((USER*)sd)->snare; }
int  sl_pc_silence(void *sd)      { return ((USER*)sd)->silence; }
int  sl_pc_extendhit(void *sd)    { return ((USER*)sd)->extendhit; }
int  sl_pc_afk(void *sd)          { return ((USER*)sd)->afk; }
int  sl_pc_afktime(void *sd)      { return ((USER*)sd)->afktime; }
int  sl_pc_totalafktime(void *sd) { return ((USER*)sd)->totalafktime; }
int  sl_pc_backstab(void *sd)     { return ((USER*)sd)->backstab; }
int  sl_pc_flank(void *sd)        { return ((USER*)sd)->flank; }
int  sl_pc_healing(void *sd)      { return ((USER*)sd)->healing; }
int  sl_pc_minSdam(void *sd)      { return ((USER*)sd)->minSdam; }
int  sl_pc_maxSdam(void *sd)      { return ((USER*)sd)->maxSdam; }
int  sl_pc_minLdam(void *sd)      { return ((USER*)sd)->minLdam; }
int  sl_pc_maxLdam(void *sd)      { return ((USER*)sd)->maxLdam; }
int  sl_pc_talktype(void *sd)     { return ((USER*)sd)->talktype; }
int  sl_pc_equipid(void *sd)      { return ((USER*)sd)->equipid; }
int  sl_pc_takeoffid(void *sd)    { return ((USER*)sd)->takeoffid; }
int  sl_pc_breakid(void *sd)      { return ((USER*)sd)->breakid; }
int  sl_pc_equipslot(void *sd)    { return ((USER*)sd)->equipslot; }
int  sl_pc_invslot(void *sd)      { return ((USER*)sd)->invslot; }
int  sl_pc_pickuptype(void *sd)   { return ((USER*)sd)->pickuptype; }
int  sl_pc_spottraps(void *sd)    { return ((USER*)sd)->spottraps; }
int  sl_pc_fury(void *sd)         { return ((USER*)sd)->fury; }
int  sl_pc_faceacctwo_id(void *sd){ return ((USER*)sd)->status.equip[EQ_FACEACCTWO].id; }
int  sl_pc_faceacctwo_custom(void *sd){ return ((USER*)sd)->status.equip[EQ_FACEACCTWO].custom; }
int  sl_pc_protection(void *sd)   { return ((USER*)sd)->protection; }
int  sl_pc_clone(void *sd)        { return ((USER*)sd)->clone; }
int  sl_pc_wisdom(void *sd)       { return ((USER*)sd)->wisdom; }
int  sl_pc_con(void *sd)          { return ((USER*)sd)->con; }
int  sl_pc_deathflag(void *sd)    { return ((USER*)sd)->deathflag; }
int  sl_pc_selfbar(void *sd)      { return ((USER*)sd)->selfbar; }
int  sl_pc_groupbars(void *sd)    { return ((USER*)sd)->groupbars; }
int  sl_pc_mobbars(void *sd)      { return ((USER*)sd)->mobbars; }
int  sl_pc_disptimertick(void *sd){ return ((USER*)sd)->disptimertick; }
int  sl_pc_bindmap(void *sd)      { return ((USER*)sd)->bindmap; }
int  sl_pc_bindx(void *sd)        { return ((USER*)sd)->bindx; }
int  sl_pc_bindy(void *sd)        { return ((USER*)sd)->bindy; }
int  sl_pc_ambushtimer(void *sd)  { return ((USER*)sd)->ambushtimer; }
int  sl_pc_dialogtype(void *sd)   { return ((USER*)sd)->dialogtype; }
int  sl_pc_cursed(void *sd)       { return ((USER*)sd)->cursed; }
int  sl_pc_action(void *sd)       { return ((USER*)sd)->action; }
int  sl_pc_scripttick(void *sd)   { return ((USER*)sd)->scripttick; }
int  sl_pc_dmgshield(void *sd)    { return ((USER*)sd)->dmgshield; }
int  sl_pc_dmgdealt(void *sd)     { return ((USER*)sd)->dmgdealt; }
int  sl_pc_dmgtaken(void *sd)     { return ((USER*)sd)->dmgtaken; }
const char* sl_pc_ipaddress(void *sd) { return ((USER*)sd)->ipaddress; }
const char* sl_pc_speech(void *sd)    { return ((USER*)sd)->speech; }
const char* sl_pc_question(void *sd)  { return ((USER*)sd)->question; }
const char* sl_pc_mail(void *sd)      { return ((USER*)sd)->mail; }

// ─── Read: GFX fields ────────────────────────────────────────────────────────
int  sl_pc_gfx_face(void *sd)     { return ((USER*)sd)->gfx.face; }
int  sl_pc_gfx_hair(void *sd)     { return ((USER*)sd)->gfx.hair; }
int  sl_pc_gfx_chair(void *sd)    { return ((USER*)sd)->gfx.chair; }
int  sl_pc_gfx_cface(void *sd)    { return ((USER*)sd)->gfx.cface; }
int  sl_pc_gfx_cskin(void *sd)    { return ((USER*)sd)->gfx.cskin; }
int  sl_pc_gfx_dye(void *sd)      { return ((USER*)sd)->gfx.dye; }
int  sl_pc_gfx_weapon(void *sd)   { return ((USER*)sd)->gfx.weapon; }
int  sl_pc_gfx_cweapon(void *sd)  { return ((USER*)sd)->gfx.cweapon; }
int  sl_pc_gfx_armor(void *sd)    { return ((USER*)sd)->gfx.armor; }
int  sl_pc_gfx_carmor(void *sd)   { return ((USER*)sd)->gfx.carmor; }
int  sl_pc_gfx_shield(void *sd)   { return ((USER*)sd)->gfx.shield; }
int  sl_pc_gfx_cshield(void *sd)  { return ((USER*)sd)->gfx.cshield; }
int  sl_pc_gfx_helm(void *sd)     { return ((USER*)sd)->gfx.helm; }
int  sl_pc_gfx_chelm(void *sd)    { return ((USER*)sd)->gfx.chelm; }
int  sl_pc_gfx_mantle(void *sd)   { return ((USER*)sd)->gfx.mantle; }
int  sl_pc_gfx_cmantle(void *sd)  { return ((USER*)sd)->gfx.cmantle; }
int  sl_pc_gfx_crown(void *sd)    { return ((USER*)sd)->gfx.crown; }
int  sl_pc_gfx_ccrown(void *sd)   { return ((USER*)sd)->gfx.ccrown; }
int  sl_pc_gfx_faceAcc(void *sd)  { return ((USER*)sd)->gfx.faceAcc; }
int  sl_pc_gfx_cfaceAcc(void *sd) { return ((USER*)sd)->gfx.cfaceAcc; }
int  sl_pc_gfx_faceAccT(void *sd) { return ((USER*)sd)->gfx.faceAccT; }
int  sl_pc_gfx_cfaceAccT(void *sd){ return ((USER*)sd)->gfx.cfaceAccT; }
int  sl_pc_gfx_boots(void *sd)    { return ((USER*)sd)->gfx.boots; }
int  sl_pc_gfx_cboots(void *sd)   { return ((USER*)sd)->gfx.cboots; }
int  sl_pc_gfx_necklace(void *sd) { return ((USER*)sd)->gfx.necklace; }
int  sl_pc_gfx_cnecklace(void *sd){ return ((USER*)sd)->gfx.cnecklace; }
const char* sl_pc_gfx_name(void *sd){ return ((USER*)sd)->gfx.name; }

// ─── Read: computed / indirect fields ────────────────────────────────────────
extern int   clif_isregistered(unsigned int);

int  sl_pc_actid(void *sd)        { return clif_isregistered(((USER*)sd)->status.id); }
const char* sl_pc_email(void *sd) { return clif_getaccountemail(((USER*)sd)->status.id); }
const char* sl_pc_clanname(void *sd)      { return clandb_name(((USER*)sd)->status.clan); }
int         sl_pc_baseclass(void *sd)     { return classdb_path(((USER*)sd)->status.class); }
const char* sl_pc_baseClassName(void *sd) { return classdb_name(classdb_path(((USER*)sd)->status.class), 0); }
const char* sl_pc_className(void *sd)     { return classdb_name(((USER*)sd)->status.class, 0); }
const char* sl_pc_classNameMark(void *sd) { return classdb_name(((USER*)sd)->status.class, ((USER*)sd)->status.mark); }

// ─── Write: direct field setters ─────────────────────────────────────────────
void sl_pc_set_hp(void *sd, int v)          { ((USER*)sd)->status.hp = v; }
void sl_pc_set_mp(void *sd, int v)          { ((USER*)sd)->status.mp = v; }
void sl_pc_set_max_hp(void *sd, int v)      { ((USER*)sd)->max_hp = v; }
void sl_pc_set_max_mp(void *sd, int v)      { ((USER*)sd)->max_mp = v; }
void sl_pc_set_exp(void *sd, int v)         { ((USER*)sd)->status.exp = v; }
void sl_pc_set_level(void *sd, int v)       { ((USER*)sd)->status.level = v; }
void sl_pc_set_class(void *sd, int v)       { ((USER*)sd)->status.class = v; }
void sl_pc_set_totem(void *sd, int v)       { ((USER*)sd)->status.totem = v; }
void sl_pc_set_tier(void *sd, int v)        { ((USER*)sd)->status.tier = v; }
void sl_pc_set_mark(void *sd, int v)        { ((USER*)sd)->status.mark = v; }
void sl_pc_set_country(void *sd, int v)     { ((USER*)sd)->status.country = v; }
void sl_pc_set_clan(void *sd, int v)        { ((USER*)sd)->status.clan = v; }
void sl_pc_set_gm_level(void *sd, int v)    { ((USER*)sd)->status.gm_level = v; }
void sl_pc_set_side(void *sd, int v)        { ((USER*)sd)->status.side = v; }
void sl_pc_set_state(void *sd, int v)       { ((USER*)sd)->status.state = v; }
void sl_pc_set_hair(void *sd, int v)        { ((USER*)sd)->status.hair = v; }
void sl_pc_set_hair_color(void *sd, int v)  { ((USER*)sd)->status.hair_color = v; }
void sl_pc_set_face_color(void *sd, int v)  { ((USER*)sd)->status.face_color = v; }
void sl_pc_set_armor_color(void *sd, int v) { ((USER*)sd)->status.armor_color = v; }
void sl_pc_set_skin_color(void *sd, int v)  { ((USER*)sd)->status.skin_color = v; }
void sl_pc_set_face(void *sd, int v)        { ((USER*)sd)->status.face = v; }
void sl_pc_set_money(void *sd, int v)       { ((USER*)sd)->status.money = v; }
void sl_pc_set_bankmoney(void *sd, int v)   { ((USER*)sd)->status.bankmoney = v; }
void sl_pc_set_maxslots(void *sd, int v)    { ((USER*)sd)->status.maxslots = v; }
void sl_pc_set_maxinv(void *sd, int v)      { ((USER*)sd)->status.maxinv = v; }
void sl_pc_set_partner(void *sd, int v)     { ((USER*)sd)->status.partner = v; }
void sl_pc_set_pk(void *sd, int v)          { ((USER*)sd)->status.pk = v; }
void sl_pc_set_basehp(void *sd, int v)      { ((USER*)sd)->status.basehp = v; }
void sl_pc_set_basemp(void *sd, int v)      { ((USER*)sd)->status.basemp = v; }
void sl_pc_set_karma(void *sd, int v)       { ((USER*)sd)->status.karma = v; }
void sl_pc_set_alignment(void *sd, int v)   { ((USER*)sd)->status.alignment = v; }
void sl_pc_set_basegrace(void *sd, int v)   { ((USER*)sd)->status.basegrace = v; }
void sl_pc_set_basemight(void *sd, int v)   { ((USER*)sd)->status.basemight = v; }
void sl_pc_set_basewill(void *sd, int v)    { ((USER*)sd)->status.basewill = v; }
void sl_pc_set_basearmor(void *sd, int v)   { ((USER*)sd)->status.basearmor = v; }
void sl_pc_set_novice_chat(void *sd, int v) { ((USER*)sd)->status.novice_chat = v; }
void sl_pc_set_subpath_chat(void *sd, int v){ ((USER*)sd)->status.subpath_chat = v; }
void sl_pc_set_clan_chat(void *sd, int v)   { ((USER*)sd)->status.clan_chat = v; }
void sl_pc_set_tutor(void *sd, int v)       { ((USER*)sd)->status.tutor = v; }
void sl_pc_set_profile_vitastats(void *sd, int v) { ((USER*)sd)->status.profile_vitastats = v; }
void sl_pc_set_profile_equiplist(void *sd, int v) { ((USER*)sd)->status.profile_equiplist = v; }
void sl_pc_set_profile_legends(void *sd, int v)   { ((USER*)sd)->status.profile_legends = v; }
void sl_pc_set_profile_spells(void *sd, int v)    { ((USER*)sd)->status.profile_spells = v; }
void sl_pc_set_profile_inventory(void *sd, int v) { ((USER*)sd)->status.profile_inventory = v; }
void sl_pc_set_profile_bankitems(void *sd, int v) { ((USER*)sd)->status.profile_bankitems = v; }
void sl_pc_set_npc_g(void *sd, int v)       { ((USER*)sd)->npc_g = v; }
void sl_pc_set_npc_gc(void *sd, int v)      { ((USER*)sd)->npc_gc = v; }
void sl_pc_set_last_click(void *sd, int v)  { ((USER*)sd)->last_click = v; }
void sl_pc_set_time(void *sd, int v)        { ((USER*)sd)->time = v; }
void sl_pc_set_rage(void *sd, int v)        { ((USER*)sd)->rage = v; }
void sl_pc_set_polearm(void *sd, int v)     { ((USER*)sd)->polearm = v; }
void sl_pc_set_deduction(void *sd, int v)   { ((USER*)sd)->deduction = v; }
void sl_pc_set_speed(void *sd, int v)       { ((USER*)sd)->speed = v; }
void sl_pc_set_attacker(void *sd, int v)    { ((USER*)sd)->attacker = v; }
void sl_pc_set_invis(void *sd, int v)       { ((USER*)sd)->invis = v; }
void sl_pc_set_damage(void *sd, int v)      { ((USER*)sd)->damage = v; }
void sl_pc_set_crit(void *sd, int v)        { ((USER*)sd)->crit = v; }
void sl_pc_set_critchance(void *sd, int v)  { ((USER*)sd)->critchance = v; }
void sl_pc_set_critmult(void *sd, int v)    { ((USER*)sd)->critmult = v; }
void sl_pc_set_rangeTarget(void *sd, int v) { ((USER*)sd)->rangeTarget = v; }
void sl_pc_set_disguise(void *sd, int v)    { ((USER*)sd)->disguise = v; }
void sl_pc_set_disguise_color(void *sd, int v){ ((USER*)sd)->disguise_color = v; }
void sl_pc_set_paralyzed(void *sd, int v)   { ((USER*)sd)->paralyzed = v; }
void sl_pc_set_blind(void *sd, int v)       { ((USER*)sd)->blind = v; }
void sl_pc_set_drunk(void *sd, int v)       { ((USER*)sd)->drunk = v; }
void sl_pc_set_board_candel(void *sd, int v){ ((USER*)sd)->board_candel = v; }
void sl_pc_set_board_canwrite(void *sd, int v){ ((USER*)sd)->board_canwrite = v; }
void sl_pc_set_boardshow(void *sd, int v)   { ((USER*)sd)->boardshow = v; }
void sl_pc_set_boardnameval(void *sd, int v){ ((USER*)sd)->boardnameval = v; }
void sl_pc_set_snare(void *sd, int v)       { ((USER*)sd)->snare = v; }
void sl_pc_set_silence(void *sd, int v)     { ((USER*)sd)->silence = v; }
void sl_pc_set_extendhit(void *sd, int v)   { ((USER*)sd)->extendhit = v; }
void sl_pc_set_afk(void *sd, int v)         { ((USER*)sd)->afk = v; }
void sl_pc_set_confused(void *sd, int v)    { ((USER*)sd)->confused = v; }
void sl_pc_set_spottraps(void *sd, int v)   { ((USER*)sd)->spottraps = v; }
void sl_pc_set_selfbar(void *sd, int v)     { ((USER*)sd)->selfbar = v; }
void sl_pc_set_groupbars(void *sd, int v)   { ((USER*)sd)->groupbars = v; }
void sl_pc_set_mobbars(void *sd, int v)     { ((USER*)sd)->mobbars = v; }
void sl_pc_set_mute(void *sd, int v)        { ((USER*)sd)->status.mute = v; }
void sl_pc_set_settingFlags(void *sd, int v){ ((USER*)sd)->status.settingFlags = (unsigned int)v; }
void sl_pc_set_optFlags_xor(void *sd, int v){ ((USER*)sd)->optFlags ^= (unsigned int)v; }
void sl_pc_set_uflags_xor(void *sd, int v)  { ((USER*)sd)->uFlags ^= (unsigned int)v; }
void sl_pc_set_talktype(void *sd, int v)    { ((USER*)sd)->talktype = v; }
void sl_pc_set_cursed(void *sd, int v)      { ((USER*)sd)->cursed = v; }
void sl_pc_set_deathflag(void *sd, int v)   { ((USER*)sd)->deathflag = v; }
void sl_pc_set_bindmap(void *sd, int v)     { ((USER*)sd)->bindmap = v; }
void sl_pc_set_bindx(void *sd, int v)       { ((USER*)sd)->bindx = v; }
void sl_pc_set_bindy(void *sd, int v)       { ((USER*)sd)->bindy = v; }
void sl_pc_set_protection(void *sd, int v)  { ((USER*)sd)->protection = v; }
void sl_pc_set_dmgshield(void *sd, int v)   { ((USER*)sd)->dmgshield = v; }
void sl_pc_set_dmgdealt(void *sd, int v)    { ((USER*)sd)->dmgdealt = v; }
void sl_pc_set_dmgtaken(void *sd, int v)    { ((USER*)sd)->dmgtaken = v; }
void sl_pc_set_heroshow(void *sd, int v)    { ((USER*)sd)->status.heroes = v; }
void sl_pc_set_fakeDrop(void *sd, int v)    { ((USER*)sd)->fakeDrop = v; }
void sl_pc_set_sex(void *sd, int v)         { ((USER*)sd)->status.sex = v; }
void sl_pc_set_clone(void *sd, int v)       { ((USER*)sd)->clone = v; }
void sl_pc_set_classRank(void *sd, int v)   { ((USER*)sd)->status.classRank = v; }
void sl_pc_set_clanRank(void *sd, int v)    { ((USER*)sd)->status.clanRank = v; }
void sl_pc_set_fury(void *sd, int v)        { ((USER*)sd)->fury = v; }
void sl_pc_set_coref_container(void *sd, int v) { ((USER*)sd)->coref_container = (unsigned int)v; }
void sl_pc_set_wisdom(void *sd, int v)      { ((USER*)sd)->wisdom = v; }
void sl_pc_set_con(void *sd, int v)         { ((USER*)sd)->con = v; }
void sl_pc_set_backstab(void *sd, int v)    { ((USER*)sd)->backstab = v; }
void sl_pc_set_flank(void *sd, int v)       { ((USER*)sd)->flank = v; }
void sl_pc_set_healing(void *sd, int v)     { ((USER*)sd)->healing = v; }
void sl_pc_set_pbColor(void *sd, int v)     { ((USER*)sd)->pbColor = v; }

// ─── Write: GFX setters ───────────────────────────────────────────────────────
void sl_pc_set_gfx_face(void *sd, int v)    { ((USER*)sd)->gfx.face = v; }
void sl_pc_set_gfx_hair(void *sd, int v)    { ((USER*)sd)->gfx.hair = v; }
void sl_pc_set_gfx_chair(void *sd, int v)   { ((USER*)sd)->gfx.chair = v; }
void sl_pc_set_gfx_cface(void *sd, int v)   { ((USER*)sd)->gfx.cface = v; }
void sl_pc_set_gfx_cskin(void *sd, int v)   { ((USER*)sd)->gfx.cskin = v; }
void sl_pc_set_gfx_dye(void *sd, int v)     { ((USER*)sd)->gfx.dye = v; }
void sl_pc_set_gfx_weapon(void *sd, int v)  { ((USER*)sd)->gfx.weapon = v; }
void sl_pc_set_gfx_cweapon(void *sd, int v) { ((USER*)sd)->gfx.cweapon = v; }
void sl_pc_set_gfx_armor(void *sd, int v)   { ((USER*)sd)->gfx.armor = v; }
void sl_pc_set_gfx_carmor(void *sd, int v)  { ((USER*)sd)->gfx.carmor = v; }
void sl_pc_set_gfx_shield(void *sd, int v)  { ((USER*)sd)->gfx.shield = v; }
void sl_pc_set_gfx_cshield(void *sd, int v) { ((USER*)sd)->gfx.cshield = v; }
void sl_pc_set_gfx_helm(void *sd, int v)    { ((USER*)sd)->gfx.helm = v; }
void sl_pc_set_gfx_chelm(void *sd, int v)   { ((USER*)sd)->gfx.chelm = v; }
void sl_pc_set_gfx_mantle(void *sd, int v)  { ((USER*)sd)->gfx.mantle = v; }
void sl_pc_set_gfx_cmantle(void *sd, int v) { ((USER*)sd)->gfx.cmantle = v; }
void sl_pc_set_gfx_crown(void *sd, int v)   { ((USER*)sd)->gfx.crown = v; }
void sl_pc_set_gfx_ccrown(void *sd, int v)  { ((USER*)sd)->gfx.ccrown = v; }
void sl_pc_set_gfx_faceAcc(void *sd, int v) { ((USER*)sd)->gfx.faceAcc = v; }
void sl_pc_set_gfx_cfaceAcc(void *sd, int v){ ((USER*)sd)->gfx.cfaceAcc = v; }
void sl_pc_set_gfx_faceAccT(void *sd, int v){ ((USER*)sd)->gfx.faceAccT = v; }
void sl_pc_set_gfx_cfaceAccT(void *sd, int v){ ((USER*)sd)->gfx.cfaceAccT = v; }
void sl_pc_set_gfx_boots(void *sd, int v)   { ((USER*)sd)->gfx.boots = v; }
void sl_pc_set_gfx_cboots(void *sd, int v)  { ((USER*)sd)->gfx.cboots = v; }
void sl_pc_set_gfx_necklace(void *sd, int v){ ((USER*)sd)->gfx.necklace = v; }
void sl_pc_set_gfx_cnecklace(void *sd, int v){ ((USER*)sd)->gfx.cnecklace = v; }
void sl_pc_set_gfx_name(void *sd, const char *v) {
    strncpy(((USER*)sd)->gfx.name, v ? v : "", sizeof(((USER*)sd)->gfx.name) - 1);
    ((USER*)sd)->gfx.name[sizeof(((USER*)sd)->gfx.name) - 1] = '\0';
}
void sl_pc_set_name(void *sd, const char *v) {
    strncpy(((USER*)sd)->status.name, v ? v : "", sizeof(((USER*)sd)->status.name) - 1);
    ((USER*)sd)->status.name[sizeof(((USER*)sd)->status.name) - 1] = '\0';
}
void sl_pc_set_title(void *sd, const char *v) {
    strncpy(((USER*)sd)->status.title, v ? v : "", sizeof(((USER*)sd)->status.title) - 1);
    ((USER*)sd)->status.title[sizeof(((USER*)sd)->status.title) - 1] = '\0';
}
void sl_pc_set_clan_title(void *sd, const char *v) {
    strncpy(((USER*)sd)->status.clan_title, v ? v : "", sizeof(((USER*)sd)->status.clan_title) - 1);
    ((USER*)sd)->status.clan_title[sizeof(((USER*)sd)->status.clan_title) - 1] = '\0';
}
void sl_pc_set_afkmessage(void *sd, const char *v) {
    strncpy(((USER*)sd)->status.afkmessage, v ? v : "", sizeof(((USER*)sd)->status.afkmessage) - 1);
    ((USER*)sd)->status.afkmessage[sizeof(((USER*)sd)->status.afkmessage) - 1] = '\0';
}
void sl_pc_set_speech(void *sd, const char *v) {
    strncpy(((USER*)sd)->speech, v ? v : "", sizeof(((USER*)sd)->speech) - 1);
    ((USER*)sd)->speech[sizeof(((USER*)sd)->speech) - 1] = '\0';
}

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
void sl_pc_setminimaptoggle(void *sd, int flag)   { ((USER*)sd)->status.miniMapToggle = flag; }
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
            strcpy(user->status.legends[x].text, text ? text : "");
            strcpy(user->status.legends[x].name, name ? name : "");
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
void sl_pc_set_vregenoverflow(void *sd, int v) { ((USER*)sd)->vregenoverflow = (float)v; }
int  sl_pc_mregenoverflow(void *sd) { return (int)((USER*)sd)->mregenoverflow; }
void sl_pc_set_mregenoverflow(void *sd, int v) { ((USER*)sd)->mregenoverflow = (float)v; }

/* --- PC group membership fields --- */
int  sl_pc_group_count(void *sd)  { return ((USER*)sd)->group_count; }
void sl_pc_set_group_count(void *sd, int v) { ((USER*)sd)->group_count = v; }
int  sl_pc_group_on(void *sd)     { return ((USER*)sd)->group_on; }
void sl_pc_set_group_on(void *sd, int v) { ((USER*)sd)->group_on = v; }
int  sl_pc_group_leader(void *sd) { return (int)((USER*)sd)->group_leader; }
void sl_pc_set_group_leader(void *sd, int v) { ((USER*)sd)->group_leader = (unsigned int)v; }

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
    char escape[255];

    SqlStmt *stmt = SqlStmt_Malloc(sql_handle);
    if (!stmt) { SqlStmt_ShowDebug(stmt); return; }

    if (SQL_ERROR == SqlStmt_Prepare(stmt,
            "SELECT `ParPosition` FROM `Parcels` WHERE `ParChaIdDestination` = '%u'",
            (unsigned int)receiver) ||
        SQL_ERROR == SqlStmt_Execute(stmt) ||
        SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_INT, &pos, 0, NULL, NULL)) {
        SqlStmt_ShowDebug(stmt);
        SqlStmt_Free(stmt);
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
 * Returns NULL if slot is empty (id == 0). */
void *sl_pc_getinventoryitem(void *sd_ptr, int slot) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return NULL;
    if (sd->status.inventory[slot].id == 0) return NULL;
    return &sd->status.inventory[slot];
}

/* getEquippedItem — return pointer into sd->status.equip[slot].
 * Returns NULL if slot is empty. */
void *sl_pc_getequippeditem_sd(void *sd_ptr, int slot) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return NULL;
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
    if (!sd) return 0;
    return (int)sd->status.banks[slot].item_id;
}

/* checkBankAmount — return amount at bank slot. */
int sl_pc_checkbankamounts(void *sd_ptr, int slot) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return 0;
    return (int)sd->status.banks[slot].amount;
}

/* checkBankOwner — return owner char-id at bank slot. */
int sl_pc_checkbankowners(void *sd_ptr, int slot) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return 0;
    return (int)sd->status.banks[slot].owner;
}

/* checkBankEngrave — return engrave string at bank slot. */
const char *sl_pc_checkbankengraves(void *sd_ptr, int slot) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return "";
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
    if (curitem >= 0 && sd->status.inventory[curitem].id) {
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

/* menu — mirrors pcl_menu.
 * Sends a menu to the client via the seq-menu packet.
 * clif_scriptmenuseq(sd, npc_id, topic, options[], n, previous, next) */
void sl_pc_menu_send(void *sd_ptr, const char *msg, const char **options, int n) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    clif_scriptmenuseq(sd, sd->last_click, msg, options, n, 0, 0);
}

/* menuseq — mirrors pcl_menuseq (same packet as menu, different Lua name).
 * clif_scriptmenuseq(sd, npc_id, topic, options[], n, previous, next) */
void sl_pc_menuseq_send(void *sd_ptr, const char *msg, const char **options, int n) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    clif_scriptmenuseq(sd, sd->last_click, msg, options, n, 0, 0);
}

/* menustring — uses the non-seq menu packet (clif_scriptmenu).
 * clif_scriptmenu(sd, npc_id, topic, options[], n)
 * Note: clif_scriptmenu takes non-const char* arrays; cast away const. */
void sl_pc_menustring_send(void *sd_ptr, const char *msg, const char **options, int n) {
    USER *sd = (USER *)sd_ptr;
    if (!sd) return;
    clif_scriptmenu(sd, sd->last_click, (char *)msg, (char **)options, n);
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
