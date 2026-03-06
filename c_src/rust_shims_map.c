/*
 * rust_shims_map.c — read_pass shim in a separate TU.
 *
 * Separated from rust_shims.c because map_server.h pulls in mmo.h → item_db.h
 * which has static-inline definitions that conflict with the non-inline
 * definitions in rust_shims.c.
 *
 * This TU ONLY defines read_pass and is allowed to include map_server.h.
 */

#include "map_server.h"

/* map_server.h defines read_pass as a macro:
 *   #define read_pass(m, x, y) (map[m].pass[(x) + (y)*map[m].xs])
 * #undef it so we can provide a real function with the same name.
 */
#undef read_pass

int read_pass(int m, int x, int y) {
    return (int)(map[m].pass[x + y * map[m].xs]);
}
