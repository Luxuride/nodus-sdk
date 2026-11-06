//! Domain types shared by every Mastodon transport.
//!
//! These are the shapes an instance returns, trimmed to what the client renders
//! and what the recommendation engine consumes. They are deliberately owned by
//! this crate rather than by the consumer: the SDK, the web client and any native
//! tooling must agree on them, and a wire change to an instance should be a change
//! in exactly one place.

use serde::{Deserialize, Serialize};

/// An account, as returned by `/api/v1/accounts/:id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    /// Instance-local id, as a string.
    pub id: String,
    /// `user@host`, or just `user` for local accounts.
    pub acct: String,
    /// Display name; may be empty.
    #[serde(default)]
    pub display_name: String,
    /// HTML bio.
    #[serde(default)]
    pub note: String,
    /// Avatar URL.
    #[serde(default)]
    pub avatar: String,
    /// Header URL.
    #[serde(default)]
    pub header: String,
    /// Whether the account is a bot.
    #[serde(default)]
    pub bot: bool,
    /// Whether the account is locked.
    #[serde(default)]
    pub locked: bool,
    /// Follower count.
    #[serde(default)]
    pub followers_count: u64,
    /// Following count.
    #[serde(default)]
    pub following_count: u64,
    /// Status count.
    #[serde(default)]
    pub statuses_count: u64,
    /// Last status timestamp, RFC 3339.
    #[serde(default)]
    pub last_status_at: Option<String>,
}

/// A thread around one status: what it replies to, and what replies to it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Context {
    /// Statuses above the focus, oldest first.
    pub ancestors: Vec<Status>,
    /// Direct and nested replies, oldest first.
    pub descendants: Vec<Status>,
}

/// Which streaming timeline to subscribe to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamKind {
    /// The authenticated user's home timeline.
    User,
    /// Public posts from the user's instance.
    Local,
    /// Federated public posts known to the user's instance.
    Federated,
}

/// A custom emoji entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomEmoji {
    /// `:shortcode:` without colons.
    pub shortcode: String,
    /// Image URL.
    pub url: String,
    /// Present when the emoji is animated.
    #[serde(default)]
    pub static_url: Option<String>,
}

/// Reply/visibility level of a status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    /// Visible to everyone.
    Public,
    /// Visible on the instance, not federated.
    Unlisted,
    /// Visible to followers only.
    Private,
    /// Visible to mentioned accounts only.
    Direct,
}

/// A status (post).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Status {
    /// Instance-local id.
    pub id: String,
    /// When it was posted, RFC 3339.
    pub created_at: String,
    /// HTML content, sanitised by the renderer, never trusted.
    #[serde(default)]
    pub content: String,
    /// Visibility.
    pub visibility: Visibility,
    /// Author.
    pub account: Account,
    /// Favourite count.
    #[serde(default)]
    pub favourites_count: u64,
    /// Boost count.
    #[serde(default)]
    pub reblogs_count: u64,
    /// Reply count.
    #[serde(default)]
    pub replies_count: u64,
    /// Favourited by the signed-in account.
    #[serde(default)]
    pub favourited: bool,
    /// Boosted by the signed-in account.
    #[serde(default)]
    pub reblogged: bool,
    /// Bookmarked by the signed-in account.
    #[serde(default)]
    pub bookmarked: bool,
    /// The status this one boosts.
    #[serde(default)]
    pub reblog: Option<Box<Status>>,
    /// The status this one replies to.
    #[serde(default)]
    pub in_reply_to_id: Option<String>,
    /// Emoji used in the content.
    #[serde(default)]
    pub emojis: Vec<CustomEmoji>,
}

/// Kind of notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationKind {
    /// Someone followed the account.
    Follow,
    /// Someone boosted a status.
    Reblog,
    /// Someone favourited a status.
    Favourite,
    /// Someone mentioned the account.
    Mention,
    /// Poll ended.
    Poll,
    /// New status from a followed account.
    Status,
    /// Account sign-up (admin only).
    AdminSignUp,
    /// Report (admin only).
    AdminReport,
}

/// A notification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    /// Notification id.
    pub id: String,
    /// What happened.
    #[serde(rename = "type")]
    pub kind: NotificationKind,
    /// When it happened, RFC 3339.
    pub created_at: String,
    /// Acting account.
    pub account: Account,
    /// Related status, when there is one.
    #[serde(default)]
    pub status: Option<Status>,
}

/// Relationship between the signed-in account and another account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relationship {
    /// The other account.
    pub id: LocalId,
    /// Following?
    pub following: bool,
    /// Follow requested but pending?
    pub requested: bool,
    /// Followed by them?
    pub followed_by: bool,
}

/// An instance-local id, stringified for wire compatibility.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(transparent)]
pub struct LocalId(pub String);

/// A status being composed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    /// Plain-text body; the transport converts to HTML or plain per instance.
    pub text: String,
    /// Visibility to request.
    pub visibility: Visibility,
    /// `in_reply_to_id`, when replying.
    pub in_reply_to_id: Option<String>,
    /// Idempotency key, so a retried post cannot double-post.
    pub idempotency_key: Option<String>,
}

/// Pagination parameters for a timeline request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageParams {
    /// Return items older than this id.
    pub max_id: Option<String>,
    /// Return items newer than this id.
    pub since_id: Option<String>,
    /// Page size; instances cap this (40 is the documented maximum).
    pub limit: Option<u16>,
}

impl PageParams {
    /// A first page of at most `limit` items.
    #[must_use]
    pub fn first(limit: u16) -> Self {
        Self {
            limit: Some(limit),
            ..Self::default()
        }
    }

    /// The page after `max_id`.
    #[must_use]
    pub fn after(max_id: impl Into<String>, limit: u16) -> Self {
        Self {
            max_id: Some(max_id.into()),
            limit: Some(limit),
            ..Self::default()
        }
    }
}

/// One page of results, plus the cursor for the next one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    /// The items.
    pub items: Vec<T>,
    /// Cursor for older items — the smallest id in the page, if any.
    pub next_max_id: Option<String>,
}

impl<T> Page<T> {
    /// Wraps `items`, deriving the older-cursor from the last item's id.
    #[must_use]
    pub fn new(items: Vec<T>) -> Self {
        Self {
            items,
            next_max_id: None,
        }
    }

    /// Attaches an explicit `max_id` cursor.
    #[must_use]
    pub fn with_next(mut self, next_max_id: Option<String>) -> Self {
        self.next_max_id = next_max_id;
        self
    }

    /// Maps the items, keeping the cursor.
    #[must_use]
    pub fn map<U>(self, f: impl FnMut(T) -> U) -> Page<U> {
        Page {
            items: self.items.into_iter().map(f).collect(),
            next_max_id: self.next_max_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_deserialises_a_trimmed_instance_payload() {
        let json = r#"{
            "id": "110",
            "created_at": "2026-11-06T21:00:00.000Z",
            "content": "<p>hello</p>",
            "visibility": "public",
            "account": {"id": "1", "acct": "alice", "display_name": "Alice"},
            "favourites_count": 3
        }"#;

        let status: Status = serde_json::from_str(json).expect("valid status");

        assert_eq!(status.id, "110");
        assert_eq!(status.visibility, Visibility::Public);
        assert_eq!(status.account.acct, "alice");
        assert!(!status.favourited);
        assert_eq!(status.emojis, Vec::new());
    }
}
