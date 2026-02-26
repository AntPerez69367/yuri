/* sl_compat.c — real C symbols for scripting dispatch.
 *
 * These provide linkable symbols so Rust extern "C" declarations in
 * npc.rs / mob.rs can resolve at link time.  The static inline versions
 * in scripting.h are compiled away and never produce symbols.
 */
#include <stdarg.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
#include "scripting.h"
#include "core.h"
#include "net_crypt.h"
#include "session.h"
#include "map_parse.h"
#include "clan_db.h"
#include "config.h"
#include "db_mysql.h"

/* -------------------------------------------------------------------------
 * Functions previously in scripting.c that are referenced as callbacks or
 * used by sl_g_* wrappers.
 * --------------------------------------------------------------------- */

int sl_throw(struct block_list *bl, va_list ap) {
    USER *sd = NULL;
    nullpo_ret(0, sd = (USER *)bl);
    char *buf = va_arg(ap, char *);
    int len = va_arg(ap, int);
    if (!rust_session_exists(sd->fd) || rust_session_get_eof(sd->fd)) {
        rust_session_set_eof(sd->fd, 8);
        return 0;
    }
    WFIFOHEAD(sd->fd, len);
    memcpy(WFIFOP(sd->fd, 0), buf, len);
    WFIFOSET(sd->fd, encrypt(sd->fd));
    return 0;
}

/* sl_updatepeople is provided as a Rust #[no_mangle] symbol in ffi/scripting.rs */

int sl_doscript_blargs(char *root, const char *method, int nargs, ...) {
    struct block_list *args[16] = {0};
    va_list ap; va_start(ap, nargs);
    for (int i = 0; i < nargs && i < 16; i++)
        args[i] = va_arg(ap, struct block_list *);
    va_end(ap);
    return rust_sl_doscript_blargs_vec(root, method, nargs, args);
}

int sl_doscript_strings(char *root, const char *method, int nargs, ...) {
    const char *args[16] = {0};
    va_list ap; va_start(ap, nargs);
    for (int i = 0; i < nargs && i < 16; i++)
        args[i] = va_arg(ap, const char *);
    va_end(ap);
    return rust_sl_doscript_strings_vec(root, method, nargs, args);
}

/* Map registry helpers — extract map index from USER* so Rust can call without
 * knowing the block_list struct layout. */
int map_readglobalreg_sd(void *sd, const char *attrname) {
    return map_readglobalreg(((USER *)sd)->bl.m, attrname);
}

int map_setglobalreg_sd(void *sd, const char *attrname, int val) {
    return map_setglobalreg(((USER *)sd)->bl.m, attrname, val);
}
