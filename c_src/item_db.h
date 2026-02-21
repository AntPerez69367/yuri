#pragma once

enum {
  EQ_WEAP,
  EQ_ARMOR,
  EQ_SHIELD,
  EQ_HELM,
  EQ_LEFT,
  EQ_RIGHT,
  EQ_SUBLEFT,
  EQ_SUBRIGHT,
  EQ_FACEACC,
  EQ_CROWN,
  EQ_MANTLE,
  EQ_NECKLACE,
  EQ_BOOTS,
  EQ_COAT,
  EQ_FACEACCTWO
};

enum {
  ITM_EAT,        // 0
  ITM_USE,        // 1
  ITM_SMOKE,      // 2
  ITM_WEAP,       // 3
  ITM_ARMOR,      // 4
  ITM_SHIELD,     // 5
  ITM_HELM,       // 6
  ITM_LEFT,       // 7
  ITM_RIGHT,      // 8
  ITM_SUBLEFT,    // 9
  ITM_SUBRIGHT,   // 10
  ITM_FACEACC,    // 11
  ITM_CROWN,      // 12
  ITM_MANTLE,     // 13
  ITM_NECKLACE,   // 14
  ITM_BOOTS,      // 15
  ITM_COAT,       // 16
  ITM_HAND,       // 17
  ITM_ETC,        // 18
  ITM_USESPC,     // 19
  ITM_TRAPS,      // 20
  ITM_BAG,        // 21
  ITM_MAP,        // 22
  ITM_QUIVER,     // 23
  ITM_MOUNT,      // 24
  ITM_FACE,       // 25
  ITM_SET,        // 26
  ITM_SKIN,       // 27
  ITM_HAIR_DYE,   // 28
  ITM_FACEACCTWO  // 29
};

struct item_data {
  unsigned int id, sound, minSdam, maxSdam, minLdam, maxLdam, sound_hit, time,
      amount;
  char name[64], yname[64], text[64], buytext[64];
  unsigned char type, class, sex, level, icon_color, ethereal;
  unsigned char unequip;
  int price, sell, rank, stack_amount, look, look_color, dura, might, will,
      grace, ac, dam, hit, vita, mana, protection, protected, healing, wisdom,
      con, attack_speed, icon;
  int mightreq, depositable, exchangeable, droppable, thrown, thrownconfirm,
      repairable, max_amount, skinnable, bod;
  char *script, *equip_script, *unequip_script;
};

// item_db.c deleted — implemented in Rust (src/database/item_db.rs)
// rust_itemdb_* declarations use Rust struct names to match yuri.h (cbindgen output)
struct ItemData;
struct ItemData* rust_itemdb_search(unsigned int id);
struct ItemData* rust_itemdb_searchexist(unsigned int id);
struct ItemData* rust_itemdb_searchname(const char* s);
int rust_itemdb_init(void);
void rust_itemdb_term(void);
unsigned int rust_itemdb_id(const char* s);
/* The following functions return pointers into Rust-owned memory (fields of
 * the BoxItemData stored in the static item database).  The lifetime of each
 * pointer is tied to the item database: pointers remain valid until
 * rust_itemdb_term() is called.  Callers MUST NOT free the returned pointer. */
char* rust_itemdb_name(unsigned int id);
char* rust_itemdb_yname(unsigned int id);
char* rust_itemdb_text(unsigned int id);
char* rust_itemdb_buytext(unsigned int id);
char* rust_itemdb_script(unsigned int id);
char* rust_itemdb_equipscript(unsigned int id);
char* rust_itemdb_unequipscript(unsigned int id);
unsigned int rust_itemdb_sound(unsigned int id);
unsigned int rust_itemdb_soundhit(unsigned int id);
int rust_itemdb_sell(unsigned int id);
int rust_itemdb_type(unsigned int id);
int rust_itemdb_level(unsigned int id);
int rust_itemdb_class(unsigned int id);
int rust_itemdb_sex(unsigned int id);
int rust_itemdb_price(unsigned int id);
int rust_itemdb_rank(unsigned int id);
int rust_itemdb_stackamount(unsigned int id);
int rust_itemdb_look(unsigned int id);
int rust_itemdb_lookcolor(unsigned int id);
int rust_itemdb_icon(unsigned int id);
int rust_itemdb_iconcolor(unsigned int id);
int rust_itemdb_dura(unsigned int id);
int rust_itemdb_might(unsigned int id);
int rust_itemdb_mightreq(unsigned int id);
int rust_itemdb_will(unsigned int id);
int rust_itemdb_grace(unsigned int id);
int rust_itemdb_ac(unsigned int id);
int rust_itemdb_dam(unsigned int id);
int rust_itemdb_hit(unsigned int id);
int rust_itemdb_vita(unsigned int id);
int rust_itemdb_mana(unsigned int id);
int rust_itemdb_protection(unsigned int id);
int rust_itemdb_protected(unsigned int id);
int rust_itemdb_healing(unsigned int id);
int rust_itemdb_wisdom(unsigned int id);
int rust_itemdb_con(unsigned int id);
int rust_itemdb_attackspeed(unsigned int id);
int rust_itemdb_mindam(unsigned int id);
int rust_itemdb_maxdam(unsigned int id);
int rust_itemdb_minSdam(unsigned int id);
int rust_itemdb_maxSdam(unsigned int id);
int rust_itemdb_minLdam(unsigned int id);
int rust_itemdb_maxLdam(unsigned int id);
int rust_itemdb_mincritdam(unsigned int id);
int rust_itemdb_maxcritdam(unsigned int id);
int rust_itemdb_exchangeable(unsigned int id);
int rust_itemdb_depositable(unsigned int id);
int rust_itemdb_droppable(unsigned int id);
int rust_itemdb_thrown(unsigned int id);
int rust_itemdb_thrownconfirm(unsigned int id);
int rust_itemdb_repairable(unsigned int id);
int rust_itemdb_maxamount(unsigned int id);
int rust_itemdb_skinnable(unsigned int id);
int rust_itemdb_unequip(unsigned int id);
int rust_itemdb_ethereal(unsigned int id);
int rust_itemdb_time(unsigned int id);
int rust_itemdb_breakondeath(unsigned int id);
int rust_itemdb_dodge(unsigned int id);
int rust_itemdb_block(unsigned int id);
int rust_itemdb_parry(unsigned int id);
int rust_itemdb_resist(unsigned int id);
int rust_itemdb_physdeduct(unsigned int id);
int rust_itemdb_reqvita(unsigned int id);
int rust_itemdb_reqmana(unsigned int id);
static inline struct item_data* itemdb_search(unsigned int id)       { return (struct item_data*)rust_itemdb_search(id); }
static inline struct item_data* itemdb_searchexist(unsigned int id)  { return (struct item_data*)rust_itemdb_searchexist(id); }
static inline struct item_data* itemdb_searchname(const char* s)     { return (struct item_data*)rust_itemdb_searchname(s); }
static inline int  itemdb_init(void)                                  { return rust_itemdb_init(); }
static inline void itemdb_term(void)                                  { rust_itemdb_term(); }
static inline unsigned int itemdb_id(const char* s)                  { return rust_itemdb_id(s); }
static inline char* itemdb_name(unsigned int id)        { return rust_itemdb_name(id); }
static inline char* itemdb_yname(unsigned int id)       { return rust_itemdb_yname(id); }
static inline char* itemdb_text(unsigned int id)        { return rust_itemdb_text(id); }
static inline char* itemdb_buytext(unsigned int id)     { return rust_itemdb_buytext(id); }
static inline char* itemdb_script(unsigned int id)      { return rust_itemdb_script(id); }
static inline char* itemdb_equipscript(unsigned int id) { return rust_itemdb_equipscript(id); }
static inline char* itemdb_unequipscript(unsigned int id){ return rust_itemdb_unequipscript(id); }
static inline int  itemdb_type(unsigned int id)         { return rust_itemdb_type(id); }
static inline int  itemdb_level(unsigned int id)        { return rust_itemdb_level(id); }
static inline int  itemdb_class(unsigned int id)        { return rust_itemdb_class(id); }
static inline int  itemdb_sex(unsigned int id)          { return rust_itemdb_sex(id); }
static inline int  itemdb_price(unsigned int id)        { return rust_itemdb_price(id); }
static inline int  itemdb_sell(unsigned int id)         { extern int rust_itemdb_sell(unsigned int); return rust_itemdb_sell(id); }
static inline int  itemdb_rank(unsigned int id)         { return rust_itemdb_rank(id); }
static inline int  itemdb_stackamount(unsigned int id)  { return rust_itemdb_stackamount(id); }
static inline int  itemdb_look(unsigned int id)         { return rust_itemdb_look(id); }
static inline int  itemdb_lookcolor(unsigned int id)    { return rust_itemdb_lookcolor(id); }
static inline int  itemdb_icon(unsigned int id)         { return rust_itemdb_icon(id); }
static inline int  itemdb_iconcolor(unsigned int id)    { return rust_itemdb_iconcolor(id); }
static inline unsigned int itemdb_sound(unsigned int id)    { return rust_itemdb_sound(id); }
static inline unsigned int itemdb_soundhit(unsigned int id) { return rust_itemdb_soundhit(id); }
static inline int  itemdb_dura(unsigned int id)         { return rust_itemdb_dura(id); }
static inline int  itemdb_might(unsigned int id)        { return rust_itemdb_might(id); }
static inline int  itemdb_mightreq(unsigned int id)     { return rust_itemdb_mightreq(id); }
static inline int  itemdb_will(unsigned int id)         { return rust_itemdb_will(id); }
static inline int  itemdb_grace(unsigned int id)        { return rust_itemdb_grace(id); }
static inline int  itemdb_ac(unsigned int id)           { return rust_itemdb_ac(id); }
static inline int  itemdb_dam(unsigned int id)          { return rust_itemdb_dam(id); }
static inline int  itemdb_hit(unsigned int id)          { return rust_itemdb_hit(id); }
static inline int  itemdb_vita(unsigned int id)         { return rust_itemdb_vita(id); }
static inline int  itemdb_mana(unsigned int id)         { return rust_itemdb_mana(id); }
static inline int  itemdb_protection(unsigned int id)   { return rust_itemdb_protection(id); }
static inline int  itemdb_protected(unsigned int id)    { return rust_itemdb_protected(id); }
static inline int  itemdb_healing(unsigned int id)      { return rust_itemdb_healing(id); }
static inline int  itemdb_wisdom(unsigned int id)       { return rust_itemdb_wisdom(id); }
static inline int  itemdb_con(unsigned int id)          { return rust_itemdb_con(id); }
static inline int  itemdb_attackspeed(unsigned int id)  { return rust_itemdb_attackspeed(id); }
static inline int  itemdb_mindam(unsigned int id)       { return rust_itemdb_mindam(id); }
static inline int  itemdb_maxdam(unsigned int id)       { return rust_itemdb_maxdam(id); }
static inline int  itemdb_minSdam(unsigned int id)      { return rust_itemdb_minSdam(id); }
static inline int  itemdb_maxSdam(unsigned int id)      { return rust_itemdb_maxSdam(id); }
static inline int  itemdb_minLdam(unsigned int id)      { return rust_itemdb_minLdam(id); }
static inline int  itemdb_maxLdam(unsigned int id)      { return rust_itemdb_maxLdam(id); }
static inline int  itemdb_mincritdam(unsigned int id)   { return rust_itemdb_mincritdam(id); }
static inline int  itemdb_maxcritdam(unsigned int id)   { return rust_itemdb_maxcritdam(id); }
static inline int  itemdb_exchangeable(unsigned int id) { return rust_itemdb_exchangeable(id); }
static inline int  itemdb_depositable(unsigned int id)  { return rust_itemdb_depositable(id); }
static inline int  itemdb_droppable(unsigned int id)    { return rust_itemdb_droppable(id); }
static inline int  itemdb_thrown(unsigned int id)       { return rust_itemdb_thrown(id); }
static inline int  itemdb_thrownconfirm(unsigned int id){ return rust_itemdb_thrownconfirm(id); }
static inline int  itemdb_repairable(unsigned int id)   { return rust_itemdb_repairable(id); }
static inline int  itemdb_maxamount(unsigned int id)    { return rust_itemdb_maxamount(id); }
static inline int  itemdb_skinnable(unsigned int id)    { return rust_itemdb_skinnable(id); }
static inline int  itemdb_unequip(unsigned int id)      { return rust_itemdb_unequip(id); }
static inline int  itemdb_ethereal(unsigned int id)     { return rust_itemdb_ethereal(id); }
static inline int  itemdb_time(unsigned int id)         { return rust_itemdb_time(id); }
static inline int  itemdb_breakondeath(unsigned int id) { return rust_itemdb_breakondeath(id); }
static inline int  itemdb_dodge(unsigned int id)        { return rust_itemdb_dodge(id); }
static inline int  itemdb_block(unsigned int id)        { return rust_itemdb_block(id); }
static inline int  itemdb_parry(unsigned int id)        { return rust_itemdb_parry(id); }
static inline int  itemdb_resist(unsigned int id)       { return rust_itemdb_resist(id); }
static inline int  itemdb_physdeduct(unsigned int id)   { return rust_itemdb_physdeduct(id); }
static inline unsigned int itemdb_reqvita(unsigned int id) { return (unsigned int)rust_itemdb_reqvita(id); }
static inline unsigned int itemdb_reqmana(unsigned int id) { return (unsigned int)rust_itemdb_reqmana(id); }
static inline int  itemdb_read(void)                    { return rust_itemdb_init(); }
