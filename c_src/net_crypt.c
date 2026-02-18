#include "net_crypt.h"

#include <arpa/inet.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "config.h"
#include "map_server.h"
#include "md5calc.h"
#include "session.h"

const unsigned char svkey1packets[] = {2, 3, 10, 64, 68, 94, 96, 98, 102, 111};
const unsigned char clkey1packets[] = {2,  3,  4,  11, 21, 38,  58,  66,
                                       67, 75, 80, 87, 98, 113, 115, 123};

bool is_key_client(int opcode) {
  for (int x = 0; x < (sizeof(clkey1packets) / sizeof(clkey1packets[0])); x++) {
    if (opcode == clkey1packets[x]) return 0;
  }

  return 1;
}

bool is_key_server(int opcode) {
  for (int x = 0; x < (sizeof(svkey1packets) / sizeof(svkey1packets[0])); x++) {
    if (opcode == svkey1packets[x]) {
      return false;
    }
  }
  return true;
}

int encrypt(int fd) {
  USER *sd = NULL;
  char key[16];
  sd = rust_session_get_data(fd);

  if (sd == NULL) return 1;

  set_packet_indexes((unsigned char *)WFIFOP(fd, 0));

  if (is_key_server(WFIFOB(fd, 3))) {
    generate_key2((unsigned char *)WFIFOP(fd, 0), sd->EncHash, key, 0);
    tk_crypt_dynamic((unsigned char *)WFIFOP(fd, 0), key);
  } else {
    tk_crypt_static((unsigned char *)WFIFOP(fd, 0));
  }
  return (int)SWAP16(*(unsigned short *)WFIFOP(fd, 1)) + 3;
}

int decrypt(int fd) {
  USER *sd = NULL;
  char key[16];
  sd = (USER *)rust_session_get_data(fd);

  if (sd == NULL) return 1;

  if (is_key_client(RFIFOB(fd, 3))) {
    generate_key2((unsigned char *)RFIFOP(fd, 0), sd->EncHash, key, 1);
    tk_crypt_dynamic((unsigned char *)RFIFOP(fd, 0), key);
  } else {
    tk_crypt_static((unsigned char *)RFIFOP(fd, 0));
  }
  return 0;
}

char *generate_hashvalues(const char *name, char *outbuffer, int buflen) {
  struct cvs_MD5Context context;
  unsigned char checksum[16];
  int i;

  if (buflen < 33) {
    return 0;
  }

  cvs_MD5Init(&context);
  cvs_MD5Update(&context, (unsigned char *)name, strlen(name));
  cvs_MD5Final(checksum, &context);

  for (i = 0; i < 16; i++) {
    sprintf(&outbuffer[i * 2], "%02x", (unsigned int)checksum[i]);
  }

  outbuffer[32] = 0;
  return outbuffer;
}

char *populate_table(const char *name, char *table, int tablelen) {
  char hash[64];
  int i;

  if (tablelen < 0x401) {
    return 0;
  }

  if (!generate_hashvalues(name, &hash[0], sizeof(hash))) {
    return 0;
  }

  table[0] = 0;
  sprintf(&table[0], "%s", &hash[0]);
  generate_hashvalues(&table[0], &hash[0], sizeof(hash));
  table[0] = 0;
  sprintf(&table[0], "%s", &hash[0]);

  for (i = 0; i < 32; i++) {
    generate_hashvalues(&table[0], &hash[0], sizeof(hash));
    strcat(table, hash);
  }

  return &table[0];
}

#ifdef USE_RANDOM_INDEXES
#define rnd() (rand())
#else
#define rnd() (0x1337)
#endif

int set_packet_indexes(unsigned char *packet) {
  unsigned char k1 = ((rnd() & 0x7FFF) % 0x9B + 0x64) ^ 0x21;
  unsigned short k2 = ((rnd() & 0x7FFF) + 0x100) ^ 0x7424;
  unsigned short psize = (unsigned short)(packet[1] << 8) + (packet[2] & 0xFF);

  psize += 3;
  packet[psize] = (unsigned char)k2 & 0xFF;
  packet[psize + 1] = k1;
  packet[psize + 2] = (unsigned char)(k2 >> 8) & 0xFF;
  packet[1] = (unsigned char)(psize >> 8) & 0xFF;
  packet[2] = (unsigned char)psize & 0xFF;

  return psize + 3;
}
#undef rnd

char *generate_key2(unsigned char *packet, char *table, char *keyout,
                    int fromclient) {
  unsigned short psize = (unsigned short)(packet[1] << 8) + (packet[2] & 0xFF);
  unsigned int k1 = packet[psize + 1];
  unsigned int k2 =
      (unsigned int)(packet[psize + 2] << 8) + (packet[psize] & 0xFF);
  int i;

  if (fromclient) {
    k1 ^= 0x25;
    k2 ^= 0x2361;
  } else {
    k1 ^= 0x21;
    k2 ^= 0x7424;
  }

  k1 *= k1;

  for (i = 0; i < 9; i++) {
    keyout[i] = table[(k1 * i + k2) & 0x3FF];
    k1 += 3;
  }
  keyout[9] = 0;

  return keyout;
}

void tk_crypt_static(unsigned char *buff) { tk_crypt_dynamic(buff, xor_key); }

void tk_crypt_dynamic(unsigned char *buff, const char *key) {
  unsigned int group = 0;
  unsigned int groupCount = 0;
  unsigned int packetLen = 0;
  uint8_t packetInc = 0;
  uint8_t keyVal = 0;

  buff++;
  packetLen = SWAP16(*(unsigned short *)buff) - 5;
  buff += 3;
  packetInc = *buff;
  buff++;

  // buff now points to the first data byte
  if (packetLen > 65535) {
    return;
  }

  for (int i = 0; i < packetLen; i++) {
    *(buff + i) ^= key[i % 9];

    keyVal = (uint8_t)(group % 256);
    if (keyVal != packetInc) {
      *(buff + i) ^= keyVal;
    }

    *(buff + i) ^= packetInc;

    groupCount++;
    if (groupCount == 9) {
      group++;
      groupCount = 0;
    }
  }
}
