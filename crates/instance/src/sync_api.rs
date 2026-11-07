//! The instance surface the sync worker needs, as a trait.
//!
//! [`InstanceApi`](crate::api::InstanceApi) describes what a *signed-in person*
//! can do in a browser. This describes what the *crawler* needs on their behalf
//! while the browser is closed: the follow graph, favourites and bookmarks.
//!
//! The two are separate on purpose:
//!
//! * authorising the crawler means asking for scopes (`read:follows`,
//!   `read:favourites`, `read:bookmarks`) that the client never needs, so a user
//!   can grant sync without granting the client more than it uses;
//! * the crawler never mutates an instance (no follow, no post, no boost) — a
//!   synchronisation bug there would edit somebody's account, so the type system
//!   is a better place than a code review to make that impossible.

use core::fmt::Debug;
use core::future::Future;

use crate::model::{Account, LocalId, Page, PageParams, Status};

/// The read-only, account-wide surface the sync worker crawls.
///
/// Every method is paginated with the same [`PageParams`] the client uses, so a
/// crawl step and a scroll step share their cursor semantics and neither can
/// invent a different notion of "next page".
pub trait SyncApi {
    /// Transport error type.
    type Error: Debug;

    /// The accounts `id` follows, newest follow first.
    fn following(
        &self,
        id: &LocalId,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Account>, Self::Error>>;

    /// The signed-in account's favourites, newest first.
    fn favourites(
        &self,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Status>, Self::Error>>;

    /// The signed-in account's bookmarks, newest first.
    fn bookmarks(
        &self,
        params: PageParams,
    ) -> impl Future<Output = Result<Page<Status>, Self::Error>>;
}
