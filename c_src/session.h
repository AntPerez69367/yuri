#pragma once

#include <netinet/in.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/socket.h>
#include <sys/types.h>

#include "yuri.h"

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

// Buffer access macros
// These use direct C session access. Once rust_server_run() replaces
// the C event loop in core.c, these will switch to Rust FFI calls.
#define RFIFOP(fd, pos) (session[fd]->rdata + session[fd]->rdata_pos + (pos))
#define RFIFOB(fd, pos) \
  (*(unsigned char *)(session[fd]->rdata + session[fd]->rdata_pos + (pos)))
#define RFIFOW(fd, pos) \
  (*(unsigned short *)(session[fd]->rdata + session[fd]->rdata_pos + (pos)))
#define RFIFOL(fd, pos) \
  (*(unsigned int *)(session[fd]->rdata + session[fd]->rdata_pos + (pos)))
#define RFIFOSKIP(fd, len)                                                  \
  ((session[fd]->rdata_size - session[fd]->rdata_pos - (len) < 0)           \
       ? (printf("Skip error in file %s at line %d\n", __FILE__, __LINE__), \
          exit(1))                                                          \
       : (session[fd]->rdata_pos += (len)))
#define RFIFOREST(fd) (session[fd]->rdata_size - session[fd]->rdata_pos)
#define RFIFOFLUSH(fd)                                                         \
  do {                                                                         \
    if (session[fd]->rdata_size == session[fd]->rdata_pos) {                   \
      session[fd]->rdata_size = session[fd]->rdata_pos = 0;                    \
    } else {                                                                   \
      session[fd]->rdata_size -= session[fd]->rdata_pos;                       \
      memmove(session[fd]->rdata, session[fd]->rdata + session[fd]->rdata_pos, \
              session[fd]->rdata_size);                                        \
      session[fd]->rdata_pos = 0;                                              \
    }                                                                          \
  } while (0)
#define RFIFOSPACE(fd) (session[fd]->max_rdata - session[fd]->rdata_size)
#define WFIFOHEAD(fd, size)                                                \
  do {                                                                     \
    if ((fd) && session[fd]->wdata_size + (size) > session[fd]->max_wdata) \
      realloc_fifo(fd, size);                                              \
  } while (0)
#define WFIFOSPACE(fd) (session[fd]->max_wdata - session[fd]->wdata_size)
#define WFIFOP(fd, pos) \
  (char *)(session[fd]->wdata + session[fd]->wdata_size + (pos))
#define WFIFOB(fd, pos) \
  (*(unsigned char *)(session[fd]->wdata + session[fd]->wdata_size + (pos)))
#define WFIFOW(fd, pos) \
  (*(unsigned short *)(session[fd]->wdata + session[fd]->wdata_size + (pos)))
#define WFIFOL(fd, pos) \
  (*(unsigned int *)(session[fd]->wdata + session[fd]->wdata_size + (pos)))

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
int WFIFOSET(int, int);
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
