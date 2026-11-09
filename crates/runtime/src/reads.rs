//! What a caller needs from the instance, with no cache, no stream and no store.
//!
//! There are two clients in this project and they want the same three answers: the browser
//! one ([`Sdk`]) keeps a store so a screen can render before the network answers, and the
//! server one cannot — it has one request to spend and nobody to notify afterwards. Both
//! implement [`Reads`] over the same instance API, so the server's page and the browser's
//! first page are fetched by the same code rather than by two hand-rolled HTTP calls.

use nodus_instance::InstanceApi;
use nodus_instance::actions::{Browse, ScheduleActions, Search as SearchApi};
use nodus_instance::model::{
    Account, Context, Conversation, List, LocalId, Notification, Page, PageParams,
    ScheduledStatus, SearchResults, Status, StreamKind,
};

/// One read a page asks for, as data.
///
/// The enum exists so a caller can name a read it cannot perform yet: [`Reads::read`] answers
/// `Ok(None)` when the client in front of it has no answer for that want, which is a
/// capability statement ("this build cannot serve it"), not a failure and not emptiness.
#[derive(Debug, Clone)]
pub enum Want {
    /// A timeline: home, or the local/federated public one.
    Timeline {
        /// Which timeline.
        kind: StreamKind,
        /// Cursor and page size.
        params: PageParams,
    },
    /// Notifications, newest first.
    Notifications {
        /// Cursor and page size.
        params: PageParams,
    },
    /// One status.
    Status {
        /// The status id.
        id: String,
    },
    /// The replies above and below a status.
    Context {
        /// The status id.
        id: String,
    },
    /// One account.
    Account {
        /// The account id.
        id: String,
    },
    /// An account's statuses.
    AccountStatuses {
        /// The account id.
        id: String,
        /// Cursor and page size.
        params: PageParams,
    },
    /// Direct messages, grouped by conversation.
    Conversations {
        /// Cursor and page size.
        params: PageParams,
    },
    /// Favourited statuses.
    Favourites {
        /// Cursor and page size.
        params: PageParams,
    },
    /// Bookmarked statuses.
    Bookmarks {
        /// Cursor and page size.
        params: PageParams,
    },
    /// The lists this account keeps.
    Lists,
    /// Everything waiting to be published.
    Scheduled,
    /// Who the instance suggests.
    Directory {
        /// Restricted to this instance's accounts.
        local_only: bool,
        /// Offset into the listing.
        offset: u32,
    },
    /// Accounts, statuses and hashtags matching a query.
    Search {
        /// What to look for.
        query: String,
        /// Ask the instance to fetch remote matches.
        resolve: bool,
    },
}

/// What a [`Want`] answered with.
#[derive(Debug, Clone)]
pub enum Answer {
    /// A page of statuses.
    Statuses(Box<Page<Status>>),
    /// A page of notifications.
    Notifications(Box<Page<Notification>>),
    /// One status.
    Status(Box<Status>),
    /// A status's context.
    Context(Box<Context>),
    /// One account.
    Account(Account),
    /// A page of accounts (an account's followers, the directory, mutes, blocks).
    Accounts(Vec<Account>),
    /// A page of conversations.
    Conversations(Box<Page<Conversation>>),
    /// The account's lists.
    Lists(Vec<List>),
    /// Scheduled posts.
    Scheduled(Vec<ScheduledStatus>),
    /// Search results.
    Search(Box<SearchResults>),
}

impl Answer {
    /// The statuses, when this answer is a page of them.
    #[must_use]
    pub fn statuses(self) -> Option<Page<Status>> {
        match self {
            Self::Statuses(page) => Some(*page),
            _ => None,
        }
    }

    /// The notifications, when this answer is a page of them.
    #[must_use]
    pub fn notifications(self) -> Option<Page<Notification>> {
        match self {
            Self::Notifications(page) => Some(*page),
            _ => None,
        }
    }
}

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

    /// A timeline: the home one, or the local/federated public one.
    async fn timeline(&self, kind: StreamKind, params: PageParams) -> Result<Page<Status>, Self::Error>;

    /// The replies above and below a status.
    async fn context(&self, id: &str) -> Result<Context, Self::Error>;

    /// One account.
    async fn account(&self, id: &str) -> Result<Account, Self::Error>;

    /// An account's statuses, newest first.
    async fn account_statuses(&self, id: &str, params: PageParams)
        -> Result<Page<Status>, Self::Error>;

    /// Direct messages, grouped by conversation.
    async fn conversations(&self, params: PageParams) -> Result<Page<Conversation>, Self::Error>;

    /// Statuses this account favourited.
    async fn favourites(&self, params: PageParams) -> Result<Page<Status>, Self::Error>;

    /// Statuses this account bookmarked.
    async fn bookmarks(&self, params: PageParams) -> Result<Page<Status>, Self::Error>;

    /// The lists this account keeps.
    async fn lists(&self) -> Result<Vec<List>, Self::Error>;

    /// Everything waiting to be published.
    async fn scheduled(&self) -> Result<Vec<ScheduledStatus>, Self::Error>;

    /// The instance's own idea of who to follow.
    async fn directory(&self, local_only: bool, offset: u32) -> Result<Vec<Account>, Self::Error>;

    /// Accounts, statuses and hashtags matching `query`.
    async fn search(&self, query: &str, resolve: bool) -> Result<SearchResults, Self::Error>;

    /// Answer `want`, or `Ok(None)` when this client cannot serve it.
    ///
    /// The default body is built from the typed methods below, so an implementor that can
    /// serve everything (both of this project's clients can, over the wire) writes nothing
    /// extra. A client that cannot serve a want overrides this and answers `Ok(None)` for it:
    /// that is a capability statement, never an error and never emptiness.
    async fn read(&self, want: &Want) -> Result<Option<Answer>, Self::Error> {
        Ok(Some(match want {
            Want::Timeline { kind, params } => {
                Answer::Statuses(Box::new(self.timeline(*kind, params.clone()).await?))
            }
            Want::Notifications { params } => {
                Answer::Notifications(Box::new(self.notifications(params.clone()).await?))
            }
            Want::Status { id } => Answer::Status(Box::new(self.status(id).await?)),
            Want::Context { id } => Answer::Context(Box::new(self.context(id).await?)),
            Want::Account { id } => Answer::Account(self.account(id).await?),
            Want::AccountStatuses { id, params } => {
                Answer::Statuses(Box::new(self.account_statuses(id, params.clone()).await?))
            }
            Want::Conversations { params } => {
                Answer::Conversations(Box::new(self.conversations(params.clone()).await?))
            }
            Want::Favourites { params } => Answer::Statuses(Box::new(self.favourites(params.clone()).await?)),
            Want::Bookmarks { params } => Answer::Statuses(Box::new(self.bookmarks(params.clone()).await?)),
            Want::Lists => Answer::Lists(self.lists().await?),
            Want::Scheduled => Answer::Scheduled(self.scheduled().await?),
            Want::Directory {
                local_only,
                offset,
            } => Answer::Accounts(self.directory(*local_only, *offset).await?),
            Want::Search { query, resolve } => {
                Answer::Search(Box::new(self.search(query, *resolve).await?))
            }
        }))
    }

    /// Whether this client can answer `want` without asking the network.
    ///
    /// The server asks this before painting, so a page it cannot fill becomes the "needs the
    /// app" state instead of an empty page that looks like a real one.
    fn supports(&self, _want: &Want) -> bool {
        true
    }
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

impl<I> Reads for Headless<I>
where
    I: InstanceApi
        + Browse<Error = <I as InstanceApi>::Error>
        + SearchApi<Error = <I as InstanceApi>::Error>
        + ScheduleActions<Error = <I as InstanceApi>::Error>,
{
    type Error = <I as InstanceApi>::Error;

    async fn home(&self, params: PageParams) -> Result<Page<Status>, Self::Error> {
        self.instance.timeline(StreamKind::User, params).await
    }

    async fn notifications(&self, params: PageParams) -> Result<Page<Notification>, Self::Error> {
        self.instance.notifications(params).await
    }

    async fn status(&self, id: &str) -> Result<Status, Self::Error> {
        self.instance.status(id).await
    }

    async fn timeline(&self, kind: StreamKind, params: PageParams) -> Result<Page<Status>, Self::Error> {
        self.instance.timeline(kind, params).await
    }

    async fn context(&self, id: &str) -> Result<Context, Self::Error> {
        self.instance.status_context(id).await
    }

    async fn account(&self, id: &str) -> Result<Account, Self::Error> {
        self.instance.account(&LocalId(id.to_owned())).await
    }

    async fn account_statuses(&self, id: &str, params: PageParams) -> Result<Page<Status>, Self::Error> {
        self.instance.account_statuses(&LocalId(id.to_owned()), params).await
    }

    async fn conversations(&self, params: PageParams) -> Result<Page<Conversation>, Self::Error> {
        self.instance.conversations(params).await
    }

    async fn favourites(&self, params: PageParams) -> Result<Page<Status>, Self::Error> {
        self.instance.favourites(params).await
    }

    async fn bookmarks(&self, params: PageParams) -> Result<Page<Status>, Self::Error> {
        self.instance.bookmarks(params).await
    }

    async fn lists(&self) -> Result<Vec<List>, Self::Error> {
        self.instance.lists().await
    }

    async fn scheduled(&self) -> Result<Vec<ScheduledStatus>, Self::Error> {
        self.instance.scheduled().await
    }

    async fn directory(&self, local_only: bool, offset: u32) -> Result<Vec<Account>, Self::Error> {
        self.instance.directory(local_only, offset).await
    }

    async fn search(&self, query: &str, resolve: bool) -> Result<SearchResults, Self::Error> {
        self.instance.search(query, resolve).await
    }
}
