//! The protocol-neutral Nodus client core.
//!
//! This crate owns what a client needs regardless of who serves it:
//!
//! * [`model`] — the shared Actor/Post shapes and AP-aligned vocabulary;
//! * [`api::InstanceApi`] — the read surface the client state machine drives;
//! * [`sync_api::SyncApi`] — the read-only crawl surface;
//! * [`actions`] — the operations a signed-in person can perform.
//!
//! No HTTP stack, vendor endpoint, or Mastodon-only field belongs here. Mastodon
//! and Misskey implement these traits in their own compatibility crates, and

pub mod actions;
pub mod api;
pub mod model;
pub mod rest;
pub mod sync_api;

pub use api::{InstanceApi, Streaming};
pub use model::*;
pub use sync_api::SyncApi;

use url::Url;

/// Why a client operation failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The instance answered with a non-success status.
    #[error("instance returned {status}: {message}")]
    Status {
        /// HTTP status code.
        status: u16,
        /// Error text from the instance, if any.
        message: String,
    },
    /// The instance rate-limited us; callers must honour the delay.
    #[error("rate limited, retry after {retry_after_s}s")]
    RateLimited {
        /// Seconds to wait before the next request.
        retry_after_s: u32,
    },
    /// The token was rejected; sync must stop until the user re-authorizes.
    #[error("token rejected (revoked or insufficient scope)")]
    Unauthorized,
    /// A response body did not match the expected shape.
    #[error("malformed response: {0}")]
    Decode(String),
    /// The request could not be completed at the transport layer.
    #[error("transport error: {0}")]
    Transport(String),
    /// The instance API does not expose this feature.
    #[error("{0} is not supported by this instance API")]
    Unsupported(&'static str),
}

/// Result alias used by every operation in this crate.
pub type Result<T> = core::result::Result<T, Error>;

/// A registered OAuth application on one instance.
///
/// The web client registers its own application (dynamic registration) and
/// keeps the client id in browser storage; the sync worker reuses whatever was
/// registered for the instance it is talking to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppRegistration {
    /// Public client id.
    pub client_id: String,
    /// Secret, only for confidential clients; PKCE clients ignore it.
    pub client_secret: Option<String>,
    /// Redirect URI this app was registered with.
    pub redirect_uri: String,
}

/// An instance this client can talk to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instance {
    /// Base URL, e.g. `https://example.social`.
    pub base: Url,
}

impl Instance {
    /// Parse a host or URL into an instance, defaulting to `https`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Transport`] when the input is not a usable URL.
    pub fn parse(input: &str) -> Result<Self> {
        let trimmed = input.trim();
        let candidate = if trimmed.contains("://") {
            trimmed.to_owned()
        } else {
            format!("https://{trimmed}")
        };
        let base = Url::parse(&candidate).map_err(|e| Error::Transport(e.to_string()))?;
        Ok(Self { base })
    }
}
