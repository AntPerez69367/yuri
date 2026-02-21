#pragma once

struct clan_data {
  int id;
  char name[64];
  int maxslots;
  int maxperslot;
  int level;
  struct clan_bank* clanbanks;
};

struct clan_bank {
  unsigned int item_id, amount, owner, time, customIcon, customLook, pos;
  char real_name[64];
  unsigned int customLookColor, customIconColor, protected;
  char note[300];
};

// clan_db.c deleted — implemented in Rust (src/database/clan_db.rs)
// clandb_add moved to scripting.c (only caller) as a static function
struct ClanData;
struct ClanData* rust_clandb_search(int);
struct ClanData* rust_clandb_searchexist(int);
const char* rust_clandb_name(int);
struct ClanData* rust_clandb_searchname(const char*);
int rust_clandb_init(void);
void rust_clandb_term(void);

static inline struct clan_data* clandb_search(int id)          { return (struct clan_data*)(void*)rust_clandb_search(id); }
static inline struct clan_data* clandb_searchexist(int id)     { return (struct clan_data*)(void*)rust_clandb_searchexist(id); }
static inline const char* clandb_name(int id)                  { return rust_clandb_name(id); }
static inline struct clan_data* clandb_searchname(const char* s){ return (struct clan_data*)(void*)rust_clandb_searchname(s); }
static inline int  clandb_init(void)                           { return rust_clandb_init(); }
static inline void clandb_term(void)                           { rust_clandb_term(); }
static inline int  clandb_read(void)                           { return 0; }