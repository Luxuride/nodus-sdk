//! The Mastodon transports.
//!
//! One endpoint surface (`nodus-instance-api`), two implementations selected by
//! feature flag: `native` → [`native::HttpInstance`] over `reqwest`, `wasm` →
//! [`wasm::BrowserInstance`] over `gloo-net` + `web-sys`. Keeping both here, and
//! only here, is the point: an endpoint spelling, an error mapping or a
//! pagination rule cannot drift between the sync worker and the browser.
//!
//! Neither module knows anything about client state. A transport turns a method
//! call into HTTP and a response back into a model type; `nodus-state` and
//! `nodus-runtime` are what decide when to call it.
//!
//! With no feature enabled this crate is empty on purpose, so a target that only
//! needs the traits can depend on `nodus-instance-api` and stop there.

#[cfg(feature = "native")]
pub mod native;
#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
pub mod wasm;

#[cfg(feature = "native")]
pub use native::HttpInstance;
#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
pub use wasm::BrowserInstance;
