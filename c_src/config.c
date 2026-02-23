#include "config.h"

#include <arpa/inet.h>
#include <netdb.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "map_server.h"

char xor_key[10] = "";  // 9 chars + null terminator

struct point start_pos;

int char_ip;
int char_port = 2005;

char char_id[33] = "";
char char_pw[33] = "";

char login_id[33] = "";
char login_pw[33] = "";

int login_ip;
int login_port = 2010;

int require_reg = 1;
int nex_version;
int nex_deep;
char meta_file[META_MAX][256];
int metamax;

unsigned int map_ip;
unsigned int map_port;

int serverid;
int save_time = 60000;
int xp_rate;
int d_rate;
struct town_data towns[255];
int town_n = 0;

char sql_id[32] = "";
char sql_pw[32] = "";
char sql_db[32] = "";
char sql_ip[32] = "";
int sql_port = 3306;

char *data_dir = "./data/";
char *lua_dir = "./data/lua/";
char *maps_dir = "./data/maps/";
char *meta_dir = "./data/meta/";

// int get_actual_ip(const char *addr, char *buffer) {
//   struct hostent *he;
//   struct in_addr a;

//   he = gethostbyname(addr);
//   if (he) {
//     while (*he->h_addr_list) {
//       bcopy(*he->h_addr_list++, (char *)&a, sizeof(a));
//       strncpy(buffer, inet_ntoa(a), 16);
//     }
//   }

//   return 0;
// }

// int add_meta(const char *file) {
//   strcpy(meta_file[metamax], file);
//   metamax++;
//   return 0;
// }

// int add_town(const char *line) {
//   char town_name[1024];

//   printf("[config] [add_town] name=%s\n", line);
//   if (sscanf(line, "%[^\r\n]", town_name) != 1) return -1;
//   memset(towns[town_n].name, 0, 32);
//   strncpy(towns[town_n].name, town_name, 32);
//   town_n++;

//   return 0;
// }

// int config_read(const char *cfg_file) {
//   char line[1024], r1[1024], r2[1024];
//   char login_ip_s[16];
//   char char_ip_s[16];
//   char map_ip_s[16];

//   int line_num = 0;
//   FILE *fp;

//   fp = fopen(cfg_file, "r");
//   if (fp == NULL) {
//     printf("[config] [read_failure] File (%s) not found.\n", cfg_file);
//     exit(1);
//   }

//   while (fgets(line, sizeof(line), fp)) {
//     line_num++;
//     if (line[0] == '/' && line[1] == '/') {
//       continue;
//     }

//     if (sscanf(line, "%[^:]: %[^\r\n]", r1, r2) == 2) {
//       if (strcasecmp(r1, "char_ip") == 0) {
//         strncpy(char_ip_s, r2, 16);
//         char_ip_s[15] = '\0';
//         char_ip = inet_addr(char_ip_s);
//       } else if (strcasecmp(r1, "char_port") == 0) {
//         char_port = atoi(r2);
//       } else if (strcasecmp(r1, "char_id") == 0) {
//         strncpy(char_id, r2, 32);
//         char_id[31] = '\0';
//       } else if (strcasecmp(r1, "char_pw") == 0) {
//         strncpy(char_pw, r2, 32);
//         char_pw[31] = '\0';
//       } else if (strcasecmp(r1, "login_ip") == 0) {
//         strncpy(login_ip_s, r2, 16);
//         login_ip_s[15] = '\0';
//         login_ip = inet_addr(login_ip_s);
//       } else if (strcasecmp(r1, "login_port") == 0) {
//         login_port = atoi(r2);
//       } else if (strcasecmp(r1, "login_id") == 0) {
//         strncpy(login_id, r2, 32);
//         login_id[31] = '\0';
//       } else if (strcasecmp(r1, "login_pw") == 0) {
//         strncpy(login_pw, r2, 32);
//         login_pw[31] = '\0';
//       } else if (strcasecmp(r1, "map_ip") == 0) {
//         get_actual_ip(r2, map_ip_s);
//         map_ip_s[15] = '\0';
//         map_ip = inet_addr(map_ip_s);
//       } else if (strcasecmp(r1, "map_port") == 0) {
//         map_port = atoi(r2);
//       } else if (strcasecmp(r1, "sql_ip") == 0) {
//         strcpy(sql_ip, r2);
//       } else if (strcasecmp(r1, "sql_port") == 0) {
//         sql_port = atoi(r2);
//       } else if (strcasecmp(r1, "sql_id") == 0) {
//         strcpy(sql_id, r2);
//       } else if (strcasecmp(r1, "sql_pw") == 0) {
//         strcpy(sql_pw, r2);
//       } else if (strcasecmp(r1, "sql_db") == 0) {
//         strcpy(sql_db, r2);
//       } else if (strcasecmp(r1, "start_point") == 1) {
//         sscanf(r2, "%hd,%hd,%hd", &start_pos.m, &start_pos.x, &start_pos.y);
//       } else if (strcasecmp(r1, "meta") == 0) {
//         add_meta(r2);
//       } else if (strcasecmp(r1, "version") == 0) {
//         nex_version = atoi(r2);
//       } else if (strcasecmp(r1, "deep") == 0) {
//         nex_deep = atoi(r2);
//       } else if (strcasecmp(r1, "require_reg") == 0) {
//         require_reg = atoi(r2);
//       } else if (strcasecmp(r1, "town") == 0) {
//         if (add_town(r2)) {
//           printf("CFG_ERR: Town Name Parse error!\n");
//           printf(" line %d: %s\n", line_num, line);
//         }
//       } else if (strcasecmp(r1, "server_id") == 0) {
//         serverid = atoi(r2);
//       } else if (strcasecmp(r1, "xprate") == 0) {
//         xp_rate = atoi(r2);
//       } else if (strcasecmp(r1, "droprate") == 0) {
//         d_rate = atoi(r2);
//       } else if (strcasecmp(r1, "save_time") == 0) {
//         save_time = atoi(r2) * 1000;
//       } else if (strcasecmp(r1, "xor_key") == 0) {
//         strncpy(xor_key, r2, 9);
//       }
//     }
//   }
//   fclose(fp);
//   printf("[config] [read_success] file=%s\n", cfg_file);

//   return 0;
// }