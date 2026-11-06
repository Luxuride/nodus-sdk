//! Wiring for the M0 shell: no instance, no discovery, memory cache, system clock.
//!
//! The shell renders state and proves the shape of the SDK boundary before the
//! transports exist. [`Unavailable`] fails every call with a clear error, so a view
//! that accidentally depends on network data during M0 fails loudly instead of
//! silently rendering nothing.

use core::future::Future;

use mastodon_reccom_mastodon::{
    Account, Context, Draft, InstanceApi, LocalId, Notification, Page, PageParams, Relationship,
    Status, StreamKind,
};
use mastodon_reccom_shared::{AccountFeed, EventBatch, Meta};

use crate::clock::SystemClock;
use crate::discovery::Discovery;
use crate::persistence::MemoryPersistence;
use crate::sdk::Sdk;

/// Why a call was attempted before transports exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("no transport wired yet (M0 shell)")]
pub struct Unavailable;

/// Every call fails the same way; a ready future keeps the impls lint-clean and
/// documents that nothing is deferred.
fn unavailable<T>() -> core::future::Ready<Result<T, Unavailable>> {
    core::future::ready(Err(Unavailable))
}

/// Stand-in for both ports while the client is a shell.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnavailableBackend;

#[allow(
    clippy::unused_async,
    reason = "the traits are async; these stand-ins fail immediately"
)]
impl InstanceApi for UnavailableBackend {
    type Error = Unavailable;

    fn verify_credentials(&self) -> impl Future<Output = Result<Account, Self::Error>> {
        unavailable()
    }

    fn timeline(
        &self,
        _kind: StreamKind,
        _params: PageParams,
    ) -> impl Future<Output = Result<Page<Status>, Self::Error>> {
        unavailable()
    }

    fn tag_timeline(
        &self,
        _tag: &str,
        _params: PageParams,
    ) -> impl Future<Output = Result<Page<Status>, Self::Error>> {
        unavailable()
    }

    fn status(&self, _id: &str) -> impl Future<Output = Result<Status, Self::Error>> {
        unavailable()
    }

    fn status_context(&self, _id: &str) -> impl Future<Output = Result<Context, Self::Error>> {
        unavailable()
    }

    fn notifications(
        &self,
        _params: PageParams,
    ) -> impl Future<Output = Result<Page<Notification>, Self::Error>> {
        unavailable()
    }

    fn account(&self, _id: &LocalId) -> impl Future<Output = Result<Account, Self::Error>> {
        unavailable()
    }

    fn account_statuses(
        &self,
        _id: &LocalId,
        _params: PageParams,
    ) -> impl Future<Output = Result<Page<Status>, Self::Error>> {
        unavailable()
    }

    fn relationship(
        &self,
        _id: &LocalId,
    ) -> impl Future<Output = Result<Relationship, Self::Error>> {
        unavailable()
    }

    fn follow(&self, _id: &LocalId) -> impl Future<Output = Result<Relationship, Self::Error>> {
        unavailable()
    }

    fn unfollow(&self, _id: &LocalId) -> impl Future<Output = Result<Relationship, Self::Error>> {
        unavailable()
    }

    fn post_status(&self, _draft: &Draft) -> impl Future<Output = Result<Status, Self::Error>> {
        unavailable()
    }

    fn delete_status(&self, _id: &str) -> impl Future<Output = Result<(), Self::Error>> {
        unavailable()
    }
}

#[allow(
    clippy::unused_async,
    reason = "the traits are async; these stand-ins fail immediately"
)]
impl Discovery for UnavailableBackend {
    type Error = Unavailable;

    fn meta(&self) -> impl Future<Output = Result<Meta, Self::Error>> {
        unavailable()
    }

    fn feed(
        &self,
        _cursor: Option<&str>,
    ) -> impl Future<Output = Result<AccountFeed, Self::Error>> {
        unavailable()
    }

    fn refresh(&self, _feed_id: &str) -> impl Future<Output = Result<AccountFeed, Self::Error>> {
        unavailable()
    }

    fn send_events(&self, _batch: &EventBatch) -> impl Future<Output = Result<(), Self::Error>> {
        unavailable()
    }

    fn set_sync(
        &self,
        _enable: bool,
        _telemetry_opt_in: bool,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        unavailable()
    }
}

/// The SDK as the shell wires it: nothing real yet.
pub type ShellSdk = Sdk<UnavailableBackend, UnavailableBackend, MemoryPersistence, SystemClock>;

/// Builds the shell SDK.
#[must_use]
pub fn shell_sdk() -> ShellSdk {
    Sdk::new(
        UnavailableBackend,
        UnavailableBackend,
        MemoryPersistence::new(),
        SystemClock,
    )
}
