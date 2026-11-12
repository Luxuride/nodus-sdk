//! The Nodus protocol: the extensions we add on top of Mastodon.
//!
//! Two surfaces, one product. [`nodus-instance`](https://docs.rs/nodus-instance) is
//! the compatibility floor: it speaks Mastodon 4.3 and nothing else, so a client that
//! only knows it works against any instance in the fediverse. This crate is the
//! ceiling: event subscriptions with semantic kinds (a like, an edit, a delete), batch
//! requests, and the recommendation endpoints — features a plain Mastodon instance
//! cannot offer because they need a server that understands them.
//!
//! # Why a separate crate
//!
//! Keeping the extensions here means the compatibility crate cannot accidentally grow a
//! Nodus-only assumption, and the SDK can depend on both without the browser transport
//! pulling in recommendation shapes it may never call. The dependency direction is
//! one-way: `nodus-protocol` may use the Mastodon model, never the reverse.
//!
//! # Picking a path
//!
//! The rule the SDK follows, and the reason [`plan`] exists: **prefer Nodus, always keep
//! a Mastodon path.** The client asks `/api/v1/meta` once per session
//! ([`Capabilities`]), states what a screen actually needs ([`Need`]), and [`choose`]
//! answers with the surface to use and the one to fall back to. A capability answer that
//! looks fine but whose endpoint fails is handled by the caller: the plan says what to
//! try first, not what must succeed.
//!
//! Nothing in this crate has state or I/O. It is the vocabulary; `nodus-state` records
//! the negotiated result, `nodus-discovery` performs the calls, and the transports in
//! `nodus-instance` stay untouched.

pub mod batch;
pub mod capabilities;
pub mod events;
pub mod plan;
pub mod state_sync;

pub use batch::{BatchCall, BatchResponse, BatchResult, Verb};
pub use capabilities::{Capabilities, Feature};
pub use events::{Event, EventKind, Subscription};
pub use plan::{Need, Plan, Surface, choose};
pub use state_sync::{StateSyncRequest, StateSyncResponse, SyncEvent, SyncStatus};

/// The header carrying the contract version on every Nodus call.
pub const CONTRACT_HEADER: &str = "X-Contract-Version";

/// The header naming the instance a token belongs to.
///
/// Our API verifies a Mastodon token against the instance that issued it, so the
/// request has to say which instance that is; an instance URL taken from the token's
/// own account would need a network round trip before authentication.
pub const INSTANCE_HEADER: &str = "X-Instance-Host";
