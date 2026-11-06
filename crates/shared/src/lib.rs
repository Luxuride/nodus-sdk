//! Wire contract shared by the recommendation API and every client.
//!
//! This crate must stay free of I/O, async runtimes and platform-specific
//! dependencies: it is compiled for both `x86_64` (the API) and
//! `wasm32-unknown-unknown` (the Leptos client).
//!
//! Compatibility rules (docs/08-contract.md in the backend repository):
//!
//! * additive only within a minor version — new fields are optional and
//!   `#[serde(default)]`;
//! * a field is never removed or renamed inside a minor;
//! * [`CONTRACT_VERSION`] is bumped together with the git tag used by
//!   `backend` and `web`.

use serde::{Deserialize, Serialize};

/// Version of the wire contract implemented by this build.
///
/// The API reports this in `GET /api/v1/meta`; clients send it as
/// `X-Contract-Version`. Bump the minor for additive changes, the major for
/// breaking ones.
pub const CONTRACT_VERSION: &str = "0.1.0";

/// Oldest contract version this build still accepts from a client.
///
/// Raised only on a major change, once the server no longer honours the old
/// semantics. A client that sends `X-Contract-Version` below this value receives
/// `426 client_too_old` and must be updated. See `sdk/docs/03-contract.md`.
pub const MIN_CONTRACT_VERSION: &str = "0.1.0";

/// A Mastodon account as the recommendation feed reports it.
///
/// Deliberately smaller than the Mastodon API's account entity: the client
/// already knows how to fetch the full account from the instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedAccount {
    /// Stable id of the account in our database.
    pub id: String,
    /// `user@host`, or `user` for accounts on the user's own instance.
    pub acct: String,
    /// Display name as set by the account owner; empty when unset.
    pub display_name: String,
    /// Plain-text notes (HTML stripped, truncated). Never markup.
    #[serde(default)]
    pub note: String,
    /// Absolute avatar URL on the account's instance.
    pub avatar_url: String,
    /// Follower count at the last refresh.
    #[serde(default)]
    pub followers_count: i64,
    /// RFC 3339 timestamp of the last public post, if known.
    #[serde(default)]
    pub last_status_at: Option<String>,
    /// Whether the account is flagged as a bot on its instance.
    #[serde(default)]
    pub bot: bool,
    /// Whether the requesting user already follows this account.
    #[serde(default)]
    pub already_following: bool,
}

/// Why an item was recommended. Bounded on purpose: the server never leaks
/// scores or another user's private data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reason {
    /// Machine-readable code, e.g. `followed_by_similar_users`.
    pub code: String,
    /// How many similar users support this candidate.
    #[serde(default)]
    pub support: u8,
    /// Up to three sample handles, so the UI can say "followed by @bob, @carol".
    #[serde(default)]
    pub sample_followers: Vec<String>,
    /// Optional hint about the path that produced the item.
    #[serde(default)]
    pub via: Option<String>,
}

/// One recommended account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountItem {
    /// Opaque id used for impressions, clicks and dismissals.
    pub item_id: i64,
    /// The recommended account.
    pub account: FeedAccount,
    /// Human-explainable justification.
    pub reason: Reason,
}

/// Envelope for `GET /api/v1/feed/accounts`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountFeed {
    /// Snapshot id; page 2 reuses it so item ids stay stable.
    pub feed_id: String,
    /// RFC 3339 timestamp of when the snapshot was ranked.
    pub generated_at: String,
    /// Contract version the server used to serialize this response.
    pub contract: String,
    /// Ranking model version, for offline evaluation.
    pub model_version: i32,
    /// Age of the similarity data backing this feed.
    pub similarity_age_hours: f32,
    /// `None` when fully personalized; otherwise a machine-readable reason.
    #[serde(default)]
    pub degraded: Option<String>,
    /// Signals the user did not grant, e.g. `["favourites"]`.
    #[serde(default)]
    pub signals_unavailable: Vec<String>,
    /// The ranked items for this page.
    pub items: Vec<AccountItem>,
    /// Opaque keyset cursor; `None` on the last page.
    #[serde(default)]
    pub next_cursor: Option<String>,
}

/// Kinds of interaction the client reports back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// The item was visible for long enough to count.
    Impression,
    /// The item was opened.
    Click,
    /// The recommended account was followed.
    Follow,
    /// The item was hidden.
    Dismiss,
    /// The user does not want this account again.
    NotInterested,
    /// The recommended post was boosted.
    Boost,
    /// The recommended post was replied to.
    Reply,
}

/// A single interaction report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionEvent {
    /// Item the event refers to.
    pub item_id: i64,
    /// What happened.
    pub kind: EventKind,
    /// Client-side timestamp, RFC 3339; the server clamps it to ±10 minutes.
    pub client_ts: String,
    /// Visible time in milliseconds, for impressions.
    #[serde(default)]
    pub dwell_ms: Option<u32>,
}

/// Batched event submission body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventBatch {
    /// Feed snapshot the events belong to.
    pub feed_id: String,
    /// Up to 100 events per request.
    pub events: Vec<InteractionEvent>,
}

/// Public capability description served at `GET /api/v1/meta`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    /// Version of our HTTP API.
    pub api: String,
    /// Contract version this server serializes with.
    pub contract: String,
    /// Oldest contract version this server still accepts.
    pub min_contract: String,
    /// Capability flags the client gates UI on.
    pub features: Vec<String>,
}

/// Standard error envelope for every non-2xx response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiErrorBody {
    /// Machine-readable error.
    pub error: ApiError,
}

/// Machine-readable error details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    /// Stable code, e.g. `rate_limited`.
    pub code: String,
    /// Human-readable message; not stable, do not parse.
    pub message: String,
    /// Seconds to wait before retrying, for `rate_limited`.
    #[serde(default)]
    pub retry_after_s: Option<u32>,
}

/// A request to turn background sync on or off for the caller's account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncTokenRequest {
    /// Whether the service may keep an encrypted copy of the token.
    pub enable: bool,
    /// Whether interaction telemetry may be kept. Independent of `enable`.
    #[serde(default)]
    pub telemetry_opt_in: bool,
}

/// The two independent consent switches, as the service currently has them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncSettings {
    /// Whether a token is held and background sync runs.
    pub sync_enabled: bool,
    /// Whether interaction telemetry is kept.
    pub telemetry_opt_in: bool,
}

/// The caller's account as this service knows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeResponse {
    /// `user@host` as the instance spells it.
    pub acct: String,
    /// Instance the account lives on.
    pub instance_host: String,
    /// Whether a token is held for background sync.
    pub sync_enabled: bool,
    /// Whether telemetry is kept.
    pub telemetry_opt_in: bool,
    /// Contract version this service implements.
    pub contract: String,
}
