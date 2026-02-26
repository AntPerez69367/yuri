/* sl_compat.c — real C symbols for scripting dispatch.
 *
 * These provide linkable symbols so Rust extern "C" declarations in
 * npc.rs / mob.rs can resolve at link time.  The static inline versions
 * in scripting.h are compiled away and never produce symbols.
 */
#include <stdarg.h>
#include "scripting.h"

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
