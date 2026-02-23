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

  if (sd == NULL) return 1;

  set_packet_indexes((unsigned char *)WFIFOP(fd, 0));

  if (is_key_server(WFIFOB(fd, 3))) {
    char key[10];
    generate_key2((unsigned char *)WFIFOP(fd, 0), sd->EncHash, key, 0);
    tk_crypt_dynamic((unsigned char *)WFIFOP(fd, 0), key);
  } else {
    tk_crypt_static((unsigned char *)WFIFOP(fd, 0));
  }
  return (int)SWAP16(*(unsigned short *)WFIFOP(fd, 1)) + 3;
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
