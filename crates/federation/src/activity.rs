//! The activities an instance sends and receives.

use serde::{Deserialize, Serialize};

/// An `ActivityPub` activity.
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

impl Activity {
    /// The actor that performed the activity.
    #[must_use]
    pub fn actor(&self) -> &str {
        match self {
            Self::Create { actor, .. }
            | Self::Follow { actor, .. }
            | Self::Accept { actor, .. }
            | Self::Reject { actor, .. }
            | Self::Undo { actor, .. }
            | Self::Delete { actor, .. }
            | Self::Update { actor, .. }
            | Self::Announce { actor, .. }
            | Self::Like { actor, .. } => actor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follow_serializes_with_its_type_tag() {
        let value = serde_json::to_value(Activity::Follow {
            actor: "https://a.example/users/a".to_owned(),
            object: "https://b.example/users/b".to_owned(),
        })
        .expect("json");
        assert_eq!(value["type"], "Follow");
        assert_eq!(value["actor"], "https://a.example/users/a");
    }

    #[test]
    fn every_activity_names_its_actor() {
        let activities = [
            Activity::Follow {
                actor: "a".to_owned(),
                object: "b".to_owned(),
            },
            Activity::Like {
                actor: "a".to_owned(),
                object: "b".to_owned(),
            },
            Activity::Announce {
                actor: "a".to_owned(),
                object: "b".to_owned(),
            },
        ];
        for activity in activities {
            assert_eq!(activity.actor(), "a");
        }
    }
}
