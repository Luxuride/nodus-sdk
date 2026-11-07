//! The SDK itself: one generic state machine over four ports.

use web_time::Instant;

use nodus_instance::{InstanceApi, PageParams, StreamKind};
use nodus_persist::{CacheKey, Persistence};
use nodus_shared::{AccountFeed, EventBatch, PostFeed};
use nodus_state::{
    AppState, Command, Discovery, FeedState, Session, StreamState, SyncConsent, Timeline,
    TimelinePage, TimelineRequest,
};

use crate::clock::Clock;
use crate::driver::Driver;
use crate::error::SdkError;

/// Timeline page size for a background refresh.
///
/// Smaller than a manual load: a tick exists to keep the newest page current, and
/// a user scrolling wants the next page on demand rather than a deeper prefetch
/// they may never read.
const TIMELINE_PAGE: u16 = 20;

/// What one background tick did.
///
/// Returned instead of an error because a refresh nobody asked for must not raise
/// a toast: the flags tell the caller what was attempted, and `failed` tells it
/// whether the cycle is one the driver has already backed off from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Drive {
    /// One timeline page was requested.
    pub timeline: bool,
    /// One feed page was requested.
    pub feed: bool,
    /// At least one of them failed, so the driver recorded a failure.
    pub failed: bool,
}

impl Drive {
    /// Whether the tick did nothing, which is the common case.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        !self.timeline && !self.feed
    }
}

/// Client-side state machine.
///
/// Generic over its four ports so the same logic runs in the browser, in native
/// tests, and in tooling:
///
/// * `I` — the Mastodon instance ([`InstanceApi`]);
/// * `D` — our recommendation API ([`Discovery`]);
/// * `P` — the cache ([`Persistence`]);
/// * `C` — time ([`Clock`]).
///
/// Every method either performs one I/O step and then dispatches the matching
/// [`Command`], or is [`Sdk::dispatch`] itself. Views cannot reach the ports: they
/// only see [`Sdk::state`].
#[derive(Debug)]
pub struct Sdk<I, D, P, C> {
    instance: I,
    discovery: D,
    persistence: P,
    clock: C,
    state: AppState,
}

impl<I, D, P, C> Sdk<I, D, P, C> {
    /// Builds an SDK from its ports.
    pub fn new(instance: I, discovery: D, persistence: P, clock: C) -> Self {
        Self {
            instance,
            discovery,
            persistence,
            clock,
            state: AppState::default(),
        }
    }

    /// The observable state; views subscribe to individual handles.
    #[must_use]
    pub fn state(&self) -> &AppState {
        &self.state
    }
}

impl<I, D, P, C> Sdk<I, D, P, C>
where
    P: Persistence,
{
    /// The cache port, for maintenance tasks such as clearing on sign-out.
    #[must_use]
    pub fn persistence(&self) -> &P {
        &self.persistence
    }

    /// Applies a pure state change and notifies subscribers.
    ///
    /// This is the only mutation path. It is synchronous, so a view reacting to a
    /// click can change the UI on the same frame.
    pub fn dispatch(&self, command: Command) {
        match command {
            Command::FeedLoaded(feed) => self.state.feed.update(|state| state.replace(feed)),
            Command::DismissItem { item_id } => {
                self.state.feed.update(|state| state.dismiss(item_id));
            }
            Command::ClearFeed => self.state.feed.set(FeedState::default()),
            Command::PostsLoaded { feed, append } => self.state.posts.update(|state| {
                if append {
                    state.extend(feed);
                } else {
                    state.replace(feed);
                }
            }),
            Command::StreamChanged(stream) => self.state.stream.set(stream),
            Command::SyncChanged(consent) => self.state.sync.set(consent),
            Command::SignedIn(session) => self.state.session.set(Some(session)),
            Command::SignedOut => {
                // Consent belongs to the account we just forgot.
                self.state.sync.set(SyncConsent::default());
                self.state.session.set(None);
                self.state.stream.set(StreamState::Disconnected);
                self.state.feed.set(FeedState::default());
                // Replace the map rather than clearing it in place, so subscribers
                // see exactly one notification.
                self.state.timelines.set(std::collections::HashMap::new());
            }
            Command::TimelineLoaded { which, tag, page } => {
                self.state.timelines.update(|timelines| {
                    timelines.insert((which, tag), page);
                });
            }
        }
    }
}

impl<I, D, P, C> Sdk<I, D, P, C>
where
    I: InstanceApi,
    D: Discovery,
    P: Persistence,
    C: Clock,
{
    /// Current time, as the SDK sees it.
    #[must_use]
    pub fn now_unix_millis(&self) -> i64 {
        self.clock.now_unix_millis()
    }

    /// Runs whatever the scheduler says is due at `now`, and records the outcome.
    ///
    /// This is the body of the background loop: the host only has to call it when
    /// [`Driver::next_wake`] says to wake, then call it again. Errors are folded
    /// into the driver's failure count rather than returned, because a refresh the
    /// user did not ask for must not surface an error; `failed` marks the cycle for
    /// whoever wants to log it.
    pub async fn drive(&self, driver: &mut Driver, now: Instant) -> Drive {
        let plan = driver.plan(now);
        let signed_in = self.state.session.with(Option::is_some);
        let mut report = Drive::default();

        if plan.load_timeline && signed_in {
            report.timeline = true;
            driver.record_attempt(now);
            if self
                .load_timeline(TimelineRequest::Home, None, TIMELINE_PAGE)
                .await
                .is_err()
            {
                report.failed = true;
            }
        }

        if plan.refresh_feed {
            report.feed = true;
            driver.record_attempt(now);
            if self.load_feed(None).await.is_err() {
                report.failed = true;
            }
        }

        if !report.is_empty() {
            if report.failed {
                driver.record_failure();
            } else {
                driver.record_success(now);
            }
        }

        report
    }

    /// Validates the token, then stores the session.
    pub async fn sign_in(
        &self,
        host: &str,
        token_scopes: Vec<String>,
    ) -> Result<Session, SdkError<I::Error, D::Error>> {
        let account = self
            .instance
            .verify_credentials()
            .await
            .map_err(SdkError::Instance)?;
        let session = Session {
            host: host.to_owned(),
            account_id: account.id,
            acct: account.acct,
            scopes: token_scopes,
        };
        self.dispatch(Command::SignedIn(session.clone()));
        Ok(session)
    }

    /// Ends the session: clears state and cached data.
    ///
    /// Token revocation stays with the caller, which owns the token.
    pub async fn sign_out(&self) -> Result<(), SdkError<I::Error, D::Error>> {
        self.dispatch(Command::SignedOut);
        self.persistence
            .clear()
            .await
            .map_err(SdkError::Persistence)
    }

    /// Loads one page of a timeline and applies it.
    ///
    /// `cursor` is the previous page's [`TimelinePage::next_cursor`]; `None` loads
    /// the newest page.
    pub async fn load_timeline(
        &self,
        request: TimelineRequest<'_>,
        cursor: Option<&str>,
        limit: u16,
    ) -> Result<TimelinePage, SdkError<I::Error, D::Error>> {
        let params = cursor.map_or_else(
            || PageParams::first(limit),
            |cursor| PageParams::after(cursor, limit),
        );

        let page = match &request {
            TimelineRequest::Tag(tag) => self.instance.tag_timeline(tag, params).await,
            other => self.instance.timeline(stream_kind(other), params).await,
        }
        .map_err(SdkError::Instance)?;

        let page = TimelinePage {
            status_ids: page.items.iter().map(|status| status.id.clone()).collect(),
            max_id: page.next_max_id,
        };
        let (which, tag) = request.key();
        self.dispatch(Command::TimelineLoaded {
            which,
            tag,
            page: page.clone(),
        });
        Ok(page)
    }

    /// Loads one page of the recommendation feed and applies it.
    pub async fn load_feed(
        &self,
        cursor: Option<&str>,
    ) -> Result<AccountFeed, SdkError<I::Error, D::Error>> {
        let feed = self
            .discovery
            .feed(cursor)
            .await
            .map_err(SdkError::Discovery)?;
        self.dispatch(Command::FeedLoaded(feed.clone()));
        Ok(feed)
    }

    /// Asks the server for a *new* snapshot, superseding `feed_id`.
    ///
    /// `load_feed(None)` is a read and the server may answer it from the snapshot it
    /// handed out minutes ago — correct for a page load, useless for a refresh the
    /// user asked for. This names the snapshot being left behind, so the server knows
    /// the client is not merely re-rendering: it ranks again and the old snapshot stays
    /// continuable until its own lifetime ends (see `recommender/docs/07-api.md`).
    pub async fn refresh_feed(
        &self,
        feed_id: &str,
    ) -> Result<AccountFeed, SdkError<I::Error, D::Error>> {
        let feed = self
            .discovery
            .refresh(feed_id)
            .await
            .map_err(SdkError::Discovery)?;
        self.dispatch(Command::FeedLoaded(feed.clone()));
        Ok(feed)
    }

    /// Loads one page of recommended posts and applies it.
    ///
    /// `append` is the caller's claim that the cursor continues the current list;
    /// a first page replaces it, because a new snapshot and an old one are different
    /// answers to the same question.
    pub async fn load_posts(
        &self,
        cursor: Option<&str>,
    ) -> Result<PostFeed, SdkError<I::Error, D::Error>> {
        let feed = self
            .discovery
            .posts(cursor)
            .await
            .map_err(SdkError::Discovery)?;
        self.dispatch(Command::PostsLoaded {
            feed: feed.clone(),
            append: cursor.is_some(),
        });
        Ok(feed)
    }

    /// Reports a batch of interactions.
    pub async fn report_events(
        &self,
        batch: EventBatch,
    ) -> Result<(), SdkError<I::Error, D::Error>> {
        self.discovery
            .send_events(&batch)
            .await
            .map_err(SdkError::Discovery)
    }

    /// Turns server-side sync on or off; telemetry is a separate consent.
    ///
    /// The state changes only after the server accepted it: a toggle that flips on
    /// a failed request would claim a consent nobody granted.
    pub async fn set_sync(
        &self,
        enable: bool,
        telemetry_opt_in: bool,
    ) -> Result<(), SdkError<I::Error, D::Error>> {
        self.discovery
            .set_sync(enable, telemetry_opt_in)
            .await
            .map_err(SdkError::Discovery)?;
        self.dispatch(Command::SyncChanged(SyncConsent {
            enabled: enable,
            telemetry_opt_in,
        }));
        Ok(())
    }

    /// Asks the server what it holds for this account and records the answer.
    ///
    /// Called when Settings opens, so the toggle describes the server's state rather
    /// than the state of this tab: another browser (or a revocation) is invisible to
    /// us otherwise.
    pub async fn load_sync_consent(&self) -> Result<SyncConsent, SdkError<I::Error, D::Error>> {
        let me = self.discovery.me().await.map_err(SdkError::Discovery)?;
        let consent = SyncConsent {
            enabled: me.sync_enabled,
            telemetry_opt_in: me.telemetry_opt_in,
        };
        self.dispatch(Command::SyncChanged(consent));
        Ok(consent)
    }

    /// Warm-starts the home timeline from the cache.
    ///
    /// Failures are swallowed: a cold cache is not an error, and the network
    /// request that follows will fill it.
    pub async fn restore_home_timeline(&self, host: &str) {
        let key = CacheKey::Timeline {
            host: host.to_owned(),
            which: Timeline::Home,
            tag: None,
        };
        if let Ok(Some(bytes)) = self.persistence.load(&key).await {
            if let Ok(page) = serde_json::from_slice::<TimelinePage>(&bytes) {
                self.dispatch(Command::TimelineLoaded {
                    which: Timeline::Home,
                    tag: None,
                    page,
                });
            }
        }
    }

    /// Persists the home timeline, so the next boot renders before the network.
    pub async fn cache_home_timeline(
        &self,
        host: &str,
    ) -> Result<(), SdkError<I::Error, D::Error>> {
        let key = CacheKey::Timeline {
            host: host.to_owned(),
            which: Timeline::Home,
            tag: None,
        };
        let page = self
            .state
            .timelines
            .with(|timelines| timelines.get(&(Timeline::Home, None)).cloned());
        let Some(page) = page else {
            return Ok(());
        };
        let bytes = serde_json::to_vec(&page).map_err(|error| {
            SdkError::Persistence(crate::error::PersistenceError::Serde(error.to_string()))
        })?;
        self.persistence
            .store(&key, bytes)
            .await
            .map_err(SdkError::Persistence)
    }
}

const fn stream_kind(request: &TimelineRequest<'_>) -> StreamKind {
    match request {
        TimelineRequest::Home | TimelineRequest::Tag(_) => StreamKind::User,
        TimelineRequest::Local => StreamKind::Local,
        TimelineRequest::Federated => StreamKind::Federated,
    }
}

#[cfg(test)]
mod tests;
