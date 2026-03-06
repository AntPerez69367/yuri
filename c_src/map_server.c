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

Sql* sql_handle = NULL;

DBMap* mobsearch_db;
struct map_msg_data map_msg[MSG_MAX];
int map_loadgameregistry();

// item id pool
char* object;
int object_n = 0;

struct map_src_list {
  struct map_src_list* next;
  int id, pvp, spell;
  unsigned int sweeptime;
  char title[64];
  unsigned char cantalk, show_ghosts, region, indoor, warpout, bind;
  unsigned short bgm, bgmtype, light, weather;
  char* mapfile;
};
struct map_src_list* map_src_first = NULL;
struct map_src_list* map_src_last = NULL;

char map_ip_s[16];
char log_ip_s[16];

int log_fd;
int char_fd;
int map_fd;
// unsigned int myip=0;
int map_max = 0;
// unsigned int blcount_t=0;
int auth_n = 0;
struct userlist_data userlist;
// map and map_n are defined in src/ffi/map_db.rs (libyuri.a) as Rust statics.
// extern declarations are in map_server.h.
struct game_data gamereg;
int oldHour;
int oldMinute;
int cronjobtimer;
unsigned char *objectFlags;
int old_time, cur_time, cur_year, cur_day, cur_season;

#define BL_LIST_MAX 32768

struct block_list* bl_list[BL_LIST_MAX];

struct block_list bl_head;
int bl_list_count = 0;

time_t gettickthing(void) { return time(NULL); }
int command_input(char* val) { return 0; }


int map_timerthing(int x, int b) { return 0; }
// nmail_sendmessage ported to Rust — see src/game/map_server.rs

// map_id2bl, map_id2sd, map_addiddb, map_deliddb, map_initiddb, map_termiddb
// ported to Rust in src/game/map_server.rs

void map_clritem() {
  FREE(object);
  object_n = 0;
}

void map_delitem(unsigned int id) {
  struct block_list* bl;
  bl = map_id2bl(id);
  if (!bl) return;

  map_deliddb(bl);
  map_delblock(bl);
  FREE(bl);

  id -= FLOORITEM_START_NUM;
  if (id >= object_n || id < 0) return;

  object[id] = 0;
}

void map_additem(struct block_list* bl) {
  unsigned int i;
  for (i = 0; i < object_n; i++) {
    if (!object[i]) break;
  }
  if (i >= MAX_FLOORITEM) {
    printf("MAP_ERR: Item reached max item capacity.\n");
    return;
  }

  if (i >= object_n) {
    if (object_n) {
      REALLOC(object, char, i + 256);
    } else {
      CALLOC(object, char, 256);
    }
    object_n = i + 256;
  }

  object[i] = 1;
  i += FLOORITEM_START_NUM;
  bl->id = i;
  bl->type = BL_ITEM;
  bl->prev = NULL;
  bl->next = NULL;
  map_addiddb(bl);
  map_addblock(bl);
}

int map_src_clear() {
  struct map_src_list* p = map_src_first;

  while (p) {
    struct map_src_list* p2 = p;
    p = p->next;
    FREE(p2->mapfile);
    FREE(p2);
  }

  map_src_first = NULL;
  map_src_last = NULL;
  return 0;
}

int map_src_add(const char* r1) {
  int map_id, pvp, spell;
  unsigned int sweeptime;
  unsigned short map_bgm, light, weather;
  unsigned char cantalk, showghosts, region, indoor, warpout, bind;
  struct map_src_list* new;
  char map_title[1024], map_file[1024];
  if (sscanf(r1, "%d,%[^,],%hi,%d,%d,%hu,%hu,%u,%c,%c,%c,%c,%c, %c,%s", &map_id,
             map_title, &map_bgm, &pvp, &spell, &light, &weather, &sweeptime,
             &cantalk, &showghosts, &region, &indoor, &warpout, &bind,
             map_file) < 13) {
    return -1;
  }
  //[^,],[^\r\n]

  CALLOC(new, struct map_src_list, 1);
  CALLOC(new->mapfile, char, strlen(map_file) + 1);
  new->next = NULL;

  new->id = map_id;
  new->bgm = map_bgm;
  new->pvp = pvp;
  new->spell = spell;
  new->light = light;
  new->weather = weather;
  new->sweeptime = sweeptime;
  new->cantalk = cantalk;
  new->show_ghosts = showghosts;
  new->region = region;
  new->indoor = indoor;
  new->warpout = warpout;
  new->bind = bind;
  strncpy(new->title, map_title, 64);
  strncpy(new->mapfile, map_file, strlen(map_file));

  if (map_src_first == NULL) map_src_first = new;
  if (map_src_last) map_src_last->next = new;

  map_src_last = new;
  return 0;
}

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


/*int map_freeblock(void *bl) {
        if (!bl_free_lock) {
                FREE(bl);
        } else {
                if (bl_free_count >= BL_FREE_MAX) {
                        printf("BL_ERR: Too many block free list! Block free
count: %d\n", bl_free_count); } else bl_free[bl_free_count++] = bl;
        }
        return bl_free_lock;
}

int map_freeblock_lock() {
        return ++bl_free_lock;
}

int map_freeblock_unlock() {
        if(!--bl_free_lock) {
                int i;
                for(i=0;i<bl_free_count;i++)
                        FREE(bl_free[i]);
                bl_free_count=0;
        }
        return bl_free_lock;
}
*/


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
      /*if (x <= nAreaSizeX && y <= nAreaSizeY)
              map_foreachinblockva(func,m,0,0,(nAreaSizeX*2+2),(nAreaSizeY*2+2),type,ap);
      else if (x <= nAreaSizeX && y >= map[m].ys-(nAreaSizeY+1))
              map_foreachinblockva(func,m,0,map[m].ys-(nAreaSizeY*2+3),(nAreaSizeX*2+2),map[m].ys-1,type,ap);
      else if (x >= map[m].xs-(nAreaSizeX+1) && y <= nAreaSizeY)
              map_foreachinblockva(func,m,map[m].xs-(nAreaSizeX*2+3),0,map[m].xs-1,(nAreaSizeY*2+2),type,ap);
      else if (x >= map[m].xs-(nAreaSizeX+1) && y >= map[m].ys-(nAreaSizeY+1))
              map_foreachinblockva(func,m,map[m].xs-(nAreaSizeX*2+3),map[m].ys-(nAreaSizeY*2+3),map[m].xs-1,map[m].ys-1,type,ap);
      else
              map_foreachinblockva(func,m,x-(nAreaSizeX+1),y-(nAreaSizeY+1),x+(nAreaSizeX+1),y+(nAreaSizeY+1),type,ap);*/
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

int map_foreachinblockva(int (*func)(struct block_list*, va_list), int m,
                         int x0, int y0, int x1, int y1, int type, va_list ap) {
  int bx, by;
  int returnCount = 0;  // total sum of returned values of func() [Skotlex]
  struct block_list* bl = NULL;
  int blockcount = 0;
  int i;

  if (m < 0) return 0;

  if (!map_isloaded(m)) return 0;

  if (x0 < 0) x0 = 0;
  if (y0 < 0) y0 = 0;
  if (x1 >= map[m].xs) x1 = map[m].xs - 1;
  if (y1 >= map[m].ys) y1 = map[m].ys - 1;

  if ((type & ~BL_MOB))
    for (by = y0 / BLOCK_SIZE; by <= y1 / BLOCK_SIZE; by++)
      for (bx = x0 / BLOCK_SIZE; bx <= x1 / BLOCK_SIZE; bx++)
        for (bl = map[m].block[bx + by * map[m].bxs];
             bl && blockcount < BL_LIST_MAX; bl = bl->next) {
          if ((bl->type & type) && bl->x >= x0 && bl->x <= x1 && bl->y >= y0 &&
              bl->y <= y1 && blockcount < BL_LIST_MAX)
            bl_list[blockcount++] = bl;
        }

  if ((type & BL_MOB))
    for (by = y0 / BLOCK_SIZE; by <= y1 / BLOCK_SIZE; by++)
      for (bx = x0 / BLOCK_SIZE; bx <= x1 / BLOCK_SIZE; bx++)
        for (bl = map[m].block_mob[bx + by * map[m].bxs];
             bl && blockcount < BL_LIST_MAX; bl = bl->next) {
          MOB* mob = (MOB*)bl;
          if (mob->state != MOB_DEAD && bl->x >= x0 && bl->x <= x1 &&
              bl->y >= y0 && bl->y <= y1 && blockcount < BL_LIST_MAX)
            bl_list[blockcount++] = bl;
        }

  if (blockcount >= BL_LIST_MAX)
    printf("map_foreachinarea: block count too many!\n");

  for (i = 0; i < blockcount; i++) {
    if (bl_list[i]->prev) {  // �L?���ǂ����`�F�b�N

      va_list ap_copy;
      va_copy(ap_copy, ap);
      returnCount += func(bl_list[i], ap_copy);
      va_end(ap_copy);
    }
  }

  /*for (i = blockcount; i >= 0; i--)
          if (bl_list[i]->prev)
                  returnCount += func(bl_list[i],ap);*/

  // bl_list_count = blockcount;
  return returnCount;
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
  int bx, by;
  int returnCount = 0;  // total sum of returned values of func() [Skotlex]
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
  // ShowWarning("map_foreachincell: block count too many!\n");

  // map_freeblock_lock();	//
  // ����������̉�����֎~����
  // - Prohibit release from memory

  for (int i = 0; i < blockcount; i++)
    if (bl_list[i]->prev)  // �L?���ǂ����`�F�b�N - Check if there is
    {
      va_list ap;
      va_start(ap, type);
      returnCount += func(bl_list[i], ap);
      va_end(ap);
    }

  // map_freeblock_unlock();	// ����������� - Allow
  // release

  // bl_list_count = blockcount;
  return returnCount;
}

int map_foreachincellwithtraps(int (*func)(struct block_list*, va_list), int m,
                               int x, int y, int type, ...) {
  int bx, by;
  int returnCount = 0;  // total sum of returned values of func() [Skotlex]
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
  // ShowWarning("map_foreachincell: block count too many!\n");

  // map_freeblock_lock();	//
  // ����������̉�����֎~����

  for (i = 0; i < blockcount; i++)
    if (bl_list[i]->prev)  // �L?���ǂ����`�F�b�N
    {
      va_list ap;
      va_start(ap, type);
      returnCount += func(bl_list[i], ap);
      va_end(ap);
    }

  // map_freeblock_unlock();	// �����������

  // bl_list_count = blockcount;
  return returnCount;
}

struct block_list* map_firstincell(int m, int x, int y, int type) {
  int bx, by;
  struct block_list* bl = NULL;

  if (m < 0) return 0;
  if (x < 0) x = 0;
  if (y < 0) y = 0;
  if (x >= map[m].xs) x = map[m].xs - 1;
  if (y >= map[m].ys) y = map[m].ys - 1;
  if (!map_isloaded(m)) return 0;

  by = y / BLOCK_SIZE;
  bx = x / BLOCK_SIZE;

  if ((type & ~BL_MOB))
    for (bl = map[m].block[bx + by * map[m].bxs];
         bl /*&& bl_list_count<BL_LIST_MAX*/; bl = bl->next)
      if ((bl->type & type) && bl->x == x && bl->y == y) {
        if (bl->type != BL_ITEM)
          return bl;
        else {
          FLOORITEM* fl = (FLOORITEM*)bl;
          if (itemdb_type(fl->data.id) != ITM_TRAPS) return bl;
        }
      }

  if ((type & BL_MOB))
    for (bl = map[m].block_mob[bx + by * map[m].bxs];
         bl /*&& bl_list_count<BL_LIST_MAX*/; bl = bl->next) {
      MOB* mob = (MOB*)bl;
      if (mob->state != MOB_DEAD && bl->x == x && bl->y == y) return bl;
    }

  return 0;
}

struct block_list* map_firstincellwithtraps(int m, int x, int y, int type) {
  int bx, by;
  struct block_list* bl = NULL;

  if (m < 0) return 0;
  if (x < 0) x = 0;
  if (y < 0) y = 0;
  if (x >= map[m].xs) x = map[m].xs - 1;
  if (y >= map[m].ys) y = map[m].ys - 1;

  by = y / BLOCK_SIZE;
  bx = x / BLOCK_SIZE;

  if ((type & ~BL_MOB))
    for (bl = map[m].block[bx + by * map[m].bxs];
         bl /*&& bl_list_count<BL_LIST_MAX*/; bl = bl->next)
      if ((bl->type & type) && bl->x == x && bl->y == y) {
        return bl;
      }

  if ((type & BL_MOB))
    for (bl = map[m].block_mob[bx + by * map[m].bxs];
         bl /*&& bl_list_count<BL_LIST_MAX*/; bl = bl->next) {
      MOB* mob = (MOB*)bl;
      if (mob->state != MOB_DEAD && bl->x == x && bl->y == y) return bl;
    }

  return 0;
}

int map_respawn(int (*func)(struct block_list*, va_list), int m, int type,
                ...) {
  va_list ap;

  va_start(ap, type);
  map_respawnmobs(func, m, type, ap);
  va_end(ap);
  return 0;
}

int map_respawnmobs(int (*func)(struct block_list*, va_list), int m, int type,
                    va_list ap) {
  int x0, x1, y0, y1, bx, by;
  int returnCount = 0;  // total sum of returned values of func() [Skotlex]
  struct block_list* bl = NULL;
  int blockcount = 0;
  int i;

  if (m < 0) return 0;

  if (!map_isloaded(m)) return 0;

  x0 = 0;
  y0 = 0;
  x1 = map[m].xs - 1;
  y1 = map[m].ys - 1;

  if ((type & BL_MOB))
    for (by = y0 / BLOCK_SIZE; by <= y1 / BLOCK_SIZE; by++)
      for (bx = x0 / BLOCK_SIZE; bx <= x1 / BLOCK_SIZE; bx++)
        for (bl = map[m].block_mob[bx + by * map[m].bxs];
             bl && blockcount < BL_LIST_MAX; bl = bl->next) {
          if (bl->x >= x0 && bl->x <= x1 && bl->y >= y0 && bl->y <= y1 &&
              blockcount < BL_LIST_MAX)
            bl_list[blockcount++] = bl;
        }

  if (blockcount >= BL_LIST_MAX)
    printf("map_foreachinarea: block count too many!\n");

  for (i = 0; i < blockcount; i++)
    if (bl_list[i]->prev)  // �L?���ǂ����`�F�b�N
      returnCount += func(bl_list[i], ap);

  return returnCount;
}

int map_loadregistry(int id) {
  return rust_map_loadregistry(id);
}
int map_lastdeath_mob(MOB* p) {
  if (SQL_ERROR ==
      Sql_Query(sql_handle,
                "UPDATE `Spawns%i` SET `SpnLastDeath`='%u' WHERE `SpnX`='%u' "
                "AND `SpnY`='%u' AND `SpnMapId`='%u' AND `SpnId`='%u'",
                serverid, p->last_death, p->startx, p->starty, p->bl.m,
                p->id)) {
    Sql_ShowDebug(sql_handle);
  }

  return 0;
}

int map_registrysave(int m, int i) {
  struct global_reg* p = &map[m].registry[i];
  long long save_id = -1;
  SqlStmt* stmt;
  unsigned int reg_id;

  stmt = SqlStmt_Malloc(sql_handle);
  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    return 0;
  }
  if (SQL_ERROR ==
          SqlStmt_Prepare(stmt,
                          "SELECT `MrgPosition` FROM `MapRegistry` WHERE "
                          "`MrgMapId` = '%d' AND `MrgIdentifier`='%s'",
                          m, p->str) ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 0, SQLDT_UINT, &reg_id, 0, NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return 0;
  }

  if (SQL_SUCCESS == SqlStmt_NextRow(stmt)) {
    save_id = reg_id;
  }
  SqlStmt_Free(stmt);

  if (save_id != -1) {
    if (p->val == 0) {
      if (SQL_ERROR == Sql_Query(sql_handle,
                                 "DELETE FROM `MapRegistry` WHERE `MrgMapId` = "
                                 "'%u' AND `MrgIdentifier` = '%s'",
                                 m, p->str)) {
        Sql_ShowDebug(sql_handle);
        return 0;
      }
    } else {
      if (SQL_ERROR ==
          Sql_Query(
              sql_handle,
              "UPDATE `MapRegistry` SET `MrgIdentifier` = '%s', `MrgValue` = "
              "'%d' WHERE `MrgMapId` = '%u' AND `MrgPosition`='%d'",
              p->str, p->val, m, save_id)) {
        Sql_ShowDebug(sql_handle);
        return 0;
      }
    }
  } else {
    if (p->val > 0) {
      if (SQL_ERROR ==
          Sql_Query(
              sql_handle,
              "INSERT INTO `MapRegistry` (`MrgMapId`, `MrgIdentifier`, "
              "`MrgValue`, `MrgPosition`) VALUES ('%d', '%s', '%d', '%u')",
              m, p->str, p->val, i)) {
        Sql_ShowDebug(sql_handle);
        return 0;
      }
    }
  }

  return 0;
}
/*int map_registrydelete(int m, int i) {
        struct global_reg* p=&map[m].registry[i];

        if(SQL_ERROR == Sql_Query(sql_handle,"DELETE FROM `mapreg` WHERE
`map_id` = '%u' AND `name`='%s'",m,p->str)) Sql_ShowDebug(sql_handle);

        return 0;
}*/

// map data (title,id,and sound) is readed from configuration file
int map_read() {  // int id, const char *title, char bgm, int pvp, int spell,
                  // unsigned short light, unsigned short weather, unsigned int
                  // sweeptime, unsigned char cantalk, unsigned char showghosts,
                  // unsigned char region, unsigned char indoor, unsigned char
                  // warpout, const char *map_file) {
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
  // struct map_src_list *i = NULL;
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
    CALLOC(map[id].tile, unsigned short, map[id].xs* map[id].ys);
    CALLOC(map[id].obj, unsigned short, map[id].xs* map[id].ys);
    CALLOC(map[id].map, unsigned char, map[id].xs* map[id].ys);
    CALLOC(map[id].pass, unsigned short, map[id].xs* map[id].ys);

    map[id].bxs = (map[id].xs + BLOCK_SIZE - 1) / BLOCK_SIZE;
    map[id].bys = (map[id].ys + BLOCK_SIZE - 1) / BLOCK_SIZE;
    CALLOC(map[id].warp, struct warp_list*, map[id].bxs * map[id].bys);
    CALLOC(map[id].block, struct block_list*, map[id].bxs * map[id].bys);
    // CALLOC(map[id].block_npc, struct block_list*, map[id].bxs*map[id].bys);
    // CALLOC(map[id].block_count, int, map[id].bxs*map[id].bys);
    // CALLOC(map[id].block_npc_count, int, map[id].bxs*map[id].bys);
    CALLOC(map[id].block_mob, struct block_list*, map[id].bxs * map[id].bys);
    // CALLOC(map[id].item_sweep,FLOORITEM*,10000);
    // CALLOC(map[id].block_mob_count, int, map[id].bxs*map[id].bys);
    CALLOC(map[id].registry, struct global_reg, MAX_MAPREG);
    // map[id].block_mob_count=0;
    // map[id].item_sweep_count=0;
    // map[id].max_sweep_count=10000;
    while (!feof(fp)) {
      fread(&buff, 2, 1, fp);
      map[id].tile[pos] = SWAP16(buff);
      fread(&buff, 2, 1, fp);
      map[id].pass[pos] = SWAP16(buff);
      fread(&buff, 2, 1, fp);
      map[id].obj[pos] = SWAP16(buff);
      // map[id].pass[pos]=0;
      // all map section is walkable
      // map[id].map[pos] = 0;
      pos++;
      if (pos >= map[id].xs * map[id].ys) break;
    }
    pos = 0;
    fclose(fp);
    map_loadregistry(id);
  }

  SqlStmt_Free(stmt);
  printf("Map data file reading finished. %d map loaded!\n", map_n);
  // timer_insert(1800000,1800000,map_saveregistry, id, 0);
  // map_n++;
  return 0;
}

int map_reload() {
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

int map_setmapip(int id, unsigned int ip, unsigned short port) {
  if (id > MAX_MAP_PER_SERVER) return 1;

  map[id].ip = ip;
  map[id].port = port;
  return 0;
}

int lang_read(const char* cfg_file) {
  char line[1024], r1[1024], r2[1024];
  int line_num = 0, i;
  FILE* fp;
  struct {
    char* name;
    int number;
  } map_msg_db[] = {{"MAP_WHISPFAIL", MAP_WHISPFAIL},
                    {"MAP_ERRGHOST", MAP_ERRGHOST},
                    {"MAP_ERRITMPATH", MAP_ERRITMPATH},
                    {"MAP_ERRITMMARK", MAP_ERRITMMARK},
                    {"MAP_ERRITMLEVEL", MAP_ERRITMLEVEL},
                    {"MAP_ERRITMMIGHT", MAP_ERRITMMIGHT},
                    {"MAP_ERRITMGRACE", MAP_ERRITMGRACE},
                    {"MAP_ERRITMWILL", MAP_ERRITMWILL},
                    {"MAP_ERRITMSEX", MAP_ERRITMSEX},
                    {"MAP_ERRITMFULL", MAP_ERRITMFULL},
                    {"MAP_ERRITMMAX", MAP_ERRITMMAX},
                    {"MAP_ERRITM2H", MAP_ERRITM2H},
                    {"MAP_ERRMOUNT", MAP_ERRMOUNT},
                    {"MAP_ERRVITA", MAP_ERRVITA},
                    {"MAP_ERRMANA", MAP_ERRMANA},
                    {"MAP_EQHELM", MAP_EQHELM},
                    {"MAP_EQWEAP", MAP_EQWEAP},
                    {"MAP_EQARMOR", MAP_EQARMOR},
                    {"MAP_EQSHIELD", MAP_EQSHIELD},
                    {"MAP_EQLEFT", MAP_EQLEFT},
                    {"MAP_EQRIGHT", MAP_EQRIGHT},
                    {"MAP_EQSUBLEFT", MAP_EQSUBLEFT},
                    {"MAP_EQSUBRIGHT", MAP_EQSUBRIGHT},
                    {"MAP_EQFACEACC", MAP_EQFACEACC},
                    {"MAP_EQCROWN", MAP_EQCROWN},
                    {"MAP_EQMANTLE", MAP_EQMANTLE},
                    {"MAP_EQNECKLACE", MAP_EQNECKLACE},
                    {"MAP_EQBOOTS", MAP_EQBOOTS},
                    {"MAP_EQCOAT", MAP_EQCOAT},
                    {"MAP_ERRSUMMON", MAP_ERRSUMMON},
                    {NULL, 0}};

  fp = fopen(cfg_file, "r");
  if (fp == NULL) {
    printf("CFG_ERR: Language file (%s) not found.\n", cfg_file);
    return 1;
  }

  while (fgets(line, sizeof(line), fp)) {
    line_num++;
    if (line[0] == '/' && line[1] == '/') continue;

    if (sscanf(line, "%[^:]: %[^\r\n]", r1, r2) == 2) {
      for (i = 0; map_msg_db[i].name; i++) {
        if (strcasecmp(map_msg_db[i].name, r1) == 0) {
          strncpy(map_msg[map_msg_db[i].number].message, r2, 256);
          map_msg[map_msg_db[i].number].message[255] = '\0';
          map_msg[map_msg_db[i].number].len = strlen(r2);
          if (map_msg[map_msg_db[i].number].len > 255)
            map_msg[map_msg_db[i].number].len = 255;
          break;
        }
      }
    }
  }
  fclose(fp);
  printf("[map] [lang_read] file=%s\n", cfg_file);
  return 0;
}

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
    // FREE(map[i].block_npc);
    // FREE(map[i].block_count);
    // FREE(map[i].block_npc_count);
    FREE(map[i].block_mob);
    // FREE(map[i].block_mob_count);
    FREE(map[i].warp);
  }
  map_termblock();
  itemdb_term();
  magicdb_term();
  classdb_term();
  // boarddb_term();
  // mobdb_term();
  // sql_close();

  printf("[map] Map Server Shutdown\n");
}

void help_screen() {
  printf("HELP LIST\n");
  printf("---------\n");
  printf(" --conf [FILENAME]  : set config file\n");
  printf(" --lang [FILENAME]  : set lang file\n");
  exit(0);
}
int change_time_char(int none, int none2) {
  int i;
  USER* sd;
  cur_time++;

  if (cur_time == 24) {
    cur_time = 0;
    cur_day++;
    if (cur_day == 92) {
      cur_day = 1;
      cur_season++;

      if (cur_season == 5) {
        cur_season = 1;
        cur_year++;
      }
    }
  }

  for (i = 0; i < fd_max; i++) {
    if (rust_session_exists(i) && (sd = rust_session_get_data(i))) {
      clif_sendtime(sd);
    }
  }

  if (SQL_ERROR == Sql_Query(sql_handle,
                             "UPDATE `Time` SET `TimHour` ='%d', "
                             "`TimDay`='%d', `TimSeason`='%d', `TimYear`='%d'",
                             cur_time, cur_day, cur_season, cur_year))
    Sql_ShowDebug(sql_handle);

  return 0;
}
int get_time_thing(void) {
  SqlStmt* stmt;
  int time, day, season, year;
  stmt = SqlStmt_Malloc(sql_handle);
  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    return 0;
  }

  if (SQL_ERROR == SqlStmt_Prepare(stmt,
                                   "SELECT `TimHour`, `TimDay`, `TimSeason`, "
                                   "`TimYear` FROM `Time`") ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 0, SQLDT_INT, &time, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 1, SQLDT_INT, &day, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 2, SQLDT_INT, &season, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 3, SQLDT_INT, &year, 0, NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return 0;
  }

  if (SQL_SUCCESS == SqlStmt_NextRow(stmt)) {
    old_time = time;
    cur_time = time;
    cur_day = day;
    cur_season = season;
    cur_year = year;
  }

  SqlStmt_Free(stmt);

  return 0;
}
int uptime(void) {
  if (SQL_ERROR ==
      Sql_Query(sql_handle, "DELETE FROM `UpTime` WHERE `UtmId` = '3'"))
    Sql_ShowDebug(sql_handle);

  if (SQL_ERROR ==
      Sql_Query(sql_handle,
                "INSERT INTO `UpTime`(`UtmId`, `UtmValue`) VALUES('3', '%d')",
                gettickthing()))
    Sql_ShowDebug(sql_handle);

  return 0;
}
int object_flag_init(void) {
  int num = 0;
  char nothing[8] = "";
  short tile = 0;
  char flag = 0;
  char count = 0;
  int z = 1;

  char* filename = "static_objects.tbl";
  size_t path_size = strlen(data_dir) + strlen(filename) + 1;
  char* path = malloc(path_size);

  strncpy(path, data_dir, path_size);
  strncat(path, filename, path_size);

  FILE* fi = fopen(path, "rb");

  printf("[map] [object_flag_init] reading static obj table path=%s\n", path);

  if (fi == NULL) {
    printf("[map] [error] cannot read static object table path=%s\n", path);
    exit(1);
  }

  fread(&num, 4, 1, fi);
  CALLOC(objectFlags, unsigned char, num + 1);
  fread(&flag, 1, 1, fi);

  while (!feof(fi)) {
    fread(&count, 1, 1, fi);
    for (; count != 0; count--) {
      fread(&tile, 2, 1, fi);
    }

    fread(&nothing, 5, 1, fi);
    fread(&flag, 1, 1, fi);
    // objectFlags[z]=flag;
    // fwrite(&flag, 1, 1, fo);
    z++;
  }

  free(path);
  return 0;
}

// map_canmove ported to Rust — see src/game/map_server.rs

// boards_delete ported to Rust — see src/game/map_server.rs

// boards_showposts ported to Rust — see src/game/map_server.rs

// boards_readpost ported to Rust — see src/game/map_server.rs

// boards_post ported to Rust — see src/game/map_server.rs

/* N-Mail */

// nmail_read ported to Rust — see src/game/map_server.rs

// nmail_luascript ported to Rust — see src/game/map_server.rs

// nmail_poemscript ported to Rust — see src/game/map_server.rs

// nmail_sendmailcopy ported to Rust — see src/game/map_server.rs

// nmail_write ported to Rust — see src/game/map_server.rs

// nmail_sendmail ported to Rust — see src/game/map_server.rs

// map_addmob ported to Rust — see src/game/map_server.rs
// map_changepostcolor ported to Rust — see src/game/map_server.rs
// map_getpostcolor ported to Rust — see src/game/map_server.rs

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



int hasCoref(USER* sd) {
  USER* nsd = NULL;

  if (sd->coref) return 1;
  if (sd->coref_container) {
    nsd = map_id2sd(sd->coref_container);
    if (!nsd) return 0;
    return 1;
  }

  return 0;
}
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

    // Flush the stupid pipe for saving everyones stupid character

    rust_request_shutdown();
    reset = 0;
    diff = 0;

    // EnableReset=1;
    // exit(0);

    // exit(0);
    return 1;
  }

  if (reset <=
      60000) {  // Less than a minute remaining(gonna mass spell everyone)
    if (diff >= 10000) {  // every 10 seconds

      sprintf(msg, "Reset in %d seconds", reset / 1000);
      // clif_broadcast("---------------------------------------------------",-1);
      clif_broadcast(msg, -1);
      // clif_broadcast("---------------------------------------------------",-1);
      diff = 0;
    }
  } else if (reset <= 3600000) {  // 60 mins
    if (diff >= 300000) {         // every 5 mins
      sprintf(msg, "Reset in %d minutes", reset / 60000);
      // clif_broadcast("---------------------------------------------------",-1);
      clif_broadcast(msg, -1);
      // clif_broadcast("---------------------------------------------------",-1);
      diff = 0;
    }
  } else if (reset > 3600000) {  // every hour
    if (diff >= 3600000) {       // once every hour
      sprintf(msg, "Reset in %d hours", reset / 3600000);
      // clif_broadcast("---------------------------------------------------",-1);
      clif_broadcast(msg, -1);
      // clif_broadcast("---------------------------------------------------",-1);
      diff = 0;
    }
  }

  return 0;
}
int map_setglobalreg(int m, const char* reg, int val) {
  int i, exist;

  exist = -1;
  nullpo_ret(0, reg);
  if (!map_isloaded(m)) return 0;

  for (i = 0; i < map[m].registry_num; i++) {
    if (strcasecmp(map[m].registry[i].str, reg) == 0) {
      exist = i;
      break;
    }
  }
  // if registry exists, set value
  if (exist != -1) {
    if (val == 0) {
      map[m].registry[exist].val = val;
      map_registrysave(m, exist);
      strcpy(map[m].registry[exist].str, "");  // empty registry
      return 0;
    } else {
      map[m].registry[exist].val = val;
      map_registrysave(m, exist);
      return 0;
    }
  } else {
    for (i = 0; i < map[m].registry_num; i++) {
      if (strcasecmp(map[m].registry[i].str, "") == 0) {
        strcpy(map[m].registry[i].str, reg);
        map[m].registry[i].val = val;
        map_registrysave(m, i);
        return 0;
      }
    }

    if (map[m].registry_num < MAX_MAPREG) {
      map[m].registry_num = map[m].registry_num + 1;
      i = map[m].registry_num - 1;
      strcpy(map[m].registry[i].str, reg);
      map[m].registry[i].val = val;
      map_registrysave(m, i);
      return 0;
    }
  }

  return 0;
}

int map_readglobalreg(int m, const char* reg) {
  int i, exist;

  exist = -1;
  if (!map_isloaded(m)) return 0;

  for (i = 0; i < map[m].registry_num; i++) {
    if (strcasecmp(map[m].registry[i].str, reg) == 0) {
      exist = i;
      break;
    }
  }
  if (exist != -1) {
    return map[m].registry[exist].val;
  } else {
    return 0;
  }
  return 0;
}
// Game registries
// loads game registries
int map_loadgameregistry() {
  SqlStmt* stmt;
  struct global_reg reg;
  int i;

  gamereg.registry_num = 0;
  CALLOC(gamereg.registry, struct global_reg, MAX_GAMEREG);
  memset(&reg, 0, sizeof(reg));

  stmt = SqlStmt_Malloc(sql_handle);
  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    return 0;
  }

  if (SQL_ERROR == SqlStmt_Prepare(stmt,
                                   "SELECT `GrgIdentifier`, `GrgValue` FROM "
                                   "`GameRegistry%d` LIMIT %d",
                                   serverid, MAX_GAMEREG) ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_STRING, &reg.str,
                                      sizeof(reg.str), NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 1, SQLDT_INT, &reg.val, 0, NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return 0;
  }

  gamereg.registry_num = SqlStmt_NumRows(stmt);

  for (i = 0; i < gamereg.registry_num && SQL_SUCCESS == SqlStmt_NextRow(stmt);
       i++) {
    memcpy(&gamereg.registry[i], &reg, sizeof(reg));
  }

  printf("[map] [load_game_registry] count=%d\n", gamereg.registry_num);
  SqlStmt_Free(stmt);
  return 0;
}
// saves gameregistries
int map_savegameregistry(int i) {
  struct global_reg* p = &gamereg.registry[i];
  unsigned int save_id = 0;
  SqlStmt* stmt;
  unsigned int gameregid;

  stmt = SqlStmt_Malloc(sql_handle);
  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    return 0;
  }

  if (SQL_ERROR == SqlStmt_Prepare(stmt,
                                   "SELECT `GrgId` FROM `GameRegistry%d` WHERE "
                                   "`GrgIdentifier` = '%s'",
                                   serverid, p->str) ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 0, SQLDT_UINT, &gameregid, 0, NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return 0;
  }

  if (SQL_SUCCESS == SqlStmt_NextRow(stmt)) {
    save_id = gameregid;
  }

  SqlStmt_Free(stmt);

  if (save_id) {
    if (p->val == 0) {
      if (SQL_ERROR ==
          Sql_Query(sql_handle,
                    "DELETE FROM `GameRegistry%d` WHERE `GrgIdentifier` = '%s'",
                    serverid, p->str)) {
        Sql_ShowDebug(sql_handle);
        return 0;
      }
    } else {
      if (SQL_ERROR ==
          Sql_Query(sql_handle,
                    "UPDATE `GameRegistry%d` SET `GrgIdentifier` = '%s', "
                    "`GrgValue` = '%d' WHERE `GrgId` = '%d'",
                    serverid, p->str, p->val, save_id)) {
        Sql_ShowDebug(sql_handle);
        return 0;
      }
    }
  } else {
    if (p->val > 0) {
      if (SQL_ERROR ==
          Sql_Query(sql_handle,
                    "INSERT INTO `GameRegistry%d` (`GrgIdentifier`, "
                    "`GrgValue`) VALUES ('%s', '%d')",
                    serverid, p->str, p->val)) {
        Sql_ShowDebug(sql_handle);
        return 0;
      }
    }
  }

  return 0;
}
// sets game registry
int map_setglobalgamereg(const char* reg, int val) {
  int i, exist;

  exist = -1;
  nullpo_ret(0, reg);

  // if registry exists, get number
  for (i = 0; i < gamereg.registry_num; i++) {
    if (!strcasecmp(gamereg.registry[i].str, reg)) {
      exist = i;
      break;
    }
  }
  // if registry exists, set value
  if (exist != -1) {
    if (val == 0) {
      gamereg.registry[exist].val = val;
      map_savegameregistry(exist);
      strcpy(gamereg.registry[exist].str, "");  // empty registry
      return 0;
    } else {
      gamereg.registry[exist].val = val;
      map_savegameregistry(exist);
      return 0;
    }
  } else {
    for (i = 0; i < gamereg.registry_num; i++) {
      if (!strcasecmp(gamereg.registry[i].str, "")) {
        strcpy(gamereg.registry[i].str, reg);
        gamereg.registry[i].val = val;
        map_savegameregistry(i);
        return 0;
      }
    }

    if (gamereg.registry_num < MAX_GLOBALREG) {
      gamereg.registry_num = gamereg.registry_num + 1;
      i = gamereg.registry_num - 1;
      strcpy(gamereg.registry[i].str, reg);
      gamereg.registry[i].val = val;
      map_savegameregistry(i);
      return 0;
    }
  }

  return 0;
}
// reads game registry
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
  } else {
    return 0;
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

// map_cronjob ported to Rust — see src/game/map_server.rs:rust_map_cronjob

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
