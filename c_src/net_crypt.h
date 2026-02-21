#pragma once

#include <stdbool.h>

#define SWAP16(x) (short)(((x) << 8) | ((x) >> 8))
#define SWAP32(x) \
  (int)(((x) << 24) | (((x) << 8) & 0x00FF0000) | (((x) >> 8) & 0x0000FF00) | ((x) >> 24))

#define RAND_INC rand() % 0xFF

/* ── Rust crypto primitives — explicit declarations (mirrors yuri.h) ─────── */
bool   rust_crypt_is_key_client(int opcode);
bool   rust_crypt_is_key_server(int opcode);
char  *rust_crypt_generate_hashvalues(const char *name, char *buffer, int buflen);
char  *rust_crypt_populate_table(const char *name, char *table, int tablelen);
int    rust_crypt_set_packet_indexes(unsigned char *packet);
char  *rust_crypt_generate_key2(unsigned char *packet, const char *table, char *keyout, int fromclient);
void   rust_crypt_dynamic(unsigned char *buff, const char *key);
void   rust_crypt_static(unsigned char *buff, const char *xor_key);

/* xor_key is defined in config.c */
extern char xor_key[10];

/* ── Inline wrappers preserving the original C API ───────────────────────── */

static inline bool is_key_client(int opcode)  { return rust_crypt_is_key_client(opcode); }
static inline bool is_key_server(int opcode)  { return rust_crypt_is_key_server(opcode); }

static inline char *generate_hashvalues(const char *name, char *buf, int len) {
    return rust_crypt_generate_hashvalues(name, buf, len);
}
static inline char *populate_table(const char *name, char *table, int len) {
    return rust_crypt_populate_table(name, table, len);
}
static inline int set_packet_indexes(unsigned char *pkt) {
    return rust_crypt_set_packet_indexes(pkt);
}
static inline char *generate_key2(unsigned char *pkt, char *table, char *key, int fc) {
    return rust_crypt_generate_key2(pkt, table, key, fc);
}
static inline void tk_crypt_dynamic(unsigned char *buff, const char *key) {
    rust_crypt_dynamic(buff, key);
}
static inline void tk_crypt_static(unsigned char *buff) {
    rust_crypt_static(buff, xor_key);
}

/* ── Active C functions (still in net_crypt.c) ───────────────────────────── */
int encrypt(int fd);
int decrypt(int fd);
