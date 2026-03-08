//! Rust replacement for c_deps/timer.c
//!
//! Provides the same ABI as the C timer system so all existing callers
//! (C code in c_deps, and Rust code via `extern "C"` / `#[link_name]`) work
//! without changes.  timer.c is removed from build.rs once this module is
//! compiled in.
//!
//! # Safety model
//! The map server is single-threaded on its event-loop thread.  All timer
//! callbacks fire from `timer_do`, which is called from the event loop.
//! The global `TIMER_STATE` is accessed only from that thread; no locking is
//! needed.

use std::os::raw::c_int;
use std::sync::OnceLock;
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

/// `unsigned int gettick_nocache(void)` — C ABI export (no caching on Linux anyway).
#[no_mangle]
pub extern "C" fn gettick_nocache() -> u32 {
    get_tick_ms()
}

/// `unsigned int gettick(void)` — C ABI export.
#[no_mangle]
pub extern "C" fn gettick() -> u32 {
    get_tick_ms()
}

// ──────────────────────────────────────────────────────────────────────────────
// Date/time helpers (also defined in timer.c, used from Lua via yuri.h)
// ──────────────────────────────────────────────────────────────────────────────

/// `int getDay(void)` — returns day-of-week adjusted for the original server's
/// timezone offset (UTC-5, mapped so Monday=4 … Sunday=3).
#[no_mangle]
pub extern "C" fn getDay() -> c_int {
    // Mirror the C formula: ((t - 18000) % 604800) / 86400
    // where 18000 = 5*3600 (UTC-5 offset) and 604800 = 7*86400.
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let day = ((t - 18000).rem_euclid(604800)) / 86400;
    if day < 4 {
        (day + 4) as c_int
    } else {
        (day - 3) as c_int
    }
}

/// `int getHour(void)` — local hour (0-23).
#[no_mangle]
pub extern "C" fn getHour() -> c_int {
    use std::time::{SystemTime, UNIX_EPOCH};
    // Use local time via libc localtime_r.
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as libc::time_t)
        .unwrap_or(0);
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        tm.tm_hour as c_int
    }
}

/// `int getMinute(void)` — local minute (0-59).
#[no_mangle]
pub extern "C" fn getMinute() -> c_int {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as libc::time_t)
        .unwrap_or(0);
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        tm.tm_min as c_int
    }
}

/// `int getSecond(void)` — local second (0-59).
#[no_mangle]
pub extern "C" fn getSecond() -> c_int {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as libc::time_t)
        .unwrap_or(0);
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        tm.tm_sec as c_int
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Timer data types
// ──────────────────────────────────────────────────────────────────────────────

const TIMER_ONCE_AUTODEL: u8 = 0x01;
const TIMER_INTERVAL: u8 = 0x02;
const TIMER_REMOVE_HEAP: u8 = 0x10;

/// Callback signature matching `int (*func)(int, int)` in C.
type TimerFn = unsafe extern "C" fn(c_int, c_int) -> c_int;

struct TimerData {
    tick: u32,
    func: Option<TimerFn>,
    /// Combination of TIMER_* flags.
    typ: u8,
    interval: u32,
    id: c_int,
    data1: c_int,
    data2: c_int,
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
// Global timer state (single-threaded event loop, no locking required)
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

/// Global timer state — accessed only from the single event-loop thread.
static mut TIMER_STATE: Option<TimerState> = None;

unsafe fn state() -> &'static mut TimerState {
    if TIMER_STATE.is_none() {
        TIMER_STATE = Some(TimerState::new());
    }
    TIMER_STATE.as_mut().unwrap()
}

// ──────────────────────────────────────────────────────────────────────────────
// Public C-ABI exports
// ──────────────────────────────────────────────────────────────────────────────

/// `void timer_init(void)` — no-op (matches C).
#[no_mangle]
pub extern "C" fn timer_init() {}

/// `int timer_clear(void)` — free all timer memory.
#[no_mangle]
pub unsafe extern "C" fn timer_clear() -> c_int {
    TIMER_STATE = None;
    0
}

/// `int timer_insert(uint32_t initial_delay_ms, uint32_t interval_ms, fn, id, data) -> timer_id`
///
/// Matches the C signature: first arg is the initial delay (added to gettick()),
/// second is the repeat interval.
#[no_mangle]
pub unsafe extern "C" fn timer_insert(
    tick_delay: u32,
    interval: u32,
    func: Option<TimerFn>,
    id: c_int,
    data: c_int,
) -> c_int {
    let s = state();
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
    tid as c_int
}

/// `int timer_remove(int tid)` — mark a timer for deletion.
#[no_mangle]
pub unsafe extern "C" fn timer_remove(tid: c_int) -> c_int {
    let s = state();
    let tid = tid as usize;
    if tid >= s.data.len() {
        return -1;
    }
    s.data[tid].func = None;
    s.data[tid].typ = TIMER_ONCE_AUTODEL;
    0
}

/// `const struct TimerData* get_timer(int tid)` — not needed from Rust, but
/// exported so C code that calls it still links.  We return NULL always
/// (the C code only uses it for debug checks).
#[no_mangle]
pub unsafe extern "C" fn get_timer(_tid: c_int) -> *const () {
    std::ptr::null()
}

/// `int timer_do(uint32_t tick)` — fire all expired timers, return ms to next.
///
/// Must be called from the event loop every ~10 ms.
#[no_mangle]
pub unsafe extern "C" fn timer_do(tick: u32) -> c_int {
    const TIMER_MIN_INTERVAL: i32 = 50;
    const TIMER_MAX_INTERVAL: i32 = 1000;

    let s = state();
    let mut diff: i32 = 1000;

    loop {
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

        let to_del = if let Some(f) = s.data[tid].func {
            f(s.data[tid].data1, s.data[tid].data2) != 0
        } else {
            false
        };

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
                    s.data[tid].typ &= !TIMER_REMOVE_HEAP;
                    let tid_copy = tid;
                    s.heap_push(tid_copy);
                }
                _ => {}
            }
        }
    }

    diff.clamp(TIMER_MIN_INTERVAL, TIMER_MAX_INTERVAL)
}
