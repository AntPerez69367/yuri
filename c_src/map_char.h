#pragma once

#include "map_server.h"

#include <stdint.h>
#include <string.h>
#include <zlib.h>

// ---------------------------------------------------------------------------
// Rust FFI bridge declarations (from src/ffi/map_char.rs via yuri.h).
// These are the real linkable symbols exported by libyuri.a.
// ---------------------------------------------------------------------------
void rust_intif_load(int fd, uint32_t char_id, const char* name);
void rust_intif_quit(uint32_t char_id);
void rust_intif_save(const uint8_t* data, uint32_t len);
void rust_intif_savequit(const uint8_t* data, uint32_t len);

// ---------------------------------------------------------------------------
// auth_db helpers — still backed by SQL (Authorize table) in map_char.c
// until map_parse.c is fully ported.
// ---------------------------------------------------------------------------
int authdb_init();
int auth_check(char*, unsigned int);
int auth_delete(char*);
int auth_add(char*, unsigned int, unsigned int);
int intif_init();
int intif_timer(int, int);
int intif_mmo_tosd(int, struct mmo_charstatus*);
int intif_parse(int);

// ---------------------------------------------------------------------------
// Inline wrappers: C game logic calls these; they build packets and forward
// to Rust which sends over the char_server TCP connection.
// ---------------------------------------------------------------------------

static inline int intif_quit(USER* sd) {
  if (!sd) return -1;
  rust_intif_quit((uint32_t)sd->status.id);
  return 0;
}

static inline int intif_load(int fd, int id, char* name) {
  rust_intif_load(fd, (uint32_t)id, name);
  return 0;
}

// intif_save: compress mmo_charstatus and send 0x3004 to char_server.
static inline int intif_save(USER* sd) {
  if (!sd) return -1;
  sd->status.last_pos.m = sd->bl.m;
  sd->status.last_pos.x = sd->bl.x;
  sd->status.last_pos.y = sd->bl.y;
  sd->status.disguise       = sd->disguise;
  sd->status.disguisecolor  = sd->disguise_color;

  size_t ulen = sizeof(struct mmo_charstatus);
  uLongf clen = compressBound(ulen);
  uint8_t* buf = (uint8_t*)malloc(clen + 6);
  if (!buf) return -1;

  if (compress2(buf + 6, &clen, (const uint8_t*)&sd->status, ulen, 1) != Z_OK) {
    free(buf);
    return -1;
  }
  buf[0] = 0x04; buf[1] = 0x30; // 0x3004 LE
  uint32_t total = (uint32_t)(clen + 6);
  memcpy(buf + 2, &total, 4);
  rust_intif_save(buf, total);
  free(buf);
  return 0;
}

// intif_savequit: same as intif_save but uses 0x3007 cmd and updates dest_pos.
static inline int intif_savequit(USER* sd) {
  if (!sd) return -1;
  if (!map_isloaded(sd->status.dest_pos.m)) {
    if (sd->status.dest_pos.m == 0) {
      sd->status.dest_pos.m = sd->bl.m;
      sd->status.dest_pos.y = sd->bl.y;
      sd->status.dest_pos.x = sd->bl.x;
    }
    sd->status.last_pos.m = sd->status.dest_pos.m;
    sd->status.last_pos.x = sd->status.dest_pos.x;
    sd->status.last_pos.y = sd->status.dest_pos.y;
  } else {
    sd->status.last_pos.m = sd->bl.m;
    sd->status.last_pos.x = sd->bl.x;
    sd->status.last_pos.y = sd->bl.y;
  }
  sd->status.disguise      = sd->disguise;
  sd->status.disguisecolor = sd->disguise_color;

  size_t ulen = sizeof(struct mmo_charstatus);
  uLongf clen = compressBound(ulen);
  uint8_t* buf = (uint8_t*)malloc(clen + 6);
  if (!buf) return -1;

  if (compress2(buf + 6, &clen, (const uint8_t*)&sd->status, ulen, 1) != Z_OK) {
    free(buf);
    return -1;
  }
  buf[0] = 0x07; buf[1] = 0x30; // 0x3007 LE
  uint32_t total = (uint32_t)(clen + 6);
  memcpy(buf + 2, &total, 4);
  rust_intif_savequit(buf, total);
  free(buf);
  return 0;
}
