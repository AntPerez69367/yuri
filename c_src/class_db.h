#pragma once

struct class_data {
  char rank0[32];
  char rank1[32];
  char rank2[32];
  char rank3[32];
  char rank4[32];
  char rank5[32];
  char rank6[32];
  char rank7[32];
  char rank8[32];
  char rank9[32];
  char rank10[32];
  char rank11[32];
  char rank12[32];
  char rank13[32];
  char rank14[32];
  char rank15[32];
  unsigned short id;
  unsigned short path;
  unsigned int level[99];
  int chat;
  int icon;
};

// class_db.c deleted — implemented in Rust (src/database/class_db.rs)
// cdata is exposed from Rust as struct ClassData*[20]; use void* to avoid struct name conflict
struct ClassData;
extern struct ClassData* cdata[20];

struct ClassData;
struct ClassData* rust_classdb_search(int);
struct ClassData* rust_classdb_searchexist(int);
unsigned int rust_classdb_level(int, int);
char* rust_classdb_name(int, int);
int rust_classdb_path(int);
int rust_classdb_chat(int);
int rust_classdb_icon(int);
int rust_classdb_init(const char* data_dir);
void rust_classdb_term(void);

extern char* data_dir;

static inline struct class_data* classdb_search(int id)          { return (struct class_data*)(void*)rust_classdb_search(id); }
static inline struct class_data* classdb_searchexist(int id)     { return (struct class_data*)(void*)rust_classdb_searchexist(id); }
static inline unsigned int classdb_level(int path, int lvl)      { return rust_classdb_level(path, lvl); }
static inline char* classdb_name(int id, int rank)               { return rust_classdb_name(id, rank); }
static inline int classdb_path(int id)                           { return rust_classdb_path(id); }
static inline int classdb_chat(int id)                           { return rust_classdb_chat(id); }
static inline int classdb_icon(int id)                           { return rust_classdb_icon(id); }
static inline int classdb_init(void)                             { return rust_classdb_init(data_dir); }
static inline void classdb_term(void)                            { rust_classdb_term(); }
static inline int classdb_read(void)                             { return 0; }
static inline int leveldb_read(void)                             { return 0; }