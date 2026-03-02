#pragma once

#include <luajit.h>

#include "map_server.h"
#include "mmo.h"

// Rust implementations (defined in src/game/gm_command.rs)
int rust_is_command(USER *, const char *, int);
int rust_at_command(USER *, const char *, int);
int rust_command_reload(USER *, char *, lua_State *);

// Inline wrappers — redirect C callers to Rust without changing call sites.
static inline int is_command(USER *sd, const char *p, int len) {
    return rust_is_command(sd, p, len);
}
static inline int at_command(USER *sd, const char *p, int len) {
    return rust_at_command(sd, p, len);
}
static inline int command_reload(USER *sd, char *line, lua_State *state) {
    return rust_command_reload(sd, line, state);
}
