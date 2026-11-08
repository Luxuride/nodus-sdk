//! What a caller needs from the instance, with no cache, no stream and no store.
//!
//! There are two clients in this project and they want the same three answers: the browser
//! one ([`Sdk`]) keeps a store so a screen can render before the network answers, and the
//! server one cannot — it has one request to spend and nobody to notify afterwards. Both
//! implement [`Reads`] over the same instance API, so the server's page and the browser's
//! first page are fetched by the same code rather than by two hand-rolled HTTP calls.

use nodus_instance::InstanceApi;
use nodus_instance::model::{Notification, Page, PageParams, Status, StreamKind};

/// The reads a page of the interface needs.
///
/// `async fn` in a trait rather than `impl Future` returns: these are called from a
/// single place each and never need nameable futures.
#[expect(
    async_fn_in_trait,
    reason = "these futures are awaited where they are called, never stored or boxed"
)]
pub trait Reads {
    /// What a failed read reports.
    type Error: core::fmt::Debug;

    /// The home timeline, newest first.
    async fn home(&self, params: PageParams) -> Result<Page<Status>, Self::Error>;

    /// Notifications, newest first.
    async fn notifications(&self, params: PageParams) -> Result<Page<Notification>, Self::Error>;

    /// One status.
    async fn status(&self, id: &str) -> Result<Status, Self::Error>;
}

/// An instance with nothing in front of it.
///
/// The server's client: one request, one answer, no store to hydrate and nothing to keep in
/// sync afterwards. It exists so the server renders from the same trait the browser's SDK
/// implements, which is what keeps the two paints describing the same data.
#[derive(Debug, Clone)]
pub struct Headless<I> {
    instance: I,
}

impl<I> Headless<I> {
    /// Wraps an instance.
    pub const fn new(instance: I) -> Self {
        Self { instance }
    }

    /// The instance behind it.
    pub const fn instance(&self) -> &I {
        &self.instance
    }
}

impl<I: InstanceApi> Reads for Headless<I> {
    type Error = I::Error;

    async fn home(&self, params: PageParams) -> Result<Page<Status>, Self::Error> {
        self.instance.timeline(StreamKind::User, params).await
    }

    async fn notifications(&self, params: PageParams) -> Result<Page<Notification>, Self::Error> {
        self.instance.notifications(params).await
    }

    async fn status(&self, id: &str) -> Result<Status, Self::Error> {
        self.instance.status(id).await
    }
}
