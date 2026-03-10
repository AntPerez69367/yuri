//! Time helpers and timer system used by game logic.
//!
//! Moved from `crate::timer` (src/timer.rs) — pure Rust timer system with no C dependencies.
//!
//! # Locking model
//! The map server runs its event loop on a single thread, so `TIMER_STATE`
//! is never accessed concurrently.  A `Mutex` is present only to satisfy the
//! `OnceLock<T>: Sync` bound — it will never actually contend.
//!
//! **Important:** `timer_do` must release the `MutexGuard` before invoking a
//! timer callback, because callbacks may call `timer_insert` / `timer_remove`,
//! which re-acquire the same `Mutex`.  `std::sync::Mutex` is non-reentrant;
//! holding the guard across the call would deadlock the thread.

use std::sync::{Mutex, OnceLock};
use std::time::Instant;

// ──────────────────────────────────────────────────────────────────────────────
// Monotonic clock
// ──────────────────────────────────────────────────────────────────────────────

static START: OnceLock<Instant> = OnceLock::new();

/// Return milliseconds elapsed since the first call (monotonic, wraps at u32::MAX ~49 days).
#[inline]
pub fn get_tick_ms() -> u32 {
    START.get_or_init(Instant::now).elapsed().as_millis() as u32
}

/// `unsigned int gettick_nocache(void)` — no caching on Linux anyway.
pub fn gettick_nocache() -> u32 {
    get_tick_ms()
}

/// `unsigned int gettick(void)`.
pub fn gettick() -> u32 {
    get_tick_ms()
}

/// Returns day-of-week adjusted for the original server's
/// timezone offset (UTC-5, mapped so Monday=4 … Sunday=3).
pub fn get_day() -> i32 {
    // Mirror the C formula: ((t - 18000) % 604800) / 86400
    // where 18000 = 5*3600 (UTC-5 offset) and 604800 = 7*86400.
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let day = ((t - 18000).rem_euclid(604800)) / 86400;
    if day < 4 {
        (day + 4) as i32
    } else {
        (day - 3) as i32
    }
}

/// Returns the local hour (0-23).
pub fn get_hour() -> i32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    // Use local time via libc localtime_r.
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as libc::time_t)
        .unwrap_or(0);
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        tm.tm_hour as i32
    }
}

/// Returns the local minute (0-59).
pub fn get_minute() -> i32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as libc::time_t)
        .unwrap_or(0);
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        tm.tm_min as i32
    }
}

/// Returns the local second (0-59).
pub fn get_second() -> i32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as libc::time_t)
        .unwrap_or(0);
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        tm.tm_sec as i32
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Timer data types
// ──────────────────────────────────────────────────────────────────────────────

const TIMER_ONCE_AUTODEL: u8 = 0x01;
const TIMER_INTERVAL: u8 = 0x02;
const TIMER_REMOVE_HEAP: u8 = 0x10;

/// Callback signature matching `int (*func)(int, int)`.
type TimerFn = unsafe fn(i32, i32) -> i32;

struct TimerData {
    tick: u32,
    func: Option<TimerFn>,
    /// Combination of TIMER_* flags.
    typ: u8,
    interval: u32,
    id: i32,
    data1: i32,
    #[allow(dead_code)]
    data2: i32,
}

impl TimerData {
    const fn zeroed() -> Self {
        TimerData {
            tick: 0,
            func: None,
            typ: 0,
            interval: 0,
            id: 0,
            data1: 0,
            data2: 0,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Global timer state
// Single-threaded game loop — the Mutex never actually contends; it exists
// solely to satisfy the OnceLock<T>: Sync bound.  See module-level doc.
// ──────────────────────────────────────────────────────────────────────────────

struct TimerState {
    /// Flat array of timer slots (grows by 256 at a time, matching C).
    data: Vec<TimerData>,
    /// Heap of timer-slot indices sorted highest-tick-first (smallest at end).
    heap: Vec<usize>,
    /// Pool of freed slot indices available for reuse.
    free_list: Vec<usize>,
}

impl TimerState {
    fn new() -> Self {
        TimerState {
            data: Vec::new(),
            heap: Vec::new(),
            free_list: Vec::new(),
        }
    }

    /// Acquire a free timer slot index.
    fn acquire(&mut self) -> usize {
        // Try the free list first.
        while let Some(tid) = self.free_list.pop() {
            if tid < self.data.len() && self.data[tid].typ == 0 {
                return tid;
            }
        }
        // Extend the data array.
        let tid = self.data.len();
        self.data.push(TimerData::zeroed());
        tid
    }

    /// Insert a timer slot index into the min-heap (sorted highest-first so
    /// pop() gives the smallest tick — matches C push_timer_heap logic).
    fn heap_push(&mut self, tid: usize) {
        let target = self.data[tid].tick;
        // Binary search: find the position where target belongs (descending order).
        // C sorts descending so heap[last] is the smallest (next to fire).
        let pos = self.heap.partition_point(|&h| {
            // "tick" of slot h vs target — keep descending: place target
            // after all slots with tick > target.
            let htick = self.data[h].tick;
            // DIFF_TICK(htick, target) > 0  ⟺  htick > target (wrapping)
            (htick.wrapping_sub(target)) as i32 > 0
        });
        self.heap.insert(pos, tid);
    }
}

static TIMER_STATE: OnceLock<Mutex<TimerState>> = OnceLock::new();

fn state() -> std::sync::MutexGuard<'static, TimerState> {
    TIMER_STATE
        .get_or_init(|| Mutex::new(TimerState::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

// ──────────────────────────────────────────────────────────────────────────────
// Public C-ABI exports
// ──────────────────────────────────────────────────────────────────────────────

/// `void timer_init(void)` — no-op.
pub fn timer_init() {}

/// `int timer_clear(void)` — free all timer memory.
///
/// Clears the three vecs (preserving their heap allocations) rather than
/// dropping and recreating `TimerState`, because `OnceLock` does not permit
/// replacing its value once initialised.
pub fn timer_clear() -> i32 {
    let mut ts = state();
    ts.data.clear();
    ts.heap.clear();
    ts.free_list.clear();
    0
}

/// `int timer_insert(uint32_t initial_delay_ms, uint32_t interval_ms, fn, id, data) -> timer_id`
///
/// First arg is the initial delay (added to gettick()), second is the repeat interval.
pub fn timer_insert(
    tick_delay: u32,
    interval: u32,
    func: Option<TimerFn>,
    id: i32,
    data: i32,
) -> i32 {
    let mut s = state();
    let tid = s.acquire();
    s.data[tid] = TimerData {
        tick: get_tick_ms().wrapping_add(tick_delay),
        func,
        typ: if interval == 0 { TIMER_ONCE_AUTODEL } else { TIMER_INTERVAL },
        interval,
        id,
        data1: data,
        data2: 0,
    };
    s.heap_push(tid);
    tid as i32
}

/// `int timer_remove(int tid)` — mark a timer for deletion.
pub fn timer_remove(tid: i32) -> i32 {
    let mut s = state();
    let tid = tid as usize;
    if tid >= s.data.len() {
        return -1;
    }
    s.data[tid].func = None;
    s.data[tid].typ = TIMER_ONCE_AUTODEL;
    0
}

/// `const struct TimerData* get_timer(int tid)` — returns NULL (only used for debug checks).
pub fn get_timer(_tid: i32) -> *const () {
    std::ptr::null()
}

/// `int timer_do(uint32_t tick)` — fire all expired timers, return ms to next.
///
/// Must be called from the event loop every ~10 ms.
pub fn timer_do(tick: u32) -> i32 {
    const TIMER_MIN_INTERVAL: i32 = 50;
    const TIMER_MAX_INTERVAL: i32 = 1000;

    let mut diff: i32 = 1000;

    loop {
        // Acquire the lock at the top of each iteration so that we can release
        // it before invoking the callback (see locking model in module docs).
        let mut s = state();

        // The heap is sorted highest-first; the last element is the smallest tick.
        let tid = match s.heap.last() {
            Some(&t) => t,
            None => break,
        };

        diff = (s.data[tid].tick.wrapping_sub(tick)) as i32;
        if diff > 0 {
            break; // not yet expired
        }

        s.heap.pop();
        s.data[tid].typ |= TIMER_REMOVE_HEAP;

        // Extract callback data before releasing the lock.  Callbacks
        // (e.g. rust_mob_timer_spawns, rust_pc_timer) may call timer_insert /
        // timer_remove, which also call state().lock().  std::sync::Mutex is
        // non-reentrant, so holding the guard across the call would deadlock.
        let (f, d1, d2) = {
            let entry = &s.data[tid];
            (entry.func, entry.id, entry.data1)
        };
        drop(s); // release lock — callbacks are now free to modify timer state

        let to_del = if let Some(f) = f {
            // SAFETY: timer callbacks are C-ABI unsafe fn pointers registered
            // by game logic on the single-threaded event loop.
            unsafe { f(d1, d2) != 0 }
        } else {
            false
        };
        // Note: the callback may have called timer_remove(tid) during the
        // lock-free window, setting typ = TIMER_ONCE_AUTODEL and clearing func.
        // The to_del path below is still safe: it only ORs in TIMER_REMOVE_HEAP
        // on a slot already marked TIMER_ONCE_AUTODEL, which is idempotent and
        // causes the slot to be freed in the TIMER_ONCE_AUTODEL arm below.

        // Re-acquire for post-callback bookkeeping.
        let mut s = state();

        if to_del {
            // mark for one-shot deletion
            s.data[tid].func = None;
            s.data[tid].typ = TIMER_ONCE_AUTODEL | TIMER_REMOVE_HEAP;
        }

        if s.data[tid].typ & TIMER_REMOVE_HEAP != 0 {
            s.data[tid].typ &= !TIMER_REMOVE_HEAP;

            match s.data[tid].typ {
                TIMER_ONCE_AUTODEL => {
                    s.data[tid].typ = 0;
                    s.free_list.push(tid);
                }
                TIMER_INTERVAL => {
                    // Reschedule: if we're very late (>1s), snap to now+interval.
                    if diff <= -1000 {
                        s.data[tid].tick = tick.wrapping_add(s.data[tid].interval);
                    } else {
                        s.data[tid].tick = s.data[tid].tick.wrapping_add(s.data[tid].interval);
                    }
                    s.heap_push(tid);
                }
                _ => {}
            }
        }
        // Guard `s` drops here at end of iteration, before next state() call.
    }

    diff.clamp(TIMER_MIN_INTERVAL, TIMER_MAX_INTERVAL)
}
