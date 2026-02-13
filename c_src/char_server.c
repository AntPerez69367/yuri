#include "yuri.h"
#include "char_server.h"

#include <arpa/inet.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <zlib.h>

#include "char_db.h"
#include "char_login.h"
#include "char_map.h"
#include "config.h"
#include "core.h"
#include "db.h"
#include "mmo.h"
#include "session.h"
#include "timer.h"

struct DBMap *login_data;
struct mmo_charstatus *char_dat;
struct char_login_data *clogin_dat;
struct map_fifo_data map_fifo[MAX_MAP_SERVER];

int map_fifo_n = 0;
int map_fifo_max = 0;
struct Sql *sql_handle;
int char_fd;
int login_fd;

int zlib_init(void) {
  z_stream strm;
  int ret;
  strm.zalloc = Z_NULL;
  strm.zfree = Z_NULL;
  strm.opaque = Z_NULL;
  ret = deflateInit(&strm, Z_DEFAULT_COMPRESSION);
  return ret;
}
int mapfifo_from_mapid(int map) {
  int i, j;
  for (i = 0; i < map_fifo_max; i++) {
    if (map_fifo[i].fd > 0) {
      for (j = 0; j < map_fifo[i].map_n; j++) {
        if (map_fifo[i].map[j] == map) {
          return i;
        }
      }
    }
  }
  return -1;
}

/*==========================================
 * Login Data
 *  Register online character via memory
 *  and sql. But we prefer to use memory
 *  for fast and accurate result.
 *------------------------------------------
 */
int logindata_init() {
  login_data = uidb_alloc(DB_OPT_BASE);
  // mmo_setallonline(0);
  return 0;
}

int logindata_final(void *key, void *data, va_list ap) {
  struct item_data *dat;
  nullpo_ret(0, dat = data);
  FREE(dat);
  return 0;
}

int logindata_term() {
  if (login_data) {
    // numdb_final(login_data, logindata_final);
    login_data = NULL;
  }
  // mmo_setallonline(0);
  return 0;
}

struct char_login_data *logindata_search(unsigned int id) {
  struct char_login_data *dat;
  dat = (struct char_login_data *)uidb_get(login_data, id);
  if (!dat) {
    return NULL;
  }

  return dat;
}

/*int logindata_del(unsigned int id) {
        struct char_login_data *dat;
        dat = numdb_search(login_data, id);
        //mmo_setonline(id,0);

        if (!dat)
                return 0;

        FREE(dat);
        numdb_erase(login_data, id);
        //mmo_setonline(id, 0);
        return 0;
}*/

int logindata_change(unsigned int id, int map_server) {
  struct char_login_data *dat;
  dat = (struct char_login_data *)uidb_get(login_data, id);
  if (!dat) {
    return -1;
  }

  dat->map_server = map_server;
  return 0;
}

void do_term() {
  logindata_term();
  char_db_term();
  session_eof(login_fd);
  printf("[char] [shutdown] Char Server Shutdown.\n");
}

void help_screen() {
  printf("HELP LIST\n");
  printf("---------\n");
  printf(" --conf [FILENAME]  : set config file\n");
  exit(0);
}
int keep_login_alive(int n, int a) {
  if (login_fd) {
    WFIFOB(login_fd, 0) = 0xFF;
    WFIFOB(login_fd, 1) = 1;
    WFIFOSET(login_fd, 2);
  }
  return 0;
}
int do_init(int argc, char **argv) {
  int i;
  char *CONF_FILE = "conf/server.yaml";

  srand(gettick());

  for (i = 1; i < argc; i++) {
    if (strcmp(argv[i], "--help") == 0 || strcmp(argv[i], "--h") == 0 ||
        strcmp(argv[i], "--?") == 0 || strcmp(argv[i], "/?") == 0) {
      help_screen();
    } else if (strcmp(argv[i], "--conf") == 0) {
      CONF_FILE = argv[i + 1];
    }
  }

  if (rust_config_read(CONF_FILE) != 0) {
    fprintf(stderr, "[char] [error] Failed to load configuration from %s\n", CONF_FILE);
    fprintf(stderr, "[char] [error] Aborting startup - fix configuration and retry\n");
    exit(EXIT_FAILURE);
  }

  set_termfunc(do_term);

  printf("[char] [started] Char Server Started.\n");

  char_db_init();
  logindata_init();

  set_defaultparse(mapif_parse_auth);
  char_fd = make_listen_port(char_port);

  timer_insert(1000, 1000 * 10, check_connect_login, login_ip, login_port);

  CALLOC(char_dat, struct mmo_charstatus, 1);

  printf("[char] [ready] port=%d\n", char_port);

  zlib_init();
  return 0;
}
