//! The SDK itself: one generic state machine over four ports.

use web_time::Instant;

use nodus_instance::StreamEvent;
use nodus_instance::actions::{
    AccountActions, Browse, FilterActions, HighlightActions, ScheduleActions, Search,
};
use nodus_instance::model::{Account, Status};
use nodus_instance::{InstanceApi, PageParams, StreamKind};
use nodus_persist::{CacheKey, Persistence};
use nodus_protocol::Capabilities;
use nodus_shared::{AccountFeed, EventBatch, PostFeed};
use nodus_state::page::{PageEntry, PageItems, PageKind};
use nodus_state::{
    AppState, Command, Discovery, FeedState, Session, StreamState, SyncConsent, Timeline,
    TimelinePage, TimelineRequest,
};

use nodus_instance::model::Conversation;

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

    /// Replaces or extends one page. Extending keeps the order a screen already drew.
    fn store_page(
        &self,
        kind: PageKind,
        items: PageItems,
        cursor: Option<String>,
        append: bool,
        fetched_at: i64,
    ) {
        self.state.pages.update(|pages| {
            let entry = pages.entry(kind).or_default();
            let older = PageEntry {
                items,
                cursor,
                fetched_at: Some(fetched_at),
            };
            if append && entry.items.same_shape(&older.items) {
                entry.append(older);
            } else {
                *entry = older;
            }
        });
    }

    /// Applies a pure state change and notifies subscribers.
    ///
    /// This is the only mutation path. It is synchronous, so a view reacting to a
    /// click can change the UI on the same frame.
    /// Removes one account from a people page.
    fn drop_account(&self, kind: &PageKind, account: &str) {
        self.state.pages.update(|pages| {
            if let Some(entry) = pages.get_mut(kind) {
                if let PageItems::Accounts(held) = &mut entry.items {
                    held.retain(|known| known.id != account);
                }
            }
        });
    }

    /// Merges muted or read conversations into the stored page.
    ///
    /// Kept out of `dispatch` only because that match is already at clippy's line
    /// limit; the rule is unchanged — dispatch is the single mutation path.
    fn merge_conversations(&self, items: Vec<Conversation>) {
        self.state.pages.update(|pages| {
            if let Some(entry) = pages.get_mut(&PageKind::Conversations) {
                if let PageItems::Conversations(held) = &mut entry.items {
                    for item in items {
                        match held.iter_mut().find(|known| known.id == item.id) {
                            Some(slot) => *slot = item,
                            None => held.insert(0, item),
                        }
                    }
                }
            }
        });
    }

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
            Command::StreamChanged(stream) => {
                // Reconnecting means events were missed, not that the pages are wrong: the
                // items stay readable and stop claiming to be current, so the next visit
                // refetches instead of showing a hole where a notification should be.
                if stream == StreamState::Connected {
                    self.state.pages.update(|pages| {
                        for kind in [PageKind::Notifications, PageKind::Conversations] {
                            if let Some(entry) = pages.get_mut(&kind) {
                                entry.fetched_at = None;
                            }
                        }
                    });
                }
                self.state.stream.set(stream);
            }
            Command::SyncChanged(consent) => self.state.sync.set(Some(consent)),
            Command::SignedIn(session) => self.state.session.set(Some(session)),
            Command::CapabilitiesLoaded(capabilities) => {
                self.state.capabilities.set(capabilities);
            }
            Command::SignedOut => {
                // Consent belongs to the account we just forgot.
                self.state.sync.set(None);
                self.state.session.set(None);
                self.state.stream.set(StreamState::Disconnected);
                self.state.feed.set(FeedState::default());
                // Replace the map rather than clearing it in place, so subscribers
                // see exactly one notification.
                self.state.timelines.set(std::collections::HashMap::new());
            }
            Command::ConversationsUpserted { items } => self.merge_conversations(items),

            Command::PageAccountDropped { kind, account } => self.drop_account(&kind, &account),

            Command::PageStale { kind } => {
                self.state.pages.update(|pages| {
                    if let Some(entry) = pages.get_mut(&kind) {
                        entry.fetched_at = None;
                    }
                });
            }

            Command::PageLoaded {
                kind,
                items,
                cursor,
                append,
                fetched_at,
            } => self.store_page(kind, items, cursor, append, fetched_at),

            Command::TimelineLoaded {
                which,
                tag,
                page,
                append,
            } => {
                self.state.timelines.update(|timelines| {
                    let key = (which, tag);
                    if append {
                        if let Some(held) = timelines.get_mut(&key) {
                            held.append(page);
                            return;
                        }
                    }
                    timelines.insert(key, page);
                });
            }
            Command::StatusesUpserted { statuses } => {
                self.state.statuses.update(|held| {
                    for status in statuses {
                        held.insert(status.id.clone(), status);
                    }
                });
            }
            Command::AccountsUpserted { accounts } => {
                self.state.accounts.update(|held| {
                    for account in accounts {
                        held.insert(account.id.clone(), account);
                    }
                });
            }
            Command::StatusRemoved(id) => {
                self.state.statuses.update(|held| {
                    held.remove(&id);
                });
                self.state.timelines.update(|pages| {
                    for page in pages.values_mut() {
                        page.status_ids.retain(|held| held != &id);
                    }
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

    /// Adopts a session the browser already holds.
    ///
    /// The record in `localStorage` is a claim, not a fact: it says which account
    /// this browser was signed in as, and the token it carries is what the instance
    /// checks on the next request. Adopting it before that check is what keeps the
    /// first paint signed in — the alternative is a sign-in screen for as long as
    /// `verify_credentials` takes, on every reload.
    ///
    /// A record the instance rejects signs the browser out through the normal path:
    /// the request fails with `Unauthorized`, and the caller clears the record.
    pub fn adopt_session(&self, session: Session) {
        self.dispatch(Command::SignedIn(session));
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
            TimelineRequest::Account(id) => {
                self.instance
                    .account_statuses(&nodus_instance::LocalId(id.to_string()), params)
                    .await
            }
            other => self.instance.timeline(stream_kind(other), params).await,
        }
        .map_err(SdkError::Instance)?;

        // One write for the entities, one for the page: a page is ids, and the
        // statuses it names live in exactly one place.
        self.dispatch(Command::StatusesUpserted {
            statuses: page.items.clone(),
        });
        self.dispatch(Command::AccountsUpserted {
            accounts: page
                .items
                .iter()
                .map(|status| status.account.clone())
                .collect(),
        });

        let page = TimelinePage {
            status_ids: page.items.iter().map(|status| status.id.clone()).collect(),
            max_id: page.next_max_id,
            fetched_at: Some(self.clock.now_unix_millis()),
        };
        let (which, tag) = request.key();
        self.dispatch(Command::TimelineLoaded {
            which,
            tag,
            page: page.clone(),
            // A cursor means "older than what I hold", so it extends the page; no
            // cursor means "the newest", so it replaces.
            append: cursor.is_some(),
        });
        Ok(page)
    }

    /// Applies one streaming event to state.
    ///
    /// The reducer lives here, not in the socket transport, because state is the only
    /// thing that can make an event visible: a status that arrives on the socket must
    /// land in the same entity store a page reads, or the screen holding it cannot
    /// repaint.
    ///
    /// Returns whether the event changed anything, so a caller can skip a notification
    /// for a frame that said nothing new.
    pub fn apply(&self, event: &StreamEvent) -> bool {
        match event {
            StreamEvent::Update(status) | StreamEvent::StatusUpdate(status) => {
                let status = (**status).clone();
                self.dispatch(Command::AccountsUpserted {
                    accounts: vec![status.account.clone()],
                });
                self.dispatch(Command::StatusesUpserted {
                    statuses: vec![status],
                });
                true
            }
            StreamEvent::Delete(id) => {
                let known = self.read_status(id).is_some();
                self.dispatch(Command::StatusRemoved(id.clone()));
                known
            }
            StreamEvent::Notification(note) => {
                let mut accounts = vec![note.account.clone()];
                accounts.extend(note.status.as_ref().map(|status| status.account.clone()));
                self.dispatch(Command::AccountsUpserted { accounts });
                match &note.status {
                    Some(status) => {
                        self.dispatch(Command::StatusesUpserted {
                            statuses: vec![status.clone()],
                        });
                        true
                    }
                    None => false,
                }
            }
            StreamEvent::Conversation(thread) => match &thread.last_status {
                Some(status) => {
                    self.dispatch(Command::AccountsUpserted {
                        accounts: vec![status.account.clone()],
                    });
                    self.dispatch(Command::StatusesUpserted {
                        statuses: vec![status.clone()],
                    });
                    true
                }
                None => false,
            },
            // Filters and events this client does not model say nothing about state.
            StreamEvent::FiltersChanged | StreamEvent::Unknown(_) => false,
        }
    }

    /// Parses and applies one socket frame.
    ///
    /// The convenience a socket sink needs: `if sdk.apply_frame(raw) { … }`.
    pub fn apply_frame(&self, frame: &str) -> bool {
        StreamEvent::parse(frame).is_some_and(|event| self.apply(&event))
    }

    /// What the client already holds for a timeline, newest first.
    ///
    /// A pure read: no request, no clock, no side effect. Views render this, which is
    /// what makes a navigation free when the SDK has the page — and a status missing
    /// from the entity store simply drops out rather than rendering as a hole.
    #[must_use]
    pub fn read_timeline(&self, request: &TimelineRequest<'_>) -> (Vec<Status>, Option<String>) {
        let key = request.key();
        let ids = self
            .state
            .timelines
            .with(|timelines| timelines.get(&key).cloned())
            .unwrap_or_default();
        let statuses = self.state.statuses.with(|held| {
            ids.status_ids
                .iter()
                .filter_map(|id| held.get(id).cloned())
                .collect()
        });
        (statuses, ids.max_id)
    }

    /// When the newest page of a timeline was fetched.
    #[must_use]
    pub fn timeline_fetched_at(&self, request: &TimelineRequest<'_>) -> Option<i64> {
        let key = request.key();
        self.state
            .timelines
            .with(|timelines| timelines.get(&key).and_then(|page| page.fetched_at))
    }

    /// One status the client holds, if any.
    #[must_use]
    pub fn read_status(&self, id: &str) -> Option<Status> {
        self.state.statuses.with(|held| held.get(id).cloned())
    }

    /// One account the client holds, if any.
    #[must_use]
    pub fn read_account(&self, id: &str) -> Option<Account> {
        self.state.accounts.with(|held| held.get(id).cloned())
    }

    /// Loads a timeline only if what the client holds is missing or stale.
    ///
    /// `ttl_ms` is how long a page is trusted. `Ok(None)` means "nothing to do": the
    /// caller renders [`read_timeline`](Self::read_timeline) immediately, so a
    /// navigation paints from state and never waits on the network.
    pub async fn ensure_timeline(
        &self,
        request: TimelineRequest<'_>,
        cursor: Option<&str>,
        limit: u16,
        ttl_ms: i64,
    ) -> Result<Option<TimelinePage>, SdkError<I::Error, D::Error>> {
        if cursor.is_none() {
            let fresh = self
                .timeline_fetched_at(&request)
                .is_some_and(|at| self.clock.now_unix_millis().saturating_sub(at) < ttl_ms);
            let held = self.state.timelines.with(|timelines| {
                timelines
                    .get(&request.key())
                    .is_some_and(|page| !page.status_ids.is_empty())
            });
            if fresh && held {
                return Ok(None);
            }
        }
        self.load_timeline(request, cursor, limit).await.map(Some)
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

    /// Reads what this server can do and records it.
    ///
    /// Called once per session, before the first screen that could use an extension.
    /// A server that answers nothing usable records the Mastodon surface, so every
    /// call site has an answer instead of an `Option` it must branch on.
    pub async fn load_capabilities(&self) -> Result<Capabilities, SdkError<I::Error, D::Error>> {
        let meta = self.discovery.meta().await.map_err(SdkError::Discovery)?;
        let capabilities = Capabilities::from_parts(meta.contract, &meta.features);
        self.dispatch(Command::CapabilitiesLoaded(capabilities.clone()));
        Ok(capabilities)
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
                    // A restored page replaces whatever is held: it is the newest
                    // state the last session saw, not an older page of it.
                    append: false,
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
        // An account timeline is fetched through `account_statuses`, never through
        // the streaming stream kind; the arm exists to keep the match total.
        TimelineRequest::Home | TimelineRequest::Tag(_) | TimelineRequest::Account(_) => {
            StreamKind::User
        }
        TimelineRequest::Local => StreamKind::Local,
        TimelineRequest::Federated => StreamKind::Federated,
    }
}

impl<I, D, P, C> Sdk<I, D, P, C>
where
    // Every action trait reports through the instance's own error: one error type per
    // transport, so a caller matches on `SdkError` rather than on four failure shapes.
    I: InstanceApi
        + Browse<Error = <I as InstanceApi>::Error>
        + AccountActions<Error = <I as InstanceApi>::Error>
        + Search<Error = <I as InstanceApi>::Error>
        + ScheduleActions<Error = <I as InstanceApi>::Error>
        + FilterActions<Error = <I as InstanceApi>::Error>
        + HighlightActions<Error = <I as InstanceApi>::Error>,
    D: Discovery,
    P: Persistence,
    C: Clock,
{
    /// A stored page, as the last fetch left it.
    #[must_use]
    pub fn page(&self, kind: &PageKind) -> Option<PageEntry> {
        self.state.pages.with(|pages| pages.get(kind).cloned())
    }

    /// Loads a page unless the one held answers the question.
    ///
    /// Returns whether the instance was asked. An older page is only *appended* when the
    /// cursor asked for is the cursor the held page ended on: after an offline spell the
    /// instance's ordering may have moved, and appending across the gap would splice two
    /// ranges together with a hole in the middle.
    pub async fn ensure_page(
        &self,
        kind: PageKind,
        cursor: Option<&str>,
        limit: u16,
        ttl_ms: i64,
    ) -> Result<bool, SdkError<<I as InstanceApi>::Error, D::Error>> {
        let now = self.clock.now_unix_millis();
        let held = self.page(&kind);
        let continued = match (&held, cursor) {
            (Some(entry), Some(cursor)) => entry.cursor.as_deref() == Some(cursor),
            _ => false,
        };
        if cursor.is_none()
            && held
                .as_ref()
                .is_some_and(|entry| entry.is_fresh(now, ttl_ms) && entry.items != PageItems::Empty)
        {
            return Ok(false);
        }

        let params = match cursor {
            Some(cursor) => PageParams::after(cursor, limit),
            None => PageParams::first(limit),
        };
        let (items, next) = self.fetch_page_items(&kind, params).await?;
        self.absorb(&items);
        self.dispatch(Command::PageLoaded {
            kind,
            items,
            cursor: next,
            append: continued,
            fetched_at: now,
        });
        Ok(true)
    }

    /// One request per page kind; the shape of the answer decides the variant.
    /// One directory page, with its offset cursor.
    ///
    /// The directory is the only list Mastodon pages by offset, so the cursor is how many
    /// accounts have been read so far rather than a post id. A short page is the end:
    /// the instance sends a bare array with no "there is more" flag.
    async fn directory_page(
        &self,
        local: bool,
        params: PageParams,
    ) -> Result<(PageItems, Option<String>), SdkError<<I as InstanceApi>::Error, D::Error>> {
        let offset: u32 = params
            .max_id
            .as_deref()
            .and_then(|cursor| cursor.parse().ok())
            .unwrap_or(0);
        let accounts = self
            .instance
            .directory(local, offset)
            .await
            .map_err(SdkError::Instance)?;
        let read = u32::try_from(accounts.len()).unwrap_or(u32::MAX);
        let next = if read >= <I as Browse>::DIRECTORY_PAGE {
            Some((offset + read).to_string())
        } else {
            None
        };
        Ok((PageItems::Accounts(accounts), next))
    }

    /// The two settings lists, which arrive whole.
    ///
    /// Mastodon pages neither, so the cursor is always `None` and a refresh replaces the
    /// page rather than appending to it.
    async fn settings_items(
        &self,
        kind: &PageKind,
    ) -> Result<(PageItems, Option<String>), SdkError<<I as InstanceApi>::Error, D::Error>> {
        let answer = if matches!(kind, PageKind::Filters) {
            let items = self.instance.filters().await.map_err(SdkError::Instance)?;
            (PageItems::Filters(items), None)
        } else {
            // Featured tags are public on an account, but this page is "mine": the
            // session names whose profile the settings screen is editing.
            let account = self
                .state
                .session
                .get()
                .map(|session| session.account_id)
                .ok_or(SdkError::NotSignedIn)?;
            let items = self
                .instance
                .featured_tags(&nodus_instance::LocalId(account))
                .await
                .map_err(SdkError::Instance)?;
            (PageItems::FeaturedTags(items), None)
        };
        Ok(answer)
    }

    async fn fetch_page_items(
        &self,
        kind: &PageKind,
        params: PageParams,
    ) -> Result<(PageItems, Option<String>), SdkError<<I as InstanceApi>::Error, D::Error>> {
        let answer = match kind {
            PageKind::Notifications => {
                let page = self
                    .instance
                    .notifications(params)
                    .await
                    .map_err(SdkError::Instance)?;
                (PageItems::Notifications(page.items), page.next_max_id)
            }
            PageKind::Conversations => {
                let page = self
                    .instance
                    .conversations(params)
                    .await
                    .map_err(SdkError::Instance)?;
                (PageItems::Conversations(page.items), page.next_max_id)
            }
            PageKind::Favourites => {
                let page = self
                    .instance
                    .favourites(params)
                    .await
                    .map_err(SdkError::Instance)?;
                (PageItems::Statuses(page.items), page.next_max_id)
            }
            PageKind::Bookmarks => {
                let page = self
                    .instance
                    .bookmarks(params)
                    .await
                    .map_err(SdkError::Instance)?;
                (PageItems::Statuses(page.items), page.next_max_id)
            }
            PageKind::Followers(id) => {
                let page = self
                    .instance
                    .followers(&nodus_instance::LocalId(id.clone()), params)
                    .await
                    .map_err(SdkError::Instance)?;
                (PageItems::Accounts(page.items), page.next_max_id)
            }
            PageKind::Following(id) => {
                let page = self
                    .instance
                    .following(&nodus_instance::LocalId(id.clone()), params)
                    .await
                    .map_err(SdkError::Instance)?;
                (PageItems::Accounts(page.items), page.next_max_id)
            }
            PageKind::Mutes => {
                let page = self
                    .instance
                    .mutes(params)
                    .await
                    .map_err(SdkError::Instance)?;
                (PageItems::Accounts(page.items), page.next_max_id)
            }
            PageKind::Blocks => {
                let page = self
                    .instance
                    .blocks(params)
                    .await
                    .map_err(SdkError::Instance)?;
                (PageItems::Accounts(page.items), page.next_max_id)
            }
            PageKind::Scheduled => {
                let items = self
                    .instance
                    .scheduled()
                    .await
                    .map_err(SdkError::Instance)?;
                (PageItems::Scheduled(items), None)
            }
            PageKind::Search(query) => {
                let results = self
                    .instance
                    .search(query, false)
                    .await
                    .map_err(SdkError::Instance)?;
                (PageItems::Search(Box::new(results)), None)
            }
            PageKind::Directory(local) => self.directory_page(*local, params).await?,
            PageKind::Filters | PageKind::FeaturedTags => self.settings_items(kind).await?,
        };
        Ok(answer)
    }

    /// Copies the statuses and accounts a page brought into the entity store.
    ///
    /// A search result and a follower list then show the same status the timeline shows,
    /// with the same favourite count, instead of a second copy taken at another moment.
    fn absorb(&self, items: &PageItems) {
        match items {
            PageItems::Statuses(statuses) => self.dispatch(Command::StatusesUpserted {
                statuses: statuses.clone(),
            }),
            PageItems::Accounts(accounts) => self.dispatch(Command::AccountsUpserted {
                accounts: accounts.clone(),
            }),
            PageItems::Search(results) => {
                self.dispatch(Command::StatusesUpserted {
                    statuses: results.statuses.clone(),
                });
                self.dispatch(Command::AccountsUpserted {
                    accounts: results.accounts.clone(),
                });
            }
            PageItems::Empty
            | PageItems::Notifications(_)
            | PageItems::Conversations(_)
            | PageItems::Scheduled(_)
            | PageItems::Filters(_)
            | PageItems::FeaturedTags(_) => {}
        }
    }
}

#[cfg(test)]
mod tests;
