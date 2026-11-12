//! The `reqwest` transport, for native targets (the calculator, integration tests).
//!
//! It implements [`Transport`](crate::rest::Transport) and nothing else: no endpoint,
//! no error mapping, no pagination. Those live once in [`crate::rest`].
//!
//! Enable with the `native` feature.

mod client;

pub use client::{HttpInstance, ReqwestTransport};
