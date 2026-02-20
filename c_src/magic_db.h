#pragma once

struct magic_data {
  int id, type;
  char name[32], yname[32], question[64];
  char script[64];
  char script2[64];
  char script3[64];
  unsigned char dispell, aether, mute, level, mark;
  unsigned char canfail;
  char alignment;
  unsigned char ticker;
  char class;
};

// magic_db.c deleted — implemented in Rust (src/database/magic_db.rs)
struct MagicData;
struct MagicData* rust_magicdb_search(int);
struct MagicData* rust_magicdb_searchexist(int);
struct MagicData* rust_magicdb_searchname(const char*);
int rust_magicdb_id(const char*);
char* rust_magicdb_name(int);
char* rust_magicdb_yname(int);
char* rust_magicdb_question(int);
char* rust_magicdb_script(int);
char* rust_magicdb_script2(int);
char* rust_magicdb_script3(int);
int rust_magicdb_type(int);
int rust_magicdb_dispel(int);
int rust_magicdb_aether(int);
int rust_magicdb_mute(int);
int rust_magicdb_canfail(int);
int rust_magicdb_alignment(int);
int rust_magicdb_ticker(int);
int rust_magicdb_level(const char*);
int rust_magicdb_init(void);
void rust_magicdb_term(void);

static inline struct magic_data* magicdb_search(int id)           { return (struct magic_data*)(void*)rust_magicdb_search(id); }
static inline struct magic_data* magicdb_searchexist(int id)      { return (struct magic_data*)(void*)rust_magicdb_searchexist(id); }
static inline struct magic_data* magicdb_searchname(const char* s){ return (struct magic_data*)(void*)rust_magicdb_searchname(s); }
static inline int   magicdb_id(const char* s)                     { return rust_magicdb_id(s); }
static inline char* magicdb_name(int id)                          { return rust_magicdb_name(id); }
static inline char* magicdb_yname(int id)                         { return rust_magicdb_yname(id); }
static inline char* magicdb_question(int id)                      { return rust_magicdb_question(id); }
static inline char* magicdb_script(int id)                        { return rust_magicdb_script(id); }
static inline char* magicdb_script2(int id)                       { return rust_magicdb_script2(id); }
static inline char* magicdb_script3(int id)                       { return rust_magicdb_script3(id); }
static inline int   magicdb_type(int id)                          { return rust_magicdb_type(id); }
static inline int   magicdb_dispel(int id)                        { return rust_magicdb_dispel(id); }
static inline int   magicdb_aether(int id)                        { return rust_magicdb_aether(id); }
static inline int   magicdb_mute(int id)                          { return rust_magicdb_mute(id); }
static inline int   magicdb_canfail(int id)                       { return rust_magicdb_canfail(id); }
static inline int   magicdb_alignment(int id)                     { return rust_magicdb_alignment(id); }
static inline int   magicdb_ticker(int id)                        { return rust_magicdb_ticker(id); }
static inline int   magicdb_level(const char* s)                  { return rust_magicdb_level(s); }
static inline int   magicdb_init(void)                            { return rust_magicdb_init(); }
static inline void  magicdb_term(void)                            { rust_magicdb_term(); }
static inline int   magicdb_read(void)                            { return 0; }