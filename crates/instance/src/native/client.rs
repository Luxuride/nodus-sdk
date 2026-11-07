//! The `reqwest` transport: a headless HTTP client, no state.

use std::time::Duration;

use reqwest::Client;
use url::Url;

use crate::rest::{HttpRequest, HttpResponse, Method, RestInstance, Transport};
use crate::{Error, Instance, Result};

/// One `reqwest` client shared by every clone.
///
/// `reqwest::Client` is reference-counted internally, so cloning this is cheap
/// and the connection pool is process-wide.
#[derive(Clone)]
pub struct ReqwestTransport {
    http: Client,
}

impl ReqwestTransport {
    /// Builds a client tuned for server-side use.
    ///
    /// # Panics
    ///
    /// Panics if the platform's TLS stack cannot be initialised; that is a
    /// deployment error, not a request error.
    #[must_use]
    pub fn new() -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("nodus-calculator/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("the TLS stack must be usable");
        Self { http }
    }

    /// Wraps a caller-supplied client (tests point it at a local server).
    #[must_use]
    pub fn with_client(http: Client) -> Self {
        Self { http }
    }

    /// The underlying client, for a caller that needs it directly.
    #[must_use]
    pub const fn client(&self) -> &Client {
        &self.http
    }
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for ReqwestTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse> {
        let HttpRequest {
            method,
            url,
            headers,
            body,
        } = request;
        let http_method = match method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
            Method::Delete => reqwest::Method::DELETE,
        };
        let mut builder = self.http.request(http_method, url.clone());
        {
            let headers_map = builder.headers_mut().expect("a fresh builder has headers");
            for (name, value) in &headers {
                if let Ok(name) = reqwest::header::HeaderName::from_bytes(name.as_bytes()) {
                    if let Ok(value) = reqwest::header::HeaderValue::from_str(value) {
                        headers_map.insert(name, value);
                    }
                }
            }
        }
        if let Some(body) = body {
            builder = builder.body(body);
        }
        let response = builder
            .send()
            .await
            .map_err(|e| Error::Transport(e.to_string()))?;
        let status = response.status().as_u16();
        let retry_after_s = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u32>().ok());
        let body = response
            .bytes()
            .await
            .map_err(|e| Error::Transport(e.to_string()))?
            .to_vec();
        Ok(HttpResponse {
            status,
            retry_after_s,
            body,
        })
    }
}

/// The native instance handle: the shared endpoint implementation over [`ReqwestTransport`].
pub type HttpInstance = RestInstance<ReqwestTransport>;

impl RestInstance<ReqwestTransport> {
    /// Builds a transport with a client tuned for server-side use.
    ///
    /// # Panics
    ///
    /// Panics if the platform's TLS stack cannot be initialised.
    #[must_use]
    pub fn new(instance: &Instance, token: impl Into<String>) -> Self {
        Self::with_transport(instance, token, ReqwestTransport::new())
    }

    /// Builds a transport with a caller-supplied client.
    #[must_use]
    pub fn with_client(instance: &Instance, token: impl Into<String>, http: Client) -> Self {
        Self::with_transport(instance, token, ReqwestTransport::with_client(http))
    }

    /// An absolute URL for `path`, which must start with `/`.
    pub fn base_url(&self) -> &Url {
        self.base()
    }
}
