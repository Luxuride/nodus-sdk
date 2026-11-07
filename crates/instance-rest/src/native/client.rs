//! The transport handle: one instance, one token, one HTTP client.

use core::fmt;
use std::time::Duration;

use reqwest::Client;
use url::Url;

use nodus_instance_api::model::PageParams;
use nodus_instance_api::{Error, Instance, Result};

/// One instance, one token, one HTTP client.
///
/// Cheap to clone (the client is reference-counted internally), so the calculator
/// builds one per job and the pool is shared process-wide.
#[derive(Clone)]
pub struct HttpInstance {
    base: Url,
    pub(super) token: Token,
    pub(super) http: Client,
}

/// The bearer token, kept in a newtype so it cannot be printed by accident.
///
/// `Debug` is redacted and there is no `Display`, which makes `{:?}` in a log
/// line — the common way a token leaks — harmless. `expose` is the one way out.
#[derive(Clone)]
pub(super) struct Token(String);

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

impl Token {
    pub(super) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for HttpInstance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpInstance")
            .field("base", &self.base.as_str())
            .field("token", &self.token)
            .finish_non_exhaustive()
    }
}

impl HttpInstance {
    /// Builds a transport with a client tuned for server-side use.
    ///
    /// # Panics
    ///
    /// Panics if the platform's TLS stack cannot be initialised; that is a
    /// deployment error, not a request error.
    #[must_use]
    pub fn new(instance: &Instance, token: impl Into<String>) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("nodus-calculator/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("the TLS stack must be usable");
        Self::with_client(instance, token, http)
    }

    /// Builds a transport with a caller-supplied client.
    #[must_use]
    pub fn with_client(instance: &Instance, token: impl Into<String>, http: Client) -> Self {
        Self {
            base: instance.base.clone(),
            token: Token(token.into()),
            http,
        }
    }

    /// The instance this transport talks to.
    #[must_use]
    pub const fn base(&self) -> &Url {
        &self.base
    }

    /// An absolute URL for `path`, which must start with `/`.
    pub(super) fn url(&self, path: &str) -> Result<Url> {
        let joined = format!("{}{path}", self.base.as_str().trim_end_matches('/'));
        Url::parse(&joined).map_err(|e| Error::Transport(format!("{joined}: {e}")))
    }

    /// An absolute URL with the pagination parameters applied as a query string.
    pub(super) fn page_url(&self, path: &str, params: &PageParams) -> Result<Url> {
        let mut url = self.url(path)?;
        {
            let mut query = url.query_pairs_mut();
            if let Some(limit) = params.limit {
                query.append_pair("limit", &limit.to_string());
            }
            if let Some(max_id) = &params.max_id {
                query.append_pair("max_id", max_id);
            }
            if let Some(since_id) = &params.since_id {
                query.append_pair("since_id", since_id);
            }
        }
        Ok(url)
    }
}
