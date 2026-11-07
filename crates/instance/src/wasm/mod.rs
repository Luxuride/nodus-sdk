//! The browser transport (`gloo-net` + `web-sys`), for `wasm32`.
//!
//! In the browser the client *is* the person: it talks to an instance directly
//! over the shared CORS channel every web app uses, which makes this the
//! product's primary data path. It supplies what native cannot: `fetch` and the
//! streaming socket.
//!
//! Enabled by the `wasm` feature and only compiled for `wasm32`.

mod client;
mod stream;

pub use client::{BrowserInstance, GlooTransport};
