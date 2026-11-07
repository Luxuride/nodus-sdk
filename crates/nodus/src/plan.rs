//! Which surface to use, given what a screen needs.
//!
//! The rule has two halves, and the second is the one that keeps the client working
//! against Mastodon: **prefer the optimized path; always keep the compatible one**.
//! [`choose`] therefore never answers "nothing" — the fallback is the Mastodon surface,
//! which every instance speaks. Whether the fallback is *good enough* is the caller's
//! judgement: a timeline can be built from `/api/v1/timelines/home`, while an event
//! subscription cannot be built from anything, so a caller that needs
//! [`Need::events`] must decide whether to run without live updates rather than ask
//! again.

use crate::capabilities::{Capabilities, Feature};

/// What one screen or one background task actually needs.
///
/// Built by chaining, so the caller reads as the sentence it is
/// (`Need::none().events().batch()`). A set of flags rather than a `Vec<Feature>`
/// because the interesting cases are the combinations ("a timeline that also streams"),
/// and asking for a feature the screen never uses would downgrade the client for no
/// reason.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Need {
    needed: Vec<Feature>,
}

impl Need {
    /// Nothing beyond the compatibility surface.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Everything the protocol offers — what the client wants when it can have it.
    #[must_use]
    pub fn everything() -> Self {
        Self::none()
            .events()
            .favourite_events()
            .batch()
            .recommendations()
            .timeline_sync()
    }

    fn with(mut self, feature: Feature) -> Self {
        if !self.needed.contains(&feature) {
            self.needed.push(feature);
        }
        self
    }

    /// Live events instead of polling.
    #[must_use]
    pub fn events(self) -> Self {
        self.with(Feature::Events)
    }

    /// Favourite and boost events, not only statuses.
    #[must_use]
    pub fn favourite_events(self) -> Self {
        self.with(Feature::EventsFavourite)
    }

    /// Several reads in one round trip.
    #[must_use]
    pub fn batch(self) -> Self {
        self.with(Feature::Batch)
    }

    /// The recommendation endpoints.
    #[must_use]
    pub fn recommendations(self) -> Self {
        self.with(Feature::Recommendations)
    }

    /// Server-side follow synchronisation.
    #[must_use]
    pub fn timeline_sync(self) -> Self {
        self.with(Feature::TimelineSync)
    }

    /// The features this need requires, in the order they were added.
    #[must_use]
    pub fn features(&self) -> &[Feature] {
        &self.needed
    }

    /// Whether nothing beyond Mastodon is required.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.needed.is_empty()
    }
}

/// The two ways to talk to a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Surface {
    /// The compatibility floor: Mastodon 4.3 only.
    Mastodon,
    /// The ceiling: our extensions on top of it.
    Nodus,
}

impl Surface {
    /// The name used in logs and in the state the SDK records.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mastodon => "mastodon",
            Self::Nodus => "nodus",
        }
    }
}

/// What to try first and what to fall back to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    /// The surface to use now.
    pub preferred: Surface,
    /// The surface that always works.
    pub fallback: Surface,
}

impl Plan {
    /// Whether the preferred surface is ours, i.e. the optimized path is available.
    #[must_use]
    pub const fn is_optimized(self) -> bool {
        matches!(self.preferred, Surface::Nodus)
    }
}

/// Decides which surface to use for `need`.
///
/// A server with no extensions and a server that advertises four of five features both
/// land on the same answer for a need that uses all five: the Mastodon surface. The
/// alternative — mixing surfaces inside one screen — would make half the state
/// conditional and is what this function exists to prevent.
#[must_use]
pub fn choose(capabilities: &Capabilities, need: &Need) -> Plan {
    let preferred = if capabilities.supports_all(need.features()) {
        Surface::Nodus
    } else {
        Surface::Mastodon
    };
    Plan {
        preferred,
        fallback: Surface::Mastodon,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(features: &[Feature]) -> Capabilities {
        Capabilities {
            contract: "0.1.0".to_owned(),
            features: features.to_vec(),
        }
    }

    #[test]
    fn a_plain_instance_falls_back_to_mastodon() {
        let plan = choose(&Capabilities::mastodon(), &Need::everything());
        assert_eq!(plan.preferred, Surface::Mastodon);
        assert_eq!(plan.fallback, Surface::Mastodon);
        assert!(!plan.is_optimized());
    }

    #[test]
    fn a_server_with_every_feature_gets_the_optimized_path() {
        let plan = choose(&with(Need::everything().features()), &Need::everything());
        assert_eq!(plan.preferred, Surface::Nodus);
        assert!(plan.is_optimized());
    }

    #[test]
    fn a_need_that_does_not_use_the_missing_feature_still_optimizes() {
        let capabilities = with(&[Feature::Batch, Feature::Recommendations]);
        let plan = choose(&capabilities, &Need::none().batch().recommendations());
        assert!(plan.is_optimized());
    }

    #[test]
    fn one_missing_feature_in_the_need_downgrades_the_whole_screen() {
        let capabilities = with(&[Feature::Batch]);
        let plan = choose(&capabilities, &Need::none().batch().events());
        assert_eq!(plan.preferred, Surface::Mastodon);
    }

    #[test]
    fn a_need_with_no_features_prefers_the_optimized_path() {
        assert!(choose(&Capabilities::mastodon(), &Need::none()).is_optimized());
    }

    #[test]
    fn asking_for_the_same_feature_twice_keeps_one_entry() {
        let need = Need::none().batch().batch();
        assert_eq!(need.features(), [Feature::Batch]);
    }
}
