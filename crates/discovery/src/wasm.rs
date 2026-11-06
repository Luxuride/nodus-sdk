//! Browser discovery transport: `gloo-net` over the platform `fetch`.
//!
//! This mirrors `crate::client::DiscoveryClient` but has no HTTP client to hold:
//! the browser owns connections, so `BrowserDiscovery` carries only the base URL,
//! the token and the contract version. The mapping helpers are duplicated from the
//! native module on purpose; sharing them would make the wasm build depend on
//! `reqwest`'s `StatusCode` or vice versa.

use core::fmt;

use gloo_net::http::{Headers, Request, RequestBuilder, Response};
use nodus_sdk::Discovery;
use nodus_shared::{
    AccountFeed, ApiErrorBody, CONTRACT_VERSION, EventBatch, MIN_CONTRACT_VERSION, Meta,
    SyncTokenRequest,
};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use url::Url;

use crate::{Error, Result};

const CONTRACT_HEADER: &str = "X-Contract-Version";
const DEFAULT_FEED_LIMIT: u32 = 50;
const DEFAULT_RETRY_AFTER_S: u64 = 60;

/// A bearer token that refuses to leak through `Debug`.
#[derive(Clone)]
struct Token(String);

impl Token {
    fn expose(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Token(<redacted>)")
    }
}

/// Client for the recommendation API in the browser.
///
/// `base` is the API origin. It is a `Url` so a malformed base is rejected when
/// the client is constructed, not on the first feed request.
#[derive(Clone)]
pub struct BrowserDiscovery {
    base: Url,
    token: Token,
    contract: String,
}

impl BrowserDiscovery {
    /// Builds a browser client for one API origin and one token.
    #[must_use]
    pub fn new(base: Url, token: impl Into<String>) -> Self {
        Self {
            base,
            token: Token(token.into()),
            contract: CONTRACT_VERSION.to_owned(),
        }
    }

    /// The API origin this client talks to.
    #[must_use]
    pub fn base(&self) -> &Url {
        &self.base
    }

    fn authorization(&self) -> String {
        format!("Bearer {}", self.token.expose())
    }

    fn headers(&self) -> Headers {
        let headers = Headers::new();
        headers.set("Authorization", self.authorization().as_str());
        headers.set(CONTRACT_HEADER, self.contract.as_str());
        headers
    }

    /// Resolves a path against the base with the platform URL parser.
    ///
    /// The browser's `URL` object is used instead of string concatenation so the
    /// result is a URL the `fetch` API accepts verbatim, with the platform's own
    /// parsing and encoding rules applied to the base and the path.
    fn url(&self, path: &str) -> Result<web_sys::Url> {
        web_sys::Url::new_with_base(path, self.base.as_str()).map_err(|_| {
            Error::Transport(format!("cannot resolve {path} against the discovery base"))
        })
    }

    fn get(&self, url: &web_sys::Url) -> RequestBuilder {
        Request::get(url.href().as_str()).headers(self.headers())
    }

    fn post(&self, url: &web_sys::Url) -> RequestBuilder {
        Request::post(url.href().as_str()).headers(self.headers())
    }

    fn put(&self, url: &web_sys::Url) -> RequestBuilder {
        Request::put(url.href().as_str()).headers(self.headers())
    }

    fn delete(&self, url: &web_sys::Url) -> RequestBuilder {
        Request::delete(url.href().as_str()).headers(self.headers())
    }
}

async fn fetch<T: DeserializeOwned>(
    request: RequestBuilder,
    method: &str,
    path: &str,
) -> Result<T> {
    let response = build(request, method, path)?
        .send()
        .await
        .map_err(|error| Error::Transport(format!("{method} {path}: {error}")))?;
    decode(&response).await
}

async fn send_empty(request: Request, method: &str, path: &str) -> Result<()> {
    let response = request
        .send()
        .await
        .map_err(|error| Error::Transport(format!("{method} {path}: {error}")))?;
    let status = response.status();
    let retry_after_header = response.headers().get("retry-after");
    let retry_after_s = retry_after_s(retry_after_header.as_deref());
    let body = response
        .text()
        .await
        .map_err(|error| Error::Transport(format!("reading the response body: {error}")))?;
    if !(200..300).contains(&status) {
        return Err(status_error(status, retry_after_s, &body));
    }
    Ok(())
}

impl Discovery for BrowserDiscovery {
    type Error = Error;

    async fn meta(&self) -> Result<Meta> {
        let path = "/api/v1/meta";
        let url = self.url(path)?;
        fetch(self.get(&url), "GET", path).await
    }

    async fn feed(&self, cursor: Option<&str>) -> Result<AccountFeed> {
        let path = "/api/v1/feed/accounts";
        let url = self.url(path)?;
        {
            // `search_params` escapes values with the browser's own encoder, which
            // is the only reliable way to round-trip an opaque cursor.
            let params = url.search_params();
            params.append("limit", &DEFAULT_FEED_LIMIT.to_string());
            if let Some(cursor) = cursor {
                params.append("cursor", cursor);
            }
        }
        fetch(self.get(&url), "GET", path).await
    }

    async fn refresh(&self, feed_id: &str) -> Result<AccountFeed> {
        let path = format!("/api/v1/feed/{feed_id}/refresh");
        let url = self.url(&path)?;
        fetch(self.post(&url), "POST", &path).await
    }

    async fn send_events(&self, batch: &EventBatch) -> Result<()> {
        let path = "/api/v1/events";
        let url = self.url(path)?;
        let request = json_request(self.post(&url), batch)?;
        send_empty(request, "POST", path).await
    }

    async fn set_sync(&self, enable: bool, telemetry_opt_in: bool) -> Result<()> {
        let path = "/api/v1/sync/token";
        let url = self.url(path)?;
        let (request, method) = if enable {
            let payload = SyncTokenRequest {
                enable,
                telemetry_opt_in,
            };
            (json_request(self.put(&url), &payload)?, "PUT")
        } else {
            (build(self.delete(&url), "DELETE", path)?, "DELETE")
        };
        send_empty(request, method, path).await
    }
}

impl fmt::Debug for BrowserDiscovery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BrowserDiscovery")
            .field("base", &self.base.as_str())
            .field("token", &self.token)
            .field("contract", &self.contract)
            .finish_non_exhaustive()
    }
}

fn build(request: RequestBuilder, method: &str, path: &str) -> Result<Request> {
    request
        .build()
        .map_err(|error| Error::Transport(format!("{method} {path}: {error}")))
}

/// Serialises a JSON request body without relying on `gloo-net`'s `json` feature.
///
/// Enabling that feature pulls extra bindings for every other request type; a
/// plain `serde_json` string is the smaller dependency.
fn json_request<T: serde::Serialize + ?Sized>(
    request: RequestBuilder,
    value: &T,
) -> Result<Request> {
    let body = serde_json::to_string(value)
        .map_err(|error| Error::Transport(format!("encoding the request body: {error}")))?;
    request
        .header("Content-Type", "application/json")
        .body(body)
        .map_err(|error| Error::Transport(format!("building the request: {error}")))
}

async fn decode<T: DeserializeOwned>(response: &Response) -> Result<T> {
    let status = response.status();
    let retry_after_header = response.headers().get("retry-after");
    let retry_after_s = retry_after_s(retry_after_header.as_deref());
    let body = response
        .text()
        .await
        .map_err(|error| Error::Transport(format!("reading the response body: {error}")))?;
    if !(200..300).contains(&status) {
        return Err(status_error(status, retry_after_s, &body));
    }
    serde_json::from_str(&body).map_err(|error| decode_error(&error))
}

/// The status-to-error table, shared shape with the native client.
fn status_error(status: u16, retry_after_s: Option<u64>, body: &str) -> Error {
    match status {
        401 | 403 => Error::Unauthorized,
        429 => Error::RateLimited {
            retry_after_s: retry_after_s.unwrap_or(DEFAULT_RETRY_AFTER_S),
        },
        426 => client_too_old(body),
        code => Error::Status {
            status: code,
            message: api_error_message(body),
        },
    }
}

/// Wraps a serde failure without copying the response body.
///
/// Takes the error by reference: `serde_json::Error`'s message is what we keep, and
/// moving it would force every call site to hand over the only copy it has.
fn decode_error(error: &serde_json::Error) -> Error {
    Error::Decode(error.to_string())
}

fn retry_after_s(value: Option<&str>) -> Option<u64> {
    value.and_then(|value| value.trim().parse().ok())
}

/// Maps a `426` to the variant that renders an update screen.
fn client_too_old(body: &str) -> Error {
    let Ok(envelope) = serde_json::from_str::<TooOldEnvelope>(body) else {
        return Error::ClientTooOld {
            server: CONTRACT_VERSION.to_owned(),
            min: MIN_CONTRACT_VERSION.to_owned(),
        };
    };
    let TooOldFields {
        server,
        min,
        min_contract,
    } = envelope.error;
    Error::ClientTooOld {
        server: server.unwrap_or_else(|| CONTRACT_VERSION.to_owned()),
        min: min
            .or(min_contract)
            .unwrap_or_else(|| MIN_CONTRACT_VERSION.to_owned()),
    }
}

fn api_error_message(body: &str) -> String {
    if let Ok(envelope) = serde_json::from_str::<ApiErrorBody>(body) {
        envelope.error.message
    } else {
        "request failed".to_owned()
    }
}

#[derive(Deserialize)]
struct TooOldEnvelope {
    error: TooOldFields,
}

#[derive(Deserialize)]
struct TooOldFields {
    #[serde(default)]
    server: Option<String>,
    #[serde(default)]
    min: Option<String>,
    #[serde(default)]
    min_contract: Option<String>,
}
