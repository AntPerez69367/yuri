#pragma once

#include <netinet/in.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/socket.h>
#include <sys/types.h>

#include "yuri.h"

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

// Struct declaration
struct socket_data {
  int eof;
  unsigned char *rdata, *wdata;
  size_t max_rdata, max_wdata;
  size_t rdata_size, wdata_size;
  size_t rdata_pos;
  time_t rdata_tick;
  struct sockaddr_in client_addr;
  int (*func_recv)(int);
  int (*func_send)(int);
  int (*func_parse)(int);
  int (*func_timeout)(int);
  int (*func_shutdown)(int);
  void *session_data;
  unsigned char increment;
  char name[32];
};

extern struct socket_data *session[FD_SETSIZE];
extern size_t rfifo_size, wfifo_size;
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

// C session functions (from session.c)
void create_session(int);
int WFIFOHEADER(int, int, int);
int make_listen_port(int);
int make_connection(long, int);
int session_eof(int);
int realloc_fifo(int, size_t);
void log_session(int, const char *);
int add_ip_lockout(unsigned int);
int do_sendrecv();
int do_parsepacket(void);
void do_socket(void);
int realloc_rfifo(int fd, unsigned int rfifo_sizen, unsigned int wfifo_sizen);
void set_defaultparse(int (*)(int));
void set_defaultaccept(int (*)(int));
void set_defaulttimeout(int (*)(int));
void set_defaultshutdown(int (*)(int));

int Remove_Throttle(int none, int nonetoo);
void c_update_fd_max(int fd);
