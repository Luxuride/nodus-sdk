//! Subscriptions to server-side events, with kinds a client can act on.
//!
//! Mastodon streaming hands out everything as text that the client re-parses; this is
//! the same idea with the interpretation done on the server: a like is a
//! [`EventKind::Favourite`] with the status it landed on, an edit is
//! [`EventKind::StatusUpdate`] carrying the new body and the old id, a delete is
//! [`EventKind::StatusDelete`] with nothing else. That is what makes "events on
//! subscribed stuff" usable: a client can update a status in place instead of
//! re-fetching the page to discover what changed.

use serde::{Deserialize, Serialize};

/// What happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// A new status from an account you subscribe to.
    Status,
    /// An edit: the status keeps its id and gains new content.
    StatusUpdate,
    /// A deletion: the id is now a tombstone.
    StatusDelete,
    /// Someone favourited one of the statuses you subscribe to.
    Favourite,
    /// Someone boosted one of the statuses you subscribe to.
    Boost,
    /// A follow, in either direction.
    Follow,
    /// A notification the Mastodon surface also knows about (mention, reply, poll).
    Notification,
}

impl EventKind {
    /// The wire spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::StatusUpdate => "status_update",
            Self::StatusDelete => "status_delete",
            Self::Favourite => "favourite",
            Self::Boost => "boost",
            Self::Follow => "follow",
            Self::Notification => "notification",
        }
    }

    /// Parses a wire spelling.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        [
            Self::Status,
            Self::StatusUpdate,
            Self::StatusDelete,
            Self::Favourite,
            Self::Boost,
            Self::Follow,
            Self::Notification,
        ]
        .into_iter()
        .find(|kind| kind.as_str() == value)
    }

    /// Whether this kind changes a status that already exists.
    ///
    /// A client uses this to decide between "upsert into the timeline" and "append",
    /// which is the difference between an edit landing in place and duplicating.
    #[must_use]
    pub const fn replaces_existing(self) -> bool {
        matches!(self, Self::StatusUpdate | Self::StatusDelete)
    }
}

/// What a subscriber asked for.
///
/// `kinds` is sent explicitly rather than as "everything minus what I do not want",
/// because a server that adds a kind later must not silently start sending it: a client
/// that does not know a kind cannot react to it, and the traffic would be paid for
/// nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    /// The kinds requested, in the order the caller listed them.
    pub kinds: Vec<EventKind>,
    /// Restricted to one hashtag, when the subscription is a tag feed.
    pub tag: Option<String>,
}

impl Subscription {
    /// A subscription to exactly these kinds.
    #[must_use]
    pub fn new(kinds: impl IntoIterator<Item = EventKind>) -> Self {
        Self {
            kinds: kinds.into_iter().collect(),
            tag: None,
        }
    }

    /// The same subscription, restricted to one hashtag.
    #[must_use]
    pub fn for_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    /// Whether the subscription would receive `kind`.
    #[must_use]
    pub fn wants(&self, kind: EventKind) -> bool {
        self.kinds.contains(&kind)
    }
}

/// One event as the server sends it.
///
/// `payload` is left as JSON on purpose: the kinds the client asked for are its own
/// concern, and decoding them here would force this crate to depend on every shape the
/// server may send.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// Monotonic event id, usable as a replay cursor.
    pub id: String,
    /// What happened.
    pub kind: EventKind,
    /// Which instance it happened on.
    pub instance_host: String,
    /// The body, whose shape depends on `kind`.
    pub payload: serde_json::Value,
}

impl Event {
    /// Whether this event is about the status `status_id`.
    ///
    /// Checks the payload leniently, because the server may include the status under
    /// `status`, nested in a `status` object, or only as `status_id`; the caller should
    /// not need to know which.
    #[must_use]
    pub fn concerns(&self, status_id: &str) -> bool {
        let direct = self
            .payload
            .get("status_id")
            .and_then(serde_json::Value::as_str);
        let nested = self
            .payload
            .get("status")
            .and_then(|status| status.get("id"))
            .and_then(serde_json::Value::as_str);
        direct == Some(status_id) || nested == Some(status_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subscription_asks_for_exactly_what_it_lists() {
        let subscription = Subscription::new([EventKind::Status, EventKind::StatusDelete]);
        assert!(subscription.wants(EventKind::StatusDelete));
        assert!(!subscription.wants(EventKind::Favourite));
    }

    #[test]
    fn a_tag_subscription_carries_the_tag() {
        let subscription = Subscription::new([EventKind::Status]).for_tag("rust");
        assert_eq!(subscription.tag.as_deref(), Some("rust"));
    }

    #[test]
    fn edits_and_deletes_replace_a_status_rather_than_add_one() {
        assert!(EventKind::StatusUpdate.replaces_existing());
        assert!(EventKind::StatusDelete.replaces_existing());
        assert!(!EventKind::Status.replaces_existing());
    }

    #[test]
    fn an_event_finds_its_status_whether_it_is_nested_or_flat() {
        let nested = Event {
            id: "1".to_owned(),
            kind: EventKind::Favourite,
            instance_host: "mastodon.social".to_owned(),
            payload: serde_json::json!({ "status": { "id": "42" } }),
        };
        let flat = Event {
            payload: serde_json::json!({ "status_id": "42" }),
            ..nested.clone()
        };
        assert!(nested.concerns("42"));
        assert!(flat.concerns("42"));
        assert!(!flat.concerns("43"));
    }

    #[test]
    fn event_kind_spellings_round_trip() {
        for kind in [
            EventKind::Status,
            EventKind::StatusUpdate,
            EventKind::StatusDelete,
            EventKind::Favourite,
            EventKind::Boost,
            EventKind::Follow,
            EventKind::Notification,
        ] {
            assert_eq!(EventKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(EventKind::parse("status_edited"), None);
    }
}
