/* sl_compat.c — real C symbols for scripting dispatch.
 *
 * These provide linkable symbols so Rust extern "C" declarations in
 * npc.rs / mob.rs can resolve at link time.  The static inline versions
 * in scripting.h are compiled away and never produce symbols.
 */
#include <stdarg.h>
#include <stdio.h>
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
