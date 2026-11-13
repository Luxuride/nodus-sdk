//! Typed access to the Nodus performance surface.
//!
//! This is the transport, not a second model: responses stay JSON because the batch
//! vocabulary is the server's, and a client decodes only the value it asked for.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use nodus_client_core::{Error, Instance, Result};

/// A capability answer from `/api/v1/nodus/capabilities`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// Wire contract, e.g. `nodus-1`.
    pub contract: String,
    /// Feature names the server implements.
    pub features: Vec<String>,
}

impl Capabilities {
    /// Whether a named feature is advertised.
    #[must_use]
    pub fn supports(&self, feature: &str) -> bool {
        self.features.iter().any(|value| value == feature)
    }
}

/// One hash-chain answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncDiff {
    /// `up_to_date`, `diff`, `diverged` or `reset`.
    pub status: String,
    /// Events after the client's root.
    pub events: Vec<Value>,
    /// Root to send next time.
    pub root: String,
}

/// The Nodus-native client.
#[derive(Debug, Clone)]
pub struct NodusClient {
    base: Url,
    token: String,
    http: Client,
}

impl NodusClient {
    /// Build a client for one instance and token.
    ///
    /// # Panics
    ///
    /// Panics when the TLS backend cannot be initialised.
    #[must_use]
    pub fn new(instance: &Instance, token: impl Into<String>) -> Self {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent(concat!("nodus-client-sdk/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("the platform TLS backend initialises");
        Self {
            base: instance.base.clone(),
            token: token.into(),
            http,
        }
    }

    fn url(&self, path: &str) -> Result<Url> {
        self.base
            .join(path.trim_start_matches('/'))
            .map_err(|error| Error::Transport(error.to_string()))
    }

    async fn decode<T: serde::de::DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<T> {
        let response = request
            .bearer_auth(&self.token)
            .header("accept", "application/json")
            .send()
            .await
            .map_err(|error| Error::Transport(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::Transport(format!("nodus request failed: {status}")));
        }
        response
            .json()
            .await
            .map_err(|error| Error::Decode(error.to_string()))
    }

    /// Read the server's capabilities.
    ///
    /// # Errors
    ///
    /// Returns a transport or decode error.
    pub async fn capabilities(&self) -> Result<Capabilities> {
        self.decode(self.http.get(self.url("/api/v1/nodus/capabilities")?))
            .await
    }

    /// Run ordered batch calls. The caller supplies the server's `BatchCall` JSON.
    ///
    /// # Errors
    ///
    /// Returns a transport or decode error.
    pub async fn batch(&self, calls: &[Value]) -> Result<Value> {
        self.decode(self.http.post(self.url("/api/v1/nodus/batch")?).json(calls))
            .await
    }

    /// The current root of a collection.
    ///
    /// # Errors
    ///
    /// Returns a transport or decode error.
    pub async fn sync_root(&self, collection: &str) -> Result<Value> {
        let mut url = self.url("/api/v1/nodus/sync/root")?;
        url.query_pairs_mut().append_pair("collection", collection);
        self.decode(self.http.get(url)).await
    }

    /// Ask for events after a known root.
    ///
    /// # Errors
    ///
    /// Returns a transport or decode error.
    pub async fn sync_diff(&self, collection: &str, since: &str) -> Result<SyncDiff> {
        self.decode(
            self.http
                .post(self.url("/api/v1/nodus/sync/diff")?)
                .json(&serde_json::json!({ "collection": collection, "since": since })),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_read_feature_names() {
        let answer: Capabilities = serde_json::from_value(serde_json::json!({
            "contract": "nodus-1",
            "features": ["batch", "events", "state_sync"]
        }))
        .expect("capabilities");
        assert!(answer.supports("batch"));
        assert!(!answer.supports("search"));
    }

    #[test]
    fn a_diff_keeps_the_next_root() {
        let diff: SyncDiff = serde_json::from_value(serde_json::json!({
            "status": "diff",
            "events": [],
            "root": "abc"
        }))
        .expect("diff");
        assert_eq!(diff.root, "abc");
        assert_eq!(diff.status, "diff");
    }
}
