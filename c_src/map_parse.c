#include "map_parse.h"

#include "yuri.h"
#include <arpa/inet.h>
#include <math.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <zlib.h>

#include "board_db.h"
#include "clan_db.h"
#include "class_db.h"
#include "config.h"
#include "core.h"
#include "creation_db.h"
#include "db_mysql.h"
#include "gm_command.h"
#include "item_db.h"
#include "magic_db.h"
#include "map_char.h"
#include "map_server.h"
#include "mmo.h"
#include "mob.h"
#include "net_crypt.h"
#include "pc.h"
#include "rndm.h"
#include "scripting.h"
#include "session.h"
#include "showmsg.h"
#include "timer.h"

unsigned int groups[MAX_GROUPS][MAX_GROUP_MEMBERS];
int val[32];

int flags[16] = {1,   2,   4,    8,    16,   32,   64,    128,
                 256, 512, 1024, 2048, 4096, 8192, 16386, 32768};

int getclifslotfromequiptype(int equipType) {
  int type;

  switch (equipType) {
    case EQ_WEAP:
      type = 0x01;
      break;
    case EQ_ARMOR:
      type = 0x02;
      break;
    case EQ_SHIELD:
      type = 0x03;
      break;
    case EQ_HELM:
      type = 0x04;
      break;
    case EQ_NECKLACE:
      type = 0x06;
      break;
    case EQ_LEFT:
      type = 0x07;
      break;
    case EQ_RIGHT:
      type = 0x08;
      break;
    case EQ_BOOTS:
      type = 13;
      break;
    case EQ_MANTLE:
      type = 14;
      break;
    case EQ_COAT:
      type = 16;
      break;
    case EQ_SUBLEFT:
      type = 20;
      break;
    case EQ_SUBRIGHT:
      type = 21;
      break;
    case EQ_FACEACC:
      type = 22;
      break;
    case EQ_CROWN:
      type = 23;
      break;
    default:
      type = 0;
  }

  return type;
}

char *replace_str(char *str, char *orig, char *rep) {
  // puts(replace_str("Hello, world!", "world", "Miami"));

  static char buffer[4096];
  char *p;

  if (!(p = strstr(str, orig)))  // Is 'orig' even in 'str'?
    return str;

  strncpy(buffer, str,
          p - str);  // Copy characters from 'str' start to 'orig' st$
  buffer[p - str] = '\0';

  sprintf(buffer + (p - str), "%s%s", rep, p + strlen(orig));

  return buffer;
}

char *clif_getName(unsigned int id) {
  // char* name;
  // CALLOC(name,char,16);
  // memset(name,0,16);

  static char name[16];
  memset(name, 0, 16);

  SqlStmt *stmt;

  stmt = SqlStmt_Malloc(sql_handle);
  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
  }

  if (SQL_ERROR == SqlStmt_Prepare(
                       stmt,
                       "SELECT `ChaName` FROM `Character` WHERE `ChaId` = '%u'",
                       id) ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_STRING, &name,
                                      sizeof(name), NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
  }

  if (SQL_SUCCESS == SqlStmt_NextRow(stmt)) {
  }

  SqlStmt_Free(stmt);

  return &name[0];
}

int clif_Hacker(char *name, const char *reason) {
  char StringBuffer[1024];
  printf(CL_MAGENTA "%s " CL_NORMAL "possibly hacking" CL_BOLD "%s" CL_NORMAL
                    "\n",
         name, reason);
  sprintf(StringBuffer, "%s possibly hacking: %s", name, reason);
  clif_broadcasttogm(StringBuffer, -1);
  return 0;
}
int clif_sendurl(USER *sd, int type, const char *url) {
  if (!sd) return 0;

  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOB(sd->fd, 3) = 0x66;
  WFIFOB(sd->fd, 4) = 0x03;
  WFIFOB(sd->fd, 5) = type;  // type. 0 = ingame browser, 1= popup open browser
                             // then close client, 2 = popup
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

  char url1[] = "https://www.website.com/boards";  // this first URL doesnt
                                                   // appear to do anything
  char url2[] = "https://www.website.com/boards";  // This is the actual URL
                                                   // that the browser goes to

  char url3[] = "?abc123";

  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOB(sd->fd, 3) = 0x62;
  WFIFOB(sd->fd, 5) = 0x00;  // type  0 = board

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

// profile URL 0x62
// worldmap location URL 0x70

int CheckProximity(struct point one, struct point two, int radius) {
  int ret = 0;

  if (one.m == two.m)
    if (abs(one.x - two.x) <= radius && abs(one.y - two.y) <= radius) ret = 1;

  return ret;
}

int clif_accept2(int fd, char *name, int name_len) {
  char n[32];

  // struct auth_node* db=NULL;
  // printf("Namelen: %d\n",name_len);

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
  // printf("Name: %s\n",n);

  /*for(i=0;i<AUTH_FIFO_SIZE;i++) {
          if((auth_fifo[i].ip == (unsigned
  int)rust_session_get_client_ip(fd))) {
                  if(!strcasecmp(n,auth_fifo[i].name)) {
                  intif_load(fd, auth_fifo[i].id, auth_fifo[i].name);
                  auth_fifo[i].ip = 0;
                  auth_fifo[i].id = 0;

                  return 0;
          }
  }
  }*/

  int id = 0;

  SqlStmt *stmt;
  stmt = SqlStmt_Malloc(sql_handle);
  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    return -1;
  }

  if (SQL_ERROR == SqlStmt_Prepare(
                       stmt,
                       "SELECT `ChaId` FROM `Character` WHERE `ChaName` = '%s'",
                       n) ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 0, SQLDT_UINT, &id, 0, NULL, NULL)) {
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
  WFIFOB(sd->fd, 5) = 0;                    // dunno
  WFIFOB(sd->fd, 6) = width;                // width of paper
  WFIFOB(sd->fd, 7) = height;               // height of paper
  WFIFOB(sd->fd, 8) = 0;                    // dunno
  WFIFOW(sd->fd, 9) = SWAP16(strlen(buf));  // length of message
  strcpy(WFIFOP(sd->fd, 11), buf);          // message
  WFIFOSET(sd->fd, encrypt(sd->fd));
  return 0;
}

int clif_paperpopupwrite(USER *sd, const char *buf, int width, int height,
                         int invslot) {
  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  WFIFOHEAD(sd->fd, strlen(buf) + 11 + 3);
  WFIFOB(sd->fd, 0) = 0xAA;
  WFIFOW(sd->fd, 1) = SWAP16(strlen(buf) + 11);
  WFIFOB(sd->fd, 3) = 0x1B;
  WFIFOB(sd->fd, 5) = invslot;              // invslot
  WFIFOB(sd->fd, 6) = 0;                    // dunno
  WFIFOB(sd->fd, 7) = width;                // width of paper
  WFIFOB(sd->fd, 8) = height;               // height of paper
  WFIFOW(sd->fd, 9) = SWAP16(strlen(buf));  // length of message
  strcpy(WFIFOP(sd->fd, 11), buf);          // message
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
  // len=strlen(sd->status.name);
  strcpy(WFIFOP(sd->fd, 13), xor_key);
  len = 11;
  WFIFOB(sd->fd, len + 11) = strlen(sd->status.name);
  strcpy(WFIFOP(sd->fd, len + 12), sd->status.name);
  len += strlen(sd->status.name) + 1;
  // WFIFOL(sd->fd,len+11)=SWAP32(sd->status.id);
  len += 4;

  WFIFOB(sd->fd, 10) = len;
  WFIFOW(sd->fd, 1) = SWAP16(len + 8);
  // set_packet_indexes(WFIFOP(sd->fd, 0));
  WFIFOSET(sd->fd, len + 11);  // + 3);

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
  // set_packet_indexes(WFIFOP(sd->fd, 0));
  WFIFOSET(sd->fd, len + 11);  // + 3);

  return 0;
}

int clif_sendBoardQuestionaire(USER *sd, struct board_questionaire *q,
                               int count) {
  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }
  // Player(2):sendBoardQuestions("Defendant :","Name of Person who commited the
  // crime.",2,"When :","When was the crime commited?",1)

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

  /*printf("packet\n");
  for (int i = 0; i<len;i++) {
  printf("%i.      %c         %i
  %02X\n",i,WFIFOB(sd->fd,i),WFIFOB(sd->fd,i),WFIFOB(sd->fd,i));
  }
  printf("\n");*/

  WFIFOSET(sd->fd, encrypt(sd->fd));
  return 0;
}

// clif_closeit — ported to src/game/map_parse/dialogs.rs

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

/*int clif_sendguidelist(USER *sd) {
        int count=0;
        int x;
        int len=0;

        for(x=0;x<256;x++) {
                if(sd->status.guide[x]) {

                if (!rust_session_exists(sd->fd))
                {
                        rust_session_set_eof(sd->fd, 8);
                        return 0;
                }

                WFIFOHEAD(sd->fd,10);
                WFIFOB(sd->fd,0)=0xAA;
                WFIFOW(sd->fd,1)=SWAP16(0x07);
                WFIFOB(sd->fd,3)=0x12;
                WFIFOB(sd->fd,4)=0x03;
                WFIFOB(sd->fd,5)=0x00;
                WFIFOB(sd->fd,6)=0x02;
                WFIFOW(sd->fd,7)=sd->status.guide[x];
                WFIFOB(sd->fd,9)=0;
                WFIFOSET(sd->fd,encrypt(sd->fd));
                }
        }
        return 0;
}*/

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
  WFIFOB(sd->fd, 6) = 0x0A;  // 0x00;
  WFIFOSET(sd->fd, encrypt(sd->fd));

  return 0;
}

int pc_sendpong(int id, int none) {
  // return 0;
  USER *sd = map_id2sd((unsigned int)id);
  nullpo_ret(1, sd);

  // if (DIFF_TICK(gettick(), sd->LastPongStamp) >= 300000) rust_session_get_eof(sd->fd)
  // = 12;

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

    sd->LastPingTick = gettick();  // For measuring their arrival of response
  }

  return 0;
}

// clif_sendguidespecific — ported to src/game/map_parse/chat.rs
// clif_broadcast_sub — ported to src/game/map_parse/chat.rs
// clif_gmbroadcast_sub — ported to src/game/map_parse/chat.rs
// clif_broadcasttogm_sub — ported to src/game/map_parse/chat.rs
// clif_broadcast — ported to src/game/map_parse/chat.rs
// clif_gmbroadcast — ported to src/game/map_parse/chat.rs
// clif_broadcasttogm — ported to src/game/map_parse/chat.rs

int clif_getequiptype(int val) {
  int type = 0;

  switch (val) {
    case EQ_WEAP:
      type = 1;
      break;
    case EQ_ARMOR:
      type = 2;
      break;
    case EQ_SHIELD:
      type = 3;
      break;
    case EQ_HELM:
      type = 4;
      break;
    case EQ_NECKLACE:
      type = 6;
      break;
    case EQ_LEFT:
      type = 7;
      break;
    case EQ_RIGHT:
      type = 8;
      break;
    case EQ_BOOTS:
      type = 13;
      break;
    case EQ_MANTLE:
      type = 14;
      break;
    case EQ_COAT:
      type = 16;
      break;
    case EQ_SUBLEFT:
      type = 20;
      break;
    case EQ_SUBRIGHT:
      type = 21;
      break;
    case EQ_FACEACC:
      type = 22;
      break;
    case EQ_CROWN:
      type = 23;
      break;

    default:
      return 0;
      break;
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

  for (i = 0; i < len; i++) {
    printf("%02X ", stringthing[i]);
  }

  printf("\n");

  for (i = 0; i < len; i++) {
    if (stringthing[i] <= 32 || stringthing[i] > 126) {
      printf("   ");
    } else {
      printf("%02X ", stringthing[i]);
    }
  }

  printf("\n");
  return 0;
}

// clif_sendtowns — ported to src/game/map_parse/dialogs.rs

int clif_user_list(USER *sd) {
  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

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

// clif_pc_damage — ported to src/game/map_parse/combat.rs

// clif_send_pc_health — ported to src/game/map_parse/combat.rs

void clif_delay(int milliseconds) {
  clock_t start_time = clock();

  while (clock() < start_time + milliseconds)
    ;
}

// clif_send_pc_healthscript — ported to src/game/map_parse/combat.rs

// clif_send_selfbar — ported to src/game/map_parse/combat.rs

// clif_send_groupbars — ported to src/game/map_parse/combat.rs

// clif_send_mobbars — ported to src/game/map_parse/combat.rs

// clif_findspell_pos — ported to src/game/map_parse/combat.rs

// clif_calc_critical — ported to src/game/map_parse/combat.rs
// clif_has_aethers — ported to src/game/map_parse/combat.rs

// clif_mob_look_start_func — ported to src/game/map_parse/visual.rs

// clif_mob_look_close_func — ported to src/game/map_parse/visual.rs

// clif_object_look_sub — ported to src/game/map_parse/visual.rs

// clif_object_look_sub2 — ported to src/game/map_parse/visual.rs
// clif_object_look_specific — ported to src/game/map_parse/visual.rs
// clif_mob_look_start — ported to src/game/map_parse/visual.rs
// clif_mob_look_close — ported to src/game/map_parse/visual.rs

// clif_send_duration — ported to src/game/map_parse/combat.rs

// clif_send_aether — ported to src/game/map_parse/combat.rs

int clif_npc_move(struct block_list *bl, va_list ap) {
  unsigned char *buf;
  USER *sd = NULL;
  NPC *nd = NULL;

  va_arg(ap, int);  // type
  nullpo_ret(0, sd = (USER *)bl);
  nullpo_ret(0, nd = va_arg(ap, NPC *));

  CALLOC(buf, unsigned char, 32);
  WBUFB(buf, 0) = 0xAA;
  WBUFB(buf, 1) = 0x00;
  WBUFB(buf, 2) = 0x0C;
  WBUFB(buf, 3) = 0x0C;
  // WBUFB(buf, 4) = 0x03;
  WBUFL(buf, 5) = SWAP32(nd->bl.id);
  WBUFW(buf, 9) = SWAP16(nd->bl.bx);
  WBUFW(buf, 11) = SWAP16(nd->bl.by);
  WBUFB(buf, 13) = nd->side;
  WBUFB(buf, 14) = 0x00;

  clif_send(buf, 32, &nd->bl, AREA_WOS);  // come back
  FREE(buf);

  /*WFIFOHEAD(sd->fd,14);
  WFIFOHEADER(sd->fd, 0x0C, 11);
  WFIFOL(sd->fd,5) = SWAP32(cnd->bl.id);
  WFIFOW(sd->fd,9) = SWAP16(cnd->bl.bx);
  WFIFOW(sd->fd,11) = SWAP16(cnd->bl.by);
  WFIFOB(sd->fd,13) = cnd->side;
  encrypt(sd->fd);
  WFIFOSET(sd->fd,14);*/
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
  // printf("Moved\n");
  return 0;
}
// clif_mob_damage — ported to src/game/map_parse/combat.rs
// clif_send_mob_health_sub — ported to src/game/map_parse/combat.rs
// clif_send_mob_health_sub_nosd — ported to src/game/map_parse/combat.rs
// clif_send_mob_health — ported to src/game/map_parse/combat.rs

// clif_send_mob_healthscript — ported to src/game/map_parse/combat.rs

// clif_mob_kill — ported to src/game/map_parse/combat.rs

// clif_send_destroy — ported to src/game/map_parse/combat.rs

// clif_send_timer — ported to src/game/map_parse/dialogs.rs

// clif_parsenpcdialog — ported to src/game/map_parse/dialogs.rs
int clif_send_sub(struct block_list *bl, va_list ap) {
  unsigned char *buf = NULL;
  int len;
  struct block_list *src_bl = NULL;
  int type;
  USER *sd = NULL;
  USER *tsd = NULL;

  // nullpo_ret(0, bl);
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

int clif_send(const unsigned char *buf, int len, struct block_list *bl,
              int type) {
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
        if (rust_session_exists(i) && (sd = rust_session_get_data(i)) && sd->bl.m == bl->m &&
            sd != (USER *)bl) {
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
      map_foreachinarea(clif_send_sub, bl->m, bl->x, bl->y, AREA, BL_PC, buf,
                        len, bl, type);
      break;
    case SAMEAREA:
    case SAMEAREA_WOS:
      map_foreachinarea(clif_send_sub, bl->m, bl->x, bl->y, SAMEAREA, BL_PC,
                        buf, len, bl, type);
      break;
    case CORNER:
      map_foreachinarea(clif_send_sub, bl->m, bl->x, bl->y, CORNER, BL_PC, buf,
                        len, bl, type);
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

int clif_sendtogm(unsigned char *buf, int len, struct block_list *bl,
                  int type) {
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
        if (rust_session_exists(i) && (sd = rust_session_get_data(i)) && sd->bl.m == bl->m &&
            sd != (USER *)bl) {
          WFIFOHEAD(i, len + 3);
          memcpy(WFIFOP(i, 0), buf, len);
          WFIFOSET(i, encrypt(i));
        }
      }
      break;
    case AREA:
    case AREA_WOS:
      map_foreachinarea(clif_send_sub, bl->m, bl->x, bl->y, AREA, BL_PC, buf,
                        len, bl, type);
      break;
    case SAMEAREA:
    case SAMEAREA_WOS:
      map_foreachinarea(clif_send_sub, bl->m, bl->x, bl->y, SAMEAREA, BL_PC,
                        buf, len, bl, type);
      break;
    case CORNER:
      map_foreachinarea(clif_send_sub, bl->m, bl->x, bl->y, CORNER, BL_PC, buf,
                        len, bl, type);
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

// clif_mystaytus — ported to src/game/map_parse/player_state.rs

// clif_lookgone — ported to src/game/map_parse/visual.rs

// clif_cnpclook_sub — ported to src/game/map_parse/visual.rs

// clif_cmoblook_sub — ported to src/game/map_parse/visual.rs

int clif_show_ghost(USER *sd, USER *tsd) {
  /*if(map[sd->bl.m].show_ghosts && tsd->status.state==1 &&
  (sd->bl.id!=tsd->bl.id)) { if(sd->status.state!=1 && !(sd->optFlags &
  optFlag_ghosts)) { return 0;
          }
  }*/

  // IF the map has SHOW GHOSTS set, then this overrides all  (to be used on
  // path arena/clan & pc subpath areas) Default setting for ALL maps is to have
  // Show Ghosts set to 0 (off).

  if (!sd->status.gm_level) {  // This set of rules ONLY applies to non GMs
    if (!map[sd->bl.m].show_ghosts && tsd->status.state == 1 &&
        sd->bl.id != tsd->bl.id) {
      if (map[sd->bl.m].pvp) {
        if (sd->status.state == 1 && sd->optFlags & optFlag_ghosts)
          return 1;
        else
          return 0;
      } else
        return 1;
    }
  }

  return 1;
}

// clif_charlook_sub — ported to src/game/map_parse/visual.rs

// clif_blockmovement — ported to src/game/map_parse/movement.rs
// clif_getchararea — ported to src/game/map_parse/player_state.rs

int clif_getitemarea(USER *sd) {
  // map_foreachinarea(clif_object_look_sub,sd->bl.m,sd->bl.x,sd->bl.y,SAMEAREA,BL_ITEM,LOOK_GET,sd);

  return 0;
}

// clif_sendchararea — ported to src/game/map_parse/movement.rs
// clif_charspecific — ported to src/game/map_parse/movement.rs

// clif_sendack — ported to src/game/map_parse/player_state.rs

// clif_retrieveprofile — ported to src/game/map_parse/player_state.rs

// clif_screensaver — ported to src/game/map_parse/player_state.rs

// clif_sendtime — ported to src/game/map_parse/player_state.rs

// clif_sendid — ported to src/game/map_parse/player_state.rs
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

// clif_sendmapinfo — ported to src/game/map_parse/player_state.rs

// clif_sendxy — ported to src/game/map_parse/player_state.rs

// clif_sendxynoclick — ported to src/game/map_parse/player_state.rs

// clif_sendxychange — ported to src/game/map_parse/player_state.rs

// clif_sendstatus — ported to src/game/map_parse/player_state.rs

// clif_sendoptions — ported to src/game/map_parse/player_state.rs

// clif_spawn — ported to src/game/map_parse/visual.rs

// clif_parsewalk — ported to src/game/map_parse/movement.rs

// clif_noparsewalk — ported to src/game/map_parse/movement.rs

// clif_guitextsd — ported to src/game/map_parse/chat.rs
// clif_guitext — ported to src/game/map_parse/chat.rs

// sendRewardParcel — ported to src/game/map_parse/events.rs

// clif_getReward — ported to src/game/map_parse/events.rs

// clif_sendRewardInfo — ported to src/game/map_parse/events.rs


// clif_intcheck — ported to src/game/map_parse/events.rs

/// RANKING SYSTEM HANDLING - ADDED IN V749 CLIENT ////

// retrieveEventDates — ported to src/game/map_parse/events.rs


// checkPlayerScore — ported to src/game/map_parse/events.rs


// updateRanks — ported to src/game/map_parse/events.rs


// checkPlayerRank — ported to src/game/map_parse/events.rs


int checkevent_claim(int eventid, int fd, USER *sd) {
  int claim = 0;

  SqlStmt *stmt;
  stmt = SqlStmt_Malloc(sql_handle);
  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    return 0;
  }
  if (SQL_ERROR == SqlStmt_Prepare(stmt,
                                   "SELECT `EventClaim` FROM `RankingScores` "
                                   "WHERE `EventId` = '%u' AND `ChaId` = '%u'",
                                   eventid, sd->status.id) ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 0, SQLDT_UINT, &claim, 0, NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return claim;
  }

  if (SQL_SUCCESS !=
      SqlStmt_NextRow(stmt)) {  // If no record found, set claim=2 (no icon,
                                // disabled getreward)
    // SqlStmt_ShowDebug(stmt);
    claim = 2;
  }

  SqlStmt_Free(stmt);
  return claim;
}

void dateevent_block(int pos, int eventid, int fd, USER *sd) {
  WFIFOB(fd, pos) = 0;  // Always 0
  WFIFOB(fd, pos + 1) = eventid;
  WFIFOB(fd, pos + 2) = 142;  // 142
  WFIFOB(fd, pos + 3) = 227;  // 227
  retrieveEventDates(eventid, pos, fd);
  WFIFOB(fd, pos + 20) = checkevent_claim(
      eventid, fd, sd);  // Envelope.  0 = new, 1 = read/unclaimed, 2 = no
                         // reward -- enables/disables the GetReward button
}

void filler_block(int pos, int eventid, int fd, USER *sd) {
  int player_score = checkPlayerScore(eventid, sd);
  int player_rank = checkPlayerRank(eventid, sd);

  WFIFOB(fd, pos + 1) =
      RFIFOB(fd, 7);  // This controls which event is displayed. It is the event
                      // request id packet
  WFIFOB(fd, pos + 2) = 142;
  WFIFOB(fd, pos + 3) = 227;
  WFIFOB(fd, pos + 4) =
      1;  // show self score - Leave to always enabled. If player does not have
          // a score or is equal to 0, the client automatically blanks it out
  clif_intcheck(player_rank, pos + 8, fd);    // Self rank
  clif_intcheck(player_score, pos + 12, fd);  // Self score
  WFIFOB(fd, pos + 13) = checkevent_claim(eventid, fd, sd);
}

int gettotalscores(int eventid) {
  int scores;

  SqlStmt *stmt;
  stmt = SqlStmt_Malloc(sql_handle);
  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    return 0;
  }
  if (SQL_ERROR ==
          SqlStmt_Prepare(
              stmt,
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
  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    return 0;
  }
  if (SQL_ERROR ==
          SqlStmt_Prepare(stmt, "SELECT `EventId` FROM `RankingEvents`") ||
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
  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    return 0;
  }

  if (SQL_ERROR ==
          SqlStmt_Prepare(stmt, "SELECT `EventName` FROM `RankingEvents`") ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_STRING, &name,
                                      sizeof(name), NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return 0;
  }

  for (i = 0;
       (i < SqlStmt_NumRows(stmt)) && (SQL_SUCCESS == SqlStmt_NextRow(stmt));
       i++) {
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
  int offset =
      RFIFOB(fd, 17) -
      10;  // The purpose of this -10 is because the packet request is value 10
           // for page 1. Because of mysql integration, we want to offset so we
           // start on row 0 for player scores loading
  int i = 0;

  SqlStmt *stmt;
  stmt = SqlStmt_Malloc(sql_handle);
  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    return 0;
  }

  if (totalscores > 10) {
    SqlStmt_Prepare(
        stmt,
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
      SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_STRING, &name,
                                      sizeof(name), NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 1, SQLDT_INT, &score, 0, NULL, NULL) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 2, SQLDT_INT, &rank, 0, NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return 0;
  }

  if (SqlStmt_NumRows(stmt) < 10) {
    WFIFOB(fd, pos - 1) = SqlStmt_NumRows(stmt);
  }  // added 04-26-2017 removes trailing zeros that were present in the ranking
     // feature

  for (i = 0;
       (i < SqlStmt_NumRows(stmt)) && (SQL_SUCCESS == SqlStmt_NextRow(stmt));
       i++) {
    sprintf(buf, "%s", name);
    WFIFOB(fd, pos) = strlen(buf);
    pos++;
    strncpy(WFIFOP(fd, pos), buf, strlen(buf));
    pos += strlen(buf);
    pos += 3;

    WFIFOB(fd, pos) = rank;  // # of rank
    pos += 4;
    clif_intcheck(score, pos, fd);
    pos++;
  }

  return pos;
}

int clif_parseranking(USER *sd, int fd) {
  WFIFOHEAD(fd, 0);
  WFIFOB(fd, 0) = 0xAA;  // Packet Delimiter
  WFIFOB(fd, 1) = 0x02;  //  Paacket header
  WFIFOB(fd, 3) = 0x7D;  // Packet ID
  WFIFOB(fd, 5) = 0x03;  // Directs packets into the main window.
  WFIFOB(fd, 6) = 0;     // Always set to 0

  // Original Packet = { 0xAA, 0x02, 0x95, 0x7D, 0x26, 0x03, 0x00, 0x0D, 0x01,
  // 0x03, 0x8E, 0xE3, 0x01, 0x33, 0xC5, 0x78, 0x00, 0x00, 0x00, 0x00, 0x01,
  // 0x33, 0xC5, 0x93, 0x00, 0x03, 0x99, 0xB7, 0x02, 0x09, 0x53, 0x6E, 0x6F,
  // 0x77, 0x20, 0x46, 0x75, 0x72, 0x79, 0x01, 0x03, 0x8E, 0xE2, 0x01, 0x33,
  // 0xC5, 0x78, 0x00, 0x00, 0x00, 0x00, 0x01, 0x33, 0xC5, 0x93, 0x00, 0x03,
  // 0x99, 0xB7, 0x02, 0x0B, 0x53, 0x6E, 0x6F, 0x77, 0x20, 0x46, 0x72, 0x65,
  // 0x6E, 0x7A, 0x79, 0x01, 0x03, 0x8E, 0xE1, 0x01, 0x33, 0xC5, 0x78, 0x00,
  // 0x00, 0x00, 0x00, 0x01, 0x33, 0xC5, 0x93, 0x00, 0x03, 0x99, 0xB7, 0x02,
  // 0x0B, 0x53, 0x6E, 0x6F, 0x77, 0x20, 0x46, 0x6C, 0x75, 0x72, 0x72, 0x79,
  // 0x00, 0xF6, 0x01, 0xBF, 0x01, 0x33, 0xA2, 0xC7, 0x00, 0x00, 0x00, 0x00,
  // 0x01, 0x33, 0xC5, 0x77, 0x00, 0x03, 0x99, 0xB7, 0x02, 0x09, 0x53, 0x6E,
  // 0x6F, 0x77, 0x20, 0x46, 0x75, 0x72, 0x79, 0x00, 0xF6, 0x01, 0xBE, 0x01,
  // 0x33, 0xA2, 0xC7, 0x00, 0x00, 0x00, 0x00, 0x01, 0x33, 0xC5, 0x77, 0x00,
  // 0x03, 0x99, 0xB7, 0x02, 0x0B, 0x53, 0x6E, 0x6F, 0x77, 0x20, 0x46, 0x72,
  // 0x65, 0x6E, 0x7A, 0x79, 0x00, 0xF6, 0x01, 0xBD, 0x01, 0x33, 0xA2, 0xC7,
  // 0x00, 0x00, 0x00, 0x00, 0x01, 0x33, 0xC5, 0x77, 0x00, 0x03, 0x99, 0xB7,
  // 0x02, 0x0B, 0x53, 0x6E, 0x6F, 0x77, 0x20, 0x46, 0x6C, 0x75, 0x72, 0x72,
  // 0x79, 0x00, 0xF5, 0xDC, 0x3E, 0x01, 0x33, 0xA2, 0x63, 0x00, 0x00, 0x00,
  // 0x00, 0x01, 0x33, 0xA2, 0x67, 0x00, 0x03, 0x99, 0xB7, 0x02, 0x14, 0x47,
  // 0x72, 0x61, 0x6E, 0x64, 0x20, 0x54, 0x47, 0x20, 0x43, 0x6F, 0x6D, 0x70,
  // 0x65, 0x74, 0x69, 0x74, 0x69, 0x6F, 0x6E, 0x00, 0xF5, 0xDC, 0x3D, 0x01,
  // 0x33, 0xA2, 0x67, 0x00, 0x00, 0x00, 0x00, 0x01, 0x33, 0xA2, 0x67, 0x00,
  // 0x03, 0x99, 0xB7, 0x02, 0x16, 0x54, 0x47, 0x20, 0x64, 0x61, 0x69, 0x6C,
  // 0x79, 0x20, 0x43, 0x6F, 0x6D, 0x70, 0x65, 0x74, 0x69, 0x74, 0x69, 0x6F,
  // 0x6E, 0x20, 0x35, 0x00, 0xF5, 0xDB, 0xD9, 0x01, 0x33, 0xA2, 0x66, 0x00,
  // 0x00, 0x00, 0x00, 0x01, 0x33, 0xA2, 0x66, 0x00, 0x03, 0x99, 0xB7, 0x02,
  // 0x16, 0x54, 0x47, 0x20, 0x64, 0x61, 0x69, 0x6C, 0x79, 0x20, 0x43, 0x6F,
  // 0x6D, 0x70, 0x65, 0x74, 0x69, 0x74, 0x69, 0x6F, 0x6E, 0x20, 0x34, 0x00,
  // 0xF5, 0xDB, 0x75, 0x01, 0x33, 0xA2, 0x65, 0x00, 0x00, 0x00, 0x00, 0x01,
  // 0x33, 0xA2, 0x65, 0x00, 0x03, 0x99, 0xB7, 0x02, 0x16, 0x54, 0x47, 0x20,
  // 0x64, 0x61, 0x69, 0x6C, 0x79, 0x20, 0x43, 0x6F, 0x6D, 0x70, 0x65, 0x74,
  // 0x69, 0x74, 0x69, 0x6F, 0x6E, 0x20, 0x33, 0x00, 0xF5, 0xDB, 0x11, 0x01,
  // 0x33, 0xA2, 0x64, 0x00, 0x00, 0x00, 0x00, 0x01, 0x33, 0xA2, 0x64, 0x00,
  // 0x03, 0x99, 0xB7, 0x02, 0x16, 0x54, 0x47, 0x20, 0x64, 0x61, 0x69, 0x6C,
  // 0x79, 0x20, 0x43, 0x6F, 0x6D, 0x70, 0x65, 0x74, 0x69, 0x74, 0x69, 0x6F,
  // 0x6E, 0x20, 0x32, 0x00, 0xF5, 0xDA, 0xAD, 0x01, 0x33, 0xA2, 0x63, 0x00,
  // 0x00, 0x00, 0x00, 0x01, 0x33, 0xA2, 0x63, 0x00, 0x03, 0x99, 0xB7, 0x02,
  // 0x16, 0x54, 0x47, 0x20, 0x64, 0x61, 0x69, 0x6C, 0x79, 0x20, 0x43, 0x6F,
  // 0x6D, 0x70, 0x65, 0x74, 0x69, 0x74, 0x69, 0x6F, 0x6E, 0x20, 0x31, 0x00,
  // 0xF5, 0xD7, 0xF1, 0x01, 0x33, 0xA2, 0x5C, 0x00, 0x00, 0x00, 0x00, 0x01,
  // 0x33, 0xA2, 0x60, 0x00, 0x03, 0x99, 0xB7, 0x02, 0x08, 0x53, 0x63, 0x61,
  // 0x76, 0x65, 0x6E, 0x67, 0x65, 0x01, 0x03, 0x8E, 0xE3, 0x00, 0x00, 0x0A,
  // 0x06, 0x48, 0x6F, 0x63, 0x61, 0x72, 0x69, 0x00, 0x00, 0x00, 0x01, 0x00,
  // 0x00, 0x01, 0x50, 0x07, 0x41, 0x6D, 0x62, 0x65, 0x72, 0x6C, 0x79, 0x00,
  // 0x00, 0x00, 0x02, 0x00, 0x00, 0x01, 0x3E, 0x09, 0x46, 0x72, 0x65, 0x6E,
  // 0x63, 0x68, 0x46, 0x72, 0x79, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00,
  // 0xFC, 0x06, 0x73, 0x69, 0x75, 0x6C, 0x65, 0x74, 0x00, 0x00, 0x00, 0x04,
  // 0x00, 0x00, 0x00, 0xEE, 0x04, 0x4D, 0x75, 0x72, 0x63, 0x00, 0x00, 0x00,
  // 0x05, 0x00, 0x00, 0x00, 0x38, 0x0A, 0x4C, 0x69, 0x6E, 0x75, 0x78, 0x6B,
  // 0x69, 0x64, 0x64, 0x79, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x38,
  // 0x05, 0x41, 0x75, 0x64, 0x69, 0x6F, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00,
  // 0x00, 0x2A, 0x07, 0x4E, 0x65, 0x6C, 0x6C, 0x69, 0x65, 0x6C, 0x00, 0x00,
  // 0x00, 0x08, 0x00, 0x00, 0x00, 0x1C, 0x08, 0x50, 0x6F, 0x68, 0x77, 0x61,
  // 0x72, 0x61, 0x6E, 0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x1C, 0x04,
  // 0x53, 0x75, 0x72, 0x69, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x00, 0x00, 0x1C,
  // 0x00, 0x00, 0x00, 0x10 }; int psize =
  // sizeof(cappacket)/sizeof(cappacket[0]); printf("cap packet size:
  // %i\n",psize); for (i=0;i<660;i++) { WFIFOB(fd, i) = cappacket[i]; }

  int i = 0;
  for (i = 8; i < 1500; i++) {
    WFIFOB(fd, i) = 0x00;
  }  // Write Zero's to all fields that we won't be using.
  WFIFOB(fd, 7) = getevents();  // number of events  * affirmed
  int chosen_event = RFIFOB(fd, 7);

  updateRanks(chosen_event);

  int pos = 8;

  pos = getevent_name(pos, fd, sd);
  filler_block(pos, chosen_event, fd, sd);
  pos += 15;             // was a 6
  WFIFOB(fd, pos) = 10;  // # of scores to display on page. max 10
  int totalscores = gettotalscores(chosen_event);
  pos++;
  pos = getevent_playerscores(chosen_event, totalscores, pos, fd);
  pos += 3;
  WFIFOB(fd, pos) =
      totalscores;  // This number displays in the top right of the popup and
                    // indicates how many users played in specific event
  pos += 1;

  WFIFOB(fd, 2) = pos - 3;  // packetsize packet. The -3 is because the
                            // encryption algorithm ends 3 bytes onto the end
  WFIFOSET(fd, encrypt(fd));

  return 0;
}

// clif_parsewalkpong — ported to src/game/map_parse/movement.rs

// clif_parsemap — ported to src/game/map_parse/movement.rs

// clif_sendmapdata — ported to src/game/map_parse/movement.rs

// clif_sendside — ported to src/game/map_parse/movement.rs
int clif_sendmob_side(MOB *mob) {
  unsigned char buf[16];
  WBUFB(buf, 0) = 0xAA;
  WBUFB(buf, 1) = 0x00;
  WBUFB(buf, 2) = 0x07;
  WBUFB(buf, 3) = 0x11;
  WBUFB(buf, 4) = 0x03;
  WBUFL(buf, 5) = SWAP32(mob->bl.id);
  WBUFB(buf, 9) = mob->side;
  // crypt(WBUFP(buf, 0));
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
// clif_parseside — ported to src/game/map_parse/movement.rs

// clif_parseemotion — ported to src/game/map_parse/chat.rs
// clif_sendmsg — ported to src/game/map_parse/chat.rs
// clif_sendminitext — ported to src/game/map_parse/chat.rs
// clif_sendwisp — ported to src/game/map_parse/chat.rs
// clif_retrwisp — ported to src/game/map_parse/chat.rs
// clif_failwisp — ported to src/game/map_parse/chat.rs

int clif_parsedropitem(USER *sd) {
  char RegStr[] = "goldbardupe";
  int DupeTimes = pc_readglobalreg(sd, RegStr);
  if (DupeTimes) {
    // char minibuf[]="Character under quarentine.";
    // clif_sendminitext(sd,minibuf);
    return 0;
  }

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

  sd->invslot = id;  // this sets player.invSlot so that we can access it in the
                     // on_drop_while_cast func

  sl_doscript_blargs(
      itemdb_yname(sd->status.inventory[id].id), "on_drop", 1,
      &sd->bl);  // running this before pc_dropitemmap allows us to simulate a
                 // drop (fake) as seen on ntk. prevents abuse.

  for (int x = 0; x < MAX_MAGIC_TIMERS; x++) {  // Spell stuff
    if (sd->status.dura_aether[x].id > 0 &&
        sd->status.dura_aether[x].duration > 0) {
      sl_doscript_blargs(magicdb_yname(sd->status.dura_aether[x].id),
                         "on_drop_while_cast", 1, &sd->bl);
    }
  }

  for (int x = 0; x < MAX_MAGIC_TIMERS; x++) {  // Spell stuff
    if (sd->status.dura_aether[x].id > 0 &&
        sd->status.dura_aether[x].aether > 0) {
      sl_doscript_blargs(magicdb_yname(sd->status.dura_aether[x].id),
                         "on_drop_while_aether", 1, &sd->bl);
    }
  }

  if (sd->fakeDrop) return 0;

  pc_dropitemmap(sd, id, all);

  return 0;
}
// clif_deductdura — ported to src/game/map_parse/combat.rs
// clif_deductweapon — ported to src/game/map_parse/combat.rs

// clif_deductarmor — ported to src/game/map_parse/combat.rs

// clif_checkdura — ported to src/game/map_parse/combat.rs

// clif_deductduraequip — ported to src/game/map_parse/combat.rs

// clif_checkinvbod — ported to src/game/map_parse/items.rs

// clif_senddelitem — ported to src/game/map_parse/items.rs

// clif_sendadditem — ported to src/game/map_parse/items.rs

// clif_equipit — ported to src/game/map_parse/items.rs
// clif_sendequip — ported to src/game/map_parse/items.rs

int clif_mapmsgnum(USER *sd, int id) {
  int msgnum = 0;
  switch (id) {
    case EQ_HELM:
      msgnum = MAP_EQHELM;
      break;
    case EQ_WEAP:
      msgnum = MAP_EQWEAP;
      break;
    case EQ_ARMOR:
      msgnum = MAP_EQARMOR;
      break;
    case EQ_SHIELD:
      msgnum = MAP_EQSHIELD;
      break;
    case EQ_RIGHT:
      msgnum = MAP_EQRIGHT;
      break;
    case EQ_LEFT:
      msgnum = MAP_EQLEFT;
      break;
    case EQ_SUBLEFT:
      msgnum = MAP_EQSUBLEFT;
      break;
    case EQ_SUBRIGHT:
      msgnum = MAP_EQSUBRIGHT;
      break;
    case EQ_FACEACC:
      msgnum = MAP_EQFACEACC;
      break;
    case EQ_CROWN:
      msgnum = MAP_EQCROWN;
      break;
    case EQ_BOOTS:
      msgnum = MAP_EQBOOTS;
      break;
    case EQ_MANTLE:
      msgnum = MAP_EQMANTLE;
      break;
    case EQ_COAT:
      msgnum = MAP_EQCOAT;
      break;
    case EQ_NECKLACE:
      msgnum = MAP_EQNECKLACE;
      break;

    default:
      return -1;
      break;
  }

  return msgnum;
}
// clif_sendgroupmessage — ported to src/game/map_parse/chat.rs
// clif_sendsubpathmessage — ported to src/game/map_parse/chat.rs
// clif_sendclanmessage — ported to src/game/map_parse/chat.rs
// clif_sendnovicemessage — ported to src/game/map_parse/chat.rs
// ignorelist_add — ported to src/game/map_parse/chat.rs
// ignorelist_remove — ported to src/game/map_parse/chat.rs

// clif_isignore — ported to src/game/map_parse/chat.rs
// canwhisper — ported to src/game/map_parse/chat.rs
// clif_parsewisp — ported to src/game/map_parse/chat.rs

// clif_sendsay — ported to src/game/map_parse/chat.rs

// clif_sendscriptsay — ported to src/game/map_parse/chat.rs

// clif_distance — private helper, kept as Rust fn in chat.rs
// clif_sendnpcsay — ported to src/game/map_parse/chat.rs
// clif_sendmobsay — ported to src/game/map_parse/chat.rs
// clif_sendnpcyell — ported to src/game/map_parse/chat.rs
// clif_sendmobyell — ported to src/game/map_parse/chat.rs
// clif_speak — ported to src/game/map_parse/chat.rs
// clif_parseignore — ported to src/game/map_parse/chat.rs
// clif_parsesay — ported to src/game/map_parse/chat.rs
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
  // WFIFOB(sd->fd,6)=0x00;
  WFIFOSET(sd->fd, encrypt(sd->fd));
  return 0;
}

// clif_refresh — ported to src/game/map_parse/player_state.rs

int clif_refreshnoclick(USER *sd) {
  clif_sendmapinfo(sd);
  clif_sendxynoclick(sd);
  clif_mob_look_start(sd);
  map_foreachinarea(clif_object_look_sub, sd->bl.m, sd->bl.x, sd->bl.y,
                    SAMEAREA, BL_ALL, LOOK_GET, sd);
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

    if (sd->status.settingFlags & FLAG_GROUP) {  // not enabled
      // sprintf(buff,"Join a group     :ON");
    } else {
      if (sd->group_count > 0) {
        clif_leavegroup(sd);
      }

      sprintf(buff, "Join a group     :OFF");
      clif_sendstatus(sd, 0);
      clif_sendminitext(sd, buff);
    }
  }

  // sd->refresh_check=1;
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
  // WFIFOB(sd->fd, 4) = 0x03;
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
  WFIFOB(sd->fd, 18) = 0x00;  // hear others
  WFIFOB(sd->fd, 19) = 0x00;
  WFIFOB(sd->fd, 20) = sd->flags;
  WFIFOB(sd->fd, 21) = 0x01;
  WFIFOL(sd->fd, 22) = SWAP32(sd->status.settingFlags);
  WFIFOSET(sd->fd, encrypt(sd->fd));
  return 0;
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
  WFIFOB(sd->fd, 5) = 0x19;  // packet subtype 24 = take damage, 25 = onKill, 88
                             // = unEquip, 89 = Equip

  WFIFOL(sd->fd, 6) = SWAP32(sd->status.exp);
  WFIFOL(sd->fd, 10) = SWAP32(sd->status.money);

  WFIFOB(sd->fd, 14) = (int)percentage;  // exp percent
  WFIFOB(sd->fd, 15) = sd->drunk;
  WFIFOB(sd->fd, 16) = sd->blind;
  WFIFOB(sd->fd, 17) = 0;
  WFIFOB(sd->fd, 18) = 0;  // hear others
  WFIFOB(sd->fd, 19) = 0;  // seeminly nothing
  WFIFOB(sd->fd, 20) =
      sd->flags;  // 1=New parcel, 16=new Message, 17=New Parcel + Message
  WFIFOB(sd->fd, 21) = 0;  // seemingly nothing
  WFIFOL(sd->fd, 22) = SWAP32(sd->status.settingFlags);
  WFIFOL(sd->fd, 26) = SWAP32(tnl);
  WFIFOB(sd->fd, 30) = sd->armor;
  WFIFOB(sd->fd, 31) = sd->dam;
  WFIFOB(sd->fd, 32) = sd->hit;
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
      sd->underLevelFlag = 0;  // means we leveled, unset flag

    if (sd->underLevelFlag)
      percentage = ((float)sd->status.exp / classdb_level(path, level)) * 100;
  } else {
    percentage = ((float)sd->status.exp / 4294967295) * 100;
  }

  return percentage;
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
  WFIFOB(sd->fd, 5) = 89;  // packet subtype 24 = take damage, 25 = onKill,  88
                           // = unEquip, 89 = Equip

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

  WFIFOB(sd->fd, 44) = sd->drunk;  // drunk
  WFIFOB(sd->fd, 45) = sd->blind;  // blind
  WFIFOB(sd->fd, 46) = 0x00;
  WFIFOB(sd->fd, 47) = 0x00;  // hear others
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
  WFIFOB(sd->fd, 5) = 88;  // packet subtype 24 = take damage, 25 = onKill,  88
                           // = unEquip, 89 = Equip

  WFIFOB(sd->fd, 6) = 0x00;
  WFIFOB(sd->fd, 7) = 20;    // dam?
  WFIFOB(sd->fd, 8) = 0x00;  // hit?
  WFIFOB(sd->fd, 9) = 0x00;
  WFIFOB(sd->fd, 10) = 0x00;  // might?
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
  WFIFOB(sd->fd, 47) = 0x00;  // hear others
  WFIFOB(sd->fd, 48) = 0x00;
  WFIFOB(sd->fd, 49) = sd->flags;
  WFIFOL(sd->fd, 50) = tnl;

  WFIFOSET(sd->fd, encrypt(sd->fd));

  return 0;
}

// clif_sendbluemessage — ported to src/game/map_parse/chat.rs
// clif_playsound — ported to src/game/map_parse/chat.rs
// clif_sendaction — ported to src/game/map_parse/combat.rs
// clif_sendmob_action — ported to src/game/map_parse/combat.rs
// clif_sendanimation_xy — ported to src/game/map_parse/combat.rs

// clif_sendanimation — ported to src/game/map_parse/combat.rs

// clif_animation — ported to src/game/map_parse/combat.rs

// clif_sendanimations — ported to src/game/map_parse/combat.rs

// clif_sendmagic — ported to src/game/map_parse/combat.rs

// clif_parsemagic — ported to src/game/map_parse/combat.rs

// clif_scriptmes — ported to src/game/map_parse/dialogs.rs
// clif_scriptmenu — ported to src/game/map_parse/dialogs.rs

// clif_scriptmenuseq — ported to src/game/map_parse/dialogs.rs

// clif_inputseq — ported to src/game/map_parse/dialogs.rs

// clif_parseuseitem — ported to src/game/map_parse/items.rs
// clif_parseeatitem — ported to src/game/map_parse/items.rs
// clif_parsegetitem — ported to src/game/map_parse/items.rs

// clif_unequipit — ported to src/game/map_parse/items.rs
// clif_parseunequip — ported to src/game/map_parse/items.rs

int clif_parselookat_sub(struct block_list *bl, va_list ap) {
  USER *sd = NULL;
  nullpo_ret(0, bl);
  nullpo_ret(0, sd = va_arg(ap, USER *));

  sl_doscript_blargs("onLook", NULL, 2, &sd->bl, bl);
  return 0;
}

int clif_parselookat_scriptsub(USER *sd, struct block_list *bl) {
  /*MOB* mob = NULL;
  FLOORITEM* fl = NULL;
  USER *tsd = NULL;
  struct npc_data* nd = NULL;
  char buff[255];
  char *nameof = NULL;
  int d,c,b,a;
  nullpo_ret(0, bl);
  nullpo_ret(0, sd);
  float percentage=0.00;

  //unsigned int percentage=0;
  if(bl->type==BL_MOB) {
          mob=(MOB*)bl;
          if(mob->state==MOB_DEAD) return 0;
          percentage=((float)mob->current_vita/(float)mob->maxvita)*100;
          //percentage=mob->current_vita*100/mob->data->vita;
          if(sd->status.gm_level >= 50) {
                  //sprintf(buff,"%s (%d%%) \a %u \a %u \a %u \a
  %s",mob->data->name,(int)percentage,mob->id, mob->data->id, mob->bl.id,
  mob->data->yname); sprintf(buff,"%s (%s) \a ID: %u \a Lvl: %u \a Vita: %u \a
  AC: %i",mob->data->name,mob->data->yname, mob->data->id, mob->data->level,
  mob->current_vita, mob->ac); } else { sprintf(buff,"%s",mob->data->name);
          }
  } else if(bl->type==BL_PC) {
          tsd=(USER*)bl;
          a=b=c=d=rust_session_get_client_ip(tsd->fd);
          a &=0xff;
          b=(b>>8)&0xff;
          c=(c>>16)&0xff;
          d=(d>>24)&0xff;
          percentage = ((float)tsd->status.hp / (float)tsd->max_hp) * 100;

          if((tsd->optFlags & optFlag_stealth))return 0;

          //if (classdb_name(tsd->status.class, tsd->status.mark)) {
          //	sprintf(buff, "%s", classdb_name(tsd->status.class,
  tsd->status.mark));
          //}else {
                  sprintf(buff, " ");
          //}

          if(sd->status.gm_level >= 50) {
                  sprintf(buff,"%s %s \a (%d%%) \a (IP: %u.%u.%u.%u) \a %u",
  buff,tsd->status.name,(int)percentage,a,b,c,d,tsd->status.id); } else {
                  sprintf(buff,"%s %s", buff,tsd->status.name);
          }

  } else if(bl->type==BL_ITEM) {
          fl=(FLOORITEM*)bl;
          if(fl) {
                  if(strlen(fl->data.real_name)) {
                          nameof=fl->data.real_name;
                  } else {
                          nameof=itemdb_name(fl->data.id);
                  }
                  if(fl->data.amount>1) {
                          if(sd->status.gm_level >= 50) {
                                  sprintf(buff,"%s (%u) \a %u \a
  %s",nameof,fl->data.amount,fl->data.id,itemdb_yname(fl->data.id)); } else
  if(itemdb_type(fl->data.id) != ITM_TRAPS) { sprintf(buff,"%s
  (%u)",nameof,fl->data.amount); } else { return 0;
                          }
                  } else {
                          if(sd->status.gm_level >= 50) {
                                  sprintf(buff,"%s \a %u \a
  %s",nameof,fl->data.id,itemdb_yname(fl->data.id)); } else
  if(itemdb_type(fl->data.id) != ITM_TRAPS) { sprintf(buff,"%s",nameof); } else
  { return 0;
                          }
                  }
          }
  } else if(bl->type==BL_NPC) {
          nd=(NPC*)bl;

          if(nd->bl.subtype==0) {
                  //if(nd->bl.graphic_id>0) {
                  if(sd->status.gm_level >= 50) {
                          sprintf(buff,"%s \a %u \a
  %s",nd->npc_name,nd->id,nd->name); } else { sprintf(buff,"%s",nd->npc_name);
                  }
                  //}
          } else {
                  return 0;
          }
  }
  if(strlen(buff)>1) {
          clif_sendminitext(sd,buff);
  }*/
  return 0;
}
int clif_parselookat_2(USER *sd) {
  int dx;
  int dy;

  dx = sd->bl.x;
  dy = sd->bl.y;

  switch (sd->status.side) {
    case 0:
      dy--;
      break;
    case 1:
      dx++;
      break;
    case 2:
      dy++;
      break;
    case 3:
      dx--;
      break;
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

// clif_parseattack — ported to src/game/map_parse/combat.rs

int clif_parsechangepos(USER *sd) {
  if (!RFIFOB(sd->fd, 5)) {
    pc_changeitem(sd, RFIFOB(sd->fd, 6) - 1, RFIFOB(sd->fd, 7) - 1);
  } else {
    clif_sendminitext(sd, "You are busy.");
  }
  return 0;
}

/*int clif_showguide(USER *sd) {
        int g_count=0;
        int x;
        int len=0;

        if (!rust_session_exists(sd->fd))
        {
                rust_session_set_eof(sd->fd, 8);
                return 0;
        }

        WFIFOHEAD(sd->fd,255);
        WFIFOB(sd->fd,0)=0xAA;
        //WFIFOW(sd->fd,1)=SWAP16(7);
        WFIFOB(sd->fd,3)=0x12;
        WFIFOB(sd->fd,4)=0x03;
        WFIFOB(sd->fd,5)=0;
        WFIFOB(sd->fd,6)=0;
        for(x=0;x<256;x++) {
        //	if(x<15) {
        //	printf("Guide at %d is %d\n",x,sd->status.guide[x]);
        //	}
                if(sd->status.guide[x]>0) {
                        //printf("%d\n",len);
                        WFIFOB(sd->fd,8+(g_count*2))=sd->status.guide[x];
                        WFIFOB(sd->fd,9+(g_count*2))=0;
                        g_count++;
                }
        }
        len=g_count*2;
        //len=2;
        WFIFOB(sd->fd,7)=g_count;
        //WFIFOB(sd->fd,8)=1;
        //WFIFOB(sd->fd,9)=0;
        WFIFOW(sd->fd,1)=SWAP16(len+5);
        //WFIFOW(sd->fd,8)=SWAP16(1);
        WFIFOSET(sd->fd,encrypt(sd->fd));

        return 0;
}*/

/*int clif_showguide2(USER *sd) {
        WFIFOB(sd->fd,0)=0xAA;
        WFIFOW(sd->fd,1)=SWAP16(24);
        WFIFOB(sd->fd,3)=0x12;
        WFIFOB(sd->fd,4)=0x03;
        WFIFOW(sd->fd,5)=SWAP16(1);
        WFIFOW(sd->fd,7)=SWAP16(16);
        WFIFOB(sd->fd,9)=1;
        WFIFOL(sd->fd,10)=0;
        WFIFOL(sd->fd,14)=0;
        WFIFOL(sd->fd,18)=0;
        WFIFOL(sd->fd,22)=0;
        WFIFOB(sd->fd,26)=0;

        encrypt(WFIFOP(sd->fd,0));
        WFIFOSET(sd->fd,27);

        sl_doscript_blargs(guidedb_yname(SWAP16(RFIFOW(sd->fd,7))),"run",1,&sd->bl);

}*/
// clif_parsewield — ported to src/game/map_parse/items.rs
// clif_addtocurrent — ported to src/game/map_parse/items.rs
// clif_dropgold — ported to src/game/map_parse/items.rs

// clif_open_sub — ported to src/game/map_parse/items.rs
// clif_parsechangespell — ported to src/game/map_parse/items.rs
// clif_removespell — ported to src/game/map_parse/items.rs
// clif_throwitem_sub — ported to src/game/map_parse/items.rs
// clif_throwitem_script — ported to src/game/map_parse/items.rs
// clif_throw_check — ported to src/game/map_parse/items.rs
// clif_throwconfirm — ported to src/game/map_parse/items.rs
// clif_parsethrow — ported to src/game/map_parse/items.rs

int clif_parseviewchange(USER *sd) {
  int dx = 0, dy = 0;
  int x0, y0, x1, y1, direction = 0;
  // unsigned short checksum;

  direction = RFIFOB(sd->fd, 5);
  dx = RFIFOB(sd->fd, 6);
  dy = RFIFOB(sd->fd, 7);
  x0 = SWAP16(RFIFOW(sd->fd, 8));
  y0 = SWAP16(RFIFOW(sd->fd, 10));
  x1 = RFIFOB(sd->fd, 12);
  y1 = RFIFOB(sd->fd, 13);
  // checksum = SWAP16(RFIFOW(sd->fd, 14));

  if (sd->status.state == 3) {
    clif_sendminitext(sd, "You cannot do that while riding a mount.");
    return 0;
  }

  switch (direction) {
    case 0:
      dy++;
      break;
    case 1:
      dx--;
      break;
    case 2:
      dy--;
      break;
    case 3:
      dx++;
      break;
    default:
      break;
  }

  clif_sendxychange(sd, dx, dy);
  clif_mob_look_start(sd);
  map_foreachinblock(clif_object_look_sub, sd->bl.m, x0, y0, x0 + (x1 - 1),
                     y0 + (y1 - 1), BL_ALL, LOOK_GET, sd);
  clif_mob_look_close(sd);
  map_foreachinblock(clif_charlook_sub, sd->bl.m, x0, y0, x0 + (x1 - 1),
                     y0 + (y1 - 1), BL_PC, LOOK_GET, sd);
  map_foreachinblock(clif_cnpclook_sub, sd->bl.m, x0, y0, x0 + (x1 - 1),
                     y0 + (y1 - 1), BL_NPC, LOOK_GET, sd);
  map_foreachinblock(clif_cmoblook_sub, sd->bl.m, x0, y0, x0 + (x1 - 1),
                     y0 + (y1 - 1), BL_MOB, LOOK_GET, sd);
  map_foreachinblock(clif_charlook_sub, sd->bl.m, x0, y0, x0 + (x1 - 1),
                     y0 + (y1 - 1), BL_PC, LOOK_SEND, sd);

  return 0;
}

int clif_parsefriends(USER *sd, char *friendList, int len) {
  int i = 0;
  int j = 0;
  char friends[20][16];
  char escape[16];
  int friendCount = 0;
  SqlStmt *stmt = SqlStmt_Malloc(sql_handle);

  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    return 0;
  }

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

  if (SQL_ERROR ==
          SqlStmt_Prepare(stmt, "SELECT * FROM `Friends` WHERE `FndChaId` = %d",
                          sd->status.id) ||
      SQL_ERROR == SqlStmt_Execute(stmt)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return 0;
  }

  if (SqlStmt_NumRows(stmt) == 0) {
    if (SQL_ERROR == Sql_Query(sql_handle,
                               "INSERT INTO `Friends` (`FndChaId`) VALUES (%d)",
                               sd->status.id))
      Sql_ShowDebug(sql_handle);
  }

  for (i = 0; i < 20; i++) {
    Sql_EscapeString(sql_handle, escape, friends[i]);

    if (SQL_ERROR == Sql_Query(sql_handle,
                               "UPDATE `Friends` SET `FndChaName%d` = '%s' "
                               "WHERE `FndChaId` = '%u'",
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
  memcpy(sd->profile_data, RFIFOP(sd->fd, 5 + sd->profilepic_size),
         sd->profile_size);
  return 0;
}

// this is for preventing hackers
int check_packet_size(int fd, int len) {
  // USER *sd=rust_session_get_data(fd);

  if ((size_t)RFIFOREST(fd) >
      (size_t)len) {  // there is more here, so check for congruity
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

  if (SQL_ERROR ==
      Sql_Query(sql_handle,
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
      // mob=(MOB*)bl;
      clif_object_look_specific(sd, SWAP32(RFIFOL(sd->fd, 5)));
      // clif_mob_look3(sd,mob);
    }
  }
  return 0;
}
int clif_handle_menuinput(USER *sd) {
  int npcinf;
  npcinf = RFIFOB(sd->fd, 5);

  if (!hasCoref(sd)) return 0;

  switch (npcinf) {
    case 0:  // menu
      sl_async_freeco(sd);
      break;
    case 1:  // input
      clif_parsemenu(sd);
      break;
    case 2:  // buy
      clif_parsebuy(sd);
      break;
    case 3:  // input
      clif_parseinput(sd);
      break;
    case 4:  // sell
      clif_parsesell(sd);
      break;
    default:
      sl_async_freeco(sd);
      break;
  }

  return 0;
}
// clif_handle_clickgetinfo — ported to src/game/map_parse/dialogs.rs
int clif_handle_powerboards(USER *sd) {
  USER *tsd = NULL;

  // if(canusepowerboards(sd))
  //	{

  tsd = map_id2sd(SWAP32(RFIFOL(sd->fd, 11)));
  if (tsd)
    sd->pbColor = RFIFOB(sd->fd, 15);
  else
    sd->pbColor = 0;

  if (tsd != NULL)
    sl_doscript_blargs("powerBoard", NULL, 2, &sd->bl, &tsd->bl);
  else
    sl_doscript_blargs("powerBoard", NULL, 2, &sd->bl, 0);

  //	  tsd=map_id2sd(SWAP32(RFIFOL(sd->fd,11)));
  //	  if(tsd) {
  //		int armColor=RFIFOB(sd->fd,15);
  /*		if(sd->status.gm_level) {
                  tsd->status.armor_color=armColor;
                  } else {
                          if(armColor==0) tsd->status.armor_color=armColor;
                          if(armColor==60) tsd->status.armor_color=armColor;
                          if(armColor==61) tsd->status.armor_color=armColor;
                          if(armColor==63) tsd->status.armor_color=armColor;
                          if(armColor==65) tsd->status.armor_color=armColor;
                  }
                  map_foreachinarea(clif_updatestate,tsd->bl.m,tsd->bl.x,tsd->bl.y,AREA,BL_PC,tsd);
            }
            clif_sendpowerboard(sd);
          }
          else
  clif_Hacker(sd->status.name,"Accessing dye boards");
*/
  return 0;
}

// clif_sendminimap — ported to src/game/map_parse/player_state.rs

int clif_handle_boards(USER *sd) {
  int postcolor;
  switch (RFIFOB(sd->fd, 5)) {
    case 1:  // Show Board
      sd->bcount = 0;
      sd->board_popup = 0;
      clif_showboards(sd);
      break;
    case 2:  // Show posts from board #
      if (RFIFOB(sd->fd, 8) == 127) sd->bcount = 0;

      boards_showposts(sd, SWAP16(RFIFOW(sd->fd, 6)));

      break;
    case 3:  // Read post/nmail
      boards_readpost(sd, SWAP16(RFIFOW(sd->fd, 6)), SWAP16(RFIFOW(sd->fd, 8)));
      break;
    case 4:  // Make post
      boards_post(sd, SWAP16(RFIFOW(sd->fd, 6)));
      break;
    case 5:  // delete post!
      boards_delete(sd, SWAP16(RFIFOW(sd->fd, 6)));
      break;
    case 6:  // Send nmail
      if (sd->status.level >= 10)
        nmail_write(sd);
      else
        clif_sendminitext(sd,
                          "You must be at least level 10 to view/send nmail.");
      break;
    case 7:  // Change
      if (sd->status.gm_level) {
        postcolor = map_getpostcolor(SWAP16(RFIFOW(sd->fd, 6)),
                                     SWAP16(RFIFOW(sd->fd, 8)));
        postcolor ^= 1;
        map_changepostcolor(SWAP16(RFIFOW(sd->fd, 6)),
                            SWAP16(RFIFOW(sd->fd, 8)), postcolor);
        nmail_sendmessage(sd, "Post updated.", 6, 0);
      }
      break;
    case 8:  // SPECIAL WRITE
      sl_doscript_blargs(boarddb_yname(SWAP16(RFIFOW(sd->fd, 6))), "write", 1,
                         &sd->bl);

    case 9:  // Nmail

      sd->bcount = 0;
      boards_showposts(sd, 0);

      break;
  }
  return 0;
}

int clif_print_disconnect(int fd) {
  if (rust_session_get_eof(fd) == 4)  // Ignore this.
    return 0;

  printf(CL_NORMAL "(Reason: " CL_GREEN);
  switch (rust_session_get_eof(fd)) {
    case 0x00:
    case 0x01:
      printf("NORMAL_EOF");
      break;
    case 0x02:
      printf("SOCKET_SEND_ERROR");
      break;
    case 0x03:
      printf("SOCKET_RECV_ERROR");
      break;
    case 0x04:
      printf("ZERO_RECV_ERROR(NORMAL)");
      break;
    case 0x05:
      printf("MISSING_WDATA");
      break;
    case 0x06:
      printf("WDATA_REALLOC");
      break;
    case 0x07:
      printf("NO_MMO_DATA");
      break;
    case 0x08:
      printf("SESSIONDATA_EXISTS");
      break;
    case 0x09:
      printf("PLAYER_CONNECTING");
      break;
    case 0x0A:
      printf("INVALID_EXCHANGE");
      break;
    case 0x0B:
      printf("ACCEPT_NAMELEN_ERROR");
      break;
    case 0x0C:
      printf("PLAYER_TIMEOUT");
      break;
    case 0x0D:
      printf("INVALID_PACKET_HEADER");
      break;
    case 0x0E:
      printf("WPE_HACK");
      break;
    default:
      printf("UNKNOWN");
      break;
  }
  printf(CL_NORMAL ")\n");
  return 0;
}
int clif_parse(int fd) {
  unsigned short len;
  USER *sd = NULL;
  unsigned char CurrentSeed;

  if (fd < 0) return 0;
  if (!rust_session_exists(fd)) return 0;

  sd = (USER *)rust_session_get_data(fd);

  // for(pnum=0;pnum<3 && rust_session_exists(fd) && session[fd]->rdata_size;pnum++) {
  if (rust_session_get_eof(fd)) {
    if (sd) {
      printf("[map] [session_eof] name=%s\n", sd->status.name);
      clif_handle_disconnect(sd);
      clif_closeit(sd);
      // sd->fd=0;
    }
    // printf("Reason for disconnect: %d\n",rust_session_get_eof(fd));
    clif_print_disconnect(fd);
    session_eof(fd);
    return 0;
  }

  // if(!session[fd]->rdata_size) return 0;
  if (RFIFOREST(fd) > 0 && RFIFOB(fd, 0) != 0xAA) {
    rust_session_set_eof(fd, 13);
    return 0;
  }

  if (RFIFOREST(fd) < 3) return 0;

  len = SWAP16(RFIFOW(fd, 1)) + 3;

  // if(check_packet_size(fd,len)) return 0; //Hacker prevention?
  // ok the biggest packet we might POSSIBLY get wont be bigger than 10k, so set
  // a limit
  if (RFIFOREST(fd) < len) return 0;

  // printf("parsing %d\n",fd);
  if (!sd) {
    switch (RFIFOB(fd, 3)) {
      case 0x10:
        // clif_debug(RFIFOP(sd->fd,4),SWAP16(RFIFOW(sd->fd,1)))
        clif_accept2(fd, (char *)RFIFOP(fd, 16), RFIFOB(fd, 15));

        break;

      default:
        // session[fd]->eof=1;
        break;
    }

    RFIFOSKIP(fd, len);
    return 0;
  }

  nullpo_ret(0, sd);
  CurrentSeed = RFIFOB(fd, 4);

  /*if ((sd->PrevSeed == 0 && sd->NextSeed == 0 && CurrentSeed == 0)
  || ((sd->PrevSeed || sd->NextSeed) && CurrentSeed != sd->NextSeed)) {
          char RegStr[] = "WPEtimes";
          char AlertStr[32] = "";
          int WPEtimes = 0;

          sprintf(AlertStr, "Packet editing of 0x%02X detected", RFIFOB(fd, 3));
          clif_Hacker(sd->status.name, AlertStr);
          WPEtimes = pc_readglobalreg(sd, RegStr) + 1;
          pc_setglobalreg(sd, RegStr, WPEtimes);
          rust_session_set_eof(sd->fd, 14);
          return 0;
  }*/

  sd->PrevSeed = CurrentSeed;
  sd->NextSeed = CurrentSeed + 1;

  int logincount = 0;
  USER *tsd = NULL;
  for (int i = 0; i < fd_max; i++) {
    if (rust_session_exists(i) && (tsd = rust_session_get_data(i))) {
      if (sd->status.id == tsd->status.id) logincount++;

      if (logincount >= 2) {
        printf("%s attempted dual login on IP:%s\n", sd->status.name,
               sd->status.ipaddress);
        rust_session_set_eof(sd->fd, 1);
        rust_session_set_eof(tsd->fd, 1);
        break;
      }
    }
  }

  // Incoming Packet Decryption
  decrypt(fd);

  // printf("packet id: %i\n",RFIFOB(fd,3));

  /*printf("Packet:\n");
for (int i = 0; i < SWAP16(RFIFOW(fd, 1)); i++) {
printf("%02X ",RFIFOB(fd,i));
}
printf("\n");*/

  switch (RFIFOB(fd, 3)) {
    case 0x05:
      // clif_cancelafk(sd); -- conflict with light function, causes character
      // to never enter AFK status
      clif_parsemap(sd);
      break;
    case 0x06:
      clif_cancelafk(sd);
      clif_parsewalk(sd);
      break;
    case 0x07:
      clif_cancelafk(sd);
      sd->time += 1;
      if (sd->time < 4) {
        clif_parsegetitem(sd);
      }
      break;
    case 0x08:
      clif_cancelafk(sd);
      clif_parsedropitem(sd);
      break;
    case 0x09:
      clif_cancelafk(sd);
      clif_parselookat_2(sd);

      break;
    case 0x0A:
      clif_cancelafk(sd);

      clif_parselookat(sd);
      break;
    case 0x0B:
      clif_cancelafk(sd);
      clif_closeit(sd);
      break;
    case 0x0C:  // < missing object/char/monster
      clif_handle_missingobject(sd);
      break;
    case 0x0D:
      clif_parseignore(sd);
      break;
    case 0x0E:
      clif_cancelafk(sd);
      if (sd->status.gm_level) {
        clif_parsesay(sd);
      } else {
        sd->chat_timer += 1;
        if (sd->chat_timer < 2 && !sd->status.mute) {
          clif_parsesay(sd);
        }
      }
      break;

    case 0x0F:  // magic
      clif_cancelafk(sd);
      sd->time += 1;

      if (!sd->paralyzed && sd->sleep == 1.0f) {
        if (sd->time < 4) {
          if (map[sd->bl.m].spell || sd->status.gm_level) {
            clif_parsemagic(sd);
          } else {
            clif_sendminitext(sd, "That doesn't work here.");
          }
        }
      }
      break;
    case 0x11:
      clif_cancelafk(sd);
      clif_parseside(sd);
      break;
    case 0x12:
      clif_cancelafk(sd);
      clif_parsewield(sd);
      break;
    case 0x13:
      clif_cancelafk(sd);
      sd->time++;

      if (sd->attacked != 1 && sd->attack_speed > 0) {
        sd->attacked = 1;
        timer_insert(((sd->attack_speed * 1000) / 60),
                     ((sd->attack_speed * 1000) / 60), pc_atkspeed,
                     sd->status.id, 0);
        clif_parseattack(sd);
      } else {
        // clif_parseattack(sd);
      }
      break;
    case 0x17:
      clif_cancelafk(sd);

      int pos = RFIFOB(sd->fd, 6);
      int confirm = RFIFOB(sd->fd, 5);

      if (itemdb_thrownconfirm(sd->status.inventory[pos - 1].id) == 1) {
        if (confirm == 1)
          clif_parsethrow(sd);
        else
          clif_throwconfirm(sd);
      } else
        clif_parsethrow(sd);

      /*printf("throw packet\n");
      for (int i = 0; i< SWAP16(RFIFOW(sd->fd,1));i++) {
              printf("%02X ",RFIFOB(sd->fd,i));
      }
      printf("\n");*/

      break;
    case 0x18:
      clif_cancelafk(sd);
      // clif_sendtowns(sd);
      clif_user_list(sd);
      break;
    case 0x19:
      clif_cancelafk(sd);
      clif_parsewisp(sd);
      break;
    case 0x1A:
      clif_cancelafk(sd);
      clif_parseeatitem(sd);

      break;
    case 0x1B:
      if (sd->loaded) clif_changestatus(sd, RFIFOB(sd->fd, 6));

      break;
    case 0x1C:
      clif_cancelafk(sd);
      clif_parseuseitem(sd);

      break;
    case 0x1D:
      clif_cancelafk(sd);
      sd->time++;
      if (sd->time < 4) {
        clif_parseemotion(sd);
      }
      break;
    case 0x1E:
      clif_cancelafk(sd);
      sd->time++;
      if (sd->time < 4) clif_parsewield(sd);
      break;
    case 0x1F:
      clif_cancelafk(sd);
      if (sd->time < 4) clif_parseunequip(sd);
      break;
    case 0x20:  // Clicked 'O'
      clif_cancelafk(sd);
      clif_open_sub(sd);
      // map_foreachincell(clif_open_sub,sd->bl.m,sd->bl.x,sd->bl.y,BL_NPC,sd);
      break;
    case 0x23:
      // paperpopupwritable SAVE
      clif_paperpopupwrite_save(sd);
      break;
    case 0x24:
      clif_cancelafk(sd);
      clif_dropgold(sd, SWAP32(RFIFOL(sd->fd, 5)));
      break;
    case 0x27:  // PACKET SENT WHEN SOMEONE CLICKS QUEST tab or SHIFT Z key
      clif_cancelafk(sd);

      // clif_sendurl(sd,0,"https://www.website.com/questguide/");

      /*if(SWAP16(RFIFOW(sd->fd,5))==0) {
              clif_showguide(sd);
      } else {
              clif_showguide2(sd);
      }*/

      break;
    case 0x29:
      clif_cancelafk(sd);
      clif_handitem(sd);
      //	clif_parse_exchange(sd);
      break;
    case 0x2A:
      clif_cancelafk(sd);
      clif_handgold(sd);
      break;
    case 0x2D:
      clif_cancelafk(sd);

      if (RFIFOB(sd->fd, 5) == 0) {
        clif_mystaytus(sd);
      } else {
        // clif_startexchange(sd,sd->bl.id);
        clif_groupstatus(sd);
      }

      break;
    case 0x2E:
      clif_cancelafk(sd);

      clif_addgroup(sd);
      break;
    case 0x30:
      clif_cancelafk(sd);

      if (RFIFOB(sd->fd, 5) == 1) {
        clif_parsechangespell(sd);
      } else {
        clif_parsechangepos(sd);
      }
      break;
    case 0x32:
      clif_cancelafk(sd);

      clif_parsewalk(sd);

      break;
    case 0x34:
      clif_cancelafk(sd);

      clif_postitem(sd);

      /*case 0x36: -- clan bank packet
              clif_cancelafk(sd);
              clif_parseClanBankWithdraw(sd);*/

    case 0x38:
      clif_cancelafk(sd);

      clif_refresh(sd);
      break;

    case 0x39:  // menu & input
      clif_cancelafk(sd);

      clif_handle_menuinput(sd);

      break;
    case 0x3A:
      clif_cancelafk(sd);

      clif_parsenpcdialog(sd);

      // if(hasCoref(sd)) clif_parsenpcdialog(sd);

      break;

    case 0x3B:
      clif_cancelafk(sd);

      clif_handle_boards(sd);

      break;
    case 0x3F:  // Map change packet
      pc_warp(sd, SWAP16(RFIFOW(sd->fd, 5)), SWAP16(RFIFOW(sd->fd, 7)),
              SWAP16(RFIFOW(sd->fd, 9)));
      break;
    case 0x41:
      clif_cancelafk(sd);
      clif_parseparcel(sd);
      break;
    case 0x42:  // Client crash debug.
      break;
    case 0x43:
      clif_cancelafk(sd);
      clif_handle_clickgetinfo(sd);
      break;
      // Packet 45 responds from 3B
    case 0x4A:
      clif_cancelafk(sd);
      clif_parse_exchange(sd);
      break;
    case 0x4C:
      clif_cancelafk(sd);
      clif_handle_powerboards(sd);
      break;
    case 0x4F:  // Profile change
      clif_cancelafk(sd);
      clif_changeprofile(sd);
      break;
    case 0x60:  // PING
      break;
    case 0x66:
      clif_cancelafk(sd);
      clif_sendtowns(sd);
      break;
    case 0x69:  // Obstruction(something blocking movement)
      // clif_debug(RFIFOP(sd->fd,5),SWAP16(RFIFOW(sd->fd,1))-2);
      // if(sd->status.gm_level>0) {
      //	clif_handle_obstruction(sd);
      //}
      break;
    case 0x6B:  // creation system
      clif_cancelafk(sd);
      createdb_start(sd);
      break;
    case 0x73:  // web board
      clif_cancelafk(sd);
      // BOARD AA 00 0B 73 08 00 00 74 32 B1 42
      // LOOK  AA 00 0B 73 07 04 00 E7 9E 13 16

      if (RFIFOB(sd->fd, 5) == 0x04) {  // Userlook
        clif_sendprofile(sd);
      }
      if (RFIFOB(sd->fd, 5) == 0x00) {  // Board
        clif_sendboard(sd);
      }

      // clif_debug(RFIFOP(sd->fd, 0), SWAP16(RFIFOW(sd->fd, 1)));

      break;
    case 0x75:
      clif_parsewalkpong(sd);
      break;

    case 0x7B:  // Request Item Information!
      printf("request: %u\n", RFIFOB(sd->fd, 5));
      switch (RFIFOB(sd->fd, 5)) {
        case 0:  // Request the file asking for
          send_meta(sd);
          break;
        case 1:  // Requqest the list to use
          send_metalist(sd);

          break;
      }
      break;

    case 0x7C:  // map
      clif_cancelafk(sd);
      clif_sendminimap(sd);
      break;

    case 0x7D:  // Ranking SYSTEM
      clif_cancelafk(sd);
      switch (RFIFOB(
          fd, 5)) {  // Packet fd 5 is the mode choice. 5 = send reward, 6 = get
                     // reward, everything else is to show the ranking list
        case 5: {
          clif_sendRewardInfo(sd, fd);
          break;
        }
        case 6: {
          clif_getReward(sd, fd);
          break;
        }
        default: {
          clif_parseranking(sd, fd);
          break;
        }
      }
      break;

    case 0x77:
      clif_cancelafk(sd);
      clif_parsefriends(sd, (char *)RFIFOP(sd->fd, 5),
                        SWAP16(RFIFOW(sd->fd, 1)) - 5);
      break;
    case 0x82:
      clif_cancelafk(sd);
      clif_parseviewchange(sd);
      break;
    case 0x83:  // screenshots...
      break;

    case 0x84:  // add to hunter list (new client function)
      clif_cancelafk(sd);
      clif_huntertoggle(sd);

      break;

    case 0x85:  // modified for 736  -- this packet is called when you double
                // click on a hunter on the userlist
      clif_sendhunternote(sd);

      clif_cancelafk(sd);
      break;

    default:
      printf("[Map] Unknown Packet ID: %02X\nPacket content:\n",
             RFIFOB(sd->fd, 3));
      clif_debug(RFIFOP(sd->fd, 0), SWAP16(RFIFOW(sd->fd, 1)));
      break;
  }

  RFIFOSKIP(fd, len);
  //}
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
  // CALLOC(ubuf,0,ulen);
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
  // WFIFOB(sd->fd,4)=0x08;
  WFIFOB(sd->fd, 5) = 0;  // this is sending file data
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
  // printf("%s\n",file);
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
  // WFIFOB(sd->fd,4)=0x00;
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
    case 0:  // up
      ny = yold - 1;
      break;
    case 1:  // right
      nx = xold + 1;
      break;
    case 2:  // down
      ny = yold + 1;
      break;
    case 3:  // left
      nx = xold - 1;
      break;
  }

  sd->bl.x = nx;
  sd->bl.y = ny;

  // if(clif_canmove(sd)) {
  //		sd->bl.x=xold;
  //	sd->bl.y=yold;

  //}

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
  // unsigned int id;
  // id = SWAP32(RFIFOL(sd->fd, 6));
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

  // if( (sd->optFlags & optFlag_stealth && !src_sd->status.gm_level) &&
  // src_sd->status.id != sd->status.id)return 0;

  if (!rust_session_exists(sd->fd)) {
    rust_session_set_eof(sd->fd, 8);
    return 0;
  }

  WFIFOHEAD(src_sd->fd, 512);
  WFIFOB(src_sd->fd, 0) = 0xAA;
  WFIFOB(src_sd->fd, 3) = 0x1D;
  // WFIFOB(src_sd->fd,4)=0x03;
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
         (sd->gfx.dye == src_sd->gfx.dye && sd->gfx.dye != 0 &&
          src_sd->gfx.dye != 0))) {
      WFIFOB(src_sd->fd, 11) = 5;  // Gm's need to see invis
    } else {
      WFIFOB(src_sd->fd, 11) = sd->status.state;
    }

    if ((sd->optFlags & optFlag_stealth) && !sd->status.state &&
        !src_sd->status.gm_level)
      WFIFOB(src_sd->fd, 11) = 2;

    if (sd->status.state == 3) {
      WFIFOW(src_sd->fd, 12) = SWAP16(sd->disguise);
    } else {
      WFIFOW(src_sd->fd, 12) = SWAP16(0);
    }

    WFIFOB(src_sd->fd, 14) = sd->speed;

    WFIFOB(src_sd->fd, 15) = 0;
    WFIFOB(src_sd->fd, 16) = sd->status.face;        // face
    WFIFOB(src_sd->fd, 17) = sd->status.hair;        // hair
    WFIFOB(src_sd->fd, 18) = sd->status.hair_color;  // hair color
    WFIFOB(src_sd->fd, 19) = sd->status.face_color;
    WFIFOB(src_sd->fd, 20) = sd->status.skin_color;
    // WFIFOB(src_sd->fd,21)=0;

    // armor
    if (!pc_isequip(sd, EQ_ARMOR)) {
      WFIFOW(src_sd->fd, 21) = SWAP16(sd->status.sex);
    } else {
      if (sd->status.equip[EQ_ARMOR].customLook != 0) {
        WFIFOW(src_sd->fd, 21) = SWAP16(sd->status.equip[EQ_ARMOR].customLook);
      } else {
        WFIFOW(src_sd->fd, 21) =
            SWAP16(itemdb_look(pc_isequip(sd, EQ_ARMOR)));  //-10000+16;
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

    // coat
    if (pc_isequip(sd, EQ_COAT)) {
      WFIFOW(src_sd->fd, 21) =
          SWAP16(itemdb_look(pc_isequip(sd, EQ_COAT)));  //-10000+16;

      if (sd->status.armor_color > 0) {
        WFIFOB(src_sd->fd, 23) = sd->status.armor_color;
      } else {
        WFIFOB(src_sd->fd, 23) = itemdb_lookcolor(pc_isequip(sd, EQ_COAT));
      }
    }

    // weapon
    if (!pc_isequip(sd, EQ_WEAP)) {
      WFIFOW(src_sd->fd, 24) = 0xFFFF;
      WFIFOB(src_sd->fd, 26) = 0x0;
    } else {
      if (sd->status.equip[EQ_WEAP].customLook !=
          0) {  // edited on 07-16-2017 to support custom WEapon Skins
        WFIFOW(src_sd->fd, 24) = SWAP16(sd->status.equip[EQ_WEAP].customLook);
        WFIFOB(src_sd->fd, 26) = sd->status.equip[EQ_WEAP].customLookColor;
      } else {
        WFIFOW(src_sd->fd, 24) = SWAP16(itemdb_look(pc_isequip(sd, EQ_WEAP)));
        WFIFOB(src_sd->fd, 26) = itemdb_lookcolor(pc_isequip(sd, EQ_WEAP));
      }
    }

    // shield
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
      // helm stuff goes here
      WFIFOB(src_sd->fd, 30) = 0;       // supposed to be 1=Helm, 0=No helm
      WFIFOW(src_sd->fd, 31) = 0xFFFF;  // supposed to be Helm num
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
    // faceacc
    if (!pc_isequip(sd, EQ_FACEACC)) {
      // beard stuff
      WFIFOW(src_sd->fd, 33) = 0xFFFF;
      WFIFOB(src_sd->fd, 35) = 0x0;
    } else {
      WFIFOW(src_sd->fd, 33) =
          SWAP16(itemdb_look(pc_isequip(sd, EQ_FACEACC)));  // beard num
      WFIFOB(src_sd->fd, 35) =
          itemdb_lookcolor(pc_isequip(sd, EQ_FACEACC));  // beard color
    }
    // crown
    if (!pc_isequip(sd, EQ_CROWN)) {
      WFIFOW(src_sd->fd, 36) = 0xFFFF;
      WFIFOB(src_sd->fd, 38) = 0x0;
    } else {
      WFIFOB(src_sd->fd, 30) = 0;
      if (sd->status.equip[EQ_CROWN].customLook != 0) {
        WFIFOW(src_sd->fd, 36) =
            SWAP16(sd->status.equip[EQ_CROWN].customLook);  // Crown
        WFIFOB(src_sd->fd, 38) =
            sd->status.equip[EQ_CROWN].customLookColor;  // Crown color
      } else {
        WFIFOW(src_sd->fd, 36) =
            SWAP16(itemdb_look(pc_isequip(sd, EQ_CROWN)));  // Crown
        WFIFOB(src_sd->fd, 38) =
            itemdb_lookcolor(pc_isequip(sd, EQ_CROWN));  // Crown color
      }
    }

    if (!pc_isequip(sd, EQ_FACEACCTWO)) {
      WFIFOW(src_sd->fd, 39) = 0xFFFF;  // second face acc
      WFIFOB(src_sd->fd, 41) = 0x0;     //" color
    } else {
      WFIFOW(src_sd->fd, 39) =
          SWAP16(itemdb_look(pc_isequip(sd, EQ_FACEACCTWO)));
      WFIFOB(src_sd->fd, 41) = itemdb_lookcolor(pc_isequip(sd, EQ_FACEACCTWO));
    }

    // mantle
    if (!pc_isequip(sd, EQ_MANTLE)) {
      WFIFOW(src_sd->fd, 42) = 0xFFFF;
      WFIFOB(src_sd->fd, 44) = 0xFF;
    } else {
      WFIFOW(src_sd->fd, 42) = SWAP16(itemdb_look(pc_isequip(sd, EQ_MANTLE)));
      WFIFOB(src_sd->fd, 44) = itemdb_lookcolor(pc_isequip(sd, EQ_MANTLE));
    }

    // necklace
    if (!pc_isequip(sd, EQ_NECKLACE) ||
        !(sd->status.settingFlags & FLAG_NECKLACE) ||
        (itemdb_look(pc_isequip(sd, EQ_NECKLACE)) ==
         -1)) {  // Necklace Toggle bug fix. 07-07-17
      WFIFOW(src_sd->fd, 45) = 0xFFFF;
      WFIFOB(src_sd->fd, 47) = 0x0;
    } else {
      WFIFOW(src_sd->fd, 45) =
          SWAP16(itemdb_look(pc_isequip(sd, EQ_NECKLACE)));  // necklace
      WFIFOB(src_sd->fd, 47) =
          itemdb_lookcolor(pc_isequip(sd, EQ_NECKLACE));  // neckalce color
    }
    // boots
    if (!pc_isequip(sd, EQ_BOOTS)) {
      WFIFOW(src_sd->fd, 48) = SWAP16(sd->status.sex);  // boots
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

    // 51 color
    // 52 outline color   128 = black
    // 53 normal color when 51 & 52 set to 0

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

      /*switch(sd->gfx.dye) {
              case 60:
                      WFIFOB(src_sd->fd,51)=8;
                      break;
              case 61:
                      WFIFOB(src_sd->fd,51)=15;
                      break;
              case 63:
                      WFIFOB(src_sd->fd,51)=4;
                      break;
              case 66:
                      WFIFOB(src_sd->fd,51)=1;
                      break;

              default:
                      WFIFOB(src_sd->fd,51)=0;
                      break;
              }*/
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

      /*len = strlen(sd->gfx.name);
      if (strcasecmp(sd->gfx.name, "")) {
              WFIFOB(src_sd->fd, 52) = len;
              strcpy(WFIFOP(src_sd->fd, 53), sd->gfx.name);
      } else {
              WFIFOW(src_sd->fd,52) = 0;
              len = 1;
      }*/
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

      /*} else if(sd->status.state==0 && (src_sd->bl.id!=sd->bl.id)) {
              if(src_sd->status.state==1) {
                      WFIFOB(sd->fd, 0) = 0xAA;
                      WFIFOB(sd->fd, 1) = 0x00;
                      WFIFOB(sd->fd, 2) = 0x06;
                      WFIFOB(sd->fd, 3) = 0x5F;
                      WFIFOB(sd->fd, 4) = 0x03;
                      WFIFOL(sd->fd, 5) = SWAP32(src_sd->bl.id);
                      encrypt(WFIFOP(sd->fd,0));
                      WFIFOSET(sd->fd,9);
              } */
    }
  }

  return 0;
}

/* This is where Board commands go */

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

// clif_buydialog — ported to src/game/map_parse/dialogs.rs

// clif_parsebuy — ported to src/game/map_parse/dialogs.rs

// clif_selldialog — ported to src/game/map_parse/dialogs.rs

// clif_parsesell — ported to src/game/map_parse/dialogs.rs

int clif_isregistered(unsigned int id) {
  int accountid = 0;

  SqlStmt *stmt = SqlStmt_Malloc(sql_handle);

  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    return 0;
  }

  if (SQL_ERROR == SqlStmt_Prepare(
                       stmt,
                       "SELECT `AccountId` FROM `Accounts` WHERE "
                       "`AccountCharId1` = '%u' OR `AccountCharId2` = '%u' OR "
                       "`AccountCharId3` = '%u' OR `AccountCharId4` = '%u' OR "
                       "`AccountCharId5` = '%u' OR `AccountCharId6` = '%u'",
                       id, id, id, id, id, id) ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 0, SQLDT_UINT, &accountid, 0, NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return 0;
  }

  if (SQL_SUCCESS != SqlStmt_NextRow(stmt)) {
  }

  return accountid;

  // if (accountid > 0) return 1;
  // else return 0;
}

char *clif_getaccountemail(unsigned int id) {
  // char email[255];

  char *email;
  CALLOC(email, char, 255);
  memset(email, 0, 255);

  int acctid = clif_isregistered(id);
  if (acctid == 0) return 0;

  SqlStmt *stmt = SqlStmt_Malloc(sql_handle);

  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    return 0;
  }

  if (SQL_ERROR ==
          SqlStmt_Prepare(
              stmt,
              "SELECT `AccountEmail` FROM `Accounts` WHERE `AccountId` = '%u'",
              acctid) ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_STRING, &email[0], 255,
                                      NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return 0;
  }

  if (SQL_SUCCESS != SqlStmt_NextRow(stmt)) {
  }

  // printf("email is: %s\n",email);
  return &email[0];
}

// clif_input — ported to src/game/map_parse/dialogs.rs

// clif_parseinput — ported to src/game/map_parse/dialogs.rs

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
  // WFIFOB(sd->fd,4)=0x03;

  // Title
  if (strlen(tsd->status.title) > 0) {
    WFIFOB(sd->fd, 5) = strlen(tsd->status.title);
    strcpy(WFIFOP(sd->fd, 6), tsd->status.title);
    len += strlen(tsd->status.title) + 1;
  } else {
    WFIFOB(sd->fd, 5) = 0;
    len += 1;
  }

  // Clan
  if (tsd->status.clan > 0) {
    WFIFOB(sd->fd, len + 5) = strlen(clandb_name(tsd->status.clan));
    strcpy(WFIFOP(sd->fd, len + 6), clandb_name(tsd->status.clan));
    len += strlen(clandb_name(tsd->status.clan)) + 1;
  } else {
    WFIFOB(sd->fd, len + 5) = 0;
    len += 1;
  }

  // Clan Title
  if (strlen(tsd->status.clan_title) > 0) {
    WFIFOB(sd->fd, len + 5) = strlen(tsd->status.clan_title);
    strcpy(WFIFOP(sd->fd, len + 6), tsd->status.clan_title);
    len += strlen(tsd->status.clan_title) + 1;
  } else {
    WFIFOB(sd->fd, len + 5) = 0;
    len += 1;
  }

  // Class
  if (classdb_name(tsd->status.class, tsd->status.mark)) {
    WFIFOB(sd->fd, len + 5) =
        strlen(classdb_name(tsd->status.class, tsd->status.mark));
    strcpy(WFIFOP(sd->fd, len + 6),
           classdb_name(tsd->status.class, tsd->status.mark));
    len += strlen(classdb_name(tsd->status.class, tsd->status.mark)) + 1;
  } else {
    WFIFOB(sd->fd, len + 5) = 0;
    len += 1;
  }

  // Name
  WFIFOB(sd->fd, len + 5) = strlen(tsd->status.name);
  strcpy(WFIFOP(sd->fd, len + 6), tsd->status.name);
  len += strlen(tsd->status.name);

  // WFIFOW(sd->fd,len+5)=SWAP16(1);
  // len-=1;
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
  WFIFOB(sd->fd, len + 13) = tsd->status.face;        // face
  WFIFOB(sd->fd, len + 14) = tsd->status.hair;        // hair
  WFIFOB(sd->fd, len + 15) = tsd->status.hair_color;  // hair color
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
  // coat
  if (pc_isequip(tsd, EQ_COAT)) {
    WFIFOW(sd->fd, len + 4) = SWAP16(itemdb_look(pc_isequip(tsd, EQ_COAT)));

    if (tsd->status.armor_color > 0) {
      WFIFOB(sd->fd, len + 6) = tsd->status.armor_color;
    } else {
      WFIFOB(sd->fd, len + 6) = itemdb_lookcolor(pc_isequip(tsd, EQ_COAT));
    }
  }

  len += 3;
  // weapon
  if (!pc_isequip(tsd, EQ_WEAP)) {
    WFIFOW(sd->fd, len + 4) = 0xFFFF;
    WFIFOB(sd->fd, len + 6) = 0;
  } else {
    if (tsd->status.equip[EQ_WEAP].customLook !=
        0) {  // edited on 07-16-2017 to support custom WEapon Skins
      WFIFOW(sd->fd, len + 4) = SWAP16(tsd->status.equip[EQ_WEAP].customLook);
      WFIFOB(sd->fd, len + 6) = tsd->status.equip[EQ_WEAP].customLookColor;
    } else {
      WFIFOW(sd->fd, len + 4) = SWAP16(itemdb_look(pc_isequip(tsd, EQ_WEAP)));
      WFIFOB(sd->fd, len + 6) = itemdb_lookcolor(pc_isequip(tsd, EQ_WEAP));
    }
  }
  len += 3;
  // shield
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
    // helm stuff goes here
    WFIFOB(sd->fd, len + 4) = 0;       // supposed to be 1=Helm, 0=No helm
    WFIFOW(sd->fd, len + 5) = 0xFFFF;  // supposed to be Helm num
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
  // faceacc
  if (!pc_isequip(tsd, EQ_FACEACC)) {
    // beard stuff
    WFIFOW(sd->fd, len + 4) = 0xFFFF;
    WFIFOB(sd->fd, len + 6) = 0;
  } else {
    WFIFOW(sd->fd, len + 4) =
        SWAP16(itemdb_look(pc_isequip(tsd, EQ_FACEACC)));  // beard num
    WFIFOB(sd->fd, len + 6) =
        itemdb_lookcolor(pc_isequip(tsd, EQ_FACEACC));  // beard color
  }
  len += 3;
  // crown
  if (!pc_isequip(tsd, EQ_CROWN)) {
    WFIFOW(sd->fd, len + 4) = 0xFFFF;
    WFIFOB(sd->fd, len + 6) = 0;
  } else {
    WFIFOB(sd->fd, len) = 0;

    if (tsd->status.equip[EQ_CROWN].customLook != 0) {
      WFIFOW(sd->fd, len + 4) =
          SWAP16(tsd->status.equip[EQ_CROWN].customLook);  // Crown
      WFIFOB(sd->fd, len + 6) =
          tsd->status.equip[EQ_CROWN].customLookColor;  // Crown color
    } else {
      WFIFOW(sd->fd, len + 4) =
          SWAP16(itemdb_look(pc_isequip(tsd, EQ_CROWN)));  // Crown
      WFIFOB(sd->fd, len + 6) =
          itemdb_lookcolor(pc_isequip(tsd, EQ_CROWN));  // Crown color
    }
  }
  len += 3;

  if (!pc_isequip(tsd, EQ_FACEACCTWO)) {
    WFIFOW(sd->fd, len + 4) = 0xFFFF;  // second face acc
    WFIFOB(sd->fd, len + 6) = 0;       //" color
  } else {
    WFIFOW(sd->fd, len + 4) =
        SWAP16(itemdb_look(pc_isequip(tsd, EQ_FACEACCTWO)));
    WFIFOB(sd->fd, len + 6) = itemdb_lookcolor(pc_isequip(tsd, EQ_FACEACCTWO));
  }

  len += 3;
  // mantle
  if (!pc_isequip(tsd, EQ_MANTLE)) {
    WFIFOW(sd->fd, len + 4) = 0xFFFF;
    WFIFOB(sd->fd, len + 6) = 0xFF;
  } else {
    WFIFOW(sd->fd, len + 4) = SWAP16(itemdb_look(pc_isequip(tsd, EQ_MANTLE)));
    WFIFOB(sd->fd, len + 6) = itemdb_lookcolor(pc_isequip(tsd, EQ_MANTLE));
  }
  len += 3;

  // necklace
  if (!pc_isequip(tsd, EQ_NECKLACE) ||
      !(tsd->status.settingFlags & FLAG_NECKLACE) ||
      (itemdb_look(pc_isequip(tsd, EQ_NECKLACE)) == -1)) {
    WFIFOW(sd->fd, len + 4) = 0xFFFF;
    WFIFOB(sd->fd, len + 6) = 0;
  } else {
    WFIFOW(sd->fd, len + 4) =
        SWAP16(itemdb_look(pc_isequip(tsd, EQ_NECKLACE)));  // necklace
    WFIFOB(sd->fd, len + 6) =
        itemdb_lookcolor(pc_isequip(tsd, EQ_NECKLACE));  // neckalce color
  }
  len += 3;
  // boots
  if (!pc_isequip(tsd, EQ_BOOTS)) {
    WFIFOW(sd->fd, len + 4) = SWAP16(tsd->status.sex);  // boots
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
  // WFIFOL(sd->fd,len+6)=0;
  // len+=4;
  for (x = 0; x < 14; x++) {
    if (tsd->status.equip[x].id > 0) {
      if (tsd->status.equip[x].customIcon != 0) {
        WFIFOW(sd->fd, len + 6) =
            SWAP16(tsd->status.equip[x].customIcon + 49152);
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

  if (strlen(equip_status) == 0) {
    strcat(equip_status, "No items equipped.");
  }

  equip_len = strlen(equip_status);
  if (equip_len > 255) equip_len = 255;
  WFIFOB(sd->fd, len + 6) = equip_len;
  strcpy(WFIFOP(sd->fd, len + 7), equip_status);
  // printf("Len is %d\n",strlen(equip_status));
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

  /*if(tsd->profile_size==0) {
          WFIFOW(sd->fd,len+6)=0;
          len+=2;
          WFIFOB(sd->fd,len+6)=0;
          len+=1;
          //WFIFOB(sd->fd,len+6)=0;
          //len+=1;
  } else {
          WFIFOB
          WFIFOB(sd->fd, len + 6) = tsd->profile_size;
          memcpy(WFIFOP(sd->fd, len + 7), tsd->profile_data, tsd->profile_size);
          len += tsd->profile_size + 1;
  }*/
  // WFIFOB(sd->fd,len+7)=0;

  /*WFIFOB(sd->fd,len+6)=strlen(tsd->profile_text);
  strcpy(WFIFOP(sd->fd,len+7),tsd->profile_text);

  len+=strlen(tsd->profile_text)+1;
  */
  // WFIFOW(sd->fd,len+6)=0;

  for (x = 0; x < MAX_LEGENDS; x++) {
    if (strlen(tsd->status.legends[x].text) &&
        strlen(tsd->status.legends[x].name)) {
      count++;
    }
  }

  WFIFOW(sd->fd, len + 6) = SWAP16(count);
  len += 2;

  for (x = 0; x < MAX_LEGENDS; x++) {
    if (strlen(tsd->status.legends[x].text) &&
        strlen(tsd->status.legends[x].name)) {
      WFIFOB(sd->fd, len + 6) = tsd->status.legends[x].icon;
      WFIFOB(sd->fd, len + 7) = tsd->status.legends[x].color;

      if (tsd->status.legends[x].tchaid > 0) {
        char *name = clif_getName(tsd->status.legends[x].tchaid);
        char *buff = replace_str(tsd->status.legends[x].text, "$player", name);

        WFIFOB(sd->fd, len + 8) = strlen(buff);
        memcpy(WFIFOP(sd->fd, len + 9), buff, strlen(buff));
        len += strlen(buff) + 3;
      } else {
        WFIFOB(sd->fd, len + 8) = strlen(tsd->status.legends[x].text);
        memcpy(WFIFOP(sd->fd, len + 9), tsd->status.legends[x].text,
               strlen(tsd->status.legends[x].text));
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
  /*struct block_list *bl=NULL;
  struct map_sessiondata *tsd=NULL;

  bl = map_id2bl(object);
  if(bl->type == BL_PC) {
          tsd = map_id2sd(object);
  }*/

  switch (side) {
    case 0:               // heading NORTH
      if (flag & OBJ_UP)  // || tsd->optFlags&optFlag_stealth)
        return 1;
      break;
    case 1:                  // RIGHT
      if (flag & OBJ_RIGHT)  // || tsd->optFlags&optFlag_stealth)
        return 1;
      break;
    case 2:                 // DOWN
      if (flag & OBJ_DOWN)  // || tsd->optFlags&optFlag_stealth)
        return 1;
      break;
    case 3:                 // LEFT
      if (flag & OBJ_LEFT)  // || tsd->optFlags&optFlag_stealth)
        return 1;
      break;
  }

  return 0;
}
int clif_object_canmove_from(int m, int x, int y, int side) {
  int object = read_obj(m, x, y);
  unsigned char flag = objectFlags[object];

  switch (side) {
    case 0:  // heading NORTH
      if (flag & OBJ_DOWN) return 1;
      break;
    case 1:  // RIGHT
      if (flag & OBJ_LEFT) return 1;
      break;
    case 2:  // DOWN
      if (flag & OBJ_UP) return 1;
      break;
    case 3:  // LEFT
      if (flag & OBJ_RIGHT) return 1;
      break;
  }

  return 0;
}
int clif_changestatus(USER *sd, int type) {
  int oldm, oldx, oldy;
  char buff[256];

  switch (type) {
    case 0x00:  // Ride/something else
      if (RFIFOB(sd->fd, 7) == 1) {
        if (sd->status.state == 0) {
          clif_findmount(sd);

          if (sd->status.state == 0)
            clif_sendminitext(
                sd, "Good try, but there is nothing here that you can ride.");

        } else if (sd->status.state == 1) {
          clif_sendminitext(sd, "Spirits can't do that.");
        } else if (sd->status.state == 2) {
          clif_sendminitext(
              sd, "Good try, but there is nothing here that you can ride.");
        } else if (sd->status.state == 3) {
          sl_doscript_blargs("onDismount", NULL, 1, &sd->bl);
        } else if (sd->status.state == 4) {
          clif_sendminitext(sd, "You cannot do that while transformed.");
        }
      }
      break;

    case 0x01:  // Whisper (F5)
      sd->status.settingFlags ^= FLAG_WHISPER;

      // sd->optFlags ^= optFlag_nowhisp;
      if (sd->status.settingFlags & FLAG_WHISPER) {
        clif_sendminitext(sd, "Listen to whisper:ON");
      } else {
        clif_sendminitext(sd, "Listen to whisper:OFF");
      }
      clif_sendstatus(sd, 0);
      break;
    case 0x02:  // group
      sd->status.settingFlags ^= FLAG_GROUP;

      if (sd->status.settingFlags & FLAG_GROUP) {
        sprintf(buff, "Join a group     :ON");
      } else {
        if (sd->group_count > 0) {
          clif_leavegroup(sd);
        }

        sprintf(buff, "Join a group     :OFF");
      }

      clif_sendstatus(sd, 0);
      clif_sendminitext(sd, buff);
      break;
    case 0x03:  // Shout
      sd->status.settingFlags ^= FLAG_SHOUT;
      if (sd->status.settingFlags & FLAG_SHOUT) {
        clif_sendminitext(sd, "Listen to shout  :ON");
      } else {
        clif_sendminitext(sd, "Listen to shout  :OFF");
      }
      clif_sendstatus(sd, 0);
      break;
    case 0x04:  // Advice
      sd->status.settingFlags ^= FLAG_ADVICE;
      if (sd->status.settingFlags & FLAG_ADVICE) {
        clif_sendminitext(sd, "Listen to advice :ON");
      } else {
        clif_sendminitext(sd, "Listen to advice :OFF");
      }
      clif_sendstatus(sd, 0);
      break;
    case 0x05:  // Magic
      sd->status.settingFlags ^= FLAG_MAGIC;
      if (sd->status.settingFlags & FLAG_MAGIC) {
        clif_sendminitext(sd, "Believe in magic :ON");
      } else {
        clif_sendminitext(sd, "Believe in magic :OFF");
      }
      clif_sendstatus(sd, 0);
      break;
    case 0x06:  // Weather
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
    case 0x07:  // Realm center (F4)
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
      map_foreachinarea(clif_object_look_sub, sd->bl.m, sd->bl.x, sd->bl.y,
                        SAMEAREA, BL_ALL, LOOK_GET, sd);
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
    case 0x08:  // exchange
      sd->status.settingFlags ^= FLAG_EXCHANGE;
      // sd->exchange_on^=1;

      if (sd->status.settingFlags & FLAG_EXCHANGE) {
        sprintf(buff, "Exchange         :ON");
      } else {
        sprintf(buff, "Exchange         :OFF");
      }

      clif_sendstatus(sd, 0);
      clif_sendminitext(sd, buff);
      break;
    case 0x09:  // Fast move
      sd->status.settingFlags ^= FLAG_FASTMOVE;

      if (sd->status.settingFlags & FLAG_FASTMOVE) {
        clif_sendminitext(sd, "Fast Move        :ON");
      } else {
        clif_sendminitext(sd, "Fast Move        :OFF");
      }
      clif_sendstatus(sd, 0);
      break;
    case 10:  // Clan chat
      sd->status.clan_chat = (sd->status.clan_chat + 1) % 2;
      if (sd->status.clan_chat) {
        clif_sendminitext(sd, "Clan whisper     :ON");
      } else {
        clif_sendminitext(sd, "Clan whisper     :OFF");
      }
      break;
    case 13:                                 // Sound
      if (RFIFOB(sd->fd, 4) == 3) return 0;  // just started so dont do anything
      sd->status.settingFlags ^= FLAG_SOUND;
      if (sd->status.settingFlags & FLAG_SOUND) {
        sprintf(buff, "Hear sounds      :ON");
      } else {
        sprintf(buff, "Hear sounds      :OFF");
      }
      clif_sendminitext(sd, buff);
      clif_sendstatus(sd, 0);
      break;

    case 14:  // Helm
      // sd->status.show_helm=(sd->status.s6=how_helm+1)%2;
      sd->status.settingFlags ^= FLAG_HELM;
      if (sd->status.settingFlags & FLAG_HELM) {
        clif_sendminitext(sd, "Show Helmet      :ON");
        pc_setglobalreg(sd, "show_helmet",
                        1);  // Added 4/6/17 to give registry for helmet status
      } else {
        clif_sendminitext(sd, "Show Helmet      :OFF");
        pc_setglobalreg(sd, "show_helmet",
                        0);  // Added 4/6/17 to give registry for helmet status
      }
      clif_sendstatus(sd, 0);
      clif_sendchararea(sd);
      clif_getchararea(sd);
      // map_foreachinarea(clif_updatestate,sd->bl.m,sd->bl.x,sd->bl.y,AREA,BL_PC,sd);
      // // was commented
      break;

    case 15:  // Necklace
      // sd->status.show_necklace=(sd->status.s6=how_neck+1)%2;
      sd->status.settingFlags ^= FLAG_NECKLACE;
      if (sd->status.settingFlags & FLAG_NECKLACE) {
        clif_sendminitext(sd, "Show Necklace      :ON");
        pc_setglobalreg(sd, "show_necklace",
                        1);  // Added 4/6/17 to give registry for helmet status
      } else {
        clif_sendminitext(sd, "Show Necklace      :OFF");
        pc_setglobalreg(sd, "show_necklace",
                        0);  // Added 4/6/17 to give registry for helmet status
      }
      clif_sendstatus(sd, 0);
      clif_sendchararea(sd);
      clif_getchararea(sd);
      // map_foreachinarea(clif_updatestate,sd->bl.m,sd->bl.x,sd->bl.y,AREA,BL_PC,sd);
      // // was commented
      break;

    default:
      break;
  }

  return 0;
}

int clif_postitem(USER *sd) {
  int slot = RFIFOB(sd->fd, 5) - 1;

  // struct item_data *item = NULL;
  // item = itemdb_search(sd->status.inventory[slot].id);

  int x = 0;
  int y = 0;

  if (sd->status.side == 0) {
    x = sd->bl.x, y = sd->bl.y - 1;
  }
  if (sd->status.side == 1) {
    x = sd->bl.x + 1, y = sd->bl.y;
  }
  if (sd->status.side == 2) {
    x = sd->bl.x;
    y = sd->bl.y + 1;
  }
  if (sd->status.side == 3) {
    x = sd->bl.x - 1;
    y = sd->bl.y;
  }

  if (x < 0 || y < 0) return 0;

  int obj = read_obj(sd->bl.m, x, y);

  if (obj == 1619 || obj == 1620) {  // board object

    if (sd->status.inventory[slot].amount > 1)
      clif_input(sd, sd->last_click, "How many would you like to post?", "");
  }

  // printf("Slot: %i\n",slot);

  // printf("item to post: %s\n",item->name);

  sd->invslot = slot;

  /*printf("packet 34 received\n");
  for (int i = 0; i < SWAP16(RFIFOW(sd->fd,1));i++) {
          printf("%02X ",RFIFOB(sd->fd,i));
  }
  printf("\n");*/

  return 0;
}

/*int clif_clanBankWithdraw(USER *sd,struct item_data *items,int count) {

        if (!rust_session_exists(sd->fd))
        {
                rust_session_set_eof(sd->fd, 8);
                return 0;
        }

        int len = 0;

        WFIFOHEAD(sd->fd,65535);
        WFIFOB(sd->fd,0)=0xAA;
        WFIFOB(sd->fd,3)=0x3D;
        //WFIFOB(sd->fd,4)=0x03;
        WFIFOB(sd->fd,5)=0x0A;

        WFIFOB(sd->fd,6) = count;

        len += 7;

        for (int x = 0; x<count; x++) {

                WFIFOB(sd->fd,len) = x+1; // slot number
                len += 1;

                if (items[x].customIcon != 0) WFIFOW(sd->fd,len) =
SWAP16(items[x].customIcon+49152); else WFIFOW(sd->fd,len) =
SWAP16(itemdb_icon(items[x].id)); // packet only supports icon number, no colors

                len += 2;

                if (!strcasecmp(items[x].real_name,"")) { // no engrave
                        WFIFOB(sd->fd,len) = strlen(itemdb_name(items[x].id));
                        strcpy(WFIFOP(sd->fd,len+1),itemdb_name(items[x].id));
                        len += strlen(itemdb_name(items[x].id)) + 1;
                } else { // has engrave
                        WFIFOB(sd->fd,len) = strlen(items[x].real_name);
                        strcpy(WFIFOP(sd->fd,len+1),items[x].real_name);
                        len += strlen(items[x].real_name) + 1;
                }

                WFIFOB(sd->fd,len) = strlen(itemdb_name(items[x].id));
                strcpy(WFIFOP(sd->fd,len+1),itemdb_name(items[x].id));
                len += strlen(itemdb_name(items[x].id)) + 1;

                //WFIFOL(sd->fd,len) = SWAP32(48); // item count
                WFIFOL(sd->fd,len) = SWAP32(items[x].amount);

                len += 4;

                WFIFOB(sd->fd,len) = 1;
                WFIFOB(sd->fd,len+1) = 0;
                WFIFOB(sd->fd,len+2) = 1;
                WFIFOB(sd->fd,len+3) = 0;

                len += 4;

                WFIFOB(sd->fd,len) = 255; // This might be the max withdraw
limit at a time?  number is always 100 or 255

                len += 1;

        }

        WFIFOW(sd->fd,1)=SWAP16(len+3);
        WFIFOSET(sd->fd,encrypt(sd->fd));

        FREE(items);
        // /lua Player(2):clanBankWithdraw()

        return 0;

}*/

/*int clif_parseClanBankWithdraw(USER *sd) {

        unsigned int slot = RFIFOB(sd->fd,7);
        unsigned int amount = SWAP32(RFIFOL(sd->fd,9));

        sl_resumeclanbankwithdraw(RFIFOB(sd->fd,5),slot,amount,sd);

        return 0;
}*/

int clif_pushback(USER *sd) {
  switch (sd->status.side) {
    case 0:
      pc_warp(sd, sd->bl.m, sd->bl.x, sd->bl.y + 2);
      break;
    case 1:
      pc_warp(sd, sd->bl.m, sd->bl.x - 2, sd->bl.y);
      break;
    case 2:
      pc_warp(sd, sd->bl.m, sd->bl.x, sd->bl.y - 2);
      break;
    case 3:
      pc_warp(sd, sd->bl.m, sd->bl.x + 2, sd->bl.y);
      break;
  }

  return 0;
}

int clif_cancelafk(USER *sd) {
  nullpo_ret(0, sd);

  // if (sd->afk) reset = 1;

  sd->afktime = 0;
  sd->afk = 0;

  /*if (reset) {
          if (SQL_ERROR == Sql_Query(sql_handle, "INSERT INTO `UnAfkLogs`
  (`UfkChaId`, `UfkMapId`, `UfkX`, `UfkY`) VALUES ('%u', '%u', '%u', '%u')",
          sd->status.id, sd->bl.m, sd->bl.x, sd->bl.y)) {
                  Sql_ShowDebug(sql_handle);
                  return 0;
          }

  }*/

  return 0;
}
/*int clif_ispass(USER *sd) {
        char md52[32]="";
        char buf[255]="";
        char name2[32]="";
        char pass2[32]="";

        strcpy(name2,name);
        strcpy(pass2,pass);
        sprintf(buf,"%s %s",strlwr(name2),strlwr(pass2));
        MD5_String(buf,md52);

        if(!strcasecmp(md5,md52)) {
                return 1;
        } else {
                return 0;
        }
}
int clif_switchchar(USER *sd, char* name, char* pass) {
        int result;
        char md5[64]="";
        char pass2[64]="";
        int expiration=0;
        int ban=0;
        int map=0;
        int nID=0;
        SqlStmt* stmt=SqlStmt_Malloc(sql_handle);

        nullpo_ret(0, sd);
        if(stmt == NULL)
        {
                SqlStmt_ShowDebug(stmt);
                return 0;
        }

    if(SQL_ERROR == SqlStmt_Prepare(stmt,"SELECT `pass` FROM `character` WHERE
`name`='%s'",name)
        || SQL_ERROR == SqlStmt_Execute(stmt)
        || SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_STRING, &md5,
sizeof(md5), NULL, NULL)
        )
        {
                SqlStmt_ShowDebug(stmt);
                SqlStmt_Free(stmt);
                return 0; //db_error
        }

    if(SQL_SUCCESS != SqlStmt_NextRow(stmt))
        {
                SqlStmt_Free(stmt);
                return 0; //name doesn't exist
        }
        if(!ispass(name,pass,md5))
        {
                SqlStmt_Free(stmt);
                return 0; //wrong password, try again!
        }

        if(SQL_ERROR == SqlStmt_Prepare(stmt,"SELECT `id`, `pass`, `ban`, `map`
FROM `character` WHERE `name`='%s'",name)
        || SQL_ERROR == SqlStmt_Execute(stmt)
        || SQL_ERROR == SqlStmt_BindColumn(stmt, 0, SQLDT_UINT, &nID, 0, NULL,
NULL)
        || SQL_ERROR == SqlStmt_BindColumn(stmt, 1, SQLDT_STRING, &pass2,
sizeof(pass2), NULL, NULL)
        || SQL_ERROR == SqlStmt_BindColumn(stmt, 2, SQLDT_UCHAR, &ban, 0, NULL,
NULL)
        || SQL_ERROR == SqlStmt_BindColumn(stmt, 3, SQLDT_USHORT, &map, 0, NULL,
NULL)
        )
        {
                SqlStmt_ShowDebug(stmt);
                SqlStmt_Free(stmt);
                return 0; //db_error
        }

        if(SQL_SUCCESS != SqlStmt_NextRow(stmt))
        {
                SqlStmt_Free(stmt);
                return 0; //name doesn't exist
        }

        if(ban)
                return 2; //you are banned, go away

        SqlStmt_Free(stmt);
        return 1;
}*/
