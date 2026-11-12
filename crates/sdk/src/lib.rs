//! Client-side Mastodon state store: the source of truth the web client renders.
//!
//! This crate is the overall SDK library. It owns no protocol logic: the client half is
//! the runtime/state machine below, and the federation half is opt-in through the
//! `activitypub` and `federation` features. `use nodus_sdk::{…}` stays the entry point
//! while the implementation is split across `nodus-state` (the state tree and its command vocabulary),
//! `nodus-runtime` (the `Sdk<I, D, P, C>` state machine), `nodus-persist` (the
//! cache port), `nodus-instance` (the instance surface as traits) and
//! `nodus-discovery` (our recommendation API transport).
//!
//! ```text
//! web ──► nodus-sdk (facade) ──► runtime ──► state ──► shared
//!                                  │           ▲
//!                                  └► persist ─┘
//!                                  └► instance (model + traits + transports)
//!         sdk ──► discovery ──► state
//! ```
//!
//! The facade depends on `discovery` for its re-exports and `discovery` depends on
//! `state`; nothing depends back on the facade, so there is no cycle. The
//! transport impls (`nodus-instance` (the `native`/`wasm` transports)) are *not* re-exported here: a consumer
//! chooses its transport explicitly (`native` or `wasm`) instead of getting
//! whichever one the facade happened to pick.
//!
//! Prefer importing from [`prelude`] for the everyday set; import from the
//! submodules when you need the finer pieces.

pub use nodus_instance::InstanceApi;
pub use nodus_persist::{CacheKey, MemoryPersistence, Persistence, PersistenceError};
pub use nodus_runtime::{
    Answer, Budget, Clock, Drive, Driver, FixedClock, Headless, Plan, Reads, Scheduler, Sdk,
    SdkError, ShellSdk, StreamPhase, SystemClock, Unavailable, UnavailableBackend, Want, shell_sdk,
};
pub use nodus_state::{
    AppState, Command, Degraded, Discovery, DwellTracker, FeedState, Handle, MAX_DWELL_MS,
    MAX_EVENTS_PER_BATCH, PostFeedState, Session, StreamState, Subscription, Timeline, TimelineKey,
    TimelinePage, TimelineRequest,
};

/// ActivityPub objects and the public reader.
#[cfg(feature = "activitypub")]
pub use nodus_activitypub_sdk as activitypub;

/// Misskey's API, mapped onto the same instance surface as Mastodon.
#[cfg(feature = "misskey")]
pub use nodus_instance_misskey as misskey;

/// Federation activities, signatures, delivery and inbox ports.
#[cfg(feature = "federation")]
pub use nodus_federation as federation;

/// Pages other than timelines: notifications, messages, saved posts, people, search.
pub mod page {
    pub use nodus_state::page::*;
}

/// The clock port and its system/fixed adapters.
pub mod clock {
    pub use nodus_runtime::clock::*;
}

/// The command vocabulary that changes [`AppState`](crate::AppState).
pub mod command {
    pub use nodus_state::command::*;
}

/// The discovery port and its transports.
pub mod discovery {
    pub use nodus_discovery::*;
    pub use nodus_state::discovery::*;
}

/// Errors crossing the SDK boundary.
pub mod error {
    pub use nodus_runtime::error::{PersistenceError, SdkError};
}

/// The observable [`Handle`](crate::Handle) state lives in.
pub mod handle {
    pub use nodus_state::handle::*;
}

/// The reads both clients answer: the wire trait and its headless implementation.
pub mod reads {
    pub use nodus_runtime::reads::*;
}

/// The cache port, its keys and the in-memory adapter.
pub mod persistence {
    pub use nodus_persist::persistence::*;
}

/// The state machine.
pub mod sdk {
    pub use nodus_runtime::sdk::*;
}

/// The M0 shell wiring: no transports, memory cache.
pub mod shell {
    pub use nodus_runtime::shell::*;
}

/// The state tree and the timeline types.
pub mod state {
    pub use nodus_state::state::*;
}

/// The instance surface as traits, plus the model it speaks.
pub mod instance_api {
    pub use nodus_instance::*;
}

/// The everyday set: state, commands, the SDK and its ports.
///
/// ```
/// use nodus_sdk::prelude::*;
///
/// // A discovery outage only degrades the Feed tab; timelines keep rendering.
/// fn only_feed_is_degraded(error: SdkError<(), ()>) -> bool {
///     error.is_discovery_only()
/// }
/// ```
pub mod prelude {
    pub use crate::{
        AppState, Command, Discovery, FeedState, Handle, InstanceApi, Sdk, SdkError, Session,
        StreamState, Timeline, TimelinePage,
    };
}
