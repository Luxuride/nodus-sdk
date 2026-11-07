//! When background work happens.
//!
//! The scheduler is a pure function of `(now, budget)`: it reads no clock, keeps
//! no state and never touches a transport. That is what makes every rule below
//! testable by handing it plain [`Instant`]s instead of sleeping.
//!
//! It also means the caller owns the timer. On `wasm`, timers are throttled in a
//! background tab and suspended with the page, so a [`Plan`] is a *hint*: wake at
//! the suggested delay, then re-plan. Re-planning is cheap and is the only thing
//! that keeps the decisions correct after a late fire.
//!
//! Why this lives next to `Sdk` rather than in the views: the state machine sees
//! every command and every stream event in one place, so it can coalesce them.
//! A burst of user commands would otherwise become one request per command.

use std::time::Duration;

use web_time::Instant;

/// How old a successful timeline read may be before another page is loaded.
///
/// Timelines move continuously, so five minutes is short enough that a returning
/// user does not see a conversation frozen mid-argument.
pub const TIMELINE_STALE: Duration = Duration::from_secs(5 * 60);

/// How old a successful feed read may be before the feed is refreshed.
///
/// Twice the timeline window because the server reuses a recent snapshot rather
/// than recomputing recommendations; refreshing sooner would spend a request to
/// receive the snapshot it just served.
pub const FEED_STALE: Duration = Duration::from_secs(10 * 60);

/// First retry delay after a failed discovery call.
pub const RETRY_BASE: Duration = Duration::from_secs(2);

/// Longest retry delay, so an outage converges instead of growing forever.
pub const RETRY_CAP: Duration = Duration::from_secs(5 * 60);

/// Shortest spacing between two planned loads.
///
/// Without it, ten keystrokes in a search box become ten timeline loads; the
/// caller keeps the last command's intent and this gap collapses the burst.
pub const MIN_ATTEMPT_GAP: Duration = Duration::from_secs(1);

/// First delay before reopening the stream socket.
pub const RECONNECT_BASE: Duration = Duration::from_secs(1);

/// Longest delay before reopening the stream socket.
///
/// Shorter than [`RETRY_CAP`]: a live stream is how new posts arrive at all, and
/// a user-visible stall of minutes is worse than a retry that fails.
pub const RECONNECT_CAP: Duration = Duration::from_secs(30);

/// `failures` is clamped to this exponent before the shift. 2^8 already takes
/// both backoffs past their caps, so larger exponents cannot change the result.
const MAX_BACKOFF_DOUBLINGS: u32 = 8;

/// Decides when background work happens. Stateless: all inputs are in [`Budget`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Scheduler;

impl Scheduler {
    /// A scheduler. Stateless, so one instance can serve every view and the
    /// whole `Sdk`; there is nothing to keep in sync.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// What the caller should do at `now`.
    #[must_use]
    pub fn plan(&self, now: Instant, budget: &Budget) -> Plan {
        let gap_elapsed = attempt_gap_elapsed(now, budget.last_attempt);
        let timeline_stale = is_stale(now, budget.last_success, TIMELINE_STALE);
        let feed_stale = is_stale(now, budget.last_success, FEED_STALE);

        // Reported even while the gap suppresses the flags: the caller still
        // needs a wake-up time, and the backoff is a property of the failures,
        // not of whether this particular instant is allowed to act.
        let retry_in = next_attempt_in(RETRY_BASE, RETRY_CAP, budget.failures);
        let reconnect_in = match budget.stream {
            StreamPhase::Connected => None,
            StreamPhase::Idle => next_attempt_in(RECONNECT_BASE, RECONNECT_CAP, budget.failures),
        };

        Plan {
            load_timeline: timeline_stale && gap_elapsed,
            refresh_feed: feed_stale && gap_elapsed,
            retry_in,
            reconnect_in,
        }
    }
}

/// What the scheduler may assume about the last fetch cycle.
///
/// The caller updates one field at a time (success, attempt, failure count,
/// stream phase) and asks for a new plan; keeping this a plain `Copy` struct
/// means the state machine can snapshot it without borrowing its own state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// When a remote read last succeeded, if ever. `None` means nothing has
    /// loaded yet, which every staleness rule treats as stale.
    pub last_success: Option<Instant>,
    /// When a remote read was last attempted, success or failure. Used only to
    /// space attempts out; `None` means no attempt has been made.
    pub last_attempt: Option<Instant>,
    /// Consecutive failures since the last success. Reset it to zero on
    /// success, or the delays stay long after the service recovers.
    pub failures: u32,
    /// The socket state, so a reconnect is only planned while the stream is
    /// actually down.
    pub stream: StreamPhase,
}

/// Whether the streaming transport is up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamPhase {
    /// No live connection; reconnect backoff applies.
    Idle,
    /// A live connection. Reconnecting would open a duplicate stream, so it is
    /// never planned while connected.
    Connected,
}

/// What the caller should do at `now`.
///
/// The flags are "do it now"; the durations are "wake me again in this long".
/// Both may be present at once (load a page, and retry discovery later), so a
/// caller must not treat the first true field as the whole plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    /// Load one more timeline page: the last successful read is missing or older
    /// than [`TIMELINE_STALE`], and no attempt happened within
    /// [`MIN_ATTEMPT_GAP`].
    pub load_timeline: bool,
    /// Refresh the feed: the last successful read is missing or older than
    /// [`FEED_STALE`], and no attempt happened within [`MIN_ATTEMPT_GAP`].
    pub refresh_feed: bool,
    /// Delay before the next attempt after consecutive failures; `None` when
    /// the last cycle did not fail.
    pub retry_in: Option<Duration>,
    /// Delay before reopening the stream; `None` while connected.
    pub reconnect_in: Option<Duration>,
}

/// Whether `last_success` is missing or older than `threshold`.
///
/// A timestamp in the future means the clock moved backwards. That counts as
/// stale: trusting it would freeze refreshes until real time caught up.
fn is_stale(now: Instant, last_success: Option<Instant>, threshold: Duration) -> bool {
    let Some(at) = last_success else {
        return true;
    };
    now.checked_duration_since(at)
        .is_none_or(|age| age >= threshold)
}

/// Whether [`MIN_ATTEMPT_GAP`] has passed since `last_attempt`.
///
/// An attempt timestamp in the future counts as *not* elapsed, so an unreadable
/// clock fails safe towards "wait" instead of towards a request storm.
fn attempt_gap_elapsed(now: Instant, last_attempt: Option<Instant>) -> bool {
    let Some(at) = last_attempt else {
        return true;
    };
    now.checked_duration_since(at)
        .is_some_and(|age| age >= MIN_ATTEMPT_GAP)
}

/// The delay before the next attempt, or `None` when nothing has failed yet.
///
/// `base` and `cap` differ per transport: a dropped stream is reopened sooner
/// than a failed recommendation call is retried.
fn next_attempt_in(base: Duration, cap: Duration, failures: u32) -> Option<Duration> {
    if failures == 0 {
        None
    } else {
        Some(backoff(base, cap, failures))
    }
}

/// `base * 2^min(failures, MAX_BACKOFF_DOUBLINGS)`, capped at `cap`.
///
/// The exponent is clamped before the shift so the multiplier cannot overflow;
/// the cap then bounds the wait a user would experience.
fn backoff(base: Duration, cap: Duration, failures: u32) -> Duration {
    let doublings = failures.min(MAX_BACKOFF_DOUBLINGS);
    let factor = 1_u32 << doublings;
    base.saturating_mul(factor).min(cap)
}

#[cfg(test)]
mod tests;
