#include "creation_db.h"

#include "scripting.h"
#include "session.h"

// Item creation is driven entirely by Lua ("itemCreation" script).
// The old SQL-backed create_db was removed — no DB table exists.
int createdb_start(USER *sd) {
  int item_c = RFIFOB(sd->fd, 5);
  int item[10], item_amount[10];
  int len = 6;
  int x;
  int curitem;

  for (x = 0; x < item_c; x++) {
    curitem = RFIFOB(sd->fd, len) - 1;
    item[x] = sd->status.inventory[curitem].id;

    if (itemdb_stackamount(item[x]) > 1) {
      item_amount[x] = RFIFOB(sd->fd, len + 1);
      len += 2;
    } else {
      item_amount[x] = 1;
      len += 1;
    }
  }
  sd->creation_works = 0;
  sd->creation_item = 0;
  sd->creation_itemamount = 0;

  printf("creation system executed by: %s\n", sd->status.name);

  lua_newtable(sl_gstate);

  int j, k;

  for (j = 0, k = 1; j < item_c; j++, k += 2) {
    lua_pushnumber(sl_gstate, item[j]);
    lua_rawseti(sl_gstate, -2, k);

    lua_pushnumber(sl_gstate, item_amount[j]);
    lua_rawseti(sl_gstate, -2, k + 1);
  }

  lua_setglobal(sl_gstate, "creationItems");
  lua_settop(sl_gstate, 0);

  sl_async_freeco(sd);
  sl_doscript_blargs("itemCreation", NULL, 1, &sd->bl);

  return 0;
}
