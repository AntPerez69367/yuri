#pragma once

struct board_data {
  int id, level, gmlevel, path, clan, special, sort;
  char name[64], yname[64], script;
};

struct bn_data {
  int id;
  char name[255];
};
// board_db.c deleted — implemented in Rust (src/database/board_db.rs)
struct BoardData;
struct BnData;
struct BoardData* rust_boarddb_search(int);
struct BoardData* rust_boarddb_searchexist(int);
struct BnData* rust_bn_search(int);
struct BnData* rust_bn_searchexist(int);
char* rust_bn_name(int);
int rust_boarddb_level(int);
char* rust_boarddb_name(int);
char* rust_boarddb_yname(int);
int rust_boarddb_path(int);
int rust_boarddb_gmlevel(int);
int rust_boarddb_clan(int);
int rust_boarddb_sort(int);
unsigned int rust_boarddb_id(const char*);
int rust_boarddb_script(int);
int rust_boarddb_init(void);
void rust_boarddb_term(void);

static inline struct board_data* boarddb_search(int id)       { return (struct board_data*)(void*)rust_boarddb_search(id); }
static inline struct board_data* boarddb_searchexist(int id)  { return (struct board_data*)(void*)rust_boarddb_searchexist(id); }
static inline struct bn_data* bn_search(int id)               { return (struct bn_data*)(void*)rust_bn_search(id); }
static inline struct bn_data* bn_searchexist(int id)          { return (struct bn_data*)(void*)rust_bn_searchexist(id); }
static inline char* bn_name(int id)                           { return rust_bn_name(id); }
static inline int   boarddb_level(int id)                     { return rust_boarddb_level(id); }
static inline char* boarddb_name(int id)                      { return rust_boarddb_name(id); }
static inline char* boarddb_yname(int id)                     { return rust_boarddb_yname(id); }
static inline int   boarddb_path(int id)                      { return rust_boarddb_path(id); }
static inline int   boarddb_gmlevel(int id)                   { return rust_boarddb_gmlevel(id); }
static inline int   boarddb_clan(int id)                      { return rust_boarddb_clan(id); }
static inline int   boarddb_sort(int id)                      { return rust_boarddb_sort(id); }
static inline unsigned int boarddb_id(const char* s)          { return rust_boarddb_id(s); }
static inline char  boarddb_script(int id)                    { return (char)rust_boarddb_script(id); }
static inline int   boarddb_init(void)                        { return rust_boarddb_init(); }
static inline void  boarddb_term(void)                        { rust_boarddb_term(); }
static inline int   boarddb_read(void)                        { return 0; }
static inline int   bn_read(void)                             { return 0; }