//! ActivityPub objects and the public half of federation.
//!
//! This crate is deliberately separate from [`nodus-instance`](../nodus_instance/index.html):
//! Mastodon's REST API and ActivityPub are different protocols with different shapes, and
//! keeping them apart stops one protocol's quirks leaking into the other. The native
//! client only reads public objects; signing and delivering activities is a later layer.

#[cfg(feature = "read")]
pub mod actor;
#[cfg(feature = "read")]
pub mod collection;
#[cfg(feature = "federation")]
pub mod federation;
#[cfg(feature = "read")]
pub mod note;

#[cfg(feature = "native")]
pub mod client;

#[cfg(feature = "read")]
pub use actor::{Actor, Emoji, Image};
#[cfg(feature = "native")]
pub use client::ActivityPub;
#[cfg(feature = "read")]
pub use collection::{Collection, CollectionPage};
#[cfg(feature = "federation")]
pub use federation::{Activity, Delivery, Inbox};
#[cfg(feature = "read")]
pub use note::{Attachment, Note};

use thiserror::Error;

/// Why an ActivityPub read failed.
#[derive(Debug, Error)]
pub enum Error {
    /// The host or handle was not a valid `user@host`.
    #[error("not a federated handle: {0}")]
    Handle(String),
    /// A URL was not absolute or not HTTPS.
    #[error("not an https URL: {0}")]
    Url(String),
    /// WebFinger did not name an ActivityPub actor.
    #[error("webfinger has no activity+json actor for {0}")]
    NoActor(String),
    /// The object is not the ActivityPub type the caller needed.
    #[error("unexpected activitypub object: {0}")]
    Object(String),
    /// The remote host answered with an error.
    #[error("remote host returned {status} for {url}")]
    Status {
        /// HTTP status.
        status: u16,
        /// Requested URL.
        url: String,
    },
    /// Transport or JSON failure.
    #[error("activitypub read failed: {0}")]
    Transport(String),
}
