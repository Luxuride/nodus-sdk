//! The observed state tree and the vocabulary that changes it.
//!
//! Everything here is plain data in a [`Handle`]: no methods that reach the
//! network, no framework types, no transport. Views read [`AppState`], a
//! [`Command`] describes a change, and `nodus-runtime` is the crate that decides
//! when to dispatch one.
//!
//! This crate is also where the [`Discovery`] port lives. That placement is not
//! cosmetic: the discovery transport (`nodus-discovery`) needs the trait and the
//! facade needs both, so putting the trait here is what keeps the dependency
//! arrows pointing one way — `discovery` → `state` → `shared`, and `sdk` →
//! `discovery`. A trait owned by the transport crate would close that triangle
//! into a cycle.

pub mod command;
pub mod discovery;
pub mod dwell;
pub mod handle;
pub mod state;

pub use command::Command;
pub use discovery::Discovery;
pub use dwell::{DwellTracker, MAX_DWELL_MS, MAX_EVENTS_PER_BATCH};
pub use handle::{Handle, Subscription};
pub use state::{
    AppState, Degraded, FeedState, PostFeedState, Session, StreamState, Timeline, TimelineKey,
    TimelinePage, TimelineRequest,
};
