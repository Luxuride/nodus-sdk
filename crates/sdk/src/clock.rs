//! Time as a dependency.
//!
//! Every rule in the client is time-shaped: cache soft-expiry, streaming backoff,
//! feed staleness, "N new posts" ordering. Taking time from a trait keeps those
//! rules testable without sleeping.

/// Wall-clock time, in Unix milliseconds.
pub trait Clock {
    /// Milliseconds since the Unix epoch.
    fn now_unix_millis(&self) -> i64;
}

/// Reads the system clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix_millis(&self) -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        let elapsed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
    }
}

/// A clock that only moves when a test tells it to.
#[derive(Debug, Clone, Default)]
pub struct FixedClock {
    millis: std::cell::Cell<i64>,
}

impl FixedClock {
    /// A clock frozen at `now_unix_millis`.
    #[must_use]
    pub fn at(now_unix_millis: i64) -> Self {
        Self {
            millis: std::cell::Cell::new(now_unix_millis),
        }
    }

    /// Moves the clock forward by `millis`.
    pub fn advance(&self, millis: i64) {
        self.millis.set(self.millis.get() + millis);
    }
}

impl Clock for FixedClock {
    fn now_unix_millis(&self) -> i64 {
        self.millis.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_moves_only_when_advanced() {
        let clock = FixedClock::at(1_000);
        assert_eq!(clock.now_unix_millis(), 1_000);

        clock.advance(500);
        assert_eq!(clock.now_unix_millis(), 1_500);
    }
}
