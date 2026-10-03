// SPDX-License-Identifier: LGPL-2.1-or-later
//! Aggregate progress: smoothed rate (EWMA) and ETA (XFR-8). Pure arithmetic.

/// Smoothing factor of the exponentially weighted moving average. 0.3 reacts
/// within a few samples yet hides single-chunk bursts.
pub const DEFAULT_ALPHA: f64 = 0.3;

/// Rates below this (bytes/s) give no ETA: the number would be meaningless.
const MIN_RATE_FOR_ETA: f64 = 1.0;

#[derive(Debug, Clone)]
pub struct RateEstimator {
    alpha: f64,
    rate: Option<f64>,
    last: Option<(i64, u64)>,
}

impl Default for RateEstimator {
    fn default() -> Self {
        RateEstimator::new(DEFAULT_ALPHA)
    }
}

impl RateEstimator {
    pub fn new(alpha: f64) -> RateEstimator {
        RateEstimator {
            alpha: alpha.clamp(0.01, 1.0),
            rate: None,
            last: None,
        }
    }

    /// Feeds a sample: total bytes done at `now_ms`. Samples that do not move
    /// time forward are ignored; a decrease in bytes (restart of a file)
    /// re-bases without a negative rate.
    pub fn sample(&mut self, now_ms: i64, bytes_done: u64) {
        let Some((t0, b0)) = self.last else {
            self.last = Some((now_ms, bytes_done));
            return;
        };
        if now_ms <= t0 {
            return;
        }
        self.last = Some((now_ms, bytes_done));
        let delta = bytes_done.saturating_sub(b0) as f64;
        let inst = delta * 1000.0 / (now_ms - t0) as f64;
        self.rate = Some(match self.rate {
            None => inst,
            Some(prev) => self.alpha * inst + (1.0 - self.alpha) * prev,
        });
    }

    /// Smoothed bytes per second (0 before two samples).
    pub fn rate(&self) -> u64 {
        self.rate.map_or(0, |r| r.max(0.0) as u64)
    }

    /// Seconds until `remaining` bytes are done at the current rate.
    pub fn eta_secs(&self, remaining: u64) -> Option<u64> {
        let r = self.rate?;
        if r < MIN_RATE_FOR_ETA {
            return None;
        }
        Some((remaining as f64 / r).ceil() as u64)
    }

    /// Forgets history (after a pause the old rate says nothing).
    pub fn reset(&mut self) {
        self.rate = None;
        self.last = None;
    }
}

/// Human text for the Transfers page: "1.2 GB of 4.0 GB".
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_sample_gives_no_rate() {
        let mut r = RateEstimator::default();
        assert_eq!(r.rate(), 0);
        r.sample(0, 0);
        assert_eq!(r.rate(), 0);
        assert_eq!(r.eta_secs(100), None);
    }

    #[test]
    fn steady_rate_is_exact() {
        let mut r = RateEstimator::default();
        r.sample(0, 0);
        r.sample(1000, 1000);
        r.sample(2000, 2000);
        assert_eq!(r.rate(), 1000);
        assert_eq!(r.eta_secs(5000), Some(5));
        assert_eq!(r.eta_secs(5001), Some(6));
    }

    #[test]
    fn ewma_blends_old_and_new() {
        let mut r = RateEstimator::new(0.5);
        r.sample(0, 0);
        r.sample(1000, 1000);
        r.sample(2000, 3000);
        assert_eq!(r.rate(), 1500);
    }

    #[test]
    fn stale_and_backward_samples() {
        let mut r = RateEstimator::new(0.5);
        r.sample(1000, 0);
        r.sample(1000, 500);
        assert_eq!(r.rate(), 0);
        r.sample(2000, 1000);
        assert_eq!(r.rate(), 1000);
        r.sample(3000, 0);
        assert_eq!(r.rate(), 500);
    }

    #[test]
    fn tiny_rate_has_no_eta_and_reset_clears() {
        let mut r = RateEstimator::default();
        r.sample(0, 0);
        r.sample(10_000, 1);
        assert_eq!(r.eta_secs(10), None);
        r.sample(11_000, 5000);
        assert!(r.rate() > 0);
        r.reset();
        assert_eq!(r.rate(), 0);
    }

    #[test]
    fn byte_formatting() {
        assert_eq!(format_bytes(999), "999 B");
        assert_eq!(format_bytes(1500), "1.5 kB");
        assert_eq!(format_bytes(4_000_000_000), "4.0 GB");
    }
}
