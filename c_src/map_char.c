#include "map_char.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <zlib.h>

#include "config.h"
#include "core.h"
#include "db.h"
#include "db_mysql.h"
#include "map_parse.h"
#include "map_server.h"
#include "mmo.h"
#include "net_crypt.h"
#include "pc.h"
#include "session.h"
#include "strlib.h"
#include "timer.h"


int intif_mmo_tosd(int fd, struct mmo_charstatus* p) {
  USER* sd;
  if (fd == map_fd) {
    return 0;
  }
  if (!p) {
    rust_session_set_eof(fd, 7);
    return 0;
  }
  // if(rust_session_get_data(fd)) { //data already exists
  //	session[fd]->eof=8;
  //	return 0;
  //}

  CALLOC(sd, USER, 1);
  if (!sd) {
    printf("[map] [intif_mmo_tosd] calloc failed for fd=%d, closing session\n", fd);
    rust_session_set_eof(fd, 7);
    return 0;
  }
  memcpy(&sd->status, p, sizeof(struct mmo_charstatus));

  sd->fd = fd;

  rust_session_set_data(fd, sd);

  populate_table(sd->status.name, sd->EncHash, sizeof(sd->EncHash));
  sd->bl.id = sd->status.id;
  sd->bl.prev = sd->bl.next = NULL;

  sd->disguise = sd->status.disguise;
  sd->disguise_color = sd->status.disguisecolor;
  sd->viewx = 8;
  sd->viewy = 7;

  strcpy(sd->ipaddress, p->ipaddress);

  SqlStmt* stmt;

  stmt = SqlStmt_Malloc(sql_handle);
  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    return 0;
  }

  if (SQL_ERROR == SqlStmt_Prepare(stmt,
                                   "SELECT `ChaMapId`, `ChaX`, `ChaY` FROM "
                                   "`Character` WHERE `ChaId` = '%d'",
                                   sd->status.id) ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_USHORT,
                                      &sd->status.last_pos.m, 0, NULL, NULL) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 1, SQLDT_USHORT,
                                      &sd->status.last_pos.x, 0, NULL, NULL) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 2, SQLDT_USHORT,
                                      &sd->status.last_pos.y, 0, NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
  }

  if (SQL_SUCCESS != SqlStmt_NextRow(stmt)) {
    // SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
  }

  // if (sd->status.last_pos.m == NULL) { sd->status.last_pos.m = 1000;
  // sd->status.last_pos.x = 8; sd->status.last_pos.y = 7; } // commented on
  // 05-28-18

  if (sd->status.gm_level) sd->optFlags = optFlag_walkthrough;  //.
  if (!map_isloaded(sd->status.last_pos.m)) {
    sd->status.last_pos.m=0; sd->status.last_pos.x=8;
    sd->status.last_pos.y=7;
  }

  pc_setpos(sd, sd->status.last_pos.m, sd->status.last_pos.x,
            sd->status.last_pos.y);
  pc_loadmagic(sd);
  pc_starttimer(sd);
  pc_requestmp(sd);

  clif_sendack(sd);
  clif_sendtime(sd);
  clif_sendid(sd);
  clif_sendmapinfo(sd);
  clif_sendstatus(sd, SFLAG_FULLSTATS | SFLAG_HPMP | SFLAG_XPMONEY);
  clif_mystaytus(sd);
  clif_spawn(sd);
  clif_refresh(sd);
  clif_sendxy(sd);
  clif_getchararea(sd);

  clif_mob_look_start(sd);
  map_foreachinarea(clif_object_look_sub, sd->bl.m, sd->bl.x, sd->bl.y,
                    SAMEAREA, BL_ALL, LOOK_GET, sd);
  clif_mob_look_close(sd);

  pc_loaditem(sd);
  pc_loadequip(sd);

  pc_magic_startup(sd);
  map_addiddb(&sd->bl);

  mmo_setonline(sd->status.id, 1);

  if (sd->status.gm_level) {
    // sd->optFlags|=optFlag_stealth;
    // printf("GM(%s) set to stealth.\n",sd->status.name);
  }

  pc_calcstat(sd);
  pc_checklevel(sd);
  clif_mystaytus(sd);
  map_foreachinarea(clif_updatestate, sd->bl.m, sd->bl.x, sd->bl.y, AREA, BL_PC,
                    sd);
  clif_retrieveprofile(sd);
  return 0;
}
// auth_check/auth_add/auth_delete/auth_timer removed — auth_check was
// commented out in map_parse.c; auth management is now in-memory (Rust state.auth_db).
// auth_delete in map_parse.c is a no-op vestige; the Authorize SQL table is unused.
