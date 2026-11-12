//! Dwell: how long each item was actually on screen, measured exactly once.
//!
//! An impression has two jobs — it is the denominator of every follow-through rate
//! and the only evidence that lets a rejection decay (D26) — and neither of them
//! needs to know *when* the page arrived. What they do need is that the row is
//! written once: `interaction_event` deduplicates on `(item_id, kind, client_ts)`,
//! so a second impression for the same item is a second row, and a client that
//! reports on load *and* on unmount would quietly halve every rate it feeds.
//!
//! So the impression is reported when the card leaves the screen, carrying the time
//! it spent there. The cost is a reporting hole at the tail of the last page: an item
//! still visible when the tab closes is only counted if the page-hide path gets to
//! flush. That hole is preferred to the alternative, because a missing observation
//! makes a rate noisier while a doubled one makes it wrong in a direction that looks
//! like evidence.
//!
//! The tracker holds no I/O and no clock: the caller supplies milliseconds and the
//! client timestamp, which keeps the arithmetic testable off the browser.

use std::collections::HashMap;

use nodus_shared::{EventBatch, EventKind, InteractionEvent};

/// The longest dwell worth reporting, in milliseconds.
///
/// Matches `nodus_client_core::evidence::DWELL_CAP_MS` — a card left open overnight is not
/// two hours of attention, and the server clamps to this value anyway.
pub const MAX_DWELL_MS: u32 = 120_000;

/// The maximum number of events the server accepts in one batch.
pub const MAX_EVENTS_PER_BATCH: usize = 100;

/// One card that is currently on screen.
#[derive(Debug, Clone)]
struct Shown {
    feed_id: String,
    started_ms: i64,
}

/// An impression waiting to be sent, with the list it belongs to.
#[derive(Debug, Clone)]
struct Pending {
    feed_id: String,
    event: InteractionEvent,
}

/// Tracks what is on screen and queues the impressions it produces.
#[derive(Debug, Default)]
pub struct DwellTracker {
    feed_id: Option<String>,
    shown: HashMap<i64, Shown>,
    pending: Vec<Pending>,
}

impl DwellTracker {
    /// A tracker with nothing shown and nothing queued.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// States which feed subsequent [`begin`](Self::begin) calls belong to.
    ///
    /// Called when a page is remembered, so a card that mounts while a refresh is in
    /// flight names the list it came from rather than the list that replaced it.
    pub fn set_feed(&mut self, feed_id: &str) {
        self.feed_id = Some(feed_id.to_owned());
    }

    /// Records that an item is on screen as of `now_ms`.
    ///
    /// Idempotent for an item already shown: the earliest start wins, so a re-render
    /// cannot restart the clock and inflate the number.
    pub fn begin(&mut self, item_id: i64, now_ms: i64) {
        let Some(feed_id) = self.feed_id.clone() else {
            return;
        };
        self.shown.entry(item_id).or_insert(Shown {
            feed_id,
            started_ms: now_ms,
        });
    }

    /// Ends an item's time on screen and queues its impression.
    ///
    /// Returns whether an impression was queued — an item that was never begun, or
    /// whose impression was already reported, yields nothing.
    pub fn finish(&mut self, item_id: i64, now_ms: i64, client_ts: &str) -> bool {
        let Some(shown) = self.shown.remove(&item_id) else {
            return false;
        };
        self.pending.push(Pending {
            feed_id: shown.feed_id,
            event: InteractionEvent {
                item_id,
                kind: EventKind::Impression,
                client_ts: client_ts.to_owned(),
                dwell_ms: Some(clamp_ms(now_ms - shown.started_ms)),
            },
        });
        true
    }

    /// Ends every item on screen, for a page hide or a feed swap.
    ///
    /// Returns how many impressions were queued.
    pub fn finish_all(&mut self, now_ms: i64, client_ts: &str) -> usize {
        let mut shown: Vec<i64> = self.shown.keys().copied().collect();
        shown.sort_unstable();
        shown
            .into_iter()
            .filter(|item_id| self.finish(*item_id, now_ms, client_ts))
            .count()
    }

    /// Removes every item on screen without reporting it.
    ///
    /// For a sign-out, where the rows would name a user who no longer exists.
    pub fn discard(&mut self) {
        self.shown.clear();
    }

    /// Drains everything queued into batches the server accepts, oldest first.
    ///
    /// Batches never mix feeds: the event carries only the item id, so the list it
    /// belongs to lives in the envelope, and one envelope has one `feed_id`.
    pub fn take_batches(&mut self, per_batch: usize) -> Vec<EventBatch> {
        let per_batch = per_batch.clamp(1, MAX_EVENTS_PER_BATCH);
        let mut batches: Vec<EventBatch> = Vec::new();
        for pending in self.pending.drain(..) {
            match batches.last_mut() {
                Some(batch)
                    if batch.feed_id == pending.feed_id && batch.events.len() < per_batch =>
                {
                    batch.events.push(pending.event);
                }
                _ => batches.push(EventBatch {
                    feed_id: pending.feed_id,
                    events: vec![pending.event],
                }),
            }
        }
        batches
    }

    /// How many impressions are waiting to be sent.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// How many items are currently on screen.
    #[must_use]
    pub fn shown(&self) -> usize {
        self.shown.len()
    }
}

/// Clamps an elapsed time into the range the server accepts.
///
/// A negative elapsed time comes from a clock that moved backwards; it is reported as
/// zero rather than as a huge number, because "shown for no time" is at least the right
/// shape of claim.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the two early returns above prove the value is in 0..MAX_DWELL_MS"
)]
pub const fn clamp_ms(elapsed_ms: i64) -> u32 {
    let cap = MAX_DWELL_MS as i64;
    if elapsed_ms < 0 {
        return 0;
    }
    if elapsed_ms >= cap {
        return MAX_DWELL_MS;
    }
    elapsed_ms as u32
}

#[cfg(test)]
mod tests {
    use super::{DwellTracker, MAX_DWELL_MS, clamp_ms};

    fn tracker() -> DwellTracker {
        let mut tracker = DwellTracker::new();
        tracker.set_feed("feed-1");
        tracker
    }

    #[test]
    fn an_item_reports_the_time_it_spent_on_screen() {
        let mut tracker = tracker();
        tracker.begin(7, 1_000);
        assert!(tracker.finish(7, 4_500, "2026-11-07T00:00:00Z"));

        let batches = tracker.take_batches(100);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].feed_id, "feed-1");
        assert_eq!(batches[0].events.len(), 1);
        assert_eq!(batches[0].events[0].dwell_ms, Some(3_500));
        assert_eq!(tracker.pending(), 0);
        assert_eq!(tracker.shown(), 0);
    }

    #[test]
    fn an_item_is_reported_once_even_if_it_unmounts_twice() {
        let mut tracker = tracker();
        tracker.begin(7, 0);
        assert!(tracker.finish(7, 1_000, "t"));
        assert!(!tracker.finish(7, 2_000, "t2"));
        assert_eq!(tracker.pending(), 1);
    }

    #[test]
    fn a_re_render_does_not_restart_the_clock() {
        let mut tracker = tracker();
        tracker.begin(7, 0);
        tracker.begin(7, 5_000);
        assert!(tracker.finish(7, 6_000, "t"));
        let batches = tracker.take_batches(100);
        assert_eq!(
            batches[0].events[0].dwell_ms,
            Some(6_000),
            "the earliest start wins, so time on screen is never double counted"
        );
    }

    #[test]
    fn an_item_that_right_awaits_no_clock_is_still_measured() {
        let mut tracker = tracker();
        tracker.begin(7, 4_000);
        assert!(tracker.finish(7, 4_050, "t"));
        let batches = tracker.take_batches(100);
        assert_eq!(batches[0].events[0].dwell_ms, Some(50));
    }

    #[test]
    fn a_page_hide_reports_everything_that_is_still_shown() {
        let mut tracker = tracker();
        for item_id in 1..=3 {
            tracker.begin(item_id, 0);
        }
        assert_eq!(tracker.finish_all(2_000, "t"), 3);
        assert_eq!(tracker.shown(), 0);
        let batches = tracker.take_batches(100);
        assert_eq!(batches[0].events.len(), 3);
        let ids: Vec<i64> = batches[0]
            .events
            .iter()
            .map(|event| event.item_id)
            .collect();
        assert_eq!(ids, vec![1, 2, 3], "a flush is ordered by item id");
    }

    #[test]
    fn batches_never_mix_feeds_and_respect_the_server_limit() {
        let mut tracker = tracker();
        for item_id in 0..150 {
            tracker.begin(item_id, 0);
        }
        tracker.finish_all(1_000, "t");
        let batches = tracker.take_batches(100);
        assert_eq!(batches.len(), 2, "150 events do not fit in one request");
        assert!(batches.iter().all(|batch| batch.feed_id == "feed-1"));
        assert_eq!(batches[0].events.len(), 100);
        assert_eq!(batches[1].events.len(), 50);

        tracker.set_feed("feed-2");
        tracker.begin(1_000, 0);
        tracker.finish(1_000, 10, "t");
        let batches = tracker.take_batches(100);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].feed_id, "feed-2");
    }

    #[test]
    fn nothing_is_reported_before_a_feed_is_known() {
        let mut tracker = DwellTracker::new();
        tracker.begin(7, 0);
        assert_eq!(tracker.shown(), 0, "an item with no list has no envelope");
        assert!(!tracker.finish(7, 1_000, "t"));
    }

    #[test]
    fn signalling_out_forgets_what_was_on_screen() {
        let mut tracker = tracker();
        tracker.begin(7, 0);
        tracker.discard();
        assert_eq!(tracker.shown(), 0);
        assert_eq!(tracker.pending(), 0);
    }

    #[test]
    fn dwell_is_clamped_and_never_negative() {
        assert_eq!(clamp_ms(-5_000), 0);
        assert_eq!(clamp_ms(0), 0);
        assert_eq!(clamp_ms(1_500), 1_500);
        assert_eq!(clamp_ms(i64::MAX), MAX_DWELL_MS);
        assert_eq!(clamp_ms(MAX_DWELL_MS.into()), MAX_DWELL_MS);
    }
}
