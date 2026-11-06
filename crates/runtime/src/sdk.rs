//! The SDK itself: one generic state machine over four ports.

use nodus_instance_api::{InstanceApi, PageParams, StreamKind};
use nodus_persist::{CacheKey, Persistence};
use nodus_shared::{AccountFeed, EventBatch};
use nodus_state::{
    AppState, Command, Discovery, FeedState, Session, StreamState, Timeline, TimelinePage,
    TimelineRequest,
};

use crate::clock::Clock;
use crate::error::SdkError;

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
            Command::StreamChanged(stream) => self.state.stream.set(stream),
            Command::SignedIn(session) => self.state.session.set(Some(session)),
            Command::SignedOut => {
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
    pub async fn set_sync(
        &self,
        enable: bool,
        telemetry_opt_in: bool,
    ) -> Result<(), SdkError<I::Error, D::Error>> {
        self.discovery
            .set_sync(enable, telemetry_opt_in)
            .await
            .map_err(SdkError::Discovery)
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
