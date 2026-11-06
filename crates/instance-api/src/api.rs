//! The instance surface, as a trait.
//!
//! Everything the client can ask a Mastodon instance to do is declared here once,
//! as [`InstanceApi`]. Implementations are the thing that differs between
//! targets: a `gloo-net` implementation in the browser, a `reqwest` one in the
//! sync worker, and a scripted one in tests. Consumers are generic over the
//! trait, so the same SDK state machine runs in all three.
//!
//! The methods return `impl Future` rather than being `async fn` in the trait
//! declaration: it keeps the returned future nameable for implementors and avoids
//! the `async_fn_in_trait` warning on a public API.

use core::fmt::Debug;
use core::future::Future;

use crate::model::{
    Account, Context, Draft, LocalId, Notification, Page, PageParams, Relationship, Status,
    StreamKind,
};

/// The read and write operations the client performs against one instance.
///
/// `Error` is the transport's own error type, so an implementation never has to
/// translate into a shared enum it does not own; the SDK maps it into
/// [`crate::Error`] at its boundary.
pub trait InstanceApi {
    /// Transport error type.
    type Error: Debug;

    /// The signed-in account, used to validate a token and to render the profile.
    fn verify_credentials(&self) -> impl Future<Output = Result<Account, Self::Error>>;

    /// A timeline, newest first.
    fn timeline(
        &self,
        kind: StreamKind,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Status>, Self::Error>>;

    /// A hashtag timeline, newest first.
    fn tag_timeline(
        &self,
        tag: &str,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Status>, Self::Error>>;

    /// One status.
    fn status(&self, id: &str) -> impl Future<Output = Result<Status, Self::Error>>;

    /// The replies above and below a status.
    fn status_context(&self, id: &str) -> impl Future<Output = Result<Context, Self::Error>>;

    /// Notifications, newest first.
    fn notifications(
        &self,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Notification>, Self::Error>>;

    /// One account.
    fn account(&self, id: &LocalId) -> impl Future<Output = Result<Account, Self::Error>>;

    /// An account's statuses.
    fn account_statuses(
        &self,
        id: &LocalId,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Status>, Self::Error>>;

    /// Whether the signed-in account follows `id`.
    fn relationship(&self, id: &LocalId)
    -> impl Future<Output = Result<Relationship, Self::Error>>;

    /// Follows `id`, returning the resulting relationship.
    fn follow(&self, id: &LocalId) -> impl Future<Output = Result<Relationship, Self::Error>>;

    /// Unfollows `id`, returning the resulting relationship.
    fn unfollow(&self, id: &LocalId) -> impl Future<Output = Result<Relationship, Self::Error>>;

    /// Publishes a draft.
    fn post_status(&self, draft: &Draft) -> impl Future<Output = Result<Status, Self::Error>>;

    /// Deletes a status the account owns.
    fn delete_status(&self, id: &str) -> impl Future<Output = Result<(), Self::Error>>;
}

/// Implemented by transports that can hold a streaming socket.
///
/// Separate from [`InstanceApi`] because streaming is optional: the sync worker
/// polls, the browser streams, and tests do neither.
pub trait Streaming {
    /// Error reported by the socket.
    type Error: Debug;

    /// Opens a socket for `kind`, delivering raw JSON event payloads.
    ///
    /// The implementation owns reconnection; the SDK only observes the resulting
    /// `StreamState` transitions in `nodus-state`.
    fn subscribe(&self, kind: StreamKind) -> impl Future<Output = Result<(), Self::Error>>;
}
