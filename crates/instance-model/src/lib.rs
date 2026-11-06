//! Mastodon domain model: the data shapes every instance transport speaks.
//!
//! These are the shapes an instance returns, trimmed to what the client renders
//! and what the recommendation engine consumes. They are deliberately owned by
//! this crate rather than by a transport or a consumer: the SDK, the web client
//! and any native tooling must agree on them, and a wire change to an instance
//! should be a change in exactly one place.
//!
//! Nothing here performs I/O: no `reqwest`, no `web-sys`, no async. That is what
//! lets a `wasm32` consumer and a native consumer share one definition, and it is
//! why the transports live in `nodus-instance-rest` and the traits in
//! `nodus-instance-api`.

pub mod model;

pub use model::{
    Account, Context, CustomEmoji, Draft, LocalId, Notification, NotificationKind, Page,
    PageParams, Relationship, Status, StreamKind, Visibility,
};
