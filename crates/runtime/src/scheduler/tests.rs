use std::time::{Duration, Instant};

use super::*;

/// Plans at `now` for one budget, so each test reads as a timeline instead of
/// struct plumbing. It goes through the same public call the `Sdk` makes.
fn plan_at(
    now: Instant,
    last_success: Option<Instant>,
    last_attempt: Option<Instant>,
    failures: u32,
    stream: StreamPhase,
) -> Plan {
    let budget = Budget {
        last_success,
        last_attempt,
        failures,
        stream,
    };
    Scheduler::new().plan(now, &budget)
}

#[test]
fn missing_success_plans_both_loads() {
    let now = Instant::now();

    let plan = plan_at(now, None, None, 0, StreamPhase::Connected);

    assert!(plan.load_timeline);
    assert!(plan.refresh_feed);
}

#[test]
fn fresh_success_plans_nothing() {
    let base = Instant::now();
    let now = base + Duration::from_secs(60);
    let success = base + Duration::from_secs(30);

    // 30s of age is well inside TIMELINE_STALE, and the attempt was a minute ago.
    let plan = plan_at(now, Some(success), Some(base), 0, StreamPhase::Connected);

    assert!(!plan.load_timeline);
    assert!(!plan.refresh_feed);
}

#[test]
fn timeline_goes_stale_before_the_feed() {
    let base = Instant::now();
    let now = base + Duration::from_secs(301);

    // 301s is past TIMELINE_STALE (300s) but short of FEED_STALE (600s).
    let plan = plan_at(now, Some(base), Some(base), 0, StreamPhase::Connected);

    assert!(plan.load_timeline);
    assert!(!plan.refresh_feed);
}

#[test]
fn feed_goes_stale_at_ten_minutes() {
    let base = Instant::now();
    let now = base + Duration::from_secs(601);

    let plan = plan_at(now, Some(base), Some(base), 0, StreamPhase::Connected);

    assert!(plan.load_timeline);
    assert!(plan.refresh_feed);
}

#[test]
fn attempt_gap_suppresses_both_flags_but_keeps_retry() {
    let base = Instant::now();

    // Attempted just now and failed twice: no flag, but still a wake-up time.
    let plan = plan_at(base, None, Some(base), 2, StreamPhase::Connected);

    assert!(!plan.load_timeline);
    assert!(!plan.refresh_feed);
    assert_eq!(plan.retry_in, Some(Duration::from_secs(8)));
}

#[test]
fn attempt_gap_expires_after_one_second() {
    let base = Instant::now();
    let now = base + Duration::from_secs(1);

    let plan = plan_at(now, None, Some(base), 0, StreamPhase::Connected);

    assert!(plan.load_timeline);
    assert!(plan.refresh_feed);
}

#[test]
fn future_success_is_treated_as_stale() {
    let base = Instant::now();
    let ahead = base + Duration::from_secs(60);

    // The clock jumped backwards: `last_success` is ahead of `now`.
    let plan = plan_at(base, Some(ahead), None, 0, StreamPhase::Connected);

    assert!(plan.load_timeline);
    assert!(plan.refresh_feed);
}

#[test]
fn future_attempt_blocks_the_gap() {
    let base = Instant::now();
    let ahead = base + Duration::from_secs(5);

    // An unreadable attempt time must fail safe towards waiting, not towards a
    // request storm.
    let plan = plan_at(base, None, Some(ahead), 0, StreamPhase::Connected);

    assert!(!plan.load_timeline);
    assert!(!plan.refresh_feed);
}

#[test]
fn retry_doubles_with_each_failure() {
    let base = Instant::now();

    // RETRY_BASE is 2s: 4s, 8s, then 16s.
    for (failures, secs) in [(1_u32, 4_u64), (2, 8), (3, 16)] {
        let plan = plan_at(base, None, Some(base), failures, StreamPhase::Connected);
        assert_eq!(plan.retry_in, Some(Duration::from_secs(secs)), "{failures}");
    }
}

#[test]
fn retry_stops_doubling_at_the_cap() {
    let base = Instant::now();

    // 2s * 2^8 = 512s is already past the 300s cap, and the exponent is
    // clamped, so a larger count cannot change the delay either.
    for failures in [8_u32, 64] {
        let plan = plan_at(base, None, Some(base), failures, StreamPhase::Connected);
        assert_eq!(plan.retry_in, Some(RETRY_CAP), "failures={failures}");
    }
}

#[test]
fn no_failures_means_no_retry() {
    let now = Instant::now();

    let plan = plan_at(now, None, None, 0, StreamPhase::Connected);

    assert_eq!(plan.retry_in, None);
}

#[test]
fn reconnect_is_planned_while_idle() {
    let base = Instant::now();

    // Idle and one failure: 1s * 2^1.
    let plan = plan_at(base, None, Some(base), 1, StreamPhase::Idle);

    assert_eq!(plan.reconnect_in, Some(Duration::from_secs(2)));
}

#[test]
fn connected_stream_never_plans_a_reconnect() {
    let base = Instant::now();

    // Six failures would schedule a reconnect if the socket were down; while
    // connected, planning one would open a duplicate stream.
    let plan = plan_at(base, None, Some(base), 6, StreamPhase::Connected);

    assert_eq!(plan.reconnect_in, None);
    // The retry backoff is independent of the stream and still reported.
    assert_eq!(plan.retry_in, Some(Duration::from_secs(128)));
}

#[test]
fn idle_without_failures_plans_no_reconnect() {
    let now = Instant::now();

    let plan = plan_at(now, None, None, 0, StreamPhase::Idle);

    assert_eq!(plan.reconnect_in, None);
}

#[test]
fn reconnect_stops_doubling_at_the_cap() {
    let base = Instant::now();

    // 1s * 2^8 = 256s, past the 30s cap.
    for failures in [8_u32, 32] {
        let plan = plan_at(base, None, Some(base), failures, StreamPhase::Idle);
        assert_eq!(
            plan.reconnect_in,
            Some(RECONNECT_CAP),
            "failures={failures}"
        );
    }
}
