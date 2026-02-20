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

int fd_max;  // Highest active fd + 1; updated via c_update_fd_max callback.

extern int do_init(int, char **);
// term_func is now managed by Rust - removed static variable

// Main server entry point - Rust owns the event loop
int main(int argc, char **argv) {
  // Initialize Rust core state
  rust_core_init();
  // Register fd_max updater so Rust keeps C's fd_max current
  rust_register_fd_max_updater(c_update_fd_max);

  signal(SIGPIPE, handle_signal);
  signal(SIGTERM, handle_signal);
  signal(SIGINT, handle_signal);
  db_init();
  timer_init();

  // Each server implements do_init which:
  // - Calls rust_config_read()
  // - Registers callbacks via set_defaultparse/timeout/shutdown
  // - Calls make_listen_port() (now routes to Rust)
  // - Sets up timers
  do_init(argc, argv);

  // Rust takes over the event loop.
  // Blocks until shutdown signal is received.
  // Port 0 = use listeners already registered by do_init via make_listen_port.
  int rc = rust_server_run(0);
  timer_clear();
  rust_core_cleanup();

  return rc == 0 ? EXIT_SUCCESS : EXIT_FAILURE;
}

void handle_signal(int signal) {
  // Delegate to Rust - sets shutdown flag and calls termination callback
  rust_handle_signal(signal);

  // For SIGINT/SIGTERM: Rust event loop will detect shutdown flag on next tick
  // and exit gracefully, calling session shutdown callbacks.
  // SIGPIPE is ignored by Rust so we never reach here for it.
}

void set_termfunc(term_func_t new_term_func) {
  // Delegate to Rust - it will store the callback safely
  rust_set_termfunc(new_term_func);
}