//! Cache port: where derived state survives a reload.
//!
//! [`Persistence`] is a port; the browser adapter (`IndexedDB`) and
//! [`MemoryPersistence`] are adapters. Values cross the port as bytes so the port
//! does not need to know about every cached type and the adapters do not need to
//! know about `serde`.

use core::future::Future;

use crate::error::PersistenceError;
use crate::state::Timeline;

/// A cache slot. Keys are coarse on purpose: one entry per host per kind, so the
/// adapter can evict a whole class (all statuses on a host) without scanning.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CacheKey {
    /// One timeline on one host.
    Timeline {
        /// Instance host.
        host: String,
        /// Which timeline.
        which: Timeline,
        /// Hashtag for [`Timeline::Tag`].
        tag: Option<String>,
    },
    /// Status bodies on one host, as an LRU blob.
    Statuses {
        /// Instance host.
        host: String,
    },
    /// Notifications on one host.
    Notifications {
        /// Instance host.
        host: String,
    },
    /// Account summaries on one host.
    Accounts {
        /// Instance host.
        host: String,
    },
    /// Custom emoji on one host.
    Emoji {
        /// Instance host.
        host: String,
    },
    /// Unsent compose drafts.
    Drafts,
    /// The discovery API's `meta` document, so boot can render before it resolves.
    DiscoveryMeta,
}

impl CacheKey {
    /// A stable string form, for adapters that key by string (`IndexedDB`).
    #[must_use]
    pub fn as_str(&self) -> String {
        match self {
            Self::Timeline { host, which, tag } => {
                format!(
                    "timeline:{host}:{which:?}:{}",
                    tag.as_deref().unwrap_or("-")
                )
            }
            Self::Statuses { host } => format!("statuses:{host}"),
            Self::Notifications { host } => format!("notifications:{host}"),
            Self::Accounts { host } => format!("accounts:{host}"),
            Self::Emoji { host } => format!("emoji:{host}"),
            Self::Drafts => "drafts".to_owned(),
            Self::DiscoveryMeta => "discovery:meta".to_owned(),
        }
    }
}

/// Byte store for cached state.
///
/// Implementations must be cheap to call and must not block: on `wasm32` the
/// browser adapter is asynchronous but resolves on the same thread.
pub trait Persistence {
    /// Reads a slot, `None` when absent or expired.
    fn load(
        &self,
        key: &CacheKey,
    ) -> impl Future<Output = core::result::Result<Option<Vec<u8>>, PersistenceError>>;

    /// Writes a slot, replacing it.
    fn store(
        &self,
        key: &CacheKey,
        value: Vec<u8>,
    ) -> impl Future<Output = core::result::Result<(), PersistenceError>>;

    /// Removes a slot; removing an absent slot is not an error.
    fn remove(
        &self,
        key: &CacheKey,
    ) -> impl Future<Output = core::result::Result<(), PersistenceError>>;

    /// Removes everything, e.g. on sign-out.
    fn clear(&self) -> impl Future<Output = core::result::Result<(), PersistenceError>>;
}

/// In-memory cache: the default in tests, and the fallback when the browser
/// denies storage (private mode, quota exhausted).
#[derive(Debug, Default)]
pub struct MemoryPersistence {
    entries: std::cell::RefCell<std::collections::HashMap<String, Vec<u8>>>,
}

impl MemoryPersistence {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of stored slots.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.borrow().len()
    }

    /// Whether nothing is stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.borrow().is_empty()
    }
}

impl Persistence for MemoryPersistence {
    fn load(
        &self,
        key: &CacheKey,
    ) -> impl Future<Output = core::result::Result<Option<Vec<u8>>, PersistenceError>> {
        let value = self.entries.borrow().get(&key.as_str()).cloned();
        core::future::ready(Ok(value))
    }

    fn store(
        &self,
        key: &CacheKey,
        value: Vec<u8>,
    ) -> impl Future<Output = core::result::Result<(), PersistenceError>> {
        self.entries.borrow_mut().insert(key.as_str(), value);
        core::future::ready(Ok(()))
    }

    fn remove(
        &self,
        key: &CacheKey,
    ) -> impl Future<Output = core::result::Result<(), PersistenceError>> {
        self.entries.borrow_mut().remove(&key.as_str());
        core::future::ready(Ok(()))
    }

    fn clear(&self) -> impl Future<Output = core::result::Result<(), PersistenceError>> {
        self.entries.borrow_mut().clear();
        core::future::ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::future::Future;
    use core::pin::pin;
    use core::task::{Context, Poll, Waker};

    /// Drives a future that never actually waits (every SDK future in tests resolves
    /// on the first poll or not at all).
    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = pin!(future);
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("future stalled: the test cache must resolve immediately"),
        }
    }

    #[test]
    fn timeline_keys_distinguish_tag_and_untagged() {
        let tagged = CacheKey::Timeline {
            host: "example.social".to_owned(),
            which: Timeline::Tag,
            tag: Some("rust".to_owned()),
        };
        let untagged = CacheKey::Timeline {
            host: "example.social".to_owned(),
            which: Timeline::Tag,
            tag: None,
        };
        assert_ne!(tagged.as_str(), untagged.as_str());
    }

    #[test]
    fn memory_cache_round_trips_and_clears() {
        let cache = MemoryPersistence::new();
        let key = CacheKey::Drafts;

        block_on(async {
            cache
                .store(&key, b"draft".to_vec())
                .await
                .expect("memory store cannot fail");
            assert_eq!(
                cache.load(&key).await.expect("memory load cannot fail"),
                Some(b"draft".to_vec())
            );
            assert_eq!(cache.len(), 1);

            cache.clear().await.expect("memory clear cannot fail");
            assert!(cache.is_empty());
            assert_eq!(
                cache.load(&key).await.expect("memory load cannot fail"),
                None
            );
        });
    }
}
