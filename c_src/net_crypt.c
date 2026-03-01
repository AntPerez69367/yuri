#include "net_crypt.h"

#include <stdbool.h>
#include <stdint.h>

#include "config.h"
#include "map_server.h"
#include "session.h"

/* encrypt / decrypt remain in C: they operate on FIFO buffers and USER->EncHash,
   both of which are tied to the C session and USER structs.
   All crypto primitives (set_packet_indexes, generate_key2, tk_crypt_dynamic,
   tk_crypt_static) now live in Rust (src/network/crypt.rs). */

int encrypt(int fd) {
  USER *sd = rust_session_get_data(fd);

  if (sd == NULL) {
    printf("[encrypt] sd is NULL for fd=%d\n", fd);
    fflush(stdout);
    return 1;
  }

  unsigned char *buf = (unsigned char *)WFIFOP(fd, 0);
  if (!buf) {
    printf("[encrypt] WFIFOP returned NULL for fd=%d\n", fd);
    fflush(stdout);
    return 1;
  }

  set_packet_indexes(buf);

  if (is_key_server(buf[3])) {
    char key[10];
    generate_key2(buf, sd->EncHash, key, 0);
    tk_crypt_dynamic(buf, key);
  } else {
    tk_crypt_static(buf);
  }
  int pkt_len = (int)SWAP16(*(unsigned short *)(buf + 1)) + 3;
  return pkt_len;
}

int decrypt(int fd) {
  USER *sd = (USER *)rust_session_get_data(fd);

  if (sd == NULL) return 1;

  if (is_key_client(RFIFOB(fd, 3))) {
    char key[10];
    generate_key2((unsigned char *)RFIFOP(fd, 0), sd->EncHash, key, 1);
    tk_crypt_dynamic((unsigned char *)RFIFOP(fd, 0), key);
  } else {
    tk_crypt_static((unsigned char *)RFIFOP(fd, 0));
  }
  return 0;
}
