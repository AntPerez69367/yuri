#include "session.h"

#include "yuri.h"  // Rust FFI functions (rust_should_shutdown)
#include <errno.h>
#include <netinet/in.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <sys/timeb.h>
#include <time.h>
#include <unistd.h>

#include "core.h"
#include "mmo.h"
#include "net_crypt.h"
#include "timer.h"

#define SOCKET_ERROR (-1)
#define S_EAGAIN EAGAIN
#define S_EWOULDBLOCK EWOULDBLOCK
#define sErrno errno
#define WFIFO_SIZE (16 * 1024)

fd_set readfds;
int fd_max;
int activeFD = -1;
size_t rfifo_size = (16 * 1024);
size_t wfifo_size = (16 * 1024);
time_t last_tick;
time_t stall_time = 60;

// server_shutdown removed - now managed by Rust via rust_should_shutdown()

struct socket_data* session[FD_SETSIZE];

int null_parse(int fd);
int null_accept(int fd);
int null_timeout(int fd);
int null_shutdown(int fd);
int Check_Throttle(struct sockaddr_in S);

int (*default_func_parse)(int) = null_parse;
int (*default_func_accept)(int) = null_accept;
int (*default_func_timeout)(int) = null_timeout;
int (*default_func_shutdown)(int) = null_shutdown;
extern int char_fd;

typedef struct _connect_history {
  struct _connect_history* next;
  uint32_t ip;
  uint32_t tick;
  int count;
  unsigned ddos : 1;
} ConnectHistory;

typedef struct _access_control {
  uint32_t ip;
  uint32_t mask;
} AccessControl;

enum _aco { ACO_DENY_ALLOW, ACO_ALLOW_DENY, ACO_MUTUAL_FAILURE };

struct stThrottle {
  struct sockaddr_in Ip;
  int Times;
  struct stThrottle* Next;
};

struct stThrottle* Throttles = NULL;

static int ddos_interval = 3 * 1000;
static int ddos_autoreset = 10 * 60 * 1000;
/// Connection history, an array of linked lists.
/// The array's index for any ip is ip&0xFFFF
static ConnectHistory* connect_history[0x10000];

int add_ip_lockout(unsigned int n) {
  unsigned int ip = ntohl(n);
  ConnectHistory* hist = connect_history[ip & 0xFFFF];
  while (hist) {
    if (ip == hist->ip) {
      hist->ddos = 1;
      hist->tick = gettick();
      printf("[lockout] ip=%u.%u.%u.%u\n", CONVIP2(ip));
      return 0;
    }
    hist = hist->next;
  }

  CALLOC(hist, ConnectHistory, 1);
  memset(hist, 0, sizeof(ConnectHistory));
  hist->ip = ip;
  hist->tick = gettick();
  hist->ddos = 1;
  hist->next = connect_history[ip & 0xFFFF];
  connect_history[ip & 0xFFFF] = hist;
  printf("[lockout] ip=%u.%u.%u.%u\n", CONVIP2(ip));
  return 0;
}
/// Timer function.
/// Deletes old connection history records.
static int connect_check_clear(int d, int data) {
  int i;
  int clear = 0;
  int list = 0;
  ConnectHistory root;
  ConnectHistory* prev_hist;
  ConnectHistory* hist;
  unsigned int tick = gettick();

  for (i = 0; i < 0x10000; ++i) {
    prev_hist = &root;
    root.next = hist = connect_history[i];
    while (hist) {
      if ((!hist->ddos && DIFF_TICK(tick, hist->tick) > ddos_interval * 3) ||
          (hist->ddos && DIFF_TICK(tick, hist->tick) >
                             ddos_autoreset)) {  // Remove connection history
        prev_hist->next = hist->next;
        FREE(hist);
        hist = prev_hist->next;
        clear++;
      } else {
        prev_hist = hist;
        hist = hist->next;
      }
      list++;
    }
    connect_history[i] = root.next;
  }

  return list;
}

/// Parses the ip address and mask and puts it into acc.
/// Returns 1 is successful, 0 otherwise.
int access_ipmask(const char* str, AccessControl* acc) {
  uint32_t ip;
  uint32_t mask;
  unsigned int a[4];
  unsigned int m[4];
  int n;

  if (strcmp(str, "all") == 0) {
    ip = 0;
    mask = 0;
  } else {
    if (((n = sscanf(str, "%u.%u.%u.%u/%u.%u.%u.%u", a, a + 1, a + 2, a + 3, m,
                     m + 1, m + 2, m + 3)) != 8 &&  // not an ip + standard mask
         (n = sscanf(str, "%u.%u.%u.%u/%u", a, a + 1, a + 2, a + 3, m)) !=
             5 &&  // not an ip + bit mask
         (n = sscanf(str, "%u.%u.%u.%u", a, a + 1, a + 2, a + 3)) !=
             4) ||  // not an ip
        a[0] > 255 ||
        a[1] > 255 || a[2] > 255 || a[3] > 255 ||  // invalid ip
        (n == 8 && (m[0] > 255 || m[1] > 255 || m[2] > 255 ||
                    m[3] > 255)) ||  // invalid standard mask
        (n == 5 && m[0] > 32)) {     // invalid bit mask
      return 0;
    }
    ip = (uint32_t)(a[0] | (a[1] << 8) | (a[2] << 16) | (a[3] << 24));
    if (n == 8) {  // standard mask
      mask = (uint32_t)(a[0] | (a[1] << 8) | (a[2] << 16) | (a[3] << 24));
    } else if (n == 5) {  // bit mask
      mask = 0;
      while (m[0]) {
        mask = (mask >> 1) | 0x80000000;
        --m[0];
      }
      mask = ntohl(mask);
    } else {  // just this ip
      mask = 0xFFFFFFFF;
    }
  }

  acc->ip = ip;
  acc->mask = mask;
  return 1;
}

void setsocketopts(int fd) {
  int yes = 1;  // reuse fix
#if !defined(WIN32)
  // set SO_REAUSEADDR to true, unix only. on windows this option causes
  // the previous owner of the socket to give up, which is not desirable
  // in most cases, neither compatible with unix.
  setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, (char*)&yes, sizeof(yes));
#ifdef SO_REUSEPORT
  setsockopt(fd, SOL_SOCKET, SO_REUSEPORT, (char*)&yes, sizeof(yes));
#endif
#endif

  // Set the socket into no-delay mode; otherwise packets get delayed for up to
  // 200ms, likely creating server-side lag. The RO protocol is mainly
  // single-packet request/response, plus the FIFO model already does packet
  // grouping anyway.
  // setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, (char *)&yes, sizeof(yes));
  setsockopt(fd, IPPROTO_TCP, 0, (char*)&yes, sizeof(yes));

  // force the socket into no-wait, graceful-close mode (should be the default,
  // but better make sure)
  //(http://msdn.microsoft.com/library/default.asp?url=/library/en-us/winsock/winsock/closesocket_2.asp)
  {
    struct linger opt;
    opt.l_onoff = 0;   // SO_DONTLINGER
    opt.l_linger = 0;  // Do not care
    if (setsockopt(fd, SOL_SOCKET, SO_LINGER, (char*)&opt, sizeof(opt))) {
      printf("setsocketopts: Unable to set SO_LINGER mode for connection %d!\n",
             fd);
    }
    // ShowWarning("setsocketopts: Unable to set SO_LINGER mode for connection
    // #%d!\n", fd);
  }

  /*setsockopt(fd,SOL_SOCKET,SO_SNDBUF,(char*)&wfifo_size,(int)sizeof(wfifo_size));
  setsockopt(fd,SOL_SOCKET,SO_RCVBUF,(char*)&rfifo_size,(int)sizeof(rfifo_size));
  */
}

void set_nonblocking(int fd, unsigned long yes) {
  // FIONBIO Use with a nonzero argp parameter to enable the nonblocking mode of
  // socket s. The argp parameter is zero if nonblocking is to be disabled.
  if (ioctl(fd, FIONBIO, &yes) != 0) {
    printf("[session] nonblocking failed\n");
  }
  // printf("set_nonblocking: Failed to set socket %d to non-blocking mode (code
  // %d) - Please report this!!!\n", fd, sErrno);
}
// Set default parse function
//----------------------------
void set_defaultparse(int (*defaultparse)(int)) {
  default_func_parse = defaultparse;
  rust_session_set_default_parse(defaultparse);
}

void set_defaultshutdown(int (*defaultshutdown)(int)) {
  default_func_shutdown = defaultshutdown;
  rust_session_set_default_shutdown(defaultshutdown);
}
void set_defaultaccept(int (*defaultaccept)(int)) {
  default_func_accept = defaultaccept;
  rust_session_set_default_accept(defaultaccept);
}

void set_defaulttimeout(int (*default_timeout)(int)) {
  default_func_timeout = default_timeout;
  rust_session_set_default_timeout(default_timeout);
}

// Socket recv and send
//----------------------------
int recv_to_fifo(int fd) {
  int len;

  if ((fd < 0) || (fd >= FD_SETSIZE)) {
    return -1;
  }

  if ((NULL == session[fd])) {
    return -1;
  }

  // printf("recv_to_fifo : %d %d\n", fd, rust_session_get_eof(fd));
  if (rust_session_get_eof(fd)) {
    return -1;
  }

  len = recv(fd, (char*)session[fd]->rdata + session[fd]->rdata_size,
             (int)RFIFOSPACE(fd), 0);

  /*	len = read(fd, r_buffer, (512*1024));
          if(len+session[fd]->rdata_size>session[fd]->max_rdata) {
          diff=len+session[fd]->rdata_size-session[fd]->max_rdata+16384;
          realloc_fifo(fd,session[fd]->max_rdata+diff,session[fd]->max_wdata);
          }

          memcpy(session[fd]->rdata+session[fd]->rdata_size,r_buffer,len);
*/

  //{ int i; printf("recv %d : ",fd); for(i=0;i<len;i++){ printf("%02x
  //",RFIFOB(fd,session[fd]->rdata_size+i)); } printf("\n");}
  if (len == SOCKET_ERROR) {  // An exception has occured
    if (sErrno != S_EWOULDBLOCK && sErrno != S_EAGAIN) {
      // printf("recv_to_fifo: code %d, closing connection #%d\n",sErrno,fd);
      // session_eof(fd);

      rust_session_set_eof(fd, 3);
    }

    // sending error eof
    return 0;
  }

  if (len == 0) {  // Normal connection end.
    // session_eof(fd);
    rust_session_set_eof(fd, 4);  // zerolen eof
    return 0;
  }

  session[fd]->rdata_size += len;
  session[fd]->rdata_tick = last_tick;

  return 0;
}
/*int RFIFOSKIP(int fd, size_t len)
{
    //struct socket_data *s;


        //s = session[fd];
        if(!session[fd]) return 0;

        if ( session[fd]->rdata_size < session[fd]->rdata_pos + len ) {
                //ShowError("RFIFOSKIP: skipped past end of read buffer!
Adjusting from %d to %d (session #%d)\n", len, RFIFOREST(fd), fd); len =
RFIFOREST(fd);
        }

        session[fd]->rdata_pos = session[fd]->rdata_pos + len;
        return 0;
}*/
int WFIFOHEADER(int fd, int packetID, int packetSize) {
  if (!rust_session_exists(fd)) {
    return 0;
  }
  WFIFOHEAD(fd, packetSize + 3);
  WFIFOB(fd, 0) = 0xAA;
  WFIFOW(fd, 1) = SWAP16(packetSize);
  WFIFOB(fd, 3) = packetID;
  WFIFOB(fd, 4) = rust_session_increment(fd);
  return 0;
}
int send_from_fifo(int fd) {
  int len;
  // struct socket_data *p;

  if ((fd < 0) || (fd >= FD_SETSIZE)) {
    return -1;
  }

  if ((NULL == session[fd])) {
    return -1;
  }

  // printf("recv_to_fifo : %d %d\n", fd, rust_session_get_eof(fd));
  if (rust_session_get_eof(fd)) {
    return -1;
  }

  if (!session[fd]->wdata_size) {
    return 0;  // Nothing to send
  }

  if (!session[fd]->wdata) {
    rust_session_set_eof(fd, 5);
    return -1;
  }
  len = send(fd, (char*)session[fd]->wdata, session[fd]->wdata_size,
             0);  // MSG_NOSIGNAL
  //{ int i; printf("send %d : ",fd);  for(i=0;i<len;i++){ printf("%02x
  //",session[fd]->wdata[i]); } printf("\n");}
  if (len == SOCKET_ERROR) {
    if (sErrno != S_EWOULDBLOCK && sErrno != S_EAGAIN) {
      // printf("send_from_fifo: error %d, ending connection #%d\n",sErrno,fd);
      session[fd]->wdata_size = 0;
      rust_session_set_eof(fd, 2);
      // session_eof(fd);
    }
    // sending eof
    return 0;
  }

  if (len > 0) {
    // some data could not be transferred?
    // shift unsent data to the beginning of the queue
    if ((size_t)len < session[fd]->wdata_size) {
      memmove(session[fd]->wdata, session[fd]->wdata + len,
              session[fd]->wdata_size - len);
    }

    session[fd]->wdata_size -= len;
  }
  return 0;
}

// Default parse function
//----------------------------
int null_accept(int fd) { return 0; }

int null_parse(int fd) {
  if (rust_session_get_eof(fd)) {
    rust_session_set_eof(fd, 1);
    // session_eof(fd);
    return 0;
  }
  printf("[session] null_parse fd=%d\n", fd);
  RFIFOSKIP(fd, RFIFOREST(fd));
  return 0;
}
int null_shutdown(int fd) { return 0; }
int null_timeout(int fd) { return 0; }
// Socket main function
//----------------------------
int connect_client(int listen_fd) {
  int fd;
  struct sockaddr_in client_address;
  socklen_t len;

  len = sizeof(client_address);

  fd = accept(listen_fd, (struct sockaddr*)&client_address, &len);
  if (Check_Throttle(client_address)) {
    close(fd);
    return -1;
  }

  if (fd < 0) {
    return -1;
  }
  if (fd >= FD_SETSIZE) {
    close(fd);
    return -1;
  }
  if (fd == 0) {
    printf("Socket #0 is reserved!\n");
    close(fd);
    return -1;
  }
  if (fd_max <= fd) {
    fd_max = fd + 1;
  }

  if (rust_session_exists(fd)) {
    printf("Socket fail: Already in use\n");
    return -1;
  }
  // printf("Accepting connection\n");
  /*result = fcntl(fd,F_SETFL,O_NONBLOCK);
//	setsockopt(fd,SOL_SOCKET,SO_REUSEADDR,NULL,0);
  setsockopt(fd,SOL_SOCKET,SO_REUSEADDR,(char *)&yes,sizeof yes); // reuse fix
//
#ifdef SO_REUSEPORT
//	setsockopt(fd,SOL_SOCKET,SO_REUSEPORT,NULL,0);
  setsockopt(fd,SOL_SOCKET,SO_REUSEPORT,(char *)&yes,sizeof yes); //reuse fix
#endif
//
//	setsockopt(fd,IPPROTO_TCP,TCP_NODELAY,NULL,0);
  setsockopt(fd,IPPROTO_TCP,TCP_NODELAY,(char *)&yes,sizeof yes); // reuse fix
*/
  setsocketopts(fd);
  set_nonblocking(fd, 1);

  // if(!connect_check(ntohl(client_address.sin_addr.s_addr))) {
  // close(fd);
  // return -1;
  //}
  // For Loggin this anomaly
  // log_session(fd,"Connecting Client: Creating FD(%d)\n");
  FD_SET(fd, &readfds);
  create_session(fd);
  session[fd]->client_addr = client_address;
  default_func_accept(fd);
  // printf("new_session : %d %d\n",fd,rust_session_get_eof(fd));
  return fd;
}

int make_listen_port(int port) {
  return rust_make_listen_port(port);
}

int make_connection(long ip, int port) {
  return rust_make_connection((uint32_t)ip, port);
}
void create_session(int fd) {
  // result = fcntl(fd, F_SETFL, O_NONBLOCK);
  // if(!session[fd]) {
  CALLOC(session[fd], struct socket_data, 1);
  CALLOC(session[fd]->rdata, unsigned char, rfifo_size);
  CALLOC(session[fd]->wdata, unsigned char, wfifo_size);
  //}
  session[fd]->max_rdata = rfifo_size;
  session[fd]->max_wdata = wfifo_size;
  session[fd]->func_recv = recv_to_fifo;
  session[fd]->func_send = send_from_fifo;
  rust_session_set_parse(fd, default_func_parse);
  session[fd]->func_timeout = default_func_timeout;

  session[fd]->rdata_tick = last_tick;
}
void flush_fifo(int fd) {
  if (session[fd] != NULL && session[fd]->func_send != NULL) {
    session[fd]->func_send(fd);
  }
}
int session_eof(int fd) {
  if (fd < 0 || fd >= FD_SETSIZE) {
    return -1;
  }

  if (session[fd] == NULL) {
    // Rust-managed session: fd is a virtual session ID, not an OS fd.
    // Let Rust's session_io_task handle the actual socket cleanup.
    rust_session_set_eof(fd, 1);
    return 0;
  }

  flush_fifo(fd);
  FD_CLR(fd, &readfds);
  shutdown(fd, SHUT_RDWR);
  close(fd);

  // log_session(fd,"Closing client FD(%d)\n");
  if (rust_session_exists(fd)) {
    free(session[fd]->rdata);
    free(session[fd]->wdata);
    free(rust_session_get_data(fd));
    free(session[fd]);
    session[fd] = NULL;
    // log_session(fd,"Freeing Session FD(%d)\n");
  }

  return 0;
}
int realloc_rfifo(int fd, unsigned int rfifo_sizen, unsigned int wfifo_sizen) {
  if (!rust_session_exists(fd)) {
    return 0;
  }

  if (session[fd] == NULL) {
    // Rust-managed session: read/write buffers are Vec<u8> and resize dynamically.
    return 0;
  }

  if (session[fd]->max_rdata != rfifo_sizen &&
      session[fd]->rdata_size < rfifo_sizen) {
    REALLOC(session[fd]->rdata, unsigned char, rfifo_sizen);
    session[fd]->max_rdata = rfifo_sizen;
  }

  if (session[fd]->max_wdata != wfifo_sizen &&
      session[fd]->wdata_size < wfifo_sizen) {
    REALLOC(session[fd]->wdata, unsigned char, wfifo_sizen);
    session[fd]->max_wdata = wfifo_sizen;
  }
  return 0;
}

int realloc_fifo(int fd, size_t addition) {
  size_t newsize;

  if (!rust_session_exists(fd)) {
    return 0;
  }

  if (session[fd] == NULL) {
    // Rust-managed session: buffers resize dynamically.
    return 0;
  }

  // if(!rfifo_sizen) return 0;
  if (!addition) {
    return 0;
  }
  // if(!s->rdata) return 0;
  if (!session[fd]->wdata) {
    return 0;
  }
  if (rust_session_get_eof(fd)) {
    return 0;
  }
  // g_Size=session[fd]->wdata_size+wfifo_sizen;
  if (session[fd]->wdata_size + addition > session[fd]->max_wdata) {
    // REALLOC(s->wdata, unsigned char, wfifo_sizen);
    newsize = wfifo_size;
    while (session[fd]->wdata_size + addition > newsize) {
      newsize += wfifo_size;
    }
    // setsockopt(fd,SOL_SOCKET,SO_SNDBUF,(char*)&s->max_wdata,(int)sizeof(s->max_wdata));

  } else if (session[fd]->max_wdata >= FIFOSIZE_SERVER) {
    if ((session[fd]->wdata_size + addition) * 4 < session[fd]->max_wdata) {
      newsize = session[fd]->max_wdata / 2;
    } else {
      return 0;
    }
  } else if (session[fd]->max_wdata >= 2 * wfifo_size &&
             (session[fd]->wdata_size + addition) * 4 <
                 session[fd]->max_wdata) {
    newsize = session[fd]->max_wdata / 2;
  } else {  // no change
    return 0;
  }

  REALLOC(session[fd]->wdata, unsigned char, newsize);
  /*if(!session[fd]->wdata) {
          printf("Realloc Failure: EOF set!\n");
          session[fd]->eof=6;
  }*/
  session[fd]->max_wdata = newsize;

  return 0;
}

// Socket Main Routine
//----------------------------
int do_sendrecv() {
  fd_set rfd, wfd;
  struct timeval timeout;
  int ret, i;
  // struct socket_data* p;

  rfd = readfds;
  // memcpy(&rfd,&readfds,sizeof(fd_set));
  FD_ZERO(&wfd);
  for (i = 1; i < fd_max; i++) {
    // p=session[i];
    if (!session[i] && FD_ISSET(i, &rfd)) {
      printf("[session] force clr fd=%d\n", i);
      FD_CLR(i, &rfd);
      continue;
    }

    if (!rust_session_exists(i)) {
      continue;
    }
    if (session[i]->wdata_size) {
      FD_SET(i, &wfd);
    }
  }

  timeout.tv_sec = 0;
  timeout.tv_usec = 0;
  // memcpy(&rfd,&readfds,sizeof(rfd));
  ret = select(fd_max, &rfd, &wfd, NULL, &timeout);

  last_tick = time(NULL);

  if (ret <= 0) {
    return 0;
  }

  for (i = 1; i < fd_max; i++) {
    activeFD = i;
    // p=session[i];
    if (!rust_session_exists(i)) {
      continue;
    }
    if (session[i]->wdata_size) {
      // printf("write:%d\n",i);
      if (session[i]->func_send) {
        session[i]->func_send(i);
      }
    }
    if (FD_ISSET(i, &rfd)) {
      // printf("read:%d\n",i);
      if (session[i]->func_recv) {
        session[i]->func_recv(i);
      }
    }
    if (rust_session_get_eof(i)) {  // Force disconnects!
      if (session[i]->func_parse) {
        rust_session_call_parse(i);
      }
    }

    if (rust_should_shutdown() && (!FD_ISSET(i, &wfd)) && rust_session_exists(i)) {
      if (session[i]->func_shutdown) {
        session[i]->func_shutdown(i);
      }
    }

    /*if (session[i]->rdata_size == 0 && session[i]->eof == 0)
            continue;
    if (session[i]->func_parse){
            rust_session_call_parse(i);
            if (!rust_session_exists(i))
                    continue;
    }

    RFIFOFLUSH(i);*/
  }
  activeFD = -1;
  return 0;
}

int do_parsepacket(void) {
  int i;

  for (i = 1; i < fd_max; i++) {
    // p=session[i];
    activeFD = i;
    if (!rust_session_exists(i)) {
      continue;
    }
    // if (session[i]->rdata_size == 0 && session[i]->eof == 0)
    //	continue;

    if (session[i]->rdata_tick &&
        DIFF_TICK(last_tick, session[i]->rdata_tick) > stall_time) {
      // printf("Session %d timed out\n", i);
      // session[i]->eof=1;
      session[i]->func_timeout(i);
    }

    if (session[i]->func_parse) {
      rust_session_call_parse(i);
      // if (!rust_session_exists(i))
      // continue;
    }

    if (!rust_session_exists(i)) {
      continue;
    }

    if (session[i]->rdata_size == rfifo_size &&
        session[i]->max_rdata == rfifo_size) {
      rust_session_set_eof(i, 1);
      continue;
    }

    RFIFOFLUSH(i);
  }
  activeFD = -1;
  return 0;
}

// Socket init
//----------------------------
void c_update_fd_max(int fd) {
  if (fd + 1 > fd_max)
    fd_max = fd + 1;
}

void do_socket(void) {
  FD_ZERO(&readfds);
  last_tick = time(NULL);
  memset(&session, 0, FD_SETSIZE * sizeof(struct socket_data*));
  memset(connect_history, 0, sizeof(connect_history));
  // add_timer_func_list(connect_check_clear, "connect_check_clear");
  timer_insert(1000, 1000, connect_check_clear, 0, 0);
  // Create socket 0(reserved) for disconnects
  CALLOC(session[0], struct socket_data, 1);
  CALLOC(session[0]->rdata, unsigned char, rfifo_size);
  CALLOC(session[0]->wdata, unsigned char, wfifo_size);
  // (fd_max updater registered in core.c main())
}

void Add_Throttle(struct sockaddr_in S) {
  bool Found = false;
  struct stThrottle* Cur = Throttles;
  while (Cur) {
    if (Cur->Ip.sin_addr.s_addr == S.sin_addr.s_addr) {
      Cur->Times++;
      Found = true;
      break;
    }

    Cur = Cur->Next;
  }

  if (!Found) {
    struct stThrottle* NewT = malloc(sizeof(struct stThrottle));
    NewT->Ip = S;
    NewT->Times = 1;
    NewT->Next = Throttles;

    Throttles = NewT;
  }
}

int Check_Throttle(struct sockaddr_in S) {
  int Ret = 0;
  struct stThrottle* Cur = Throttles;
  while (Cur) {
    if (Cur->Ip.sin_addr.s_addr == S.sin_addr.s_addr) {
      Ret = Cur->Times;
      break;
    }
    Cur = Cur->Next;
  }

  return Ret;
}

int Remove_Throttle(int none, int nonetoo) {
  Throttles = NULL;
  return 0;
}