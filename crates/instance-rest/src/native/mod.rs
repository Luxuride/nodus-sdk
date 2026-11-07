//! The `reqwest` transport, for the native targets (the calculator, integration
//! tests, any CLI).
//!
//! This is the only place in the SDK that knows how Mastodon spells its URLs,
//! maps its error statuses, or reads its pagination. The browser transport
//! (`gloo-net`) implements the same two traits with the same mapping, so a
//! crawl and a scroll cannot disagree about what "page two" means.
//!
//! Enable with the `native` feature.

mod api;
mod client;
mod request;
mod sync;

#[cfg(test)]
mod tests;

pub use client::HttpInstance;

// The error-mapping tests reach their subjects by the name this module used to
// give them.
#[cfg(test)]
use request::{MAX_ERROR_BODY, error_message, status_error};
