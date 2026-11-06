//! Client-side Mastodon state store: the source of truth the web client renders.
//!
//! This crate is a facade. It owns no types and no logic: every item below is a
//! re-export, so `use nodus_sdk::{…}` keeps working while the implementation is
//! split across `nodus-state` (the state tree and its command vocabulary),
//! `nodus-runtime` (the `Sdk<I, D, P, C>` state machine), `nodus-persist` (the
//! cache port), `nodus-instance-api` (the instance surface as traits) and
//! `nodus-discovery` (our recommendation API transport).
//!
//! ```text
//! web ──► nodus-sdk (facade) ──► runtime ──► state ──► shared
//!                                  │           ▲
//!                                  └► persist ─┘
//!                                  └► instance-api ──► instance-model
//!         sdk ──► discovery ──► state
//! ```
//!
//! The facade depends on `discovery` for its re-exports and `discovery` depends on
//! `state`; nothing depends back on the facade, so there is no cycle. The
//! transport impls (`nodus-instance-rest`) are *not* re-exported here: a consumer
//! chooses its transport explicitly (`native` or `wasm`) instead of getting
//! whichever one the facade happened to pick.
//!
//! Prefer importing from [`prelude`] for the everyday set; import from the
//! submodules when you need the finer pieces.

pub use nodus_instance_api::InstanceApi;
pub use nodus_persist::{CacheKey, MemoryPersistence, Persistence, PersistenceError};
pub use nodus_runtime::{
    Clock, FixedClock, Sdk, SdkError, ShellSdk, SystemClock, Unavailable, UnavailableBackend,
    shell_sdk,
};
pub use nodus_state::{
    AppState, Command, Degraded, Discovery, FeedState, Handle, Session, StreamState, Subscription,
    Timeline, TimelineKey, TimelinePage, TimelineRequest,
};

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
    pub use nodus_instance_api::*;
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
