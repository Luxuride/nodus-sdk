//! Mastodon API compatibility over [`nodus_client_core`].
//!
//! Everything that would break if Mastodon changed lives here: the REST
//! endpoint spellings, pagination rules, error mapping, and the native/browser
//! transports. The shared model and operation traits stay in `nodus-client-core`, so a
//! different implementation can reuse them.
//!
//! A client built only on `nodus-client-core` and this crate works against any Mastodon
//! instance; Nodus-specific extensions remain in `nodus-protocol`.

pub mod rest;

#[cfg(feature = "native")]
pub mod native;
#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
pub mod wasm;

pub use nodus_client_core::model::*;
pub use nodus_client_core::{
    AppRegistration, Error, Instance, InstanceApi, Result, Streaming, SyncApi, actions, api, model,
    sync_api,
};

#[cfg(feature = "native")]
pub use native::{HttpInstance, ReqwestTransport};
#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
pub use wasm::{BrowserInstance, GlooTransport};
