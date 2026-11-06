//! Cache port: where derived client state survives a reload.
//!
//! [`Persistence`] is a port; the browser adapter (`IndexedDB`) and
//! [`MemoryPersistence`] are adapters. Values cross the port as bytes so the port
//! does not need to know about every cached type and the adapters do not need to
//! know about `serde`.
//!
//! The crate owns the cache *contract* and the in-memory adapter, nothing else:
//! it reads `nodus-state` only to name the timeline a [`CacheKey`] points at, and
//! it never performs I/O of its own accord.

pub mod error;
pub mod persistence;

pub use error::PersistenceError;
pub use persistence::{CacheKey, MemoryPersistence, Persistence};
