//! Filters: what a reader asked never to see, or to see behind a warning.
//!
//! Mastodon applies filters on the server for timelines and notifications, but a client
//! that renders a thread from cached statuses must be able to answer the same question
//! itself — so the matching rule lives here, next to the data, rather than in a view.

use serde::{Deserialize, Serialize};

/// Where a filter applies. Mastodon calls these "contexts".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterContext {
    Home,
    Notifications,
    Public,
    Thread,
    Account,
}

impl FilterContext {
    /// Every context, in the order the API documents them.
    pub const ALL: [Self; 5] = [
        Self::Home,
        Self::Notifications,
        Self::Public,
        Self::Thread,
        Self::Account,
    ];

    /// The wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Notifications => "notifications",
            Self::Public => "public",
            Self::Thread => "thread",
            Self::Account => "account",
        }
    }

    /// What to call this in a sentence.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Home => "Home and lists",
            Self::Notifications => "Notifications",
            Self::Public => "Public timelines",
            Self::Thread => "Conversations",
            Self::Account => "Profiles",
        }
    }
}

impl core::fmt::Display for FilterContext {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// What to do with a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterAction {
    /// Show the post behind a warning the reader can open.
    #[default]
    Warn,
    /// Drop the post entirely.
    Hide,
}

impl FilterAction {
    /// The wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Warn => "warn",
            Self::Hide => "hide",
        }
    }
}

/// One phrase a filter matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterKeyword {
    pub id: String,
    pub keyword: String,
    #[serde(default)]
    pub whole_word: bool,
}

/// A filter, as the API returns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filter {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub context: Vec<FilterContext>,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub filter_action: FilterAction,
    #[serde(default)]
    pub keywords: Vec<FilterKeyword>,
}

impl Filter {
    /// Whether this filter was meant for `context`.
    pub fn applies_to(&self, context: FilterContext) -> bool {
        self.context.contains(&context)
    }

    /// Whether any of this filter's phrases occur in `text`.
    ///
    /// `text` should be the plain text of a post, not its HTML — markup would let a
    /// phrase match across a tag boundary and warn about something nobody wrote.
    pub fn matches(&self, text: &str) -> bool {
        let haystack = text.to_lowercase();
        self.keywords.iter().any(|keyword| {
            let needle = keyword.keyword.trim().to_lowercase();
            if needle.is_empty() {
                return false;
            }
            if keyword.whole_word {
                contains_word(&haystack, &needle)
            } else {
                haystack.contains(&needle)
            }
        })
    }
}

/// `needle` occurs in `haystack` with a non-word character on both sides.
fn contains_word(haystack: &str, needle: &str) -> bool {
    let mut start = 0;
    while let Some(found) = haystack[start..].find(needle) {
        let at = start + found;
        let before = haystack[..at].chars().next_back();
        let after = haystack[at + needle.len()..].chars().next();
        let bounded = |character: Option<char>| {
            character.is_none_or(|inner| !(inner.is_alphanumeric() || inner == '_'))
        };
        if bounded(before) && bounded(after) {
            return true;
        }
        start = at + needle.len();
    }
    false
}

/// A filter being created or edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterDraft {
    pub title: String,
    pub context: Vec<FilterContext>,
    pub filter_action: FilterAction,
    /// Seconds until expiry, as the API wants it.
    pub expires_in: Option<String>,
    pub keywords: Vec<String>,
    pub whole_word: bool,
}

impl FilterDraft {
    /// A new filter that matches everywhere and warns rather than hides.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            context: FilterContext::ALL.to_vec(),
            filter_action: FilterAction::Warn,
            expires_in: None,
            keywords: Vec::new(),
            whole_word: false,
        }
    }

    /// Restrict the filter to these contexts.
    #[must_use]
    pub fn contexts(mut self, context: Vec<FilterContext>) -> Self {
        self.context = context;
        self
    }

    /// Choose between warning and hiding.
    #[must_use]
    pub fn action(mut self, action: FilterAction) -> Self {
        self.filter_action = action;
        self
    }

    /// Set the phrases this filter matches.
    #[must_use]
    pub fn keywords(mut self, keywords: Vec<String>) -> Self {
        self.keywords = keywords;
        self
    }

    /// Require whole words only.
    #[must_use]
    pub fn whole_word(mut self, whole_word: bool) -> Self {
        self.whole_word = whole_word;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(action: FilterAction, keywords: &[(&str, bool)]) -> Filter {
        Filter {
            id: "1".into(),
            title: "test".into(),
            context: vec![FilterContext::Home],
            expires_at: None,
            filter_action: action,
            keywords: keywords
                .iter()
                .map(|(keyword, whole)| FilterKeyword {
                    id: "k".into(),
                    keyword: (*keyword).into(),
                    whole_word: *whole,
                })
                .collect(),
        }
    }

    #[test]
    fn a_phrase_matches_without_case() {
        let filter = filter(FilterAction::Warn, &[("Wordle", false)]);
        assert!(filter.matches("everyone is playing wordle again"));
    }

    #[test]
    fn a_whole_word_does_not_match_inside_a_longer_one() {
        let filter = filter(FilterAction::Warn, &[("cat", true)]);
        assert!(!filter.matches("concatenate"));
        assert!(filter.matches("a cat, sitting"));
        assert!(filter.matches("cat"));
        assert!(filter.matches("the cat's toy"));
    }

    #[test]
    fn an_empty_phrase_matches_nothing() {
        let filter = filter(FilterAction::Warn, &[("   ", false)]);
        assert!(!filter.matches("anything at all"));
    }

    #[test]
    fn a_filter_only_applies_to_its_own_contexts() {
        let filter = filter(FilterAction::Hide, &[("spoiler", false)]);
        assert!(filter.applies_to(FilterContext::Home));
        assert!(!filter.applies_to(FilterContext::Public));
    }

    #[test]
    fn the_default_action_is_a_warning() {
        assert_eq!(FilterAction::default(), FilterAction::Warn);
    }
}
