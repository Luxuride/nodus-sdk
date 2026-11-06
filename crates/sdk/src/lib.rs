//! Client-side Mastodon state: the source of truth the web client renders.
//!
//! The web crate does two things and nothing else: render [`AppState`] as exposed
//! by [`Sdk::state`], and send [`Command`]s to [`Sdk::dispatch`] when the user
//! acts. Everything else lives here — the session, timelines, the recommendation
//! feed, streaming status, and the caching and transport policy around them.
//!
//! # Shape of the crate
//!
//! The SDK is generic, not concrete, so the same state machine runs in the browser,
//! in native tests, and (later) in tooling:
//!
//! ```text
//! Sdk<I, D, P, C>
//!  I: InstanceApi  — the Mastodon instance surface (see nodus-instance)
//!  D: Discovery    — our recommendation API surface
//!  P: Persistence  — where cached state goes (memory, IndexedDB, …)
//!  C: Clock        — time, so tests are deterministic
//! ```
//!
//! Every dependency is a trait ([`InstanceApi`], [`Discovery`], [`Persistence`],
//! [`Clock`]), so:
//!
//! * the wasm build injects HTTP + `IndexedDB`,
//! * the native test build injects scripted doubles,
//! * nothing here needs `reqwest`, `web-sys` or a runtime to compile.
//!
//! State is observed through [`Handle`]s: `Rc<RefCell<T>>` plus a subscriber
//! list. That is single-threaded and framework-agnostic on purpose — the web crate
//! adapts a handle to a Leptos signal in one small bridge
//! (`web/crates/web/src/state/bridge.rs`), so this crate never depends on a UI
//! framework.
//!
//! # Status
//!
//! M0 (docs): state, commands, trait boundaries, and the pure state transitions.
//! The HTTP transports, streaming loop and `IndexedDB` backend arrive in M1/M2; see
//! `web/docs/03-data-layer.md` and `sdk/README.md`.

pub mod clock;
pub mod command;
pub mod discovery;
pub mod error;
pub mod handle;
pub mod persistence;
pub mod sdk;
pub mod shell;
pub mod state;

pub use clock::{Clock, FixedClock, SystemClock};
pub use command::Command;
pub use discovery::Discovery;
pub use error::{PersistenceError, SdkError};
pub use handle::{Handle, Subscription};
pub use persistence::{CacheKey, MemoryPersistence, Persistence};
pub use sdk::Sdk;
pub use shell::{ShellSdk, Unavailable, shell_sdk};
pub use state::{AppState, Degraded, FeedState, Session, StreamState, Timeline, TimelinePage};

pub use nodus_instance::InstanceApi;
