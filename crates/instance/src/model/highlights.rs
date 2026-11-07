//! Featured tags and instance rules.
//!
//! A featured tag is the profile's own table of contents: it says "this is what I post
//! about" in a way a bio cannot. Instance rules are the other direction — what the
//! server expects of everyone — and belong where someone is about to join, which is the
//! sign-in screen.

use serde::{Deserialize, Serialize};

/// A tag the account has put on its profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeaturedTag {
    pub id: String,
    /// Without the leading `#`.
    pub name: String,
    #[serde(default)]
    pub statuses_count: Option<u64>,
    #[serde(default)]
    pub last_status_at: Option<String>,
}

impl FeaturedTag {
    /// The tag as it is written in a post.
    pub fn hashtag(&self) -> String {
        format!("#{}", self.name)
    }
}

/// One rule from the instance's code of conduct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub hint: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_featured_tag_is_shown_with_its_hash() {
        let tag = FeaturedTag {
            id: "1".into(),
            name: "rust".into(),
            statuses_count: Some(3),
            last_status_at: None,
        };
        assert_eq!(tag.hashtag(), "#rust");
    }

    #[test]
    fn a_rule_without_a_hint_still_parses() {
        let rule: Rule = serde_json::from_str(r#"{"id":"1","text":"Be kind."}"#)
            .expect("a rule without a hint is a legal payload");
        assert_eq!(rule.text, "Be kind.");
        assert!(rule.hint.is_none());
    }
}
