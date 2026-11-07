//! The vocabulary of state changes.
//!
//! A [`Command`] is the only way state changes. Views never mutate
//! [`AppState`](crate::AppState) directly, which keeps "what can happen" in one
//! enum instead of spread across components, and makes the transitions testable
//! without a browser.

use nodus_instance::model::{Account, Status};
use nodus_protocol::Capabilities;

use crate::state::{Session, StreamState, SyncConsent, Timeline, TimelinePage};

/// A change the client wants applied.
///
/// Commands that are pure state transitions are handled by
/// `Sdk::dispatch` in `nodus-runtime` today. Commands that need I/O are
/// applied by the corresponding `async` method on the SDK — the enum stays the
/// single description of the resulting state changes either way.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// A fresh feed page arrived from the discovery API.
    FeedLoaded(nodus_shared::AccountFeed),

    /// The user dismissed a recommendation.
    DismissItem {
        /// `item_id` of the dismissed item.
        item_id: i64,
    },

    /// Drop the whole feed, e.g. before a refresh.
    ClearFeed,

    /// A page of recommended posts arrived; `append` continues the current list.
    PostsLoaded {
        /// The page.
        feed: nodus_shared::PostFeed,
        /// Whether this page extends what is already shown.
        append: bool,
    },

    /// The streaming socket changed state.
    StreamChanged(StreamState),

    /// The server answered what it holds for this account.
    SyncChanged(SyncConsent),

    /// The server described its capabilities (the `/api/v1/meta` answer).
    CapabilitiesLoaded(Capabilities),

    /// A signed-in session was established.
    SignedIn(Session),

    /// The session ended; clear everything scoped to it.
    SignedOut,

    /// A timeline page arrived.
    TimelineLoaded {
        /// Which timeline.
        which: Timeline,
        /// Hashtag for [`Timeline::Tag`].
        tag: Option<String>,
        /// The page.
        page: TimelinePage,
        /// Whether it extends the page already held or replaces it.
        append: bool,
    },

    /// Statuses arrived (page body, thread, or a `status.update` event).
    ///
    /// Upsert, never replace: a status already on screen must keep its identity so
    /// scroll position and focus survive the update.
    StatusesUpserted {
        /// The statuses, in any order.
        statuses: Vec<Status>,
    },

    /// Accounts arrived (page embeds, or an author the client had not seen).
    AccountsUpserted {
        /// The accounts, in any order.
        accounts: Vec<Account>,
    },
}
