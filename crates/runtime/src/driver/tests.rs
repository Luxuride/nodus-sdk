//! Tests for the tick budget.
//!
//! Every case hands the driver explicit [`Instant`]s, so a ten-minute backoff
//! costs nothing to test and cannot flake on a loaded machine.

use std::time::{Duration, Instant};

use super::Driver;
use crate::scheduler::{Budget, RECONNECT_BASE, RETRY_BASE, StreamPhase, TIMELINE_STALE};

fn start() -> Instant {
    Instant::now()
}

#[test]
fn a_fresh_driver_loads_everything_immediately() {
    let now = start();
    let plan = Driver::new().plan(now);
    assert!(plan.load_timeline, "nothing has loaded yet");
    assert!(plan.refresh_feed, "nothing has loaded yet");
    assert_eq!(plan.retry_in, None, "nothing has failed yet");
    assert_eq!(plan.reconnect_in, None, "the stream backoff is zero-based");
}

#[test]
fn a_success_clears_the_staleness_for_both_reads() {
    let now = start();
    let mut driver = Driver::new();
    driver.record_attempt(now);
    driver.record_success(now);

    let plan = driver.plan(now + Duration::from_secs(1));
    assert!(!plan.load_timeline);
    assert!(!plan.refresh_feed);
}

#[test]
fn staleness_returns_after_the_timeline_window() {
    let now = start();
    let mut driver = Driver::new();
    driver.record_success(now);

    let just_inside = (now + TIMELINE_STALE)
        .checked_sub(Duration::from_secs(1))
        .expect("a five-minute window is longer than one second");
    assert!(
        !driver.plan(just_inside).load_timeline,
        "one second inside the window is still fresh"
    );
    assert!(
        driver.plan(now + TIMELINE_STALE).load_timeline,
        "the window is inclusive at its edge"
    );
}

#[test]
fn a_failure_backs_off_and_a_success_resets_it() {
    let now = start();
    let mut driver = Driver::new();
    driver.record_attempt(now);
    driver.record_failure();
    driver.record_failure();

    let budget = driver.budget();
    assert_eq!(budget.failures, 2);
    assert_eq!(
        driver.plan(now).retry_in,
        Some(RETRY_BASE * 4),
        "the second failure already waits two doublings"
    );

    driver.record_success(now);
    assert_eq!(driver.budget().failures, 0, "success resets the delay");
}

#[test]
fn a_failure_keeps_the_success_stamp_so_the_data_still_looks_old() {
    let now = start();
    let mut driver = Driver::new();
    driver.record_success(now);
    driver.record_attempt(now + TIMELINE_STALE);
    driver.record_failure();

    assert_eq!(
        driver.budget().last_success,
        Some(now),
        "a failed refresh does not make the last good read newer"
    );
    assert!(
        driver
            .plan(now + TIMELINE_STALE + Duration::from_secs(1))
            .load_timeline,
        "the timeline is still stale once the gap has passed after a failed refresh"
    );
}

#[test]
fn an_attempt_within_the_gap_suppresses_a_due_load() {
    let now = start();
    let mut driver = Driver::new();
    driver.record_attempt(now);

    assert!(
        !driver.plan(now).load_timeline,
        "ten keystrokes must not become ten loads"
    );
    assert!(
        driver.plan(now + Duration::from_secs(1)).load_timeline,
        "the gap is one second"
    );
}

#[test]
fn a_connected_stream_is_never_reconnected() {
    let now = start();
    let mut driver = Driver::new();
    driver.record_stream(StreamPhase::Connected);
    driver.record_failure();

    assert_eq!(driver.plan(now).reconnect_in, None);
    assert!(
        driver.plan(now).retry_in.is_some(),
        "the read still backs off"
    );
}

#[test]
fn the_next_wake_is_zero_while_work_is_due() {
    let now = start();
    let driver = Driver::new();
    assert_eq!(driver.next_wake(now), Some(Duration::ZERO));
}

#[test]
fn the_next_wake_is_the_shorter_of_the_two_backoffs() {
    let now = start();
    let mut driver = Driver::new();
    driver.record_success(now);
    driver.record_attempt(now);
    driver.record_failure();

    // A satisfied success stamp means nothing is due, so only the delays remain.
    assert_eq!(driver.plan(now).retry_in, Some(RETRY_BASE * 2));
    assert_eq!(
        driver.plan(now).reconnect_in,
        Some(RECONNECT_BASE * 2),
        "a downed stream shares the failure count but not the base delay"
    );
    assert_eq!(
        driver.next_wake(now),
        Some(RECONNECT_BASE * 2),
        "the shorter of the two backoffs is the one worth waking for"
    );
}

#[test]
fn an_idle_driver_with_nothing_pending_has_no_wake() {
    let now = start();
    let mut driver = Driver::new();
    driver.record_success(now);

    assert_eq!(driver.next_wake(now + Duration::from_secs(1)), None);
}

#[test]
fn the_default_budget_is_the_empty_one() {
    let driver = Driver::default();
    assert_eq!(
        driver.budget(),
        Budget {
            last_success: None,
            last_attempt: None,
            failures: 0,
            stream: StreamPhase::Idle,
        }
    );
}
