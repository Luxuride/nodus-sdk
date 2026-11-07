//! What the server says it can do.
//!
//! The answer comes from `GET /api/v1/meta`, which we own: a Mastodon instance would
//! 404 it, and that 404 is itself the answer ("no extensions"). Unknown feature names
//! are kept and ignored rather than rejected, so a server may ship a feature the client
//! predates without breaking it.

use serde::{Deserialize, Serialize};

/// A Nodus extension a server may implement.
///
/// Deliberately a closed set: a free-form string would let two servers claim the same
/// word for different behaviour, and the client cannot defend against that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    /// `POST /api/v1/batch`: several reads in one round trip.
    Batch,
    /// `GET /api/v1/events`: a subscription with semantic event kinds.
    Events,
    /// Favourite/reblog events on subscribed accounts, not only on your own posts.
    EventsFavourite,
    /// `GET /api/v1/feed/accounts` and `/api/v1/feed/posts`.
    Recommendations,
    /// Server-side timeline synchronisation (the calculator keeps your follows).
    TimelineSync,
}

impl Feature {
    /// The wire spelling, which is also what the server advertises.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Batch => "batch",
            Self::Events => "events",
            Self::EventsFavourite => "events_favourite",
            Self::Recommendations => "recommendations",
            Self::TimelineSync => "timeline_sync",
        }
    }

    /// Parses a wire spelling; unknown names are not errors.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        [
            Self::Batch,
            Self::Events,
            Self::EventsFavourite,
            Self::Recommendations,
            Self::TimelineSync,
        ]
        .into_iter()
        .find(|feature| feature.as_str() == value)
    }
}

/// The negotiated capabilities of one server.
///
/// `contract` is the version of *our* wire contract (`0.1.0` today); a client that
/// needs a newer one must say so instead of guessing from the feature list, because a
/// feature name can be reused across a breaking change.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// Contract version the server speaks, e.g. `0.1.0`.
    pub contract: String,
    /// Features it advertises.
    pub features: Vec<Feature>,
}

impl Capabilities {
    /// A server with no extensions — a plain Mastodon instance, or our own API before
    /// anyone signed in.
    #[must_use]
    pub fn mastodon() -> Self {
        Self::default()
    }

    /// Whether the server advertises `feature`.
    #[must_use]
    pub fn supports(&self, feature: Feature) -> bool {
        self.features.contains(&feature)
    }

    /// Whether every feature in `needed` is advertised.
    #[must_use]
    pub fn supports_all(&self, needed: &[Feature]) -> bool {
        needed.iter().all(|feature| self.supports(*feature))
    }

    /// Reads a `/api/v1/meta` document.
    ///
    /// # Errors
    ///
    /// Returns an error only when the document is not an object — a missing
    /// `features` list means "no extensions", which is a valid answer, not a failure.
    pub fn from_meta(value: &serde_json::Value) -> Result<Self, MetaError> {
        let object = value.as_object().ok_or(MetaError::NotAnObject)?;
        let contract = object
            .get("contract")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let features = object
            .get("features")
            .and_then(serde_json::Value::as_array)
            .map(|names| {
                names
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .filter_map(Feature::parse)
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self { contract, features })
    }
}

/// Why a meta document could not be read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MetaError {
    /// The body was JSON but not an object (an array, `null`, a bare string).
    #[error("a meta document must be a JSON object")]
    NotAnObject,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_mastodon_instance_answers_404_which_reads_as_no_extensions() {
        let capabilities = Capabilities::mastodon();
        assert!(!capabilities.supports(Feature::Batch));
        assert!(!capabilities.supports_all(&[Feature::Events]));
    }

    #[test]
    fn unknown_features_are_ignored_rather_than_rejected() {
        let meta = serde_json::json!({
            "contract": "0.2.0",
            "features": ["batch", "telepathy", "events"]
        });
        let capabilities = Capabilities::from_meta(&meta).expect("meta");
        assert_eq!(capabilities.contract, "0.2.0");
        assert_eq!(capabilities.features, vec![Feature::Batch, Feature::Events]);
    }

    #[test]
    fn a_meta_document_without_a_feature_list_means_no_extensions() {
        let capabilities =
            Capabilities::from_meta(&serde_json::json!({ "contract": "0.1.0" })).expect("meta");
        assert_eq!(capabilities.features, vec![]);
    }

    #[test]
    fn a_non_object_meta_document_is_an_error() {
        let error = Capabilities::from_meta(&serde_json::json!(["batch"])).expect_err("array");
        assert_eq!(error, MetaError::NotAnObject);
    }

    #[test]
    fn feature_spellings_round_trip() {
        for feature in [
            Feature::Batch,
            Feature::Events,
            Feature::EventsFavourite,
            Feature::Recommendations,
            Feature::TimelineSync,
        ] {
            assert_eq!(Feature::parse(feature.as_str()), Some(feature));
        }
        assert_eq!(Feature::parse("batch_requests"), None);
    }
}
