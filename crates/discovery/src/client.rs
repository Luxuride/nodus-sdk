//! Native discovery transport: `reqwest`, one `Client`, one base URL.
//!
//! This is the calculator and native-test transport. The browser twin lives in
//! `crate::wasm` and intentionally duplicates the small mapping helpers: the two
//! modules compile under disjoint feature sets, and the mapping must not drag
//! `reqwest` into the wasm build.

use core::fmt;
use std::time::Duration;

use nodus_shared::{
    AccountFeed, ApiErrorBody, CONTRACT_VERSION, EventBatch, MIN_CONTRACT_VERSION, Meta, PostFeed,
    SyncTokenRequest,
};
use nodus_state::Discovery;
use reqwest::header::{HeaderMap, RETRY_AFTER};
use reqwest::{Client, Method, RequestBuilder, Response, StatusCode};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use url::Url;

use crate::{Error, Result};

const CONTRACT_HEADER: &str = "X-Contract-Version";
const DEFAULT_FEED_LIMIT: u32 = 50;
const DEFAULT_RETRY_AFTER_S: u64 = 60;

/// A bearer token that refuses to leak through `Debug`.
///
/// Errors, logs and tracing output routinely format structs; a plain `String`
/// would put the credential in every one of them. Only [`Token::expose`] can read
/// the value, and it is called at the header-building site.
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

/// Client for the recommendation API.
///
/// `base` is the API origin (for example `https://api.example.social`); paths are
/// appended to it. The client is `Clone` because `reqwest::Client` is a handle to
/// a shared connection pool, not a connection.
#[derive(Clone)]
pub struct DiscoveryClient {
    base: Url,
    token: Token,
    contract: String,
    http: Client,
}

impl DiscoveryClient {
    /// Builds a client with a default HTTP stack.
    ///
    /// The builder's only failure is TLS or resolver initialisation, which is a
    /// deployment problem rather than a request problem, so the fallback keeps
    /// this constructor infallible. [`Self::with_client`] is the injectable
    /// alternative for callers that need a custom stack.
    #[must_use]
    pub fn new(base: Url, token: impl Into<String>) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("nodus-discovery/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_default();
        Self::with_client(base, token, http)
    }

    /// Builds a client around an already configured HTTP stack.
    #[must_use]
    pub fn with_client(base: Url, token: impl Into<String>, http: Client) -> Self {
        Self {
            base,
            token: Token(token.into()),
            contract: CONTRACT_VERSION.to_owned(),
            http,
        }
    }

    /// The API origin this client talks to.
    #[must_use]
    pub fn base(&self) -> &Url {
        &self.base
    }

    /// Adds the two headers every discovery request must carry.
    fn request(&self, method: Method, url: Url) -> RequestBuilder {
        self.http
            .request(method, url)
            .bearer_auth(self.token.expose())
            .header(CONTRACT_HEADER, self.contract.as_str())
    }

    /// Resolves a path against the base with `Url`'s path-segment writer.
    ///
    /// Using the URL parser rather than `format!` and string concatenation keeps
    /// the base's own path prefix, percent-encodes each segment, and stops a path
    /// character from changing the shape of the request. Query parameters are
    /// appended separately with `query_pairs_mut`, which has its own encoder.
    fn url(&self, path: &str) -> Result<Url> {
        let mut url = self.base.clone();
        {
            let mut segments = url.path_segments_mut().map_err(|()| {
                Error::Transport("the API base cannot carry path segments".to_owned())
            })?;
            segments.pop_if_empty();
            for segment in path.trim_start_matches('/').split('/') {
                segments.push(segment);
            }
        }
        Ok(url)
    }

    /// Builds `/api/v1/feed/{feed_id}/{action}`.
    ///
    /// `feed_id` is server-provided and opaque, so it is pushed as a path segment
    /// instead of interpolated into a string. That makes the platform escape any
    /// reserved character and keeps the route unambiguous.
    fn feed_action_url(&self, feed_id: &str, action: &str) -> Result<Url> {
        let mut url = self.url("/api/v1/feed")?;
        {
            let mut segments = url.path_segments_mut().map_err(|()| {
                Error::Transport("/api/v1/feed: the API base cannot carry path segments".to_owned())
            })?;
            segments.push(feed_id).push(action);
        }
        Ok(url)
    }
}

async fn fetch<T: DeserializeOwned>(request: RequestBuilder) -> Result<T> {
    let response = request
        .send()
        .await
        .map_err(|error| Error::Transport(error.to_string()))?;
    decode(response).await
}

impl Discovery for DiscoveryClient {
    type Error = Error;

    async fn meta(&self) -> Result<Meta> {
        let url = self.url("/api/v1/meta")?;
        // No caching: the SDK decides when a contract check is worth a request.
        fetch(self.request(Method::GET, url)).await
    }

    async fn feed(&self, cursor: Option<&str>) -> Result<AccountFeed> {
        let mut url = self.url("/api/v1/feed/accounts")?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("limit", &DEFAULT_FEED_LIMIT.to_string());
            if let Some(cursor) = cursor {
                query.append_pair("cursor", cursor);
            }
        }
        fetch(self.request(Method::GET, url)).await
    }

    async fn posts(&self, cursor: Option<&str>) -> Result<PostFeed> {
        let mut url = self.url("/api/v1/feed/posts")?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("limit", &DEFAULT_FEED_LIMIT.to_string());
            if let Some(cursor) = cursor {
                query.append_pair("cursor", cursor);
            }
        }
        fetch(self.request(Method::GET, url)).await
    }

    async fn refresh(&self, feed_id: &str) -> Result<AccountFeed> {
        let url = self.feed_action_url(feed_id, "refresh")?;
        fetch(self.request(Method::POST, url)).await
    }

    async fn send_events(&self, batch: &EventBatch) -> Result<()> {
        let path = "/api/v1/events";
        let url = self.url(path)?;
        let request = self.request(Method::POST, url).json(batch);
        send_empty(request, Method::POST, path).await
    }

    async fn set_sync(&self, enable: bool, telemetry_opt_in: bool) -> Result<()> {
        let path = "/api/v1/sync/token";
        let url = self.url(path)?;
        if enable {
            let payload = SyncTokenRequest {
                enable,
                telemetry_opt_in,
            };
            send_empty(
                self.request(Method::PUT, url).json(&payload),
                Method::PUT,
                path,
            )
            .await
        } else {
            // The opt-out path is a delete: the server wipes the stored token and
            // the derived data, and the client keeps its own copy.
            send_empty(self.request(Method::DELETE, url), Method::DELETE, path).await
        }
    }
}

impl fmt::Debug for DiscoveryClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DiscoveryClient")
            .field("base", &self.base.as_str())
            .field("token", &self.token)
            .field("contract", &self.contract)
            .finish_non_exhaustive()
    }
}

async fn decode<T: DeserializeOwned>(response: Response) -> Result<T> {
    let status = response.status();
    let retry_after_s = retry_after_s(response.headers());
    let body = response
        .bytes()
        .await
        .map_err(|error| Error::Transport(format!("reading the response body: {error}")))?;
    if !status.is_success() {
        let text = String::from_utf8_lossy(&body);
        return Err(status_error(status, retry_after_s, text.as_ref()));
    }
    serde_json::from_slice(&body).map_err(|error| decode_error(&error))
}

async fn send_empty(request: RequestBuilder, method: Method, path: &str) -> Result<()> {
    let response = request
        .send()
        .await
        .map_err(|error| Error::Transport(format!("{method} {path}: {error}")))?;
    let status = response.status();
    let retry_after_s = retry_after_s(response.headers());
    let body = response
        .bytes()
        .await
        .map_err(|error| Error::Transport(format!("reading the response body: {error}")))?;
    if !status.is_success() {
        let text = String::from_utf8_lossy(&body);
        return Err(status_error(status, retry_after_s, text.as_ref()));
    }
    Ok(())
}

/// The status-to-error table, kept free of the request so tests can drive it.
///
/// `401` and `403` collapse to [`Error::Unauthorized`] because both send the user
/// back through authorisation. `426` stays separate from [`Error::Status`]: it is
/// the one response whose correct UI is a hard update screen, and collapsing it
/// makes an old client look like a transient blank feed.
fn status_error(status: StatusCode, retry_after_s: Option<u64>, body: &str) -> Error {
    match status.as_u16() {
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
/// The body can echo the request, and a request URL can carry a cursor, so the
/// serde `Display` is the whole payload. It names the position and the type
/// mismatch, which is what a bug report needs.
fn decode_error(error: &serde_json::Error) -> Error {
    Error::Decode(error.to_string())
}

/// Reads `Retry-After` as integer seconds.
///
/// The header may be an HTTP date, which this client does not parse; the default
/// in [`status_error`] is the safer answer. A missing or malformed header is not
/// an error: the documented fallback is 60 seconds.
fn retry_after_s(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse().ok())
}

/// Maps a `426` to the variant the UI branches on.
///
/// The documented envelope carries only a human `message`, not the two versions,
/// and that message is explicitly allowed to change, so it is not parsed. When a
/// future additive field provides `server` and `min`, they are used; otherwise the
/// transport reports the contract it sent and the minimum this build knows. The
/// variant itself is the "update the app" signal; the strings are diagnostic.
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

/// The API's message, but only when the body is the documented envelope.
///
/// A non-JSON body is never surfaced raw: it can be an HTML proxy page or an echo
/// of the request.
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

#[cfg(test)]
mod tests {
    use super::{Token, decode_error, status_error};
    use crate::Error;
    use reqwest::StatusCode;

    #[test]
    fn debug_redacts_the_token() {
        let token = Token("super-secret-value".to_owned());
        let rendered = format!("{token:?}");
        assert!(!rendered.contains("super-secret-value"));
        assert!(rendered.contains("redacted"));
    }

    #[test]
    fn a_426_is_client_too_old() {
        let body = r#"{"error":{"code":"client_too_old","message":"update the client"}}"#;
        assert!(matches!(
            status_error(StatusCode::UPGRADE_REQUIRED, None, body),
            Error::ClientTooOld { .. }
        ));
    }

    #[test]
    fn a_429_without_retry_after_defaults_to_sixty_seconds() {
        match status_error(StatusCode::TOO_MANY_REQUESTS, None, "") {
            Error::RateLimited { retry_after_s } => assert_eq!(retry_after_s, 60),
            other => panic!("expected a rate-limit error, got {other:?}"),
        }
    }

    #[test]
    fn a_non_json_body_is_a_decode_error_without_the_body() {
        let body = "token=super-secret-value";
        let parse = serde_json::from_str::<serde_json::Value>(body);
        let error = decode_error(&parse.expect_err("the body is not JSON"));
        assert!(matches!(error, Error::Decode(_)));
        assert!(!format!("{error}").contains(body));
    }
}
