//! Errors crossing the SDK boundary.
//!
//! Cache failures are not defined here: they belong to `nodus-persist`, and this
//! crate only wraps them so a view handles one error type.

use core::fmt;

pub use nodus_persist::PersistenceError;

/// Why an SDK operation failed.
///
/// Two error types because there are two backends: the Mastodon instance (`I`) and
/// our discovery API (`D`). Keeping them distinct means a view can tell "the
/// instance is down" (timelines break) from "our API is down" (only the Feed tab
/// degrades) without string matching.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SdkError<I, D> {
    /// The instance rejected or failed the request.
    #[error("instance request failed: {0:?}")]
    Instance(I),
    /// The discovery API rejected or failed the request.
    #[error("discovery request failed: {0:?}")]
    Discovery(D),
    /// Cached state could not be read or written.
    #[error(transparent)]
    Persistence(#[from] nodus_persist::PersistenceError),
    /// The operation needs a signed-in session and there is none.
    #[error("not signed in")]
    NotSignedIn,
    /// The discovery API requires a newer client.
    #[error("client is too old: contract {server} requires {min}")]
    ClientTooOld {
        /// Contract version the server speaks.
        server: String,
        /// Oldest contract version it accepts.
        min: String,
    },
}

impl<I, D> SdkError<I, D> {
    /// Whether the failure only affects the Feed tab.
    ///
    /// Timelines and the session are unaffected by a discovery outage, so the
    /// layout keeps rendering instead of showing an error page.
    #[must_use]
    pub fn is_discovery_only(&self) -> bool {
        matches!(self, Self::Discovery(_) | Self::ClientTooOld { .. })
    }
}

impl<I: fmt::Debug, D: fmt::Debug> SdkError<I, D> {
    /// The instance error, when this is one.
    #[must_use]
    pub fn instance(&self) -> Option<&I> {
        match self {
            Self::Instance(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_failures_are_scoped_to_the_feed() {
        let error: SdkError<(), &str> = SdkError::Discovery("down");
        assert!(error.is_discovery_only());

        let error: SdkError<&str, ()> = SdkError::Instance("down");
        assert!(!error.is_discovery_only());
        assert_eq!(error.instance(), Some(&"down"));
    }
}
