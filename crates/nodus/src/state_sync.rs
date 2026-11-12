//! State sync: the forward hash chain a client uses to fill a gap.
//!
//! A timeline is not a page of JSON, it is a projection. The client says which
//! collection it has and the hash of the last event it applied; the server either says
//! it is current, returns the events after that hash, or says the histories diverged
//! and the client must reset from a snapshot. `reset` is explicit because a client that
//! silently rebuilds loses scroll position and local drafts.

use serde::{Deserialize, Serialize};

/// What the server concluded about the client's position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncStatus {
    /// The client is already at the server's root.
    UpToDate,
    /// The events after the client's hash are in this answer.
    Diff,
    /// The client's hash is not in the retained history; reset from a snapshot.
    Reset,
}

/// One request against one collection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateSyncRequest {
    /// The collection to sync, e.g. `timeline:local` or `thread:01J…`.
    pub collection: String,
    /// The hash of the last event the client applied, or `None` for a new client.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    /// The maximum events to return; the server clamps this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

impl StateSyncRequest {
    /// A request for a collection, starting from its current root.
    #[must_use]
    pub fn new(collection: impl Into<String>) -> Self {
        Self {
            collection: collection.into(),
            since: None,
            limit: None,
        }
    }

    /// The same request after a known event.
    #[must_use]
    pub fn after(mut self, hash: impl Into<String>) -> Self {
        self.since = Some(hash.into());
        self
    }
}

/// One link in the collection's hash chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncEvent {
    /// The event's wire id.
    pub id: String,
    /// The chain hash after applying this event.
    pub hash: String,
    /// The previous link, or `None` for the collection's first event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<String>,
    /// What happened, reusing the subscription kinds.
    pub kind: crate::EventKind,
    /// The body.
    pub payload: serde_json::Value,
}

/// The answer to a sync request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateSyncResponse {
    /// Whether the client is current, received a diff, or must reset.
    pub status: SyncStatus,
    /// Echoes the requested collection.
    pub collection: String,
    /// The server's current root hash.
    pub root: String,
    /// The events after `since`, oldest first.
    #[serde(default)]
    pub events: Vec<SyncEvent>,
    /// The cursor for the next page, when there are more events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_client_has_no_cursor() {
        let request = StateSyncRequest::new("timeline:local");
        assert_eq!(request.since, None);
        assert_eq!(request.collection, "timeline:local");
    }

    #[test]
    fn a_known_position_is_carried() {
        let request = StateSyncRequest::new("thread:1").after("abc");
        assert_eq!(request.since.as_deref(), Some("abc"));
    }

    #[test]
    fn sync_status_round_trips_as_snake_case() {
        for status in [SyncStatus::UpToDate, SyncStatus::Diff, SyncStatus::Reset] {
            let json = serde_json::to_string(&status).expect("serialises");
            assert_eq!(
                serde_json::from_str::<SyncStatus>(&json).expect("parses"),
                status
            );
        }
    }
}
