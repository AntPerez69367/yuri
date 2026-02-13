#include "core.h"

#include <ctype.h>
#include <signal.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/time.h>
#include <time.h>

#include "yuri.h"
#include "db.h"
#include "session.h"
#include "timer.h"

extern int do_init(int, char **);
// term_func is now managed by Rust - removed static variable

// Main server entry point that runs the networking/logic loop for the
// map/login/char servers
int main(int argc, char **argv) {
  struct timeval start;
  int tick = 0;
  bool run = true;

  gettimeofday(&start, NULL);

  // Initialize Rust core state (replaces server_shutdown = 0)
  rust_core_init();

  do_socket();

  signal(SIGPIPE, handle_signal);
  signal(SIGTERM, handle_signal);
  signal(SIGINT, handle_signal);
  db_init();
  timer_init();

  // Each server is required to implement their own do_init function callback
  do_init(argc, argv);

  /**
   * Run the main server loop, ticking every 10ms This is currently single
   * threaded and is not particularly efficient.
   *
   * Previously, do_sendrecv was setup to block but I have made it non-blocking
   * for the time being because the timers get wonky if they don't tick
   * frequently.
   *
   * In the future, timers should probably run in a dedicated thread and move
   * socket processing to an async event loop.
   **/
  while (run) {
    tick = gettick_nocache();

    timer_do(tick);
    do_sendrecv();
    do_parsepacket();

    // Check Rust shutdown flag instead of C global variable
    if (rust_should_shutdown()) {
      run = false;
    }

    nanosleep((struct timespec[]){{0, SERVER_TICK_RATE_NS}}, NULL);
  }

  // Cleanup Rust state before exiting normally
  rust_core_cleanup();

  return 0;
}

void handle_signal(int signal) {
  // Delegate all signal handling to Rust
  // Rust will call the termination callback if set and request shutdown
  rust_handle_signal(signal);

  // For SIGINT/SIGTERM, do cleanup and exit
  // (SIGPIPE is ignored by Rust, so we won't reach here for it)
  if (signal == SIGINT || signal == SIGTERM) {
    timer_clear();
    for (int i = 0; i < fd_max; i++) {
      if (!session[i]) {
        continue;
      }
      // close(i);
      session_eof(i);
    }
    rust_core_cleanup();  // Clean up Rust state before exit
    exit(0);
  }
}

void set_termfunc(term_func_t new_term_func) {
  // Delegate to Rust - it will store the callback safely
  rust_set_termfunc(new_term_func);
}