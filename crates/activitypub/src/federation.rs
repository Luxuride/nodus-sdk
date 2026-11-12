//! Federation primitives: activities, delivery and inbox policy.
//!
//! This module is behind the `federation` feature. It deliberately contains no web
//! server, storage or routing: an instance composes these primitives with its own host.

use serde::{Deserialize, Serialize};

/// An ActivityPub activity an instance can send or receive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Activity {
    /// A new object was published.
    Create {
        /// Actor performing the activity.
        actor: String,
        /// Object being created.
        object: serde_json::Value,
    },
    /// Follow another actor.
    Follow {
        /// Follower actor id.
        actor: String,
        /// Followed actor id.
        object: String,
    },
    /// Accept a follow.
    Accept {
        /// Accepting actor id.
        actor: String,
        /// Activity being accepted.
        object: serde_json::Value,
    },
    /// Reject a follow.
    Reject {
        /// Rejecting actor id.
        actor: String,
        /// Activity being rejected.
        object: serde_json::Value,
    },
    /// Undo a previous activity.
    Undo {
        /// Actor performing the undo.
        actor: String,
        /// Activity being undone.
        object: serde_json::Value,
    },
    /// Delete an object.
    Delete {
        /// Actor performing the delete.
        actor: String,
        /// Object being deleted.
        object: String,
    },
    /// Update an object.
    Update {
        /// Actor performing the update.
        actor: String,
        /// Updated object.
        object: serde_json::Value,
    },
    /// Share another object.
    Announce {
        /// Actor announcing.
        actor: String,
        /// Shared object id.
        object: String,
    },
    /// Like another object.
    Like {
        /// Actor liking.
        actor: String,
        /// Liked object id.
        object: String,
    },
}

/// A delivery port.
///
/// An instance supplies the HTTP implementation; the SDK supplies the envelope and
/// signature rules. This keeps listeners and storage out of the protocol crate.
pub trait Delivery: Send + Sync {
    /// Deliver one activity to one inbox.
    fn deliver(
        &self,
        inbox: &str,
        activity: &Activity,
    ) -> impl Future<Output = Result<(), DeliveryError>> + Send;
}

/// An inbox port.
pub trait Inbox: Send + Sync {
    /// Verify and accept one incoming activity.
    fn accept(
        &self,
        body: &[u8],
        signature: &str,
    ) -> impl Future<Output = Result<Activity, DeliveryError>> + Send;
}

/// Why federation delivery or verification failed.
#[derive(Debug, thiserror::Error)]
pub enum DeliveryError {
    /// The target is not an HTTPS inbox.
    #[error("not an https inbox: {0}")]
    Inbox(String),
    /// Signature verification failed.
    #[error("signature: {0}")]
    Signature(String),
    /// The peer rejected the activity.
    #[error("remote rejected the activity: {0}")]
    Remote(String),
}
