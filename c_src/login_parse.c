#include "login_parse.h"

#include <arpa/inet.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <zlib.h>

#include "config.h"
#include "core.h"
#include "db_mysql.h"
#include "login_char.h"
#include "login_server.h"
#include "net_crypt.h"
#include "session.h"
#include "strlib.h"
#include "timer.h"


bool bannedIPCheck(char *ip) {
  SqlStmt *stmt = SqlStmt_Malloc(sql_handle);

  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    return false;
  }

  if (SQL_ERROR ==
          SqlStmt_Prepare(stmt,
                          "SELECT `BndId` FROM `BannedIP` WHERE `BndIP` = '%s'",
                          ip) ||
      SQL_ERROR == SqlStmt_Execute(stmt)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return false;
  }

  if (SqlStmt_NumRows(stmt) > 0) {
    SqlStmt_Free(stmt);
    return true;
  }
  SqlStmt_Free(stmt);
  return false;
}

int clif_accept(int fd) {
  int P = 0;
  bool DDOS = false;
  bool banned = false;

  char ip[30];
  strcpy(ip, inet_ntoa(*(struct in_addr*)(&(uint32_t){rust_session_get_client_ip(fd)})));

  banned = bannedIPCheck(ip);

  printf("[login] [accept] ip=%s\n", ip);

  if (DDOS) {
    rust_add_throttle(rust_session_get_client_ip(fd));
    printf("[login] [ddos_throttle] ip=%u.%u.%u.%u\n",
           CONVIP2(rust_session_get_client_ip(fd)));

    for (P = 0; P <= fd_max; P++) {
      if (rust_session_exists(P) == 0) {
        continue;
      }

      if (P == fd) {
        continue;
      }
      if (rust_session_get_client_ip(fd) ==
          rust_session_get_client_ip(P)) {
        rust_session_set_eof(P, 1);
        session_eof(P);
      }
    }
    rust_session_set_eof(fd, 1);
    session_eof(fd);
  } else if (banned) {
    // Add_Throttle( session[fd]->client_addr);
    printf("[login] [banned] ip=%s\n", ip);

    for (P = 0; P <= fd_max; P++) {
      if (rust_session_exists(P) == 0) {
        continue;
      }
      if (P == fd) {
        continue;
      }
      if (rust_session_get_client_ip(fd) ==
          rust_session_get_client_ip(P)) {
        rust_session_set_eof(P, 1);
        session_eof(P);
      }
    }
    rust_session_set_eof(fd, 1);
    session_eof(fd);
  } else {
    WFIFOHEAD(fd, 22);
    memcpy(WFIFOP(fd, 0),
           "\xAA\x00\x13\x7E\x1B\x43\x4F\x4E\x4E\x45\x43\x54\x45\x44\x20\x53"
           "\x45\x52\x56\x45\x52\x0A",
           22);
    WFIFOSET(fd, 22);
  }

  return 0;
}

int clif_message(int fd, char code, char *buff) {
  int packet_len = strlen(buff) + 6;
  WFIFOHEAD(fd, packet_len + 3);
  WFIFOB(fd, 0) = 0xAA;
  WFIFOW(fd, 1) = SWAP16(packet_len);
  WFIFOB(fd, 3) = 0x02;
  WFIFOB(fd, 4) = 0x02;
  WFIFOB(fd, 5) = code;
  WFIFOB(fd, 6) = strlen(buff);
  strcpy(WFIFOP(fd, 7), buff);
  WFIFOW(fd, packet_len + 8) = 0x00;
  set_packet_indexes((unsigned char *)WFIFOP(fd, 0));
  tk_crypt_static((unsigned char *)WFIFOP(fd, 0));
  WFIFOSET(fd, packet_len + 6);

  return 0;
}

int clif_sendurl(int fd, int type, const char *url) {
  WFIFOB(fd, 0) = 0xAA;
  WFIFOB(fd, 3) = 0x66;
  WFIFOB(fd, 4) = 0x03;
  WFIFOB(fd, 5) = type;  // type. 0 = ingame browser, 1= popup open browser then
                         // close client, 2 = popup
  WFIFOW(fd, 6) = SWAP16(strlen(url));
  memcpy(WFIFOP(fd, 8), url, strlen(url));
  WFIFOW(fd, 1) = SWAP16(strlen(url) + 8);
  set_packet_indexes((unsigned char *)WFIFOP(fd, 0));
  tk_crypt_static((unsigned char *)WFIFOP(fd, 0));
  WFIFOSET(fd, strlen(url) + 8);

  return 0;
}

int reg_check(const char *n, int len) {
  char buf[255];

  unsigned int id = 0;
  unsigned int accountid = 0;

  SqlStmt *stmt = NULL;

  memset(buf, 0, 255);
  memcpy(buf, n, len);

  stmt = SqlStmt_Malloc(sql_handle);
  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    return 0;
  }

  if (SQL_ERROR == SqlStmt_Prepare(
                       stmt,
                       "SELECT `ChaId` FROM `Character` WHERE `ChaName` = '%s'",
                       buf) ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 0, SQLDT_UINT, &id, 0, NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return 0;
  }

  if (SQL_SUCCESS != SqlStmt_NextRow(stmt)) {
    SqlStmt_Free(stmt);
    return 0;
  }

  if (!id) {
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
    SqlStmt_Free(stmt);
    return 0;
  }

  SqlStmt_Free(stmt);

  return accountid;
}

int maintenance_mode() {
  int flag = 0;
  int nFlag = 0;

  SqlStmt *stmt = NULL;

  stmt = SqlStmt_Malloc(sql_handle);
  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    return 0;
  }

  if (SQL_ERROR == SqlStmt_Prepare(
                       stmt, "SELECT `MaintenanceMode` FROM `Maintenance`") ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 0, SQLDT_INT, &nFlag, 0, NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return 0;
  }

  if (SQL_SUCCESS == SqlStmt_NextRow(stmt)) {
    flag = nFlag;
  }

  SqlStmt_Free(stmt);

  return flag;
}

int maintenance_override(const char *n, int len) {
  char buf[255];
  int flag = 0;
  int nFlag = 0;

  SqlStmt *stmt = NULL;

  memset(buf, 0, 255);
  memcpy(buf, n, len);

  stmt = SqlStmt_Malloc(sql_handle);
  if (stmt == NULL) {
    SqlStmt_ShowDebug(stmt);
    return 0;
  }

  if (SQL_ERROR ==
          SqlStmt_Prepare(
              stmt,
              "SELECT `ChaGMLevel` FROM `Character` WHERE `ChaName` = '%s'",
              buf) ||
      SQL_ERROR == SqlStmt_Execute(stmt) ||
      SQL_ERROR ==
          SqlStmt_BindColumn(stmt, 0, SQLDT_INT, &nFlag, 0, NULL, NULL)) {
    SqlStmt_ShowDebug(stmt);
    SqlStmt_Free(stmt);
    return 0;
  }

  if (SQL_SUCCESS == SqlStmt_NextRow(stmt)) {
    flag = nFlag;
  }

  SqlStmt_Free(stmt);

  return flag;
}

unsigned int metacrc(char *file) {
  FILE *fp = NULL;

  unsigned int checksum = 0;
  unsigned int size = 0;
  Bytef fileinf[196608];
  fp = fopen(file, "rbe");
  if (!fp) {
    return 0;
  }
  fseek(fp, 0, SEEK_END);
  size = ftell(fp);
  fseek(fp, 0, SEEK_SET);
  size_t size_read = fread(fileinf, 1, size, fp);
  fclose(fp);
  checksum = crc32(checksum, fileinf, size_read);

  return checksum;
}

int send_metafile(int fd, char *file) {
  int len = 0;
  unsigned int checksum = 0;
  uLongf clen = 0;
  Bytef *ubuf = NULL;
  Bytef *cbuf = NULL;
  unsigned int ulen = 0;
  char filebuf[255];
  unsigned int retval = 0;
  FILE *fp = NULL;

  sprintf(filebuf, "%s%s", meta_dir, file);

  checksum = metacrc(filebuf);

  fp = fopen(filebuf, "rbe");
  if (!fp) {
    return 0;
  }

  fseek(fp, 0, SEEK_END);
  ulen = ftell(fp);
  fseek(fp, 0, SEEK_SET);
  // CALLOC(ubuf,0,ulen);
  ubuf = calloc(ulen + 1, sizeof(Bytef));
  clen = compressBound(ulen);
  cbuf = calloc(clen + 1, sizeof(Bytef));
  size_t size_read = fread(ubuf, 1, ulen, fp);

  fclose(fp);

  retval = compress(cbuf, &clen, ubuf, size_read);

  if (retval != 0) {
    printf("[login] [send_metafile_error] retval=%d\n", retval);
  }

  WFIFOHEAD(fd, 65535 * 2);
  WFIFOB(fd, 0) = 0xAA;
  WFIFOB(fd, 3) = 0x6F;
  WFIFOB(fd, 5) = 0;  // this is sending file data
  WFIFOB(fd, 6) = strlen(file);
  strcpy(WFIFOP(fd, 7), file);
  len += strlen(file) + 1;
  WFIFOL(fd, len + 6) = SWAP32(checksum);
  len += 4;
  WFIFOW(fd, len + 6) = SWAP16(clen);
  len += 2;
  memcpy(WFIFOP(fd, len + 6), cbuf, clen);
  len += clen;
  WFIFOB(fd, len + 6) = 0;
  len += 1;

  WFIFOW(fd, 1) = SWAP16(len + 3);
  set_packet_indexes((unsigned char *)WFIFOP(fd, 0));
  tk_crypt_static((unsigned char *)WFIFOP(fd, 0));
  WFIFOSET(fd, len + 6 + 3);

  free(cbuf);
  free(ubuf);
  return 0;
}

int send_meta(int fd) {
  char temp[255];

  memset(temp, 0, 255);
  memcpy(temp, RFIFOP(fd, 7), RFIFOB(fd, 6));

  send_metafile(fd, temp);

  return 0;
}

int send_metalist(int fd) {
  int len = 0;
  unsigned int checksum = 0;
  char filebuf[6000];
  int x = 0;

  WFIFOHEAD(fd, 65535 * 2);
  WFIFOB(fd, 0) = 0xAA;
  WFIFOB(fd, 3) = 0x6F;
  WFIFOB(fd, 5) = 1;
  WFIFOW(fd, 6) = SWAP16(metamax);
  len += 2;
  for (x = 0; x < metamax; x++) {
    WFIFOB(fd, (len + 6)) = strlen(meta_file[x]);
    memcpy(WFIFOP(fd, len + 7), meta_file[x], strlen(meta_file[x]));
    len += strlen(meta_file[x]) + 1;
    sprintf(filebuf, "%s%s", meta_dir, meta_file[x]);
    checksum = metacrc(filebuf);
    WFIFOL(fd, len + 6) = SWAP32(checksum);
    len += 4;
  }

  WFIFOW(fd, 1) = SWAP16(len + 4);
  set_packet_indexes((unsigned char *)WFIFOP(fd, 0));
  tk_crypt_static((unsigned char *)WFIFOP(fd, 0));
  WFIFOSET(fd, len + 7 + 3);

  return 0;
}

int clif_debug(unsigned char *stringthing, int len) {
  int i = 0;

  for (i = 0; i < len; i++) {
    printf("%02X ", stringthing[i]);
  }

  printf("\n");

  for (i = 0; i < len; i++) {
    if (stringthing[i] <= 32 || stringthing[i] > 126) {
      printf("   ");
    } else {
      printf("%2c ", stringthing[i]);
    }
  }

  printf("\n");
  return 0;
}

int clif_parse(int fd) {
  unsigned short len = 0;
  unsigned short ver = 0;
  unsigned short deep = 0;
  if (rust_session_get_eof(fd)) {
    session_eof(fd);
    return 0;
  }

  if (!rust_session_get_data(fd)) {
    struct login_session_data *_new_sd = calloc(1, sizeof(struct login_session_data));
    if (!_new_sd) {
      printf("[login] [clif_parse] calloc failed for fd=%d, closing session\n", fd);
      session_eof(fd);
      return 0;
    }
    rust_session_set_data(fd, _new_sd);
  }

  struct login_session_data *sd = rust_session_get_data(fd);

  if (RFIFOB(fd, 0) != 0xAA) {
    int head_err = 0;
    while (RFIFOB(fd, 0) != 0xAA && RFIFOREST(fd)) {
      RFIFOSKIP(fd, 1);
      if (head_err++ > 30) {
        rust_session_set_eof(fd, 1);
        return 0;
      }
    }
  }

  if (RFIFOREST(fd) < 3) {
    return 0;
  }

  len = SWAP16(RFIFOW(fd, 1)) + 3;
  int L = 0;

  char OutStr[1024] = "";
  sprintf(OutStr, "ip=%u.%u.%u.%u length=%i packet=",
          CONVIP(rust_session_get_client_ip(fd)), len);

  char ip[30];
  strcpy(ip, inet_ntoa(*(struct in_addr*)(&(uint32_t){rust_session_get_client_ip(fd)})));

  for (L = 0; L < len; L++) {
    char HexStr[32];
    sprintf(HexStr, "%X ", RFIFOB(fd, L));
    strcat(OutStr, HexStr);
  }

  printf("[login] [packet_in] %s\n", OutStr);

  if (RFIFOREST(fd) < len) {
    {
      return 0;
    }
  }

  tk_crypt_static(RFIFOP(fd, 0));

  switch (RFIFOB(fd, 3)) {
    case 0x00:
      tk_crypt_static(RFIFOP(fd, 0));  // reverse the encryption
      ver = SWAP16(RFIFOW(fd, 4));
      deep = SWAP16(RFIFOW(fd, 7));

      printf("[login] [clif] [0x00] client_version=%d patch=%d\n", ver, deep);

      if (ver == nex_version) {
        WFIFOB(fd, 0) = 0xAA;
        WFIFOB(fd, 1) = 0x00;
        WFIFOB(fd, 2) = 0x11;
        WFIFOB(fd, 3) = 0x00;
        WFIFOB(fd, 4) = 0x00;
        WFIFOB(fd, 5) = 0x27;
        WFIFOB(fd, 6) = 0x4F;
        WFIFOB(fd, 7) = 0x8A;
        WFIFOB(fd, 8) = 0x4A;
        WFIFOB(fd, 9) = 0x00;
        // added stuff for baram?
        WFIFOB(fd, 10) = 0x09;
        strcpy(WFIFOP(fd, 11), xor_key);
        // set_packet_indexes(WFIFOP(fd, 0));
        WFIFOSET(fd, 20);
      } else {
        printf("[login] [patching] version=%d, client_version=%d\n",
               nex_version, ver);
        WFIFOB(fd, 0) = 0xAA;
        WFIFOW(fd, 1) = SWAP16(0x29);
        WFIFOB(fd, 3) = 0;
        WFIFOB(fd, 4) = 2;
        WFIFOW(fd, 5) = SWAP16(nex_version);
        WFIFOB(fd, 7) = 1;
        WFIFOB(fd, 8) = 0x23;
        strcpy(WFIFOP(fd, 9), "http://www.google.com");
        set_packet_indexes((unsigned char *)WFIFOP(fd, 0));
        WFIFOSET(fd, 44 + 3);
        // session[fd]->eof=1;
      }
      break;
    case 0x02:

      memset(sd->name, 0, 16);
      memset(sd->pass, 0, 16);

      if ((RFIFOB(fd, 5) > 12) || (RFIFOB(fd, 5) < 3) ||
          string_check_allchars((char *)RFIFOP(fd, 6), RFIFOB(fd, 5))) {
        clif_message(fd, 0x03, login_msg[LGN_ERRUSER]);
        break;
      }

      if ((RFIFOB(fd, 6 + RFIFOB(fd, 5)) > 8) ||
          (RFIFOB(fd, 6 + RFIFOB(fd, 5)) < 3) ||
          string_check((char *)RFIFOP(fd, 7 + RFIFOB(fd, 5)),
                       RFIFOB(fd, 6 + RFIFOB(fd, 5)))) {
        clif_message(fd, 0x05, login_msg[LGN_ERRPASS]);
        break;
      }

      memcpy(sd->name, RFIFOP(fd, 6), RFIFOB(fd, 5));
      memcpy(sd->pass, RFIFOP(fd, 7 + RFIFOB(fd, 5)),
             RFIFOB(fd, 6 + RFIFOB(fd, 5)));

      if (strlen(sd->pass) > 8 || strlen(sd->pass) < 3) {
        clif_message(fd, 0x05, login_msg[LGN_ERRPASS]);
        break;
      }

      if (char_fd) {
        WFIFOHEAD(char_fd, 20);
        WFIFOW(char_fd, 0) = 0x1001;
        WFIFOW(char_fd, 2) = fd;
        memcpy(WFIFOP(char_fd, 4), sd->name, 16);
        WFIFOSET(char_fd, 20);
      } else {
        clif_message(fd, 0x03, login_msg[LGN_ERRDB]);
      }
      break;
    case 0x03:

      if (string_check((char *)RFIFOP(fd, 6), RFIFOB(fd, 5))) {
        clif_message(fd, 0x03, login_msg[LGN_ERRUSER]);
        break;
      }

      if (string_check((char *)RFIFOP(fd, 7 + RFIFOB(fd, 5)),
                       RFIFOB(fd, 6 + RFIFOB(fd, 5)))) {
        clif_message(fd, 0x05, login_msg[LGN_ERRPASS]);
        break;
      }

      if (maintenance_mode()) {
        if (maintenance_override((char *)RFIFOP(fd, 6), RFIFOB(fd, 5)) == 0) {
          clif_message(fd, 0x03,
                       "Server is undergoing maintenance. Please visit "
                       "www.website.com "
                       "or the facebook group for more details.");
          break;
        }
      }

      if (require_reg) {
        if (!reg_check((char *)RFIFOP(fd, 6), RFIFOB(fd, 5))) {
          char name[16];
          strncpy(name, (char *)RFIFOP(fd, 6), RFIFOB(fd, 5));

          printf("[login] [require_reg] name=%s ip=%s\n", name, ip);

          clif_message(fd, 0x03,
                       "You must attach your character to an account to "
                       "play.\n\nPlease visit www.website.com to attach your "
                       "character to an account. If you have issues, please "
                       "visit https://www.website.com/helpdesk or visit our "
                       "Discord (link on website).");
          break;
        }
      }

      memcpy(sd->name, RFIFOP(fd, 6), RFIFOB(fd, 5));
      char NameCheck[16];
      strcpy(NameCheck, sd->name);
      strlwr(NameCheck);

      memcpy(sd->pass, RFIFOP(fd, 7 + RFIFOB(fd, 5)),
             RFIFOB(fd, 6 + RFIFOB(fd, 5)));

      if (char_fd) {
        WFIFOHEAD(char_fd, 40);
        WFIFOW(char_fd, 0) = 0x1003;
        WFIFOW(char_fd, 2) = fd;
        memset(WFIFOP(char_fd, 4), 0, 16 * 2);
        memcpy(WFIFOP(char_fd, 4), RFIFOP(fd, 6), RFIFOB(fd, 5));
        memcpy(WFIFOP(char_fd, 20), RFIFOP(fd, 7 + RFIFOB(fd, 5)),
               RFIFOB(fd, 6 + RFIFOB(fd, 5)));
        WFIFOL(char_fd, 36) = rust_session_get_client_ip(fd);
        WFIFOSET(char_fd, 40);
      } else {
        clif_message(fd, 0x03, login_msg[LGN_ERRDB]);
      }
      break;

    case 0x04:

      if (!strlen(sd->name) || !strlen(sd->pass)) {
        rust_session_set_eof(fd, 1);
      }

      sd->face = RFIFOB(fd, 6);
      sd->sex = RFIFOB(fd, 10);

      int startCountry = rand() % 2;

      sd->country = startCountry;

      sd->totem = RFIFOB(fd, 12);
      sd->hair = RFIFOB(fd, 7);
      sd->face_color = RFIFOB(fd, 8);
      sd->hair_color = RFIFOB(fd, 9);

      if (char_fd) {
        WFIFOHEAD(char_fd, 43);
        WFIFOW(char_fd, 0) = 0x1002;
        WFIFOW(char_fd, 2) = fd;
        memcpy(WFIFOP(char_fd, 4), sd->name, 16);
        memcpy(WFIFOP(char_fd, 20), sd->pass, 16);
        WFIFOB(char_fd, 36) = sd->face;
        WFIFOB(char_fd, 37) = sd->sex;
        WFIFOB(char_fd, 38) = sd->country;
        WFIFOB(char_fd, 39) = sd->totem;
        WFIFOB(char_fd, 40) = sd->hair;
        WFIFOB(char_fd, 41) = sd->hair_color;
        WFIFOB(char_fd, 42) = sd->face_color;
        WFIFOSET(char_fd, 43);
      } else {
        clif_message(fd, 0x03, login_msg[LGN_ERRDB]);
      }
      break;
    case 0x10:
      WFIFOHEAD(fd, 13);
      WFIFOB(fd, 0) = 0xAA;
      WFIFOW(fd, 1) = SWAP16(0x07);
      WFIFOB(fd, 3) = 0x60;
      WFIFOB(fd, 4) = 0x00;
      WFIFOB(fd, 5) = 0x55;
      WFIFOB(fd, 6) = 0xE0;
      WFIFOB(fd, 7) = 0xD8;
      WFIFOB(fd, 8) = 0xA2;
      WFIFOB(fd, 9) = 0xA0;
      set_packet_indexes((unsigned char *)WFIFOP(fd, 0));
      WFIFOSET(fd, 13);
      break;
    case 0x26:

      if (string_check((char *)RFIFOP(fd, 6), RFIFOB(fd, 5))) {
        clif_message(fd, 0x03, login_msg[LGN_ERRUSER]);
        break;
      }

      if (RFIFOB(fd, 7 + RFIFOB(fd, 5) + RFIFOB(fd, 6 + RFIFOB(fd, 5))) > 8 ||
          RFIFOB(fd, 7 + RFIFOB(fd, 5) + RFIFOB(fd, 6 + RFIFOB(fd, 5))) < 3) {
        clif_message(fd, 0x05, login_msg[LGN_ERRPASS]);
        break;
      }

      if (RFIFOB(fd, 5) > 16) {
        {
          return 0;
        }
      }
      if (RFIFOB(fd, 6 + RFIFOB(fd, 5)) > 16) {
        {
          return 0;
        }
      }

      if (string_check((char *)RFIFOP(fd, 7 + RFIFOB(fd, 5)),
                       RFIFOB(fd, 6 + RFIFOB(fd, 5))) ||
          string_check(
              (char *)RFIFOP(fd,
                             8 + RFIFOB(fd, 5) + RFIFOB(fd, 6 + RFIFOB(fd, 5))),
              RFIFOB(fd, 7 + RFIFOB(fd, 5) + RFIFOB(fd, 6 + RFIFOB(fd, 5))))) {
        clif_message(fd, 0x05, login_msg[LGN_ERRPASS]);
        break;
      }
      if (char_fd) {
        WFIFOHEAD(char_fd, 52);
        WFIFOW(char_fd, 0) = 0x1004;
        WFIFOW(char_fd, 2) = fd;
        memset(WFIFOP(char_fd, 4), 0, 16 * 3);
        memcpy(WFIFOP(char_fd, 4), RFIFOP(fd, 6), RFIFOB(fd, 5));
        memcpy(WFIFOP(char_fd, 20), RFIFOP(fd, 7 + RFIFOB(fd, 5)),
               RFIFOB(fd, 6 + RFIFOB(fd, 5)));
        memcpy(WFIFOP(char_fd, 36),
               RFIFOP(fd, 8 + RFIFOB(fd, 5) + RFIFOB(fd, 6 + RFIFOB(fd, 5))),
               RFIFOB(fd, 7 + RFIFOB(fd, 5) + RFIFOB(fd, 6 + RFIFOB(fd, 5))));
        WFIFOSET(char_fd, 52);
      } else {
        clif_message(fd, 0x03, login_msg[LGN_ERRDB]);
      }
      break;
    case 0x57:  // multi-server?
      break;
    case 0x71:  // ping me!
      break;
    case 0x7B:  // Request Item Information!
      switch (RFIFOB(fd, 5)) {
        case 0:  // Request the file asking for
          send_meta(fd);
          break;
        case 1:  // Requqest the list to use
          send_metalist(fd);

          break;
      }
      break;
    case 0x62:  //??
      break;
    case 0xFF:
      intif_auth(fd);
      break;
    default:
      printf("[login] [packet_unknown] id=%02X ip=%s:\n", RFIFOB(fd, 3),
             inet_ntoa(*(struct in_addr*)(&(uint32_t){rust_session_get_client_ip(fd)})));
      clif_debug(RFIFOP(fd, 0), SWAP16(RFIFOW(fd, 1)));
      break;
  }

  RFIFOSKIP(fd, len);
  return 0;
}
