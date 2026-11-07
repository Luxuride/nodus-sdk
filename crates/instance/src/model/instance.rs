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
