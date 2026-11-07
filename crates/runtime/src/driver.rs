//! The loop body: what one background tick does.
//!
//! [`Scheduler`] answers *when*; [`Driver`] is the thing that holds the answer
//! between ticks and remembers how the last cycle went. Keeping the two apart is
//! what lets the timing rules be tested with plain [`Instant`]s while this type
//! stays small enough to audit: it owns a [`Budget`], updates one field at a time,
//! and computes the next wake-up.
//!
//! The host still owns the timer. On `wasm` a timer is throttled in a hidden tab
//! and suspended with the page, so a due tick may arrive minutes late; the driver
//! therefore never assumes a tick happened when it was asked for, and
//! [`Driver::next_wake`] is recomputed from the budget after every tick.
//!
//! An outage is recorded once per failed cycle, not once per request: the backoff
//! belongs to "the recommendation service is unreachable", and doubling it once per
//! endpoint would make the third retry wait four times longer than the delay the
//! retry policy documents.

use std::time::Duration;

use web_time::Instant;

use crate::scheduler::{Budget, Plan, Scheduler, StreamPhase};

/// Holds the fetch budget across ticks and turns it into a plan and a wake-up.
#[derive(Debug, Clone, Copy)]
pub struct Driver {
    scheduler: Scheduler,
    budget: Budget,
}

impl Driver {
    /// A driver that has fetched nothing yet, so the first tick loads everything.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            scheduler: Scheduler::new(),
            budget: Budget {
                last_success: None,
                last_attempt: None,
                failures: 0,
                stream: StreamPhase::Idle,
            },
        }
    }

    /// The budget as it currently stands, for logging and for tests.
    #[must_use]
    pub const fn budget(&self) -> Budget {
        self.budget
    }

    /// What should happen at `now`.
    #[must_use]
    pub fn plan(&self, now: Instant) -> Plan {
        self.scheduler.plan(now, &self.budget)
    }

    /// Notes that a request started, so the minimum attempt gap applies to the
    /// next one even if this one ends up failing.
    pub fn record_attempt(&mut self, now: Instant) {
        self.budget.last_attempt = Some(now);
    }

    /// Notes that a cycle finished without error: failures reset, so the delays
    /// shrink back to their bases instead of staying long after recovery.
    pub fn record_success(&mut self, now: Instant) {
        self.budget.last_success = Some(now);
        self.budget.failures = 0;
    }

    /// Notes that a cycle failed. The success stamp is deliberately untouched: a
    /// stale successful read is still the truth about how old the data is.
    pub fn record_failure(&mut self) {
        self.budget.failures = self.budget.failures.saturating_add(1);
    }

    /// Records whether the stream is up, so a reconnect is only planned while it
    /// is not.
    pub fn record_stream(&mut self, phase: StreamPhase) {
        self.budget.stream = phase;
    }

    /// How long to sleep before the next useful tick, or `None` when the plan asks
    /// for nothing and no retry is pending.
    ///
    /// A due flag wins over a delay: if work is ready now the caller should not
    /// sleep at all, and returning `Some(0)` would make the host busy-loop.
    #[must_use]
    pub fn next_wake(&self, now: Instant) -> Option<Duration> {
        let plan = self.plan(now);
        if plan.load_timeline || plan.refresh_feed {
            return Some(Duration::ZERO);
        }
        match (plan.retry_in, plan.reconnect_in) {
            (None, None) => None,
            (Some(retry), None) => Some(retry),
            (None, Some(reconnect)) => Some(reconnect),
            (Some(retry), Some(reconnect)) => Some(retry.min(reconnect)),
        }
    }
}

impl Default for Driver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
