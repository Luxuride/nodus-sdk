//! Typed Mastodon API client shared by the background sync worker (native) and
//! the Leptos client (`wasm32`).
//!
//! One crate, one endpoint surface, two transports selected by feature flag
//! (`native` → `reqwest`, `wasm` → `gloo-net` + `web-sys`). Keeping the
//! endpoint definitions in one place is the point: the worker and the browser
//! cannot drift on pagination, error mapping or rate-limit headers.
//!
//! See `docs/03-contract.md` for why this crate lives in the
//! SDK repository rather than in either consumer.
//!
//! Layout:
//!
//! * [`model`] — the instance's data shapes ([`Status`], [`Account`], …);
//! * [`api::InstanceApi`] — what a signed-in person can do, as a trait;
//! * [`sync_api::SyncApi`] — the read-only crawl surface (`following`,
//!   `favourites`, `bookmarks`), deliberately without any write method;
//! * this module — errors and instance/app registration helpers.

pub mod api;
pub mod model;
#[cfg(feature = "native")]
pub mod native;
pub mod sync_api;
#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
pub mod wasm;

pub use api::{InstanceApi, Streaming};
pub use model::{
    Account, Context, CustomEmoji, Draft, LocalId, Notification, NotificationKind, Page,
    PageParams, Relationship, Status, StreamKind, Visibility,
};
#[cfg(feature = "native")]
pub use native::HttpInstance;
pub use sync_api::SyncApi;
#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
pub use wasm::BrowserInstance;

use url::Url;

/// Why a Mastodon call failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The instance answered with a non-success status.
    #[error("mastodon returned {status}: {message}")]
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
