//! The observed state tree.
//!
//! Everything here is plain data in a [`Handle`](crate::Handle): no methods that
//! reach the network, no framework types. Views read it, [`Command`]s change it.

use std::collections::HashMap;

use nodus_instance::model::{Account, Status};
use nodus_protocol::Capabilities;
use nodus_shared::{AccountFeed, AccountItem, PostFeed, PostItem};

use crate::handle::Handle;

/// The signed-in account, as far as the client is concerned.
///
/// The access token is intentionally absent: it lives in the client's secret
/// storage and is handed to the transport on demand, never to a view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// Instance host, e.g. `mastodon.social`.
    pub host: String,
    /// Account id on that instance.
    pub account_id: String,
    /// `user@host`, for display.
    pub acct: String,
    /// Scopes the token was granted.
    pub scopes: Vec<String>,
}

/// Connection state of the instance streaming socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    /// No socket, and none wanted.
    Disconnected,
    /// Opening the socket.
    Connecting,
    /// Receiving events.
    Connected,
    /// Backing off before the next attempt.
    Reconnecting {
        /// Attempt number, starting at 1.
        attempt: u32,
    },
    /// Gave up: token revoked, or the host has been unreachable for too long.
    Stopped,
}

/// A timeline the client can display.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Timeline {
    /// Home timeline of the signed-in account.
    Home,
    /// Local timeline of the instance.
    Local,
    /// Federated timeline of the instance.
    Federated,
    /// A hashtag timeline; the tag is the key's second element.
    Tag,
    /// An account's posts, by account id.
    ///
    /// A profile's posts are a timeline like any other: caching them under the same
    /// mechanism is what lets a profile paint from state on a second visit.
    Account(String),
}

/// A timeline the client was asked to load.
///
/// Carries the tag in the variant that needs one, so "tag timeline without a tag"
/// is unrepresentable instead of a runtime check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineRequest<'a> {
    /// Home timeline.
    Home,
    /// Local timeline.
    Local,
    /// Federated timeline.
    Federated,
    /// A hashtag timeline.
    Tag(&'a str),
    /// One account's posts.
    Account(&'a str),
}

impl TimelineRequest<'_> {
    /// The state key this request loads into.
    #[must_use]
    pub fn key(&self) -> TimelineKey {
        match self {
            Self::Home => (Timeline::Home, None),
            Self::Local => (Timeline::Local, None),
            Self::Federated => (Timeline::Federated, None),
            Self::Tag(tag) => (Timeline::Tag, Some((*tag).to_owned())),
            Self::Account(id) => (Timeline::Account((*id).to_owned()), None),
        }
    }
}

/// Key of a cached timeline: which timeline, and which tag for [`Timeline::Tag`].
pub type TimelineKey = (Timeline, Option<String>);

/// Why a feed is weaker than it could be, surfaced verbatim to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Degraded {
    /// Too few signals; the feed uses the cold-start ladder.
    ColdStart,
    /// Similarity was computed from an older snapshot.
    StaleSimilarity,
    /// No server-side sync, so the server sees only request-scoped signals.
    NoServerSync,
}

impl Degraded {
    /// Parses the wire flag, ignoring values this build does not know.
    #[must_use]
    pub fn from_wire(flag: &str) -> Option<Self> {
        match flag {
            "cold_start" => Some(Self::ColdStart),
            "stale_similarity" => Some(Self::StaleSimilarity),
            "no_server_sync" => Some(Self::NoServerSync),
            _ => None,
        }
    }
}

/// Recommendation feed state.
#[derive(Debug, Clone, Default)]
pub struct FeedState {
    /// Items in display order; already ranked by the server.
    pub items: Vec<AccountItem>,
    /// Opaque cursor for the next page.
    pub next_cursor: Option<String>,
    /// Generation timestamp reported by the server.
    pub generated_at: Option<String>,
    /// Active degradation flags.
    pub degraded: Vec<Degraded>,
    /// Item ids dismissed locally, hidden until the next refresh.
    pub dismissed: Vec<i64>,
}

impl FeedState {
    /// Replaces the feed contents with a fresh server response.
    pub fn replace(&mut self, feed: AccountFeed) {
        self.items = feed.items;
        self.next_cursor = feed.next_cursor;
        self.generated_at = Some(feed.generated_at);
        self.degraded = feed
            .degraded
            .as_deref()
            .and_then(Degraded::from_wire)
            .into_iter()
            .collect();
        self.dismissed.clear();
    }

    /// Hides an item until the next refresh.
    pub fn dismiss(&mut self, item_id: i64) {
        if !self.dismissed.contains(&item_id) {
            self.dismissed.push(item_id);
        }
    }

    /// Items that should be rendered: the feed minus local dismissals.
    pub fn visible(&self) -> impl Iterator<Item = &AccountItem> {
        self.items
            .iter()
            .filter(|item| !self.dismissed.contains(&item.item_id))
    }
}

/// Recommended posts, as the server ranked them.
///
/// Kept beside [`FeedState`] rather than merged into it: the two feeds have
/// different lifetimes on the server (an account snapshot is reused for minutes, a
/// post page never is), so one state object would have to guess which lifetime it
/// was describing.
#[derive(Debug, Clone, Default)]
pub struct PostFeedState {
    /// Snapshot id, needed to report impressions against the list they were shown in.
    pub feed_id: Option<String>,
    /// Items in display order.
    pub items: Vec<PostItem>,
    /// Opaque cursor for the next page.
    pub next_cursor: Option<String>,
    /// Generation timestamp reported by the server.
    pub generated_at: Option<String>,
    /// Active degradation flags.
    pub degraded: Vec<Degraded>,
}

impl PostFeedState {
    /// Replaces the contents with a fresh server response.
    pub fn replace(&mut self, feed: PostFeed) {
        self.feed_id = Some(feed.feed_id);
        self.items = feed.items;
        self.next_cursor = feed.next_cursor;
        self.generated_at = Some(feed.generated_at);
        self.degraded = feed
            .degraded
            .as_deref()
            .and_then(Degraded::from_wire)
            .into_iter()
            .collect();
    }

    /// Appends a continuation page, dropping ids already shown.
    ///
    /// The server guarantees no repeats within one snapshot, but a snapshot can be
    /// re-requested after a refresh, and a duplicated `item_id` is a duplicated
    /// `key` in the view — the one bug a virtualised list cannot survive.
    pub fn extend(&mut self, feed: PostFeed) {
        // A continuation page belongs to the snapshot it cursor came from, so the id
        // is kept rather than overwritten: impressions must name the list they were
        // shown in, and the first page's id is the one the server minted.
        self.feed_id.get_or_insert(feed.feed_id.clone());
        let known: Vec<i64> = self.items.iter().map(|item| item.item_id).collect();
        self.items.extend(
            feed.items
                .into_iter()
                .filter(|item| !known.contains(&item.item_id)),
        );
        self.next_cursor = feed.next_cursor;
        self.generated_at = Some(feed.generated_at);
        self.degraded = feed
            .degraded
            .as_deref()
            .and_then(Degraded::from_wire)
            .into_iter()
            .collect();
    }
}

/// One page of a timeline.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TimelinePage {
    /// Status ids in display order.
    pub status_ids: Vec<String>,
    /// Cursor for older items (`max_id`).
    pub max_id: Option<String>,
    /// When this page was fetched, in Unix milliseconds.
    ///
    /// A page without a timestamp cannot be judged stale, so it is stamped even when
    /// it came from the network seconds ago: freshness is data, not a guess.
    pub fetched_at: Option<i64>,
}

impl TimelinePage {
    /// Folds an older page into this one, keeping display order.
    ///
    /// Appending is a merge, never a replacement: a refresh must not drop pages the
    /// reader has already scrolled through, and duplicates are dropped by id so a
    /// paging hiccup cannot render the same status twice.
    pub fn append(&mut self, older: Self) {
        let known: std::collections::HashSet<String> = self.status_ids.iter().cloned().collect();
        self.status_ids.extend(
            older
                .status_ids
                .into_iter()
                .filter(|id| !known.contains(id)),
        );
        self.max_id = older.max_id;
    }

    /// The cursor to request the next (older) page.
    #[must_use]
    pub fn next_cursor(&self) -> Option<&str> {
        self.max_id.as_deref()
    }
}

/// Server-side sync consent, as the server reports it.
///
/// Held as `Option` so "we have not asked yet" is representable: a view that renders
/// `false` while the answer is in flight shows a consent the user never gave, and it
/// shows it for as long as the request takes.
///
/// Two independent consents live here because they are separate decisions: letting
/// the server hold an encrypted copy of the token (background sync) and letting it
/// keep interaction telemetry. The second stays off unless the user turns it on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncConsent {
    /// Whether the server holds an encrypted copy of the token.
    pub enabled: bool,
    /// Whether interaction telemetry is kept.
    pub telemetry_opt_in: bool,
}

/// Everything the client knows, as observed handles.
///
/// Cheap to clone (`Rc` inside). Each field is its own [`Handle`], so a view can
/// observe one field without re-rendering on unrelated changes.
#[derive(Debug, Clone)]
pub struct AppState {
    /// What this server can do: the answer to `/api/v1/meta`, once per session.
    ///
    /// Recorded in state rather than asked at each call site, so a screen can decide
    /// its path synchronously while it renders instead of negotiating mid-view.
    pub capabilities: Handle<Capabilities>,
    /// Signed-in account, `None` when signed out.
    pub session: Handle<Option<Session>>,
    /// What the recommendation server holds for this account.
    pub sync: Handle<Option<SyncConsent>>,
    /// Streaming socket state.
    pub stream: Handle<StreamState>,
    /// Recommendation feed.
    pub feed: Handle<FeedState>,
    /// Recommended posts.
    pub posts: Handle<PostFeedState>,
    /// Timeline pages, keyed by timeline and tag.
    pub timelines: Handle<HashMap<TimelineKey, TimelinePage>>,
    /// Statuses by id: the single copy every screen renders.
    ///
    /// Pages hold ids, entities hold content. One status favourited in a thread is
    /// therefore favourited in every timeline that shows it, without any screen
    /// copying it, and a `delete` event removes it everywhere at once.
    pub statuses: Handle<HashMap<String, Status>>,
    /// Accounts by id, for the same reason.
    pub accounts: Handle<HashMap<String, Account>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            capabilities: Handle::new(Capabilities::mastodon()),
            session: Handle::new(None),
            sync: Handle::new(None),
            stream: Handle::new(StreamState::Disconnected),
            feed: Handle::new(FeedState::default()),
            posts: Handle::new(PostFeedState::default()),
            timelines: Handle::new(HashMap::new()),
            statuses: Handle::new(HashMap::new()),
            accounts: Handle::new(HashMap::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(cursor: Option<&str>) -> AccountFeed {
        AccountFeed {
            feed_id: "f1".to_owned(),
            generated_at: "2026-11-06T21:00:00Z".to_owned(),
            contract: "0.1.0".to_owned(),
            model_version: 0,
            similarity_age_hours: 0.0,
            degraded: Some("cold_start".to_owned()),
            signals_unavailable: Vec::new(),
            items: Vec::new(),
            next_cursor: cursor.map(ToOwned::to_owned),
        }
    }

    #[test]
    fn replace_keeps_known_degradations_and_drops_unknown_ones() {
        let mut state = FeedState::default();
        state.replace(feed(Some("c2")));

        assert_eq!(state.degraded, vec![Degraded::ColdStart]);
        assert_eq!(state.next_cursor.as_deref(), Some("c2"));
    }

    #[test]
    fn dismiss_is_idempotent_and_cleared_by_refresh() {
        let mut state = FeedState::default();
        state.dismiss(7);
        state.dismiss(7);
        assert_eq!(state.dismissed, vec![7]);

        state.replace(feed(None));
        assert_eq!(state.dismissed, Vec::<i64>::new());
    }
}
