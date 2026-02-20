#pragma once

struct recipe_data {
  int id, tokensRequired, materials[10], superiorMaterials[2];
  char identifier[64], description[64], critIdentifier[64], critDescription[64];
  unsigned int craftTime, successRate, skillAdvance, critRate, bonus,
      skillRequired;
};

// recipe_db.c deleted — implemented in Rust (src/database/recipe_db.rs)
struct RecipeData;
struct RecipeData* rust_recipedb_search(unsigned int id);
struct RecipeData* rust_recipedb_searchexist(unsigned int id);
struct RecipeData* rust_recipedb_searchname(const char* s);
int rust_recipedb_init(void);
void rust_recipedb_term(void);

static inline struct recipe_data* recipedb_search(unsigned int id)      { return (struct recipe_data*)(void*)rust_recipedb_search(id); }
static inline struct recipe_data* recipedb_searchexist(unsigned int id) { return (struct recipe_data*)(void*)rust_recipedb_searchexist(id); }
static inline struct recipe_data* recipedb_searchname(const char* s)    { return (struct recipe_data*)(void*)rust_recipedb_searchname(s); }
static inline int  recipedb_init(void)                                  { return rust_recipedb_init(); }
static inline void recipedb_term(void)                                  { rust_recipedb_term(); }
static inline void recipedb_read(void)                                  { }
