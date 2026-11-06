//! Why a cached value could not be read or written.

/// Why a cached value could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PersistenceError {
    /// The store is unavailable (private mode, quota exhausted, permission denied).
    #[error("storage unavailable: {0}")]
    Unavailable(String),
    /// The stored entry does not match the current schema.
    #[error("cache entry is corrupt: {0}")]
    Corrupt(String),
    /// A value could not be encoded or decoded.
    #[error("cache (de)serialisation failed: {0}")]
    Serde(String),
}
