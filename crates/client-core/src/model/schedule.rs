//! Posts that have not happened yet.

use serde::{Deserialize, Serialize};

/// What a scheduled post will say when it is published.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledParams {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub sensitive: Option<bool>,
    #[serde(default)]
    pub spoiler_text: Option<String>,
    #[serde(default)]
    pub media_ids: Vec<String>,
    #[serde(default)]
    pub in_reply_to_id: Option<String>,
}

/// A post the instance will publish later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledStatus {
    pub id: String,
    /// RFC 3339, as the instance stores it.
    pub scheduled_at: String,
    #[serde(default)]
    pub params: ScheduledParams,
}

impl ScheduledStatus {
    /// A one-line preview, so a list of these is readable without opening each one.
    pub fn summary(&self) -> String {
        let text = self.params.text.trim();
        if text.is_empty() {
            "(media only)".to_owned()
        } else {
            text.lines().next().unwrap_or_default().to_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_summary_is_the_first_line() {
        let scheduled = ScheduledStatus {
            id: "1".into(),
            scheduled_at: "2026-11-08T09:00:00.000Z".into(),
            params: ScheduledParams {
                text: "first line\nsecond line".into(),
                ..ScheduledParams::default()
            },
        };
        assert_eq!(scheduled.summary(), "first line");
    }

    #[test]
    fn an_empty_body_is_named_as_media_only() {
        let scheduled = ScheduledStatus {
            id: "2".into(),
            scheduled_at: "2026-11-08T09:00:00.000Z".into(),
            params: ScheduledParams::default(),
        };
        assert_eq!(scheduled.summary(), "(media only)");
    }
}
