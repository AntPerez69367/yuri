/*
 * map_server_stubs.c — C functions from map_server.c that remain in C for Phase 3.
 *
 * Globals previously defined here that have been moved to Rust (src/game/map_server.rs):
 *   sql_handle  — #[no_mangle] pub static mut sql_handle: *mut Sql
 *   char_fd     — #[no_mangle] pub static mut char_fd: c_int
 *   map_fd      — #[no_mangle] pub static mut map_fd: c_int
 *   userlist    — #[no_mangle] pub static mut userlist: UserlistData
 *   auth_n      — #[no_mangle] pub static mut auth_n: c_int
 *   cur_time / cur_day / cur_season / cur_year / old_time
 *   gamereg / objectFlags
 *   map / map_n (src/ffi/map_db.rs)
 *   bl_head / bl_list (src/ffi/block.rs)
 *
 * Functions ported to Rust and removed from this file:
 *   map_clritem, map_delitem, map_additem    — src/game/map_server.rs
 *   map_freeblock_lock, map_freeblock_unlock — src/game/map_server.rs
 *   map_freeblock                            — src/game/map_server.rs
 *   map_freeblock (block variant)            — src/game/map_server.rs
 *   map_initiddb, map_termiddb               — src/game/map_server.rs
 *   map_id2bl, map_id2sd                     — src/game/map_server.rs
 *   map_addiddb, map_deliddb                 — src/game/map_server.rs
 *   map_setmapip                             — src/game/map_server.rs
 *   map_canmove                              — src/game/map_server.rs
 *   map_addmob                               — src/game/map_server.rs
 *   isPlayerActive, isActive                 — src/game/map_server.rs
 *   mmo_setonline                            — src/game/map_server.rs
 *   map_cronjob  (→ rust_map_cronjob)        — src/game/map_server.rs
 *   map_setmapip                             — src/game/map_server.rs
 *   lang_read                                — src/game/map_server.rs
 *   boards_delete, boards_showposts          — src/game/map_server.rs
 *   boards_readpost, boards_post             — src/game/map_server.rs
 *   nmail_read, nmail_write                  — src/game/map_server.rs
 *   nmail_luascript, nmail_poemscript        — src/game/map_server.rs
 *   nmail_sendmailcopy, nmail_sendmail       — src/game/map_server.rs
 *   nmail_sendmessage                        — src/game/map_server.rs
 *   hasCoref                                 — src/game/map_server.rs
 *   map_changepostcolor, map_getpostcolor    — src/game/map_server.rs
 *   change_time_char, get_time_thing         — src/game/map_server.rs
 *   uptime                                   — src/game/map_server.rs
 *   object_flag_init                         — src/game/map_server.rs
 *   map_src_clear, map_src_add               — src/game/map_server.rs
 *   map_lastdeath_mob                        — src/game/map_server.rs
 *   map_registrysave                         — src/game/map_server.rs
 *   map_readglobalreg, map_setglobalreg      — src/game/map_server.rs
 *   map_loadgameregistry, map_savegameregistry — src/game/map_server.rs
 *   map_setglobalgamereg                     — src/game/map_server.rs
 *   map_foreachinblockva                     — src/ffi/block.rs
 *   map_firstincell, map_firstincellwithtraps — src/ffi/block.rs
 *   map_respawnmobs                          — src/ffi/block.rs
 *   rust_map_loadregistry, rust_map_reload   — src/game/map_server.rs / src/ffi/map_db.rs
 *
 * TODO (Phase 3 — blocked on map_parse.c Rust port):
 *   map_reload         — calls map_foreachinarea + sl_updatepeople
 *   map_do_term        — calls map_savechars + C FREE loop on map[]
 *   map_reset_timer    — calls clif_broadcast + clif_handle_disconnect
 *   map_foreachincell  — C closure interface (varargs va_list)
 *   map_foreachincellwithtraps — same
 *   map_foreachinarea  — same
 *   map_foreachinblock — same
 */

#include "yuri.h"

#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/timeb.h>
#include <time.h>
#ifdef _WIN32
#include <winsock.h>
#else
#include <arpa/inet.h>
#endif
#include <netdb.h>

#include "board_db.h"
#include "clan_db.h"
#include "class_db.h"
#include "config.h"
#include "core.h"
#include "creation_db.h"
#include "gm_command.h"
#include "db.h"
#include "db_mysql.h"
#include "item_db.h"
#include "magic_db.h"
#include "map_char.h"
#include "map_parse.h"
#include "map_server.h"
#include "mmo.h"
#include "mob.h"
#include "net_crypt.h"
#include "npc.h"
#include "recipe_db.h"
#include "scripting.h"
#include "session.h"
#include "showmsg.h"
#include "strlib.h"
#include "timer.h"

#ifndef _MAP_SERVER_
#define _MAP_SERVER_
#endif

/*
 * Globals that remain local to this file (not needed from other TUs).
 * sql_handle, char_fd, map_fd, userlist, auth_n moved to Rust.
 * cur_time/cur_day/cur_season/cur_year/old_time moved to Rust.
 * gamereg, objectFlags, map, map_n moved to Rust / src/ffi/map_db.rs.
 * bl_head, bl_list moved to src/ffi/block.rs.
 */
DBMap* mobsearch_db;

int log_fd;
int map_max = 0;
char map_ip_s[16];
char log_ip_s[16];
int oldHour;
int oldMinute;
int cronjobtimer;

#define BL_LIST_MAX 32768
int bl_list_count = 0;

time_t gettickthing(void) { return time(NULL); }
int command_input(char* val) { return 0; }

int map_timerthing(int x, int b) { return 0; }

MOB* map_id2mob(unsigned int id) {
  MOB* mob;
  struct block_list* bl;

  if (id < MOB_START_NUM) id += MOB_START_NUM - 1;
  bl = map_id2bl(id);
  if (bl) {
    if (bl->type == BL_MOB) {
      mob = (MOB*)bl;
      return mob;
    }
  }
  return NULL;
}

NPC* map_id2npc(unsigned int id) {
  NPC* npc;
  struct block_list* bl;

  if (id < NPC_START_NUM) id += NPC_START_NUM - 2;
  bl = map_id2bl(id);
  if (bl) {
    if (bl->type == BL_NPC) {
      npc = (NPC*)bl;
      return npc;
    }
  }
  return NULL;
}

NPC* map_name2npc(const char* name) {
  unsigned int i;
  NPC* nd = NULL;

  for (i = NPC_START_NUM; i <= npc_id; i++) {
    nd = map_id2npc(i);

    if (nd && !strcasecmp(nd->npc_name, name)) {
      return nd;
    }
  }

  return NULL;
}

FLOORITEM* map_id2fl(unsigned int id) {
  FLOORITEM* fl;
  struct block_list* bl;

  bl = map_id2bl(id);
  if (bl) {
    if (bl->type == BL_ITEM) {
      fl = (FLOORITEM*)bl;
      return fl;
    }
  }
  return NULL;
}

USER* map_name2sd(const char* name) {
  int i;
  USER* sd = NULL;

  for (i = 0; i < fd_max; i++) {
    if (rust_session_exists(i) && (sd = rust_session_get_data(i))) {
      if (strcasecmp(name, sd->status.name) == 0) return sd;
    }
  }
  return NULL;
}

int map_foreachinarea(int (*func)(struct block_list*, va_list), int m, int x,
                      int y, int area, int type, ...) {
  int nAreaSizeX = AREAX_SIZE, nAreaSizeY = AREAY_SIZE;
  va_list ap;
  va_start(ap, type);

  int x0 = x - 9;
  int y0 = y - 8;
  int x1 = x + 9;
  int y1 = y + 8;

  if (x0 < 0) {
    x1 += -x0;
    x0 = 0;
    if (x1 >= map[m].xs) x1 = map[m].xs - 1;
  }
  if (y0 < 0) {
    y1 += -y0;
    y0 = 0;
    if (y1 >= map[m].ys) y1 = map[m].ys - 1;
  }
  if (x1 >= map[m].xs) {
    x0 -= x1 - map[m].xs + 1;
    x1 = map[m].xs - 1;
    if (x0 < 0) x0 = 0;
  }
  if (y1 >= map[m].ys) {
    y0 -= y1 - map[m].ys + 1;
    y1 = map[m].ys - 1;
    if (y0 < 0) y0 = 0;
  }

  switch (area) {
    case AREA:
      map_foreachinblockva(func, m, x - (nAreaSizeX + 1), y - (nAreaSizeY + 1),
                           x + (nAreaSizeX + 1), y + (nAreaSizeY + 1), type,
                           ap);
      break;
    case CORNER:
      if (map[m].xs > (nAreaSizeX * 2 + 1) &&
          map[m].ys > (nAreaSizeY * 2 + 1)) {
        if (x < (nAreaSizeX * 2 + 2) && x > nAreaSizeX)
          map_foreachinblockva(func, m, 0, y - (nAreaSizeY + 1),
                               x - (nAreaSizeX + 2), y + (nAreaSizeY + 1), type,
                               ap);
        if (y < (nAreaSizeY * 2 + 2) && y > nAreaSizeY) {
          map_foreachinblockva(func, m, x - (nAreaSizeX + 1), 0,
                               x + (nAreaSizeX + 1), y - (nAreaSizeY + 2), type,
                               ap);
          if (x < (nAreaSizeX * 2 + 2) && x > nAreaSizeX)
            map_foreachinblockva(func, m, 0, 0, x - (nAreaSizeX + 2),
                                 y - (nAreaSizeY + 2), type, ap);
          else if (x > map[m].xs - (nAreaSizeX * 2 + 3) &&
                   x < map[m].xs - (nAreaSizeX + 1))
            map_foreachinblockva(func, m, x + (nAreaSizeX + 2), 0,
                                 map[m].xs - 1, y + (nAreaSizeY + 2), type, ap);
        }
        if (x > map[m].xs - (nAreaSizeX * 2 + 3) &&
            x < map[m].xs - (nAreaSizeX + 1))
          map_foreachinblockva(func, m, x + (nAreaSizeX + 2),
                               y - (nAreaSizeY + 1), map[m].xs - 1,
                               y + (nAreaSizeY + 1), type, ap);
        if (y > map[m].ys - (nAreaSizeY * 2 + 3) &&
            y < map[m].ys - (nAreaSizeY + 1)) {
          map_foreachinblockva(func, m, x - (nAreaSizeX + 1),
                               y + (nAreaSizeY + 2), x + (nAreaSizeX + 1),
                               map[m].ys - 1, type, ap);
          if (x < (nAreaSizeX * 2 + 2) && x > nAreaSizeX)
            map_foreachinblockva(func, m, 0, y + (nAreaSizeY + 2),
                                 x - (nAreaSizeX + 2), map[m].ys - 1, type, ap);
          else if (x > map[m].xs - (nAreaSizeX * 2 + 3) &&
                   x < map[m].xs - (nAreaSizeX + 1))
            map_foreachinblockva(func, m, x + (nAreaSizeX + 2),
                                 y + (nAreaSizeY + 2), map[m].xs - 1,
                                 map[m].ys - 1, type, ap);
        }
      }
      break;
    case SAMEAREA:
      map_foreachinblockva(func, m, x0, y0, x1, y1, type, ap);
      break;
    case SAMEMAP:
      map_foreachinblockva(func, m, 0, 0, map[m].xs - 1, map[m].ys - 1, type,
                           ap);
      break;
  }

  va_end(ap);
  return 0;
}

int map_foreachinblock(int (*func)(struct block_list*, va_list), int m, int x0,
                       int y0, int x1, int y1, int type, ...) {
  va_list ap;
  va_start(ap, type);
  if (x0 < 0) x0 = 0;
  if (y0 < 0) y0 = 0;
  if (x1 >= map[m].xs) x1 = map[m].xs - 1;
  if (y1 >= map[m].ys) y1 = map[m].ys - 1;

  map_foreachinblockva(func, m, x0, y0, x1, y1, type, ap);

  va_end(ap);
  return 0;
}

int map_foreachincell(int (*func)(struct block_list*, va_list), int m, int x,
                      int y, int type, ...) {
  // TODO: port in Phase 3 when callers become Rust closures
  int bx, by;
  int returnCount = 0;
  struct block_list* bl = NULL;
  int blockcount = 0;

  if (x < 0 || y < 0 || x >= map[m].xs || y >= map[m].ys) return 0;

  by = y / BLOCK_SIZE;
  bx = x / BLOCK_SIZE;

  if ((type & ~BL_MOB))
    for (bl = map[m].block[bx + by * map[m].bxs];
         bl && blockcount < BL_LIST_MAX; bl = bl->next) {
      if ((bl->type & type) && bl->x == x && bl->y == y &&
          blockcount < BL_LIST_MAX) {
        if (bl->type != BL_ITEM)
          bl_list[blockcount++] = bl;
        else {
          FLOORITEM* fl = (FLOORITEM*)bl;
          if (itemdb_type(fl->data.id) != ITM_TRAPS) bl_list[blockcount++] = bl;
        }
      }
    }

  if ((type & BL_MOB))
    for (bl = map[m].block_mob[bx + by * map[m].bxs];
         bl && blockcount < BL_LIST_MAX; bl = bl->next) {
      MOB* mob = (MOB*)bl;
      if (mob->state != MOB_DEAD && bl->x == x && bl->y == y &&
          blockcount < BL_LIST_MAX)
        bl_list[blockcount++] = bl;
    }

  if (blockcount >= BL_LIST_MAX)
    printf("Map_foreachincell: block count too many!\n");

  for (int i = 0; i < blockcount; i++)
    if (bl_list[i]->prev) {
      va_list ap;
      va_start(ap, type);
      returnCount += func(bl_list[i], ap);
      va_end(ap);
    }

  return returnCount;
}

int map_foreachincellwithtraps(int (*func)(struct block_list*, va_list), int m,
                               int x, int y, int type, ...) {
  // TODO: port in Phase 3 when callers become Rust closures
  int bx, by;
  int returnCount = 0;
  struct block_list* bl = NULL;
  int blockcount = 0;
  int i;

  if (x < 0 || y < 0 || x >= map[m].xs || y >= map[m].ys) return 0;

  by = y / BLOCK_SIZE;
  bx = x / BLOCK_SIZE;

  if ((type & ~BL_MOB))
    for (bl = map[m].block[bx + by * map[m].bxs];
         bl && blockcount < BL_LIST_MAX; bl = bl->next) {
      if ((bl->type & type) && bl->x == x && bl->y == y &&
          blockcount < BL_LIST_MAX) {
        bl_list[blockcount++] = bl;
      }
    }

  if ((type & BL_MOB))
    for (bl = map[m].block_mob[bx + by * map[m].bxs];
         bl && blockcount < BL_LIST_MAX; bl = bl->next) {
      MOB* mob = (MOB*)bl;
      if (mob->state != MOB_DEAD && bl->x == x && bl->y == y &&
          blockcount < BL_LIST_MAX)
        bl_list[blockcount++] = bl;
    }

  if (blockcount >= BL_LIST_MAX)
    printf("Map_foreachincell: block count too many!\n");

  for (i = 0; i < blockcount; i++)
    if (bl_list[i]->prev) {
      va_list ap;
      va_start(ap, type);
      returnCount += func(bl_list[i], ap);
      va_end(ap);
    }

  return returnCount;
}

int map_respawn(int (*func)(struct block_list*, va_list), int m, int type,
                ...) {
  // Shim: calls Rust map_respawnmobs — see src/ffi/block.rs
  // TODO: remove in Phase 3 when callers move to Rust closures
  va_list ap;
  va_start(ap, type);
  map_respawnmobs(func, m, type, ap);
  va_end(ap);
  return 0;
}

int map_loadregistry(int id) {
  return rust_map_loadregistry(id);
}

int map_read(void) {
  unsigned short buff;
  unsigned int pos = 0;
  unsigned int i, id, sweeptime;
  unsigned short bgm, bgmtype;
  unsigned char pvp, spell, light, weather, cantalk, show_ghosts, region,
      indoor, warpout, bind;
  unsigned int reqlvl, reqvita, reqmana, lvlmax, manamax, vitamax;
  unsigned char reqmark, reqpath, summon;
  unsigned char canUse, canEat, canSmoke, canMount, canGroup, canEquip;

  char title[64], mapfile[1024], mappath[1024];
  char maprejectmsg[64];

  SqlStmt* stmt = SqlStmt_Malloc(sql_handle);
  FILE* fp;
  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    return 0;
  }

  if (SQL_ERROR ==
          SqlStmt_Prepare(
              stmt,
              "SELECT `MapId`, `MapName`, `MapBGM`, `MapBGMType`, `MapPvP`, "
              "`MapSpells`, `MapLight`, `MapWeather`, `MapSweepTime`, "
              "`MapChat`, `MapGhosts`, `MapRegion`, `MapIndoor`, `MapWarpout`, "
              "`MapBind`, `MapFile`, `MapReqLvl`, `MapReqPath`, `MapReqMark`, "
              "`MapCanSummon`, `MapReqVita`, `MapReqMana`, `MapLvlMax`, "
              "`MapVitaMax`, `MapManaMax`, `MapRejectMsg`, `MapCanUse`, "
              "`MapCanEat`, `MapCanSmoke`, `MapCanMount`, `MapCanGroup`, "
              "`MapCanEquip` FROM `Maps` WHERE `MapServer` = '%d' ORDER BY "
              "`MapId`",
              serverid) ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 0, SQLDT_UINT, &id, 0, NULL, NULL) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 1, SQLDT_STRING, &title,
                                      sizeof(title), NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 2, SQLDT_USHORT, &bgm, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 3, SQLDT_USHORT, &bgmtype, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 4, SQLDT_UCHAR, &pvp, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 5, SQLDT_UCHAR, &spell, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 6, SQLDT_UCHAR, &light, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 7, SQLDT_UCHAR, &weather, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 8, SQLDT_UINT, &sweeptime, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 9, SQLDT_UCHAR, &cantalk, 0, NULL, NULL) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 10, SQLDT_UCHAR, &show_ghosts, 0,
                                      NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 11, SQLDT_UCHAR, &region, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 12, SQLDT_UCHAR, &indoor, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 13, SQLDT_UCHAR, &warpout, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 14, SQLDT_UCHAR, &bind, 0, NULL, NULL) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 15, SQLDT_STRING, &mapfile,
                                      sizeof(mapfile), NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 16, SQLDT_UINT, &reqlvl, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 17, SQLDT_UCHAR, &reqpath, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 18, SQLDT_UCHAR, &reqmark, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 19, SQLDT_UCHAR, &summon, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 20, SQLDT_UINT, &reqvita, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 21, SQLDT_UINT, &reqmana, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 22, SQLDT_UINT, &lvlmax, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 23, SQLDT_UINT, &vitamax, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 24, SQLDT_UINT, &manamax, 0, NULL, NULL) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 25, SQLDT_STRING, &maprejectmsg,
                                      sizeof(maprejectmsg), NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 26, SQLDT_UCHAR, &canUse, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 27, SQLDT_UCHAR, &canEat, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 28, SQLDT_UCHAR, &canSmoke, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 29, SQLDT_UCHAR, &canMount, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 30, SQLDT_UCHAR, &canGroup, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 31, SQLDT_UCHAR, &canEquip, 0, NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return 0;
  }

  map_n = SqlStmt_NumRows(stmt);
  CALLOC(map, struct map_data, 65535);

  for (i = 0; i < map_n && SQL_SUCCESS == SqlStmt_NextRow(stmt); i++) {
    sprintf(mappath, "%s%s", maps_dir, mapfile);
    fp = fopen(mappath, "rb");

    if (fp == NULL) {
      printf("[map] [not_found] Map file not found id=%u path=%s\n", id,
             mappath);
      return -1;
    }

    memcpy(map[id].mapfile, mapfile, sizeof(mapfile));
    memcpy(map[id].title, title, sizeof(title));
    map[id].bgm = bgm;
    map[id].bgmtype = bgmtype;
    map[id].pvp = pvp;
    map[id].spell = spell;
    map[id].light = light;
    map[id].weather = weather;
    map[id].sweeptime = sweeptime;
    map[id].cantalk = cantalk;
    map[id].show_ghosts = show_ghosts;
    map[id].region = region;
    map[id].indoor = indoor;
    map[id].warpout = warpout;
    map[id].bind = bind;
    map[id].reqlvl = reqlvl;
    map[id].reqvita = reqvita;
    map[id].reqmana = reqmana;
    map[id].lvlmax = lvlmax;
    map[id].vitamax = vitamax;
    map[id].manamax = manamax;
    map[id].reqpath = reqpath;
    map[id].reqmark = reqmark;
    map[id].summon = summon;
    memcpy(map[id].maprejectmsg, maprejectmsg, sizeof(maprejectmsg));
    map[id].canUse = canUse;
    map[id].canEat = canEat;
    map[id].canSmoke = canSmoke;
    map[id].canMount = canMount;
    map[id].canGroup = canGroup;
    map[id].canEquip = canEquip;

    fread(&buff, 2, 1, fp);
    map[id].xs = SWAP16(buff);
    fread(&buff, 2, 1, fp);
    map[id].ys = SWAP16(buff);
    CALLOC(map[id].tile, unsigned short, map[id].xs * map[id].ys);
    CALLOC(map[id].obj, unsigned short, map[id].xs * map[id].ys);
    CALLOC(map[id].map, unsigned char, map[id].xs * map[id].ys);
    CALLOC(map[id].pass, unsigned short, map[id].xs * map[id].ys);

    map[id].bxs = (map[id].xs + BLOCK_SIZE - 1) / BLOCK_SIZE;
    map[id].bys = (map[id].ys + BLOCK_SIZE - 1) / BLOCK_SIZE;
    CALLOC(map[id].warp, struct warp_list*, map[id].bxs * map[id].bys);
    CALLOC(map[id].block, struct block_list*, map[id].bxs * map[id].bys);
    CALLOC(map[id].block_mob, struct block_list*, map[id].bxs * map[id].bys);
    CALLOC(map[id].registry, struct global_reg, MAX_MAPREG);

    while (!feof(fp)) {
      fread(&buff, 2, 1, fp);
      map[id].tile[pos] = SWAP16(buff);
      fread(&buff, 2, 1, fp);
      map[id].pass[pos] = SWAP16(buff);
      fread(&buff, 2, 1, fp);
      map[id].obj[pos] = SWAP16(buff);
      pos++;
      if (pos >= map[id].xs * map[id].ys) break;
    }
    pos = 0;
    fclose(fp);
    map_loadregistry(id);
  }

  SqlStmt_Free(stmt);
  printf("Map data file reading finished. %d map loaded!\n", map_n);
  return 0;
}

// TODO: port map_reload to Rust (Phase 3).
// Blocked by: map_foreachinarea + sl_updatepeople still in C.
int map_reload(void) {
  int i;
  if (rust_map_reload(maps_dir, serverid) != 0) {
    printf("[map] [error] rust_map_reload failed\n");
    return -1;
  }
  for (i = 0; i < map_n; i++) {
    if (map_isloaded(i))
      map_foreachinarea(sl_updatepeople, i, 0, 0, SAMEMAP, BL_PC);
  }
  printf("Map data file reading finished. %d map loaded!\n", map_n);
  return 0;
}

void help_screen(void) {
  printf("HELP LIST\n");
  printf("---------\n");
  printf(" --conf [FILENAME]  : set config file\n");
  printf(" --lang [FILENAME]  : set lang file\n");
  exit(0);
}

char* map_id2name(unsigned int id) {
  char* owner;
  CALLOC(owner, char, 255);
  memset(owner, 0, 255);

  char name[16];

  if (!id) {
    strcpy(owner, "None");
    return owner;
  }

  SqlStmt* stmt;
  stmt = SqlStmt_Malloc(sql_handle);
  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    return 0;
  }

  if (SQL_ERROR == SqlStmt_Prepare(
                       stmt,
                       "SELECT `ChaName` FROM `Character` WHERE `ChaId` = '%u'",
                       id) ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_STRING, &name,
                                      sizeof(name), NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
  }

  if (SQL_SUCCESS != SqlStmt_NextRow(stmt)) {
    SqlStmt_ShowDebug(stmt);
  }

  SqlStmt_Free(stmt);

  memcpy(owner, name, sizeof(name));

  return owner;
}

// TODO: port map_reset_timer to Rust (Phase 3).
// Blocked by: clif_broadcast and clif_handle_disconnect still in C (map_parse.c).
int map_reset_timer(int v1, int v2) {
  static int reset;
  static int diff;
  int x;
  struct map_sessiondata* sd = NULL;
  char msg[255];
  if (!reset) reset = v1;

  reset -= v2;
  diff += v2;
  if (reset <= 250) {
    clif_broadcast("Chaos is rising up. Please re-enter in a few seconds.", -1);
  }
  if (reset <= 0) {
    for (x = 0; x < fd_max; x++) {
      if (rust_session_exists(x) && (sd = rust_session_get_data(x)) && !rust_session_get_eof(x)) {
        clif_handle_disconnect(sd);
        rust_session_call_parse(x);
        RFIFOFLUSH(x);
        rust_session_set_eof(x, 1);
      }
    }
    rust_request_shutdown();
    reset = 0;
    diff = 0;
    return 1;
  }

  if (reset <= 60000) {
    if (diff >= 10000) {
      sprintf(msg, "Reset in %d seconds", reset / 1000);
      clif_broadcast(msg, -1);
      diff = 0;
    }
  } else if (reset <= 3600000) {
    if (diff >= 300000) {
      sprintf(msg, "Reset in %d minutes", reset / 60000);
      clif_broadcast(msg, -1);
      diff = 0;
    }
  } else if (reset > 3600000) {
    if (diff >= 3600000) {
      sprintf(msg, "Reset in %d hours", reset / 3600000);
      clif_broadcast(msg, -1);
      diff = 0;
    }
  }

  return 0;
}

// reads game registry value (gamereg is a Rust static in src/game/map_server.rs)
int map_readglobalgamereg(const char* reg) {
  int i, exist;

  exist = -1;
  nullpo_ret(0, reg);

  for (i = 0; i < gamereg.registry_num; i++) {
    if (!strcasecmp(gamereg.registry[i].str, reg)) {
      exist = i;
      break;
    }
  }

  if (exist != -1) {
    return gamereg.registry[exist].val;
  }
  return 0;
}

int map_loadclanbank(int id) {
  int i;
  int count = 0;

  SqlStmt* stmt;
  struct clan_bank cbank;
  struct clan_data* clan = NULL;

  memset(&cbank, 0, sizeof(cbank));

  stmt = SqlStmt_Malloc(sql_handle);
  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return -1;
  }

  clan = (struct clan_data*)rust_clandb_search(id);
  if (clan == NULL) {
    printf("[map] map_loadclanbank: clan %d not found\n", id);
    SqlStmt_Free(stmt);
    return -1;
  }

  if (SQL_ERROR ==
          SqlStmt_Prepare(
              stmt,
              "SELECT `CbkEngrave`, `CbkItmId`,`CbkAmount`,`CbkChaIdOwner`, "
              "`CbkPosition`, `CbkCustomLook`, `CbkCustomLookColor`, "
              "`CbkCustomIcon`, `CbkCustomIconColor`, `CbkProtected`, "
              "`CbkNote` FROM `ClanBanks` WHERE `CbkClnId` = '%u' LIMIT 255",
              id) ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_STRING, &cbank.real_name,
                                      sizeof(cbank.real_name), NULL, NULL) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 1, SQLDT_UINT, &cbank.item_id, 0,
                                      NULL, NULL) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 2, SQLDT_UINT, &cbank.amount, 0,
                                      NULL, NULL) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 3, SQLDT_UINT, &cbank.owner, 0,
                                      NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 4, SQLDT_UCHAR, &cbank.pos, 0, NULL, NULL) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 5, SQLDT_UINT, &cbank.customLook, 0,
                                      NULL, NULL) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 6, SQLDT_UINT,
                                      &cbank.customLookColor, 0, NULL, NULL) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 7, SQLDT_UINT, &cbank.customIcon, 0,
                                      NULL, NULL) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 8, SQLDT_UINT,
                                      &cbank.customIconColor, 0, NULL, NULL) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 9, SQLDT_UINT, &cbank.protected, 0,
                                      NULL, NULL) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 10, SQLDT_STRING, &cbank.note,
                                      sizeof(cbank.note), NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return -1;
  }

  for (i = 0; i < SqlStmt_NumRows(stmt) && SQL_SUCCESS == SqlStmt_NextRow(stmt);
       i++) {
    memcpy(&clan->clanbanks[i], &cbank, sizeof *clan->clanbanks);
    count++;
  }

  SqlStmt_Free(stmt);
  printf("[map] [clan bank slots] count=%i name=%s\n", count, clandb_name(id));
  return 0;
}

int map_saveclanbank(int id) {
  SqlStmt* stmt;

  unsigned int max = 255;

  int save_id[max];
  int item_id = -1;
  int i;
  char escape[64];
  char escape2[300];

  struct clan_data* clan = NULL;
  clan = (struct clan_data*)rust_clandb_search(id);

  if (clan == NULL) return 0;

  memset(save_id, 0, max * sizeof(int));
  stmt = SqlStmt_Malloc(sql_handle);

  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    return 0;
  }

  if (SQL_ERROR == SqlStmt_Prepare(stmt,
                                   "SELECT `CbkPosition` FROM `ClanBanks` "
                                   "WHERE `CbkClnId` = '%u' LIMIT 255",
                                   id) ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 0, SQLDT_INT, &item_id, 0, NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return 0;
  }

  for (i = 0; i < max; i++) save_id[i] = -1;

  for (i = 0; i < SqlStmt_NumRows(stmt) && SQL_SUCCESS == SqlStmt_NextRow(stmt);
       i++)
    save_id[item_id] = item_id;

  SqlStmt_Free(stmt);

  for (i = 0; i < max; i++) {
    Sql_EscapeString(sql_handle, escape, clan->clanbanks[i].real_name);
    Sql_EscapeString(sql_handle, escape2, clan->clanbanks[i].note);

    if (save_id[i] == i) {
      if (clan->clanbanks[i].item_id == 0) {
        if (SQL_ERROR == Sql_Query(sql_handle,
                                   "DELETE FROM `ClanBanks` WHERE `CbkClnId` = "
                                   "'%u' AND `CbkPosition` = '%d'",
                                   id, i)) {
          Sql_ShowDebug(sql_handle);
          return 0;
        }
      } else {
        if (SQL_ERROR ==
            Sql_Query(
                sql_handle,
                "UPDATE `ClanBanks` SET `CbkItmId` = '%u', `CbkAmount` = '%u', "
                "`CbkChaIdOwner` = '%u', `CbkTimer` = '%u', `CbkEngrave` = "
                "'%s', `CbkCustomLook` = '%u', `CbkCustomLookColor` = '%u', "
                "`CbkCustomIcon` = '%u', `CbkCustomIconColor` = '%u', "
                "`CbkProtected` = '%d', `CbkNote` = '%s' WHERE `CbkClnId` = "
                "'%u' AND `CbkPosition` = '%d'",
                clan->clanbanks[i].item_id, clan->clanbanks[i].amount,
                clan->clanbanks[i].owner, clan->clanbanks[i].time, escape,
                clan->clanbanks[i].customLook,
                clan->clanbanks[i].customLookColor,
                clan->clanbanks[i].customIcon,
                clan->clanbanks[i].customIconColor,
                clan->clanbanks[i].protected, escape2, id, i)) {
          Sql_ShowDebug(sql_handle);
          return 0;
        }
      }
    } else {
      if (clan->clanbanks[i].item_id > 0) {
        if (SQL_ERROR ==
            Sql_Query(sql_handle,
                      "INSERT INTO `ClanBanks` (`CBkClnId`, `CbkItmId`, "
                      "`CbkAmount`, `CbkChaIdOwner`, `CbkTimer`, `CbkEngrave`, "
                      "`CbkCustomLook`, `CbkCustomLookColor`, `CbkCustomIcon`, "
                      "`CbkCustomIconColor`, `CbkProtected`, `CbkNote`, "
                      "`CbkPosition`) VALUES ('%u', '%u', '%u', '%u', '%u', "
                      "'%s', '%u', '%u', '%u', '%u', '%d', '%s', '%d')",
                      id, clan->clanbanks[i].item_id, clan->clanbanks[i].amount,
                      clan->clanbanks[i].owner, clan->clanbanks[i].time, escape,
                      clan->clanbanks[i].customLook,
                      clan->clanbanks[i].customLookColor,
                      clan->clanbanks[i].customIcon,
                      clan->clanbanks[i].customIconColor,
                      clan->clanbanks[i].protected, escape2, i)) {
          Sql_ShowDebug(sql_handle);
          return 0;
        }
      }
    }
  }

  return 1;
}

int map_weather(int id, int n) {
  if (old_time != cur_time) {
    old_time = cur_time;
    sl_doscript_blargs("mapWeather", NULL, 0);
  }
  return 0;
}

// TODO: port map_do_term to Rust (Phase 3).
// Blocked by: map_savechars still in C, and the FREE(map[i].tile/...) loop
// uses the C `map[]` global array and the FREE macro.
void map_do_term(void) {
  int i;
  map_savechars(0, 0);
  map_clritem();
  map_termiddb();
  for (i = 0; i < MAX_MAP_PER_SERVER; i++) {
    FREE(map[i].tile);
    FREE(map[i].obj);
    FREE(map[i].map);
    FREE(map[i].block);
    FREE(map[i].block_mob);
    FREE(map[i].warp);
  }
  map_termblock();
  itemdb_term();
  magicdb_term();
  classdb_term();
  printf("[map] Map Server Shutdown\n");
}

int map_savechars(int none, int nonetoo) {
  USER* sd = NULL;
  int x;

  for (x = 0; x < fd_max; x++) {
    if (rust_session_exists(x) && (sd = (USER*)rust_session_get_data(x)) &&
        !rust_session_get_eof(x)) {
      intif_save(sd);
    }
  }
  return 0;
}
