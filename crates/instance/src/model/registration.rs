//! Invites and account approvals: how an instance controls who may join.

use serde::{Deserialize, Serialize};

/// One invite link. `code` is the part that goes in the URL, not the `id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invite {
    pub id: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub revoked: bool,
    #[serde(default)]
    pub uses: u32,
    #[serde(default)]
    pub max_uses: Option<u32>,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub autofollow: bool,
}

impl Invite {
    /// Whether this invite can still be used.
    #[must_use]
    pub fn usable(&self) -> bool {
        !self.revoked && self.max_uses.is_none_or(|max| self.uses < max)
    }

    /// What is left of it, in words.
    #[must_use]
    pub fn uses_left(&self) -> String {
        match self.max_uses {
            Some(max) => format!("{} of {max} used", self.uses),
            None => format!("{} used, unlimited", self.uses),
        }
    }
}
