//! Runtime-mutable rate globals.
//!
//! XP_RATE and D_RATE are AtomicI32 because GM commands can change them at
//! runtime.  They are initialised from `ServerConfig` in `config_read()`.

use std::sync::atomic::AtomicI32;

/// Experience rate multiplier -- written at startup and by GM /xprate command.
pub static XP_RATE: AtomicI32 = AtomicI32::new(0);

/// Drop rate multiplier -- written at startup and by GM /drate command.
pub static D_RATE: AtomicI32 = AtomicI32::new(0);
