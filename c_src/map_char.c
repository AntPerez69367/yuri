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
  printf("[map] [intif_mmo_tosd] ENTER fd=%d p=%p map_fd=%d\n", fd, (void*)p, map_fd);
  fflush(stdout);
  if (fd == map_fd) {
    printf("[map] [intif_mmo_tosd] REJECTED: fd == map_fd (%d)\n", map_fd);
    fflush(stdout);
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

  printf("[map] [intif_mmo_tosd] STRUCT CHECK: id=%u name=%.16s level=%d class=%d "
         "hp=%u mp=%u exp=%u money=%u sex=%d country=%d partner=%u clan=%u\n",
         sd->status.id, sd->status.name, sd->status.level, sd->status.class,
         sd->status.hp, sd->status.mp, sd->status.exp, sd->status.money,
         sd->status.sex, sd->status.country, sd->status.partner, sd->status.clan);
  fflush(stdout);

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

  /// test stuff
  /*
  WFIFOB(sd->fd,0)=0xAA;
  WFIFOB(sd->fd,1)=0x00;
  WFIFOB(sd->fd,2)=0x03;
  WFIFOB(sd->fd,3)=0x49;
  WFIFOB(sd->fd,4)=0x23;
  WFIFOB(sd->fd,5)=0x6D;
  WFIFOSET(sd->fd,6);
  */
  printf("[map] [intif_mmo_tosd] SUCCESS: player spawned name=%s map=%d x=%d y=%d\n",
         sd->status.name, sd->status.last_pos.m, sd->status.last_pos.x, sd->status.last_pos.y);
  fflush(stdout);
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
int authdb_init() {
  // auth_db=strdb_alloc(DB_OPT_BASE,32);
  return 0;
}
int auth_timer(int id, int none) {
  // struct auth_node* a=(struct auth_node*)strdb_get(auth_db,(char*)id);
  // sql_request("DELETE FROM auth WHERE char_id='%u'",id);
  // sql_get_row();
  // sql_free_row();
  Sql_Query(sql_handle, "DELETE FROM `Authorize` WHERE `AutChaId` = '%d'", id);
  // if(!a) return 1;
  // strdb_remove(auth_db,a->name);
  // FREE(a);

  // auth_fifo[id].ip=0;
  // auth_fifo[id].id=0;
  // memset(auth_fifo[id].name,0,16);
  return 1;
}
int auth_check(char* name, unsigned int ip) {
  unsigned int i;
  unsigned int id;
  char* data;

  if (SQL_ERROR == Sql_Query(sql_handle,
                             "SELECT `AutIP`, `AutChaId` FROM `Authorize` "
                             "WHERE `AutChaName` = '%s'",
                             name))
    Sql_ShowDebug(sql_handle);

  if (SQL_SUCCESS != Sql_NextRow(sql_handle)) return 0;  // Not available

  Sql_GetData(sql_handle, 0, &data, 0);
  i = strtoul(data, NULL, 10);
  Sql_GetData(sql_handle, 1, &data, 0);
  id = (unsigned int)strtoul(data, NULL, 10);
  Sql_FreeResult(sql_handle);
  if (i == ip) return id;

  return 0;
  // struct auth_node* t=(struct auth_node*)strdb_get(auth_db,name);
}
int auth_delete(char* name) {
  char* data;
  Sql_Query(sql_handle,
            "SELECT `AutTimer` FROM `Authorize` WHERE `AutChaName` = '%s'",
            name);

  if (SQL_SUCCESS != Sql_NextRow(sql_handle)) return 0;

  Sql_GetData(sql_handle, 0, &data, 0);
  timer_remove((unsigned int)strtoul(data, NULL, 10));
  Sql_FreeResult(sql_handle);

  Sql_Query(sql_handle, "DELETE FROM `Authorize` WHERE `AutChaName` = '%s'",
            name);
  // sql_request("SELECT timer FROM auth WHERE name='%s'",name);
  // if(sql_get_row())
  //	return 0;

  // timer_remove(sql_get_int(0));

  // sql_request("DELETE FROM auth WHERE name='%s'",name);
  // sql_get_row();

  // sql_free_row();
  return 0;
}
int auth_add(char* name, unsigned int id, unsigned int ip) {
  int timer;
  // sql_request("SELECT * FROM auth WHERE name='%s'",name);
  if (SQL_ERROR ==
      Sql_Query(sql_handle,
                "SELECT * FROM `Authorize` WHERE `AutChaName` = '%s'", name))
    Sql_ShowDebug(sql_handle);

  if (SQL_SUCCESS == Sql_NextRow(sql_handle)) return 0;

  timer = timer_insert(120000, 120000, auth_timer, id, 0);

  if (SQL_ERROR ==
      Sql_Query(sql_handle,
                "INSERT INTO `Authorize` (`AutChaName`, `AutChaId`, `AutIP`, "
                "`AutTimer`) VALUES('%s', '%u', '%u', '%u')",
                name, id, ip, timer))
    Sql_ShowDebug(sql_handle);

  // sql_request("INSERT INTO auth (name,char_id,ip,timer)
  // VALUES('%s','%u','%u','%u')",name,id,ip,timer); sql_get_row();
  // sql_free_row();
  // strdb_put(auth_db,t->name,t);
  return 0;
}

int intif_init() {
  // timer_insert(300000,300000,intif_timer,save_time,0);
  // intif_table_recv[3] = sizeof(struct mmo_charstatus)+3;
  return 0;
}
