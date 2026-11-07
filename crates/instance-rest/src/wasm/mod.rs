//! The browser transport (`gloo-net` + `web-sys`), for `wasm32`.
//!
//! # Why two transports
//!
//! In the browser the client *is* the person: it talks to an instance directly,
//! with the token that person granted it, over the shared CORS channel every web
//! app uses. That is the product's primary data path, so this transport is not a
//! build variant of the native one — it is the one the web client runs on, and
//! `native` exists for the parts of the product that have no browser: the
//! background calculator and the integration tests. Both are driven by the same
//! two traits in [`nodus_instance_api::api`], so an endpoint spelling, an error mapping or a
//! pagination rule cannot drift between them.
//!
//! Enabled by the `wasm` feature, and only compiled for `wasm32`; see
//! `crate::native` for the `reqwest` twin.

mod api;
mod client;
mod request;
mod stream;

#[cfg(test)]
mod tests;

pub use client::BrowserInstance;

// The error-mapping tests reach their subjects by the name this module used to
// give them.
#[cfg(test)]
use request::{MAX_ERROR_BODY, error_message, status_error};
