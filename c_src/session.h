#pragma once

#include <netinet/in.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/socket.h>
#include <sys/types.h>

#include "yuri.h"
#include "net_crypt.h"

// Route all C printf() calls through Rust's tracing system for unified output.
// snprintf into a fixed buffer then hand off to rust_log_c(level=2 = INFO).
#define printf(fmt, ...) do { \
    char _log_buf_[2048]; \
    snprintf(_log_buf_, sizeof(_log_buf_), fmt, ##__VA_ARGS__); \
    rust_log_c(2, _log_buf_); \
} while(0)

#ifdef __INTERIX
#define FD_SETSIZE 4096
#endif  // __INTERIX

// struct socket_data removed — C session[] array replaced by Rust SessionManager.

extern int fd_max;

// Buffer access macros - Rust-backed via FFI
#define RFIFOP(fd, pos)  ((unsigned char *)rust_session_rdata_ptr((fd), (pos)))
#define RFIFOB(fd, pos)  (*(unsigned char *)rust_session_rdata_ptr((fd), (pos)))
#define RFIFOW(fd, pos)  (*(unsigned short *)rust_session_rdata_ptr((fd), (pos)))
#define RFIFOL(fd, pos)  (*(unsigned int *)rust_session_rdata_ptr((fd), (pos)))
#define RFIFOSKIP(fd, len) rust_session_skip((fd), (len))
#define RFIFOREST(fd)    ((int)rust_session_available((fd)))
#define RFIFOFLUSH(fd)   rust_session_rfifoflush((fd))
#define RFIFOSPACE(fd)   (16 * 1024)
#define WFIFOHEAD(fd, size) rust_session_wfifohead((fd), (size))
#define WFIFOSPACE(fd)   (256 * 1024)
#define WFIFOP(fd, pos)  ((char *)rust_session_wdata_ptr((fd), (pos)))
#define WFIFOB(fd, pos)  (*(unsigned char *)rust_session_wdata_ptr((fd), (pos)))
#define WFIFOW(fd, pos)  (*(unsigned short *)rust_session_wdata_ptr((fd), (pos)))
#define WFIFOL(fd, pos)  (*(unsigned int *)rust_session_wdata_ptr((fd), (pos)))
#define WFIFOSET(fd, len) rust_session_commit((fd), (len))

// Raw buffer macros - operate on arbitrary pointers, not sessions
#define RBUFP(p, pos) (((unsigned char *)(p)) + (pos))
#define RBUFB(p, pos) (*(unsigned char *)RBUFP((p), (pos)))
#define RBUFW(p, pos) (*(unsigned short *)RBUFP((p), (pos)))
#define RBUFL(p, pos) (*(unsigned int *)RBUFP((p), (pos)))
#define WBUFP(p, pos) (((unsigned char *)(p)) + (pos))
#define WBUFB(p, pos) (*(unsigned char *)WBUFP((p), (pos)))
#define WBUFW(p, pos) (*(unsigned short *)WBUFP((p), (pos)))
#define WBUFL(p, pos) (*(unsigned int *)WBUFP((p), (pos)))

#define CONVIP(ip)                                             \
  ((ip) >> 0) & 0xFF, ((ip) >> 8) & 0xFF, ((ip) >> 16) & 0xFF, \
      ((ip) >> 24) & 0xFF
#define CONVIP2(ip)                                             \
  ((ip) >> 24) & 0xFF, ((ip) >> 16) & 0xFF, ((ip) >> 8) & 0xFF, \
      ((ip) >> 0) & 0xFF

// session.c deleted — all functions inlined here.
// rust_add_ip_lockout is declared in yuri.h (cbindgen-generated)

static inline int null_accept(int fd) { (void)fd; return 0; }
static inline int null_shutdown(int fd) { (void)fd; return 0; }
static inline int null_timeout(int fd) { (void)fd; return 0; }
static inline int null_parse(int fd) {
  if (rust_session_get_eof(fd)) {
    rust_session_set_eof(fd, 1);
    return 0;
  }
  printf("[session] null_parse fd=%d\n", fd);
  RFIFOSKIP(fd, RFIFOREST(fd));
  return 0;
}

static inline void set_defaultparse(int (*cb)(int))    { rust_session_set_default_parse(cb); }
static inline void set_defaultaccept(int (*cb)(int))   { rust_session_set_default_accept(cb); }
static inline void set_defaulttimeout(int (*cb)(int))  { rust_session_set_default_timeout(cb); }
static inline void set_defaultshutdown(int (*cb)(int)) { rust_session_set_default_shutdown(cb); }

static inline int make_listen_port(int port) { return rust_make_listen_port(port); }
static inline int make_connection(long ip, int port) { return rust_make_connection((uint32_t)ip, port); }

static inline int session_eof(int fd) {
  if (fd < 0 || fd >= FD_SETSIZE) return -1;
  rust_session_set_eof(fd, 1);
  return 0;
}

static inline int realloc_rfifo(int fd, unsigned int rfifo_sizen, unsigned int wfifo_sizen) {
  (void)fd; (void)rfifo_sizen; (void)wfifo_sizen;
  return 0;
}

static inline int WFIFOHEADER(int fd, int packetID, int packetSize) {
  if (!rust_session_exists(fd)) return 0;
  WFIFOHEAD(fd, packetSize + 3);
  WFIFOB(fd, 0) = 0xAA;
  WFIFOW(fd, 1) = SWAP16(packetSize);
  WFIFOB(fd, 3) = packetID;
  WFIFOB(fd, 4) = rust_session_increment(fd);
  return 0;
}

static inline void c_update_fd_max(int fd) {
  if (fd + 1 > fd_max) fd_max = fd + 1;
}
