//! The vocabulary of state changes.
//!
//! A [`Command`] is the only way state changes. Views never mutate
//! [`AppState`](crate::AppState) directly, which keeps "what can happen" in one
//! enum instead of spread across components, and makes the transitions testable
//! without a browser.

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
    },
}
