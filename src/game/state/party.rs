//! Party/group member ID table.

use std::sync::{Mutex, OnceLock};

/// Party/group member ID table. Flat 2-D: groups[256][256] = 65536 elements.
static GROUPS: OnceLock<Mutex<Box<[u32; 65536]>>> = OnceLock::new();

#[inline]
pub fn groups() -> std::sync::MutexGuard<'static, Box<[u32; 65536]>> {
    GROUPS
        .get_or_init(|| Mutex::new(Box::new([0u32; 65536])))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}
