//! Discovery port: our recommendation API.
//!
//! Separate from `InstanceApi` (`nodus-instance-api`) on purpose. The instance is
//! the user's data plane and its failures break timelines; this API is the
//! discovery plane and its failures only weaken the Feed tab. Two traits keep that
//! distinction in the type system instead of in a comment.

use core::future::Future;

use nodus_shared::{AccountFeed, EventBatch, MeResponse, Meta, PostFeed};

/// The recommendation service, as the client needs it.
pub trait Discovery {
    /// Transport error type.
    type Error: core::fmt::Debug;

    /// Contract metadata: which wire version the server speaks, what it supports.
    ///
    /// Called once at boot and cached, so the first screen can render before the
    /// network answers.
    fn meta(&self) -> impl Future<Output = Result<Meta, Self::Error>>;

    /// A page of the feed; `cursor` is the previous response's `next_cursor`.
    fn feed(&self, cursor: Option<&str>) -> impl Future<Output = Result<AccountFeed, Self::Error>>;

    /// Recomputes the feed now, honouring the server's 30-second floor.
    fn refresh(&self, feed_id: &str) -> impl Future<Output = Result<AccountFeed, Self::Error>>;

    /// A page of recommended posts; `cursor` is the previous response's `next_cursor`.
    ///
    /// Separate from [`Self::feed`] because the server treats them separately: the
    /// account feed may reuse a snapshot minted minutes ago, while a fresh post page
    /// is always a new question ("is there anything newer?").
    fn posts(&self, cursor: Option<&str>) -> impl Future<Output = Result<PostFeed, Self::Error>>;

    /// Reports user interactions. Idempotent per `(item_id, kind, client_ts)`.
    fn send_events(&self, batch: &EventBatch) -> impl Future<Output = Result<(), Self::Error>>;

    /// The server's view of this account: whether it holds a token, and whether
    /// telemetry is on.
    ///
    /// The client keeps no opinion of its own here: consent lives on the server, so
    /// a second browser or a revoked token cannot leave a toggle claiming "on".
    fn me(&self) -> impl Future<Output = Result<MeResponse, Self::Error>>;

    /// Turns server-side sync on or off, and sets the independent telemetry opt-in.
    fn set_sync(
        &self,
        enable: bool,
        telemetry_opt_in: bool,
    ) -> impl Future<Output = Result<(), Self::Error>>;
}
