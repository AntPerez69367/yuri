#include "yuri.h"
#include "login_server.h"

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <zlib.h>

#include "config.h"
#include "core.h"
#include "db.h"
#include "db_mysql.h"
#include "login_parse.h"
#include "net_crypt.h"
#include "session.h"
#include "timer.h"

int login_fd;
int char_fd;
char login_msg[MSG_MAX][256];
Sql *sql_handle = NULL;
DBMap *bf_lockout = NULL;

const char mask1[] = "aAbBcCdDeEfFgGhHiIjJkKlLmMnNoOpPqQrRsStTuUvVwWxXyYzZ";
const char mask2[] =
    "aAbBcCdDeEfFgGhHiIjJkKlLmMnNoOpPqQrRsStTuUvVwWxXyYzZ1234567890";

int Valid(const char *buf, const char *mask) {
  int x = 0;
  int n = 0;
  int found = 0;

  for (x = 0; x < strlen(buf); x++) {
    for (n = 0; n < strlen(mask); n++) {
      if (buf[x] == mask[n]) {
        found = 1;
      }
    }
    if (!found) {
      return 1;
    }
    found = 0;
  }

  return 0;
}

int string_check_allchars(const char *p, int len) {
  char buf[255];
  memset(buf, 0, 255);
  memcpy(buf, p, len);
  return Valid(buf, mask1);
}

int string_check(const char *p, int len) {
  char buf[255];
  memset(buf, 0, 255);
  memcpy(buf, p, len);
  return Valid(buf, mask2);
}

int lang_read(const char *cfg_file) {
  char line[1024];
  char r1[1024];
  char r2[1024];
  int line_num = 0;
  FILE *fp = NULL;

  fp = fopen(cfg_file, "re");
  if (fp == NULL) {
    printf("[login] [lang_read_failure]: Language file (%s) not found.\n",
           cfg_file);
    return 1;
  }

  while (fgets(line, sizeof(line), fp)) {
    line_num++;
    if (line[0] == '/' && line[1] == '/') {
      continue;
    }

    if (sscanf(line, "%[^:]: %[^\r\n]", r1, r2) == 2) {
      if (strcasecmp(r1, "LGN_ERRSERVER") == 0) {
        strncpy(login_msg[LGN_ERRSERVER], r2, 256);
        login_msg[LGN_ERRSERVER][255] = '\0';
      } else if (strcasecmp(r1, "LGN_WRONGPASS") == 0) {
        strncpy(login_msg[LGN_WRONGPASS], r2, 256);
        login_msg[LGN_WRONGPASS][255] = '\0';
      } else if (strcasecmp(r1, "LGN_WRONGUSER") == 0) {
        strncpy(login_msg[LGN_WRONGUSER], r2, 256);
        login_msg[LGN_WRONGUSER][255] = '\0';
      } else if (strcasecmp(r1, "LGN_ERRDB") == 0) {
        strncpy(login_msg[LGN_ERRDB], r2, 256);
        login_msg[LGN_ERRDB][255] = '\0';
      } else if (strcasecmp(r1, "LGN_USEREXIST") == 0) {
        strncpy(login_msg[LGN_USEREXIST], r2, 256);
        login_msg[LGN_USEREXIST][255] = '\0';
      } else if (strcasecmp(r1, "LGN_ERRPASS") == 0) {
        strncpy(login_msg[LGN_ERRPASS], r2, 256);
        login_msg[LGN_ERRPASS][255] = '\0';
      } else if (strcasecmp(r1, "LGN_ERRUSER") == 0) {
        strncpy(login_msg[LGN_ERRUSER], r2, 256);
        login_msg[LGN_ERRUSER][255] = '\0';
      } else if (strcasecmp(r1, "LGN_NEWCHAR") == 0) {
        strncpy(login_msg[LGN_NEWCHAR], r2, 256);
        login_msg[LGN_NEWCHAR][255] = '\0';
      } else if (strcasecmp(r1, "LGN_CHGPASS") == 0) {
        strncpy(login_msg[LGN_CHGPASS], r2, 256);
        login_msg[LGN_CHGPASS][255] = '\0';
      } else if (strcasecmp(r1, "LGN_DBLLOGIN") == 0) {
        strncpy(login_msg[LGN_DBLLOGIN], r2, 256);
        login_msg[LGN_DBLLOGIN][255] = '\0';
      } else if (strcasecmp(r1, "LGN_BANNED") == 0) {
        strncpy(login_msg[LGN_BANNED], r2, 256);
        login_msg[LGN_BANNED][255] = '\0';
      }
    }
  }
  fclose(fp);
  printf("[login] [lang_read_sucess] file=%s\n", cfg_file);
  return 0;
}

void do_term(void) { printf("[login] [shutdown]\n"); }

void help_screen() {
  printf("HELP LIST\n");
  printf("---------\n");
  printf(" --conf [FILENAME]  : set config file\n");
  printf(" --lang [FILENAME]  : set lang file\n");
  exit(0);
}

int do_init(int argc, char **argv) {
  int i = 0;
  char *CONF_FILE = "conf/server.yaml";
  char *LANG_FILE = "conf/lang.yaml";

  srand(gettick());

  for (i = 1; i < argc; i++) {
    if (strcmp(argv[i], "--help") == 0 || strcmp(argv[i], "--h") == 0 ||
        strcmp(argv[i], "--?") == 0 || strcmp(argv[i], "/?") == 0) {
      help_screen();
    } else if (strcmp(argv[i], "--conf") == 0) {
      CONF_FILE = argv[i + 1];
    } else if (strcmp(argv[i], "--lang") == 0) {
      LANG_FILE = argv[i + 1];
    }
  }

  if (rust_config_read(CONF_FILE) != 0) {
    printf("[login] [config_error] %s\n", CONF_FILE);
    exit(EXIT_FAILURE);
  }
  sql_handle = Sql_Malloc();
  if (sql_handle == NULL) {
    Sql_ShowDebug(sql_handle);
    exit(EXIT_FAILURE);
  }
  if (SQL_ERROR == Sql_Connect(sql_handle, sql_id, sql_pw, sql_ip,
                               (uint16_t)sql_port, sql_db)) {
    printf("[login] [sql_connect_error] id=%s port=%d\n", sql_id, sql_port);
    Sql_ShowDebug(sql_handle);
    Sql_Free(sql_handle);
    exit(EXIT_FAILURE);
  }

  // sql_init();
  lang_read(LANG_FILE);
  set_termfunc(do_term);
  printf("[login] [started] Login Server Started\n");

  set_defaultaccept(clif_accept);
  set_defaultparse(clif_parse);
  login_fd = make_listen_port(login_port);
  timer_insert(10 * 60 * 1000, 10 * 60 * 1000, Remove_Throttle, 0, 0);
  // Lockout DB
  bf_lockout = uidb_alloc(DB_OPT_BASE);

  printf("[login] [ready] port=%d\n", login_port);
  return 0;
}

// These are all unsafe and weird -  uidb expects pointer value, but we're
// trying to give it a uint.
int getInvalidCount(unsigned int ip) {
  unsigned int c = (uintptr_t)uidb_get(bf_lockout, ip);
  return c;
}

int login_clear_lockout(int i, int d) {
  uidb_remove(bf_lockout, (uintptr_t)i);
  return 1;
}
int setInvalidCount(unsigned int ip) {
  unsigned int c = (uintptr_t)uidb_get(bf_lockout, ip);

  if (!c) {
    timer_insert(10 * 60 * 1000, 10 * 60 * 1000, login_clear_lockout, ip, 0);
  }

  uidb_put(bf_lockout, ip, (void *)(uintptr_t)(c + 1));

  return c + 1;
}