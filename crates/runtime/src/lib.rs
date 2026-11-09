//! The SDK state machine: one generic `Sdk<I, D, P, C>` over four ports.
//!
//! The web crate does two things and nothing else: render [`AppState`] as
//! exposed by [`Sdk::state`], and send [`Command`]s to [`Sdk::dispatch`] when the
//! user acts. Everything that decides *when* to talk to a transport lives here:
//! the session, timeline and feed transitions, the caching policy around them,
//! the clock, and the M0 shell wiring.
//!
//! ```text
//! Sdk<I, D, P, C>
//!  I: InstanceApi  — the Mastodon instance surface (nodus-instance)
//!  D: Discovery    — our recommendation API (nodus-state)
//!  P: Persistence  — where cached state goes (nodus-persist)
//!  C: Clock        — time, so tests are deterministic
//! ```
//!
//! Every dependency is a trait, so the wasm build injects HTTP + `IndexedDB`, the
//! native test build injects scripted doubles, and nothing in this crate needs
//! `reqwest`, `web-sys` or a runtime to compile. There is no `cfg(target_arch)`
//! or `cfg(feature)` here: a transport concern in this crate would mean the split
//! leaked, and the fix is to move the concern out rather than gate it.

pub mod clock;
pub mod driver;
pub mod error;
pub mod reads;
pub mod scheduler;
pub mod sdk;
pub mod shell;

pub use clock::{Clock, FixedClock, SystemClock};
pub use driver::Driver;
pub use error::SdkError;
pub use reads::{Answer, Headless, Reads, Want};
pub use scheduler::{Budget, Plan, Scheduler, StreamPhase};
pub use sdk::{Drive, Sdk};
pub use shell::{ShellSdk, Unavailable, UnavailableBackend, shell_sdk};
