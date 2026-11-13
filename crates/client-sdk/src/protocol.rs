//! Protocol detection for instances that expose more than one API.
//!
//! The client never assumes a server speaks the API it was built for. It probes the
//! documents a server already publishes, then picks the richest surface the server
//! actually implements. Nodus is preferred, Misskey is next (its endpoint set is
//! larger than Mastodon's for notes), and Mastodon is the compatibility floor.

use serde_json::Value;

/// One API dialect a server can expose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Protocol {
    /// Nodus-native performance surface (`/api/v1/nodus/*`).
    Nodus,
    /// Misskey client API (`/api/*`, body token `i`).
    Misskey,
    /// Mastodon client API (`/api/v1/*`, bearer token).
    Mastodon,
}

/// Evidence gathered from the instance documents.
///
/// Each field is the raw document, not a parsed model: detection must keep working when
/// a server adds fields, and a client should not fail to identify a server just because
/// its own typed decoder is older than the server.
#[derive(Debug, Clone, Default)]
pub struct Probe<'a> {
    /// `/api/v1/nodus/capabilities`, when the server answered.
    pub nodus: Option<&'a Value>,
    /// `/api/v1/instance` or `/api/v2/instance`, when the server answered.
    pub mastodon: Option<&'a Value>,
    /// `/api/meta`, when the server answered.
    pub misskey: Option<&'a Value>,
}

impl Probe<'_> {
    /// Chooses the protocol with the most evidence.
    ///
    /// A Nodus document only counts when it carries a non-empty contract, because a
    /// generic 404 page or an empty object is not an implementation. Misskey's meta is
    /// identified by `maxNoteTextLength` (camelCase) or `max_note_text_length`, which a
    /// Mastodon instance never publishes. Mastodon is the fallback whenever its
    /// instance document is present.
    #[must_use]
    pub fn detect(&self) -> Option<Protocol> {
        if self.nodus.is_some_and(is_nodus) {
            return Some(Protocol::Nodus);
        }
        if self.misskey.is_some_and(is_misskey) {
            return Some(Protocol::Misskey);
        }
        if self.mastodon.is_some() {
            return Some(Protocol::Mastodon);
        }
        None
    }
}

/// Whether a document is a Nodus capabilities answer.
#[must_use]
pub fn is_nodus(value: &Value) -> bool {
    value
        .get("contract")
        .and_then(Value::as_str)
        .is_some_and(|contract| !contract.trim().is_empty())
        && value.get("features").and_then(Value::as_array).is_some()
}

/// Whether a document is Misskey's `/api/meta`.
#[must_use]
pub fn is_misskey(value: &Value) -> bool {
    value.get("maxNoteTextLength").is_some() || value.get("max_note_text_length").is_some()
}

/// The endpoints a chosen protocol uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Endpoints {
    /// Timeline path.
    pub timeline: &'static str,
    /// Instance metadata path.
    pub instance: &'static str,
    /// Whether the protocol takes a bearer token rather than a body `i` field.
    pub bearer: bool,
}

impl Protocol {
    /// The paths and token style for this protocol.
    #[must_use]
    pub const fn endpoints(self) -> Endpoints {
        match self {
            Self::Nodus => Endpoints {
                timeline: "/api/v1/nodus/batch",
                instance: "/api/v1/nodus/capabilities",
                bearer: true,
            },
            Self::Misskey => Endpoints {
                timeline: "/api/notes/timeline",
                instance: "/api/meta",
                bearer: false,
            },
            Self::Mastodon => Endpoints {
                timeline: "/api/v1/timelines/home",
                instance: "/api/v2/instance",
                bearer: true,
            },
        }
    }
}

/// Fetches the three capability documents from `base` and returns the protocol to use.
///
/// A failed request is evidence of absence, not an error: an instance that does not
/// answer `/api/meta` is still a Mastodon instance if `/api/v2/instance` answered.
#[cfg(feature = "native")]
pub async fn detect(base: &str) -> Option<Protocol> {
    let base = base.trim_end_matches('/');
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .user_agent(concat!("nodus-client-sdk/", env!("CARGO_PKG_VERSION")))
        .build()
        .ok()?;
    let nodus = fetch(&client, &format!("{base}/api/v1/nodus/capabilities")).await;
    let misskey = fetch(&client, &format!("{base}/api/meta")).await;
    let mastodon = fetch(&client, &format!("{base}/api/v2/instance")).await;
    Probe {
        nodus: nodus.as_ref(),
        mastodon: mastodon.as_ref(),
        misskey: misskey.as_ref(),
    }
    .detect()
}

#[cfg(feature = "native")]
async fn fetch(client: &reqwest::Client, url: &str) -> Option<Value> {
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json().await.ok()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn nodus_wins_when_its_contract_is_present() {
        let nodus = json!({ "contract": "0.1.0", "features": ["batch"] });
        let probe = Probe {
            nodus: Some(&nodus),
            ..Probe::default()
        };
        assert_eq!(probe.detect(), Some(Protocol::Nodus));
        assert!(Protocol::Nodus.endpoints().bearer);
    }

    #[test]
    fn an_empty_object_is_not_a_nodus_server() {
        let empty = json!({});
        let mastodon = json!({ "version": "4.3.0" });
        let probe = Probe {
            nodus: Some(&empty),
            mastodon: Some(&mastodon),
            ..Probe::default()
        };
        assert_eq!(probe.detect(), Some(Protocol::Mastodon));
    }

    #[test]
    fn misskey_is_detected_from_its_meta_document() {
        let misskey = json!({ "maxNoteTextLength": 3000 });
        let probe = Probe {
            misskey: Some(&misskey),
            ..Probe::default()
        };
        assert_eq!(probe.detect(), Some(Protocol::Misskey));
        assert!(!Protocol::Misskey.endpoints().bearer);
    }

    #[test]
    fn a_mastodon_instance_uses_the_compatibility_endpoints() {
        let mastodon = json!({ "version": "4.3.0" });
        let probe = Probe {
            mastodon: Some(&mastodon),
            ..Probe::default()
        };
        assert_eq!(probe.detect(), Some(Protocol::Mastodon));
        assert_eq!(
            Protocol::Mastodon.endpoints().timeline,
            "/api/v1/timelines/home"
        );
    }

    #[test]
    fn no_document_means_no_detection() {
        assert_eq!(Probe::default().detect(), None);
    }
}
