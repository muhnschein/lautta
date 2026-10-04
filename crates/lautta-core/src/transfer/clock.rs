// SPDX-License-Identifier: LGPL-2.1-or-later
//! Injectable wall clock (milliseconds since the Unix epoch) so persistence
//! throttling, retention and expiry are testable without sleeping.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

pub trait Clock: Send + Sync {
    fn now_ms(&self) -> i64;
}

/// The real clock.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        crate::entry::system_time_to_ms(std::time::SystemTime::now())
    }
}

/// A clock moved by hand (tests, and callers that want deterministic output).
#[derive(Debug, Default)]
pub struct ManualClock {
    now: AtomicI64,
}

impl ManualClock {
    pub fn new(start_ms: i64) -> Arc<ManualClock> {
        Arc::new(ManualClock {
            now: AtomicI64::new(start_ms),
        })
    }

    pub fn set(&self, ms: i64) {
        self.now.store(ms, Ordering::SeqCst);
    }

    pub fn advance(&self, ms: i64) {
        self.now.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> i64 {
        self.now.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_clock_moves_only_when_told() {
        let c = ManualClock::new(100);
        assert_eq!(c.now_ms(), 100);
        c.advance(50);
        assert_eq!(c.now_ms(), 150);
        c.set(7);
        assert_eq!(c.now_ms(), 7);
    }

    #[test]
    fn system_clock_is_recent() {
        assert!(SystemClock.now_ms() > 1_600_000_000_000);
    }
}
