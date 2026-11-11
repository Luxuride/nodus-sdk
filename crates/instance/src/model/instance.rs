//! What an instance says about itself.

use serde::{Deserialize, Serialize};

use super::Rule;

/// `/api/v2/instance`: the server's own account of who it is.
///
/// Every field is defaulted, because this payload varies more between versions than any
/// other in the API — it grew `configuration` and `registrations` without a version bump.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceInfo {
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub short_description: String,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub thumbnail: Option<InstanceThumbnail>,
    #[serde(default)]
    pub contact: Option<InstanceContact>,
    #[serde(default)]
    pub registrations: Option<Registrations>,
    #[serde(default)]
    pub usage: Option<Usage>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    /// Server limits, when the instance reports them.
    #[serde(default)]
    pub configuration: Option<Configuration>,
}

/// The parts of an instance's configuration this client reads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Configuration {
    /// Post limits.
    #[serde(default)]
    pub statuses: Option<Statuses>,
}

/// What an instance accepts in one post.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Statuses {
    /// The longest body, in characters.
    #[serde(default)]
    pub max_characters: Option<u64>,
    /// The most attachments one post may carry.
    #[serde(default)]
    pub max_media_attachments: Option<u64>,
}

impl InstanceInfo {
    /// The instance's post length, or Mastodon's default when it does not say.
    #[must_use]
    pub fn max_characters(&self) -> usize {
        self.configuration
            .as_ref()
            .and_then(|configuration| configuration.statuses.as_ref())
            .and_then(|statuses| statuses.max_characters)
            .and_then(|limit| usize::try_from(limit).ok())
            .filter(|limit| *limit > 0)
            .unwrap_or(500)
    }

    /// The instance's attachment count, or Mastodon's default.
    #[must_use]
    pub fn max_media_attachments(&self) -> usize {
        self.configuration
            .as_ref()
            .and_then(|configuration| configuration.statuses.as_ref())
            .and_then(|statuses| statuses.max_media_attachments)
            .and_then(|limit| usize::try_from(limit).ok())
            .filter(|limit| *limit > 0)
            .unwrap_or(4)
    }
}

/// The banner an instance publishes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceThumbnail {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub blurhash: Option<String>,
}

/// Who to write to, and which account speaks for the server.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceContact {
    #[serde(default)]
    pub email: String,
}

/// Whether accounts can be made here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registrations {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub approval_required: bool,
    #[serde(default)]
    pub message: Option<String>,
}

/// How busy the instance is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub users: Option<UsageUsers>,
}

/// Monthly-active accounts, the one number every server reports.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageUsers {
    #[serde(default)]
    pub active_month: u64,
}
