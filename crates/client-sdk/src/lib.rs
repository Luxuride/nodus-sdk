//! The Nodus client SDK.
//!
//! One state machine and command vocabulary for every client. The crate owns no
//! protocol logic: `nodus-client-core` defines the shared model and operation
//! traits, `nodus-runtime` runs the state machine, `nodus-state` is the observed
//! tree, and `nodus-discovery` is the recommendation transport. `ActivityPub`,
//! Mastodon and Misskey are optional protocol adapters around that core.
//!
//! ```text
//! web/mobile/desktop ──► nodus-client-sdk ──► runtime ──► state
//!                                                │          ▲
//!                                                └► persist ┘
//!                           client-core (model + traits)
//!                              ▲                 ▲
//!                     nodus-mastodon      nodus-misskey
//! ```
//!
//! [`Protocol`] and [`Probe`] turn the instance documents into a transport choice;
//! the caller then picks `nodus-mastodon`, `nodus-misskey`, or the Nodus surface.
//! Detection never replaces the compatibility fallback.

pub use nodus_client_core as core;
pub use nodus_client_core::InstanceApi;
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

/// Detect which client API an instance speaks before opening a transport.
pub mod protocol;
pub use protocol::{Endpoints, Probe, Protocol};

/// The Nodus-native performance surface.
#[cfg(feature = "native")]
pub mod nodus;
#[cfg(feature = "native")]
pub use nodus::{Capabilities, NodusClient, SyncDiff};

/// Mastodon's API, mapped onto the shared client model.
#[cfg(feature = "mastodon")]
pub use nodus_mastodon as mastodon;

/// Misskey's API, mapped onto the shared client model.
#[cfg(feature = "misskey")]
pub use nodus_misskey as misskey;

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
    pub use nodus_client_core::*;
}

/// The everyday set: state, commands, the SDK and its ports.
///
/// ```
/// use nodus_client_sdk::prelude::*;
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
