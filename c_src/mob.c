// mob.c — C helpers that require direct USER/map[] access.
// All game logic has been ported to src/game/mob.rs.
// This file provides:
//   mob_free_helper     — FREE() wrapper for Rust
//   mob_addtocurrent    — floor-item merge callback (used by mobdb_dropitem)
//   mobdb_dropitem      — reads groups[], attacker->groupid (USER-dependent)
//   mob_null            — no-op va_list callback
//   mobdb_init          — calls rust_mobdb_init() + rust_mobspawn_read()
//   mob_find_target     — reads sd->status.dura_aether[], gm_level, id
//   mob_attack          — reads sd->uFlags, optFlags, calls clif_send_*
//   mob_calc_critical   — reads sd->status.level, sd->grace
//   mob_move            — collision callback: reads sd->status.state, gm_level
#include "mob.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "config.h"
#include "core.h"
#include "db_mysql.h"
#include "map_parse.h"
#include "map_server.h"
#include "rndm.h"
#include "scripting.h"
#include "strlib.h"
#include "timer.h"

// Note: all global variables (mob_id, MOB_SPAWN_MAX, etc.) are now owned
// by Rust (src/game/mob.rs) via #[export_name]. Do not redeclare them here.

void mob_free_helper(MOB *m) { FREE(m); }

int mob_addtocurrent(struct block_list *bl, va_list ap) {
  int *def;
  FLOORITEM *fl;
  FLOORITEM *fl2;

  nullpo_ret(0, fl = (FLOORITEM *)bl);

  def = va_arg(ap, int *);
  va_arg(ap, int);  // id
  nullpo_ret(0, fl2 = va_arg(ap, FLOORITEM *));
  va_arg(ap, USER *);  // sd

  if (def[0]) return 0;

  if (fl->data.id == fl2->data.id) {
    fl->data.amount += fl2->data.amount;
    def[0] = 1;
    return 0;
  }
  return 0;
}

int mobdb_dropitem(unsigned int blockid, unsigned int id, int amount, int dura,
                   int protected, int owner, int m, int x, int y, USER *sd) {
  MOB *mob = NULL;
  FLOORITEM *fl;
  int def[1];
  int z;

  if (blockid >= MOB_START_NUM && blockid < FLOORITEM_START_NUM) {
    mob = map_id2mob((unsigned int)blockid);
  }

  def[0] = 0;
  CALLOC(fl, FLOORITEM, 1);
  fl->bl.m = m;
  fl->bl.x = x;
  fl->bl.y = y;
  fl->data.id = id;
  fl->data.amount = amount;
  fl->data.dura = dura;
  fl->data.protected = protected;
  fl->data.owner = owner;

  map_foreachincell(mob_addtocurrent, m, x, y, BL_ITEM, def, id, fl, sd);

  fl->timer = time(NULL);

  memset(&fl->looters, 0, sizeof(int) * MAX_GROUP_MEMBERS);

  if (mob) {
    struct map_sessiondata *attacker = map_id2sd(mob->attacker);
    if (attacker) {
      if (attacker->group_count > 0) {
        for (z = 0; z < attacker->group_count; z++) {
          fl->looters[z] = groups[attacker->groupid][z];
        }
      } else {
        fl->looters[0] = attacker->bl.id;
      }
    }
  }

  if (!def[0]) {
    map_additem(&fl->bl);
    map_foreachinarea(clif_object_look_sub2, m, x, y, AREA, BL_PC, LOOK_SEND,
                      &fl->bl);
  } else {
    FREE(fl);
  }

  return 0;
}

int mob_null(struct block_list *bl, va_list ap) { (void)bl; (void)ap; return 0; }

int mobdb_init() {
  if (rust_mobdb_init() != 0) return -1;
  rust_mobspawn_read();
  return 0;
}

int mob_find_target(struct block_list *bl, va_list ap) {
  MOB *mob;
  USER *sd;
  int i = 0;
  char invis = 0;
  char seeinvis = 0;
  short num = 0;

  nullpo_ret(0, mob = va_arg(ap, MOB *));
  nullpo_ret(0, sd = (USER *)bl);
  seeinvis = mob->data->seeinvis;
  for (i = 0; i < MAX_MAGIC_TIMERS; i++) {
    if (sd->status.dura_aether[i].duration > 0) {
      if (!strcasecmp(magicdb_name(sd->status.dura_aether[i].id), "sneak"))
        invis = 1;
      if (!strcasecmp(magicdb_name(sd->status.dura_aether[i].id), "cloak"))
        invis = 2;
      if (!strcasecmp(magicdb_name(sd->status.dura_aether[i].id), "hide"))
        invis = 3;
    }
  }

  switch (invis) {
    case 1:
      if (seeinvis != 1 && seeinvis != 3 && seeinvis != 5) return 0;
      break;
    case 2:
      if (seeinvis != 2 && seeinvis != 3 && seeinvis != 5) return 0;
      break;
    case 3:
      if (seeinvis != 4 && seeinvis != 5) return 0;
      break;
    default:
      break;
  }

  if (sd->status.state == 1) return 0;

  if (mob->confused && mob->confused_target == sd->bl.id) return 0;

  if (mob->target) {
    num = rnd(1000);
    if (num <= 499 && sd->status.gm_level < 50) {
      mob->target = sd->status.id;
    }
  } else if (sd->status.gm_level < 50) {
    mob->target = sd->status.id;
  }

  return 0;
}

int mob_attack(MOB *mob, int id) {
  USER *sd = NULL;
  MOB *tmob = NULL;
  int x = 0;

  if (id < 0) return 0;

  struct block_list *bl = map_id2bl((unsigned int)id);
  if (bl == NULL) return 0;

  if (bl->type == BL_PC) {
    sd = (USER *)bl;
  } else if (bl->type == BL_MOB) {
    tmob = (MOB *)bl;
  }

  if (sd) {
    if (sd->uFlags & uFlag_immortal || sd->optFlags & optFlag_stealth) {
      mob->target = 0;
      mob->attacker = 0;
      return 0;
    }
  }

  if (sd != NULL) {
    sl_doscript_blargs("hitCritChance", NULL, 2, &mob->bl, &sd->bl);
  } else if (tmob != NULL) {
    sl_doscript_blargs("hitCritChance", NULL, 2, &mob->bl, &tmob->bl);
  }

  if (mob->critchance) {
    if (sd != NULL) {
      sl_doscript_blargs("swingDamage", NULL, 2, &mob->bl, &sd->bl);
      for (x = 0; x < MAX_MAGIC_TIMERS; x++) {
        if (mob->da[x].id > 0 && mob->da[x].duration > 0) {
          sl_doscript_blargs(magicdb_yname(mob->da[x].id),
                             "on_hit_while_cast", 2, &mob->bl, &sd->bl);
        }
      }
    } else if (tmob != NULL) {
      sl_doscript_blargs("swingDamage", NULL, 2, &mob->bl, &tmob->bl);
      for (x = 0; x < MAX_MAGIC_TIMERS; x++) {
        if (mob->da[x].id > 0 && mob->da[x].duration > 0) {
          sl_doscript_blargs(magicdb_yname(mob->da[x].id),
                             "on_hit_while_cast", 2, &mob->bl, &tmob->bl);
        }
      }
    }
    int dmg = (int)(mob->damage += 0.5f);
    if (sd != NULL) {
      if (mob->critchance == 1) {
        clif_send_pc_health(sd, dmg, 33);
      } else {
        clif_send_pc_health(sd, dmg, 255);
      }
      clif_sendstatus(sd, SFLAG_HPMP);
    } else if (tmob != NULL) {
      if (mob->critchance == 1) {
        clif_send_mob_health(tmob, dmg, 33);
      } else {
        clif_send_mob_health(tmob, dmg, 255);
      }
    }
  }

  return 0;
}

int mob_calc_critical(MOB *mob, USER *sd) {
  int equat;
  int chance;
  float crit;

  equat = (mob->data->hit + mob->data->level + (mob->data->might / 5) + 20) -
          (sd->status.level + (sd->grace / 2));
  equat = equat - (sd->grace / 4) + sd->status.level;

  chance = rnd(100);

  if (equat < 5) equat = 5;
  if (equat > 95) equat = 95;

  if (chance < equat) {
    crit = (float)equat * 0.33f;
    if (chance < crit) {
      return 2;
    } else {
      return 1;
    }
  } else {
    return 0;
  }
}

int mob_move(struct block_list *bl, va_list ap) {
  MOB *mob;
  MOB *m2;
  USER *sd;

  nullpo_ret(0, mob = va_arg(ap, MOB *));
  if (mob->canmove == 1) return 0;
  if (bl->type == BL_NPC) {
    if (bl->subtype) {
      return 0;
    }
  } else if (bl->type == BL_MOB) {
    m2 = (MOB *)bl;
    if (m2) {
      if (m2->state == MOB_DEAD) {
        return 0;
      }
    }
  } else if (bl->type == BL_PC) {
    sd = (USER *)bl;
    if ((map[mob->bl.m].show_ghosts && sd->status.state == PC_DIE) ||
        sd->status.state == -1 || sd->status.gm_level >= 50) {
      return 0;
    }
  }

  mob->canmove = 1;
  return 0;
}
