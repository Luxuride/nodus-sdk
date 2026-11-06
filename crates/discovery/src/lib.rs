//! Client for the Nodus discovery API: our recommendation service, not Mastodon.
//!
//! The two planes are deliberately separate. `nodus-instance-rest` talks to the
//! user's server; this crate talks to us, over JSON REST with a Mastodon bearer
//! token and a `X-Contract-Version` header on every request. Keeping the transport
//! out of `nodus-sdk` is what lets the state machine compile without `reqwest` or
//! `gloo-net`, and it is why this is a sibling crate rather than a module behind a
//! feature of the SDK. The `Discovery` trait itself lives in `nodus-state`, so this
//! crate can implement it without depending on the facade.
//!
//! Enable `native` for `DiscoveryClient` (`reqwest`, the calculator and native
//! tests) or `wasm` for `BrowserDiscovery` (`gloo-net` and `web-sys`, the web
//! client). Both map statuses the same way. In particular, a `426` becomes
//! [`Error::ClientTooOld`] instead of a generic [`Error::Status`], because an old
//! contract needs an "update the app" screen and not a retry loop.

/// The error a discovery request can fail with.
///
/// The variants are deliberately coarse: the SDK and the UI branch on *what to
/// do* (re-authorise, back off, update, retry), not on the API's `code` string.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The API answered with a non-success status.
    #[error("discovery returned {status}: {message}")]
    Status {
        /// HTTP status code, for logging and for the rare status-specific screen.
        status: u16,
        /// The API's message when it sent the documented error envelope.
        message: String,
    },
    /// The API rate-limited us; the caller must honour the delay.
    #[error("rate limited, retry after {retry_after_s}s")]
    RateLimited {
        /// Seconds to wait before retrying.
        retry_after_s: u64,
    },
    /// The token was rejected, so the feed cannot make progress.
    ///
    /// This is separate from [`Status`](Self::Status) because the UI has to send
    /// the user back through the instance's authorisation flow.
    #[error("token rejected (revoked or insufficient scope)")]
    Unauthorized,
    /// The server no longer accepts this build's contract version.
    ///
    /// The UI must render an "update required" screen; retrying cannot help.
    #[error("client is too old: contract {server} requires {min}")]
    ClientTooOld {
        /// The contract version involved in the rejection: the server's version
        /// when the response reports it, otherwise the version this client sent.
        server: String,
        /// The oldest contract version the server accepts.
        min: String,
    },
    /// A response body did not match the shape this client expects.
    ///
    /// The serde message is kept, the raw body is not: a body can echo the
    /// request, including a URL that carries a cursor.
    #[error("malformed response: {0}")]
    Decode(String),
    /// The request could not be completed at the transport layer.
    #[error("transport error: {0}")]
    Transport(String),
}

/// Result alias with the discovery [`Error`].
pub type Result<T> = core::result::Result<T, Error>;

#[cfg(feature = "native")]
pub mod client;
#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
pub mod wasm;

#[cfg(feature = "native")]
pub use client::DiscoveryClient;
#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
pub use wasm::BrowserDiscovery;
