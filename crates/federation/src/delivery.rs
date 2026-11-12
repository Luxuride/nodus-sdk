//! Delivery and inbox ports. The host supplies HTTP, retries, storage and policy.

use crate::Activity;

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

/// Deliver one activity to one inbox.
pub trait Delivery: Send + Sync {
    /// Perform the delivery.
    fn deliver(
        &self,
        inbox: &str,
        activity: &Activity,
    ) -> impl Future<Output = Result<(), DeliveryError>> + Send;
}

/// Accept one incoming activity after signature verification.
pub trait Inbox: Send + Sync {
    /// Verify and parse an incoming request.
    fn accept(
        &self,
        body: &[u8],
        signature: &str,
    ) -> impl Future<Output = Result<Activity, DeliveryError>> + Send;
}
