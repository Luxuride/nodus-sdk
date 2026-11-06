//! The observed state tree.
//!
//! Everything here is plain data in a [`Handle`](crate::Handle): no methods that
//! reach the network, no framework types. Views read it, [`Command`]s change it.

use std::collections::HashMap;

use nodus_shared::{AccountFeed, AccountItem};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Timeline {
    /// Home timeline of the signed-in account.
    Home,
    /// Local timeline of the instance.
    Local,
    /// Federated timeline of the instance.
    Federated,
    /// A hashtag timeline; the tag is the key's second element.
    Tag,
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

/// One page of a timeline.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TimelinePage {
    /// Status ids in display order.
    pub status_ids: Vec<String>,
    /// Cursor for older items (`max_id`).
    pub max_id: Option<String>,
}

impl TimelinePage {
    /// The cursor to request the next (older) page.
    #[must_use]
    pub fn next_cursor(&self) -> Option<&str> {
        self.max_id.as_deref()
    }
}

/// Everything the client knows, as observed handles.
///
/// Cheap to clone (`Rc` inside). Each field is its own [`Handle`], so a view can
/// observe one field without re-rendering on unrelated changes.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Signed-in account, `None` when signed out.
    pub session: Handle<Option<Session>>,
    /// Streaming socket state.
    pub stream: Handle<StreamState>,
    /// Recommendation feed.
    pub feed: Handle<FeedState>,
    /// Timeline pages, keyed by timeline and tag.
    pub timelines: Handle<HashMap<TimelineKey, TimelinePage>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            session: Handle::new(None),
            stream: Handle::new(StreamState::Disconnected),
            feed: Handle::new(FeedState::default()),
            timelines: Handle::new(HashMap::new()),
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
