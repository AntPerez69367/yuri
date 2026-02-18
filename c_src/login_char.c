#include "login_char.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "config.h"
#include "db_mysql.h"
#include "login_parse.h"
#include "login_server.h"
#include "mmo.h"
#include "net_crypt.h"
#include "session.h"

static int packet_len_table[] = {69, 5, 5, 27, 5, 0};

int intif_debug(int fd, int len) {
  int x = 0;
  for (x = 0; x < len; x++) {
    printf("%u ", RFIFOB(fd, x));
  }
  printf("\n");
  return 0;
}
int intif_auth(int fd) {
  if (char_fd > 0) {
    session_eof(fd);
    return 0;
  }
  int packet_len = 69;
  if (RFIFOREST(fd) < packet_len) {
    return 0;
  }

  if ((strcmp((char *)RFIFOP(fd, 5), login_id) != 0) ||
      (strcmp((char *)RFIFOP(fd, 37), login_pw) != 0)) {
    WFIFOHEAD(fd, 3);
    WFIFOW(fd, 0) = 0x1000;
    WFIFOB(fd, 2) = 0x01;
    WFIFOSET(fd, 3);
    rust_session_set_eof(fd, 1);
    return 0;
  }

  char_fd = fd;

  rust_session_set_parse(char_fd, intif_parse);
  realloc_rfifo(char_fd, FIFOSIZE_SERVER, FIFOSIZE_SERVER);
  WFIFOHEAD(char_fd, 3);
  WFIFOW(char_fd, 0) = 0x1000;
  WFIFOB(char_fd, 2) = 0x00;
  WFIFOSET(char_fd, 3);

  printf(
      "[login] [char_server_connect] Connection from Char Server accepted.\n");
  return 0;
}
int intif_parse_2001(int fd) {
  if (!rust_session_exists(RFIFOW(fd, 2))) {
    return 0;
  }

  if (RFIFOB(fd, 4) == 0x01) {
    clif_message(RFIFOW(fd, 2), 0x03, login_msg[LGN_USEREXIST]);
  } else if (RFIFOB(fd, 4)) {
    clif_message(RFIFOW(fd, 2), 0x03, login_msg[LGN_ERRDB]);
  } else {
    clif_message(RFIFOW(fd, 2), 0x00, "\x00");
  }

  return 0;
}
int intif_parse_2002(int fd) {
  if (!rust_session_exists(RFIFOW(fd, 2))) {
    return 0;
  }

  if (RFIFOB(fd, 4) == 0x01) {
    clif_message(RFIFOW(fd, 2), 0x03, login_msg[LGN_USEREXIST]);
  } else if (RFIFOB(fd, 4)) {
    clif_message(RFIFOW(fd, 2), 0x03, login_msg[LGN_ERRDB]);
  } else {
    clif_message(RFIFOW(fd, 2), 0x00, login_msg[LGN_NEWCHAR]);
  }

  return 0;
}
int intif_parse_connectconfirm(int fd) {
  if (!rust_session_exists(RFIFOW(fd, 2))) {
    return 0;
  }

  struct login_session_data *sd = rust_session_get_data(RFIFOW(fd, 2));

  if (RFIFOB(fd, 4) == 0) {
    printf("[login] [auth_success] name=%s ip=%u.%u.%u.%u\n", sd->name,
           CONVIP(rust_session_get_client_ip(RFIFOW(fd, 2))));

    char ipaddress[16] = "";

    sprintf(ipaddress, "%u.%u.%u.%u",
            CONVIP(rust_session_get_client_ip(RFIFOW(fd, 2))));

    SqlStmt *stmt = NULL;
    stmt = SqlStmt_Malloc(sql_handle);
    if (stmt == NULL) {
      SqlStmt_ShowDebug(stmt);
      return 0;
    }

    if (SQL_ERROR ==
        Sql_Query(
            sql_handle,
            "UPDATE `Character` SET `ChaLastIP` = '%s' WHERE `ChaName` = '%s'",
            ipaddress, sd->name)) {
      Sql_ShowDebug(sql_handle);
      return 0;
    }

    WFIFOHEAD(RFIFOW(fd, 2), 8);
    WFIFOB(RFIFOW(fd, 2), 0) = '\xAA';
    WFIFOB(RFIFOW(fd, 2), 1) = '\x00';
    WFIFOB(RFIFOW(fd, 2), 2) = '\x05';
    WFIFOB(RFIFOW(fd, 2), 3) = '\x02';
    WFIFOB(RFIFOW(fd, 2), 4) = '\x17';
    WFIFOB(RFIFOW(fd, 2), 5) = '\x00';
    WFIFOB(RFIFOW(fd, 2), 6) = '\x00';
    WFIFOB(RFIFOW(fd, 2), 7) = '\x00';
    set_packet_indexes((unsigned char *)WFIFOP(RFIFOW(fd, 2), 0));
    tk_crypt_static((unsigned char *)WFIFOP(RFIFOW(fd, 2), 0));
    WFIFOSET(RFIFOW(fd, 2), 8 + 3);
    int len = 0;
    char *thing = NULL;

    WFIFOHEAD(RFIFOW(fd, 2), 23 + 255);
    WFIFOB(RFIFOW(fd, 2), 0) = '\xAA';
    WFIFOB(RFIFOW(fd, 2), 3) = '\x03';
    WFIFOL(RFIFOW(fd, 2), 4) = SWAP32(RFIFOL(fd, 21));
    WFIFOW(RFIFOW(fd, 2), 8) = SWAP16(RFIFOW(fd, 25));

    len = strlen((char *)RFIFOP(fd, 5)) + 16;
    thing = (char *)RFIFOP(fd, 5);
    WFIFOB(RFIFOW(fd, 2), 10) = len;

    WFIFOW(RFIFOW(fd, 2), 11) = SWAP16(9);

    strcpy(WFIFOP(RFIFOW(fd, 2), 13), xor_key);
    WFIFOB(RFIFOW(fd, 2), 22) = strlen(thing);
    strcpy(WFIFOP(RFIFOW(fd, 2), 23), thing);
    WFIFOL(RFIFOW(fd, 2), 23 + strlen(thing)) = SWAP32(RFIFOW(fd, 2));

    WFIFOW(RFIFOW(fd, 2), 1) = SWAP16(len + 8);
    set_packet_indexes((unsigned char *)WFIFOP(RFIFOW(fd, 2), 0));
    WFIFOSET(RFIFOW(fd, 2), 11 + len + 3);
  } else if (RFIFOB(fd, 4) == 0x01) {
    clif_message(RFIFOW(fd, 2), 0x03, login_msg[LGN_ERRDB]);
  } else if (RFIFOB(fd, 4) == 0x02) {
    clif_message(RFIFOW(fd, 2), 0x03, login_msg[LGN_WRONGUSER]);
  } else if (RFIFOB(fd, 4) == 0x03) {
    printf("[login] [auth_failure] name=%s ip=%u.%u.%u.%u\n", sd->name,
           CONVIP(rust_session_get_client_ip(RFIFOW(fd, 2))));
    if (setInvalidCount(rust_session_get_client_ip(RFIFOW(fd, 2))) >=
        10) {
      rust_add_ip_lockout(rust_session_get_client_ip(RFIFOW(fd, 2)));
      rust_session_set_eof(RFIFOW(fd, 2), 1);
    }
    clif_message(RFIFOW(fd, 2), 0x03, login_msg[LGN_WRONGPASS]);
  } else if (RFIFOB(fd, 4) == 0x04) {
    clif_message(RFIFOW(fd, 2), 0x03, login_msg[LGN_BANNED]);
  } else if (RFIFOB(fd, 4) == 0x05) {
    clif_message(RFIFOW(fd, 2), 0x03, login_msg[LGN_ERRSERVER]);
  } else if (RFIFOB(fd, 4) == 0x06) {
    clif_message(RFIFOW(fd, 2), 0x03, login_msg[LGN_DBLLOGIN]);
  } else {
    printf("[login] [internal_error] intif_parse_connectconfirm failure\n");
  }

  return 0;
}

int intif_parse_changepass(int fd) {
  if (!rust_session_exists(RFIFOW(fd, 2))) {
    {
      return 0;
    }
  }

  if (!RFIFOB(fd, 4)) {
    printf("[login] [pass_change_success] ip=%u.%u.%u.%u\n",
           CONVIP(rust_session_get_client_ip(RFIFOW(fd, 2))));
    clif_message(RFIFOW(fd, 2), 0x00, login_msg[LGN_CHGPASS]);
  } else if (RFIFOB(fd, 4) == 0x01) {
    clif_message(RFIFOW(fd, 2), 0x03, login_msg[LGN_ERRDB]);
  } else if (RFIFOB(fd, 4) == 0x02) {
    clif_message(RFIFOW(fd, 2), 0x03, login_msg[LGN_WRONGUSER]);

  } else if (RFIFOB(fd, 4) == 0x03) {
    printf("[login] [pass_change_failure] ip=%u.%u.%u.%u\n",
           CONVIP(rust_session_get_client_ip(RFIFOW(fd, 2))));
    if (setInvalidCount(rust_session_get_client_ip(RFIFOW(fd, 2))) >=
        10) {
      rust_add_ip_lockout(rust_session_get_client_ip(RFIFOW(fd, 2)));
      rust_session_set_eof(RFIFOW(fd, 2), 1);
    }
    clif_message(RFIFOW(fd, 2), 0x03, login_msg[LGN_WRONGPASS]);
  }
  return 0;
}
int intif_parse(int fd) {
  int cmd = 0;
  int packet_len = 0;

  if (rust_session_get_eof(fd)) {
    printf("[login] [char_server_disconnect] Char Server connection lost.\n");
    char_fd = 0;
    session_eof(fd);
    return 0;
  }

  if (RFIFOB(fd, 0) == 0xAA) {
    int len = SWAP16(RFIFOW(fd, 1)) + 3;
    if (len <= RFIFOREST(fd)) {
      {
        RFIFOSKIP(fd, len);
      }
    }
    return 0;
  }

  cmd = RFIFOW(fd, 0);

  if (cmd < 0x2000 ||
      cmd >=
          0x2000 + (sizeof(packet_len_table) / sizeof(packet_len_table[0])) ||
      packet_len_table[cmd - 0x2000] == 0) {
    return 0;
  }

  packet_len = packet_len_table[cmd - 0x2000];

  if (packet_len == -1) {
    if (RFIFOREST(fd) < 6) {
      return 2;
    }
    packet_len = RFIFOL(fd, 2);
  }
  if ((int)RFIFOREST(fd) < packet_len) {
    return 2;
  }
  // printf("LOGIN: %d\n",cmd);
  switch (cmd) {
    case 0x2001:
      intif_parse_2001(fd);
      break;
    case 0x2002:
      intif_parse_2002(fd);
      break;
    case 0x2003:
      intif_parse_connectconfirm(fd);
      break;
    case 0x2004:
      intif_parse_changepass(fd);
      break;

    default:
      break;
  }

  RFIFOSKIP(fd, packet_len);
  return 0;
}
