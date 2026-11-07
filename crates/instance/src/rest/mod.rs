//! One endpoint implementation, two transports.
//!
//! The duplication this module removes: previously every endpoint was written
//! twice — once over `reqwest`, once over `gloo-net` — together with its error
//! mapping, its query building and its pagination rule. A rule fixed in one copy
//! stayed broken in the other.
//!
//! Now the split runs through one seam, [`Transport`], which is small enough to
//! review in one screen: build a request, send it, hand back status, headers and
//! bytes. Everything above it — URL building, form encoding, JSON decoding,
//! status → [`Error`] mapping, the "a short page has no next cursor" rule — is
//! written once against that seam.
//!
//! The transports themselves (`native::ReqwestTransport`,
//! `wasm::GlooTransport`) contain no endpoint knowledge at all.

use core::fmt;
use core::future::Future;

use serde::de::DeserializeOwned;
use url::Url;

use crate::model::{Account, Conversation, List, Notification, Page, PageParams, Status};
use crate::{Error, Instance, Result};

pub mod api;

/// Seconds to wait when an instance rate-limits us without saying for how long.
///
/// Mastodon always sends `Retry-After` on a 429, but a reverse proxy in front of
/// it may not; 300 s is the documented window for its own limits, so it is the
/// safe guess rather than an aggressive one.
pub const DEFAULT_RETRY_AFTER_S: u32 = 300;

/// How much of an unexpected error body is kept in the error message.
pub const MAX_ERROR_BODY: usize = 200;

/// The bearer token, kept in a newtype so it cannot be printed by accident.
///
/// `Debug` is redacted and there is no `Display`, which makes `{:?}` in a log
/// line — the common way a token leaks — harmless. `expose` is the one way out.
#[derive(Clone)]
pub struct Token(String);

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

impl Token {
    /// The raw token, for the one place that needs it: an `Authorization` header.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl From<String> for Token {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// The verbs the instance surface uses. `PATCH` is absent because no endpoint we
/// call needs it; adding one is a one-line change here and in each transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// Read.
    Get,
    /// Write.
    Post,
    /// Replace.
    ///
    /// Only edits and list renames use it, but both are full replacements of the
    /// resource they address, so `PUT` is the truthful verb rather than a `POST`
    /// with a `_method` override.
    Put,
    /// Partial update: only the fields sent change.
    Patch,
    /// Remove.
    Delete,
}

impl Method {
    /// The wire spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
}

/// A request, in transport-neutral terms.
#[derive(Debug, Clone)]
pub struct HttpRequest {
    /// Verb.
    pub method: Method,
    /// Absolute URL, query string included.
    pub url: Url,
    /// Headers besides the ones the transport adds itself (auth, user agent).
    pub headers: Vec<(String, String)>,
    /// Body bytes; `None` for a body-less request.
    pub body: Option<Vec<u8>>,
}

/// A response, already read into memory.
///
/// The bodies on this surface are JSON documents of a few hundred kilobytes at
/// worst, so buffering is simpler than a stream and lets the same decoding code
/// serve both transports.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    /// HTTP status.
    pub status: u16,
    /// `Retry-After`, when the server sent one.
    pub retry_after_s: Option<u32>,
    /// Body bytes.
    pub body: Vec<u8>,
}

/// The seam between the endpoint implementation and the platform.
///
/// Implementors do three things: attach the auth header, send, and read the
/// body. They must not interpret statuses — that lives here so a 401 means the
/// same thing in the browser and in the worker.
pub trait Transport: Clone + 'static {
    /// Sends one request.
    fn send(&self, request: HttpRequest) -> impl Future<Output = Result<HttpResponse>>;
}

/// One instance, one token, one transport.
///
/// Everything that does not depend on the platform lives here; `native` and
/// `wasm` add a `Transport` and, in the browser, a streaming socket.
#[derive(Clone)]
pub struct RestInstance<T: Transport> {
    base: Url,
    token: Token,
    transport: T,
}

impl<T: Transport> fmt::Debug for RestInstance<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RestInstance")
            .field("base", &self.base.as_str())
            .field("token", &self.token)
            .finish_non_exhaustive()
    }
}

impl<T: Transport> RestInstance<T> {
    /// Builds an instance from a token and a transport.
    ///
    /// Each platform adds a `new(instance, token)` on top that supplies its own
    /// transport, so a caller never names the transport type.
    #[must_use]
    pub fn with_transport(instance: &Instance, token: impl Into<String>, transport: T) -> Self {
        Self {
            base: instance.base.clone(),
            token: Token::from(token.into()),
            transport,
        }
    }

    /// The instance this transport talks to.
    #[must_use]
    pub const fn base(&self) -> &Url {
        &self.base
    }

    /// The token, for the one place that must write it down: a streaming URL.
    #[must_use]
    pub fn token(&self) -> &Token {
        &self.token
    }

    /// The transport, for a caller that needs a socket (streaming) or a test.
    #[must_use]
    pub const fn transport(&self) -> &T {
        &self.transport
    }

    /// An absolute URL for `path`, which must start with `/`.
    pub fn url(&self, path: &str) -> Result<Url> {
        let joined = format!("{}{path}", self.base.as_str().trim_end_matches('/'));
        Url::parse(&joined).map_err(|e| Error::Transport(format!("{joined}: {e}")))
    }

    /// An absolute URL with the pagination parameters applied as a query string.
    pub fn page_url(&self, path: &str, params: &PageParams) -> Result<Url> {
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

    /// Builds a request with the bearer token attached.
    fn request(&self, method: Method, url: Url, body: Option<(String, Vec<u8>)>) -> HttpRequest {
        let mut headers = vec![(
            "Authorization".to_owned(),
            format!("Bearer {}", self.token.expose()),
        )];
        if let Some((content_type, body)) = body {
            headers.push(("Content-Type".to_owned(), content_type));
            return HttpRequest {
                method,
                url,
                headers,
                body: Some(body),
            };
        }
        HttpRequest {
            method,
            url,
            headers,
            body: None,
        }
    }

    /// A GET with the bearer token attached.
    pub fn get(&self, url: Url) -> HttpRequest {
        self.request(Method::Get, url, None)
    }

    /// A POST with the bearer token attached.
    pub fn post(&self, url: Url) -> HttpRequest {
        self.request(Method::Post, url, None)
    }

    /// A PUT with the bearer token attached.
    pub fn put(&self, url: Url) -> HttpRequest {
        self.request(Method::Put, url, None)
    }

    /// A form-encoded PUT.
    pub fn put_form(&self, url: Url, form: &[(&str, String)]) -> HttpRequest {
        self.request(Method::Put, url, Some(form_body(form)))
    }

    /// A form-encoded partial update.
    ///
    /// `PATCH` rather than `PUT` because the profile is the account's, and a `PUT` that
    /// omitted a field would be read by some instances as "clear it".
    pub fn patch_form(&self, url: Url, form: &[(&str, String)]) -> HttpRequest {
        self.request(Method::Patch, url, Some(form_body(form)))
    }

    /// A form-encoded POST.
    /// A POST with a caller-built body (multipart uploads).
    pub fn post_body(&self, url: Url, content_type: &str, body: Vec<u8>) -> HttpRequest {
        let headers = vec![("content-type".to_owned(), content_type.to_owned())];
        HttpRequest {
            method: Method::Post,
            url,
            headers,
            body: Some(body),
        }
    }

    pub fn post_form(&self, url: Url, form: &[(&str, String)]) -> HttpRequest {
        self.request(Method::Post, url, Some(form_body(form)))
    }

    /// A DELETE with the bearer token attached.
    pub fn delete(&self, url: Url) -> HttpRequest {
        self.request(Method::Delete, url, None)
    }

    /// A form-encoded DELETE.
    ///
    /// Mastodon's "remove these accounts from this list" is a DELETE with a body; a
    /// query string would work for small batches and break on long ones, so the body it
    /// is, and it is the only reason this method exists.
    pub fn delete_form(&self, url: Url, form: &[(&str, String)]) -> HttpRequest {
        self.request(Method::Delete, url, Some(form_body(form)))
    }

    /// Sends a request and decodes the body, mapping statuses to [`Error`].
    pub async fn fetch<R: DeserializeOwned>(&self, request: HttpRequest) -> Result<R> {
        let response = self.transport.send(request).await?;
        decode(&response)
    }

    /// Fetches a page, deriving the older-cursor from the last item's id.
    ///
    /// Mastodon has no `next` link on these endpoints; the cursor is the id of the
    /// last item *only* when the page came back full. Using a short page's last id
    /// would silently stop the crawl, so the length is what decides.
    pub async fn fetch_page<R: HasId + DeserializeOwned>(
        &self,
        request: HttpRequest,
        params: &PageParams,
    ) -> Result<Page<R>> {
        let items: Vec<R> = self.fetch(request).await?;
        let full = params
            .limit
            .is_none_or(|limit| items.len() >= usize::from(limit));
        let next = if full {
            items.last().map(|item| item.id().to_owned())
        } else {
            None
        };
        Ok(Page::new(items).with_next(next))
    }
}

/// Anything with an id the instance paginates by.
pub trait HasId {
    /// The id.
    fn id(&self) -> &str;
}

impl HasId for Account {
    fn id(&self) -> &str {
        &self.id
    }
}

impl HasId for Status {
    fn id(&self) -> &str {
        &self.id
    }
}

impl HasId for Notification {
    fn id(&self) -> &str {
        &self.id
    }
}

impl HasId for Conversation {
    fn id(&self) -> &str {
        &self.id
    }
}

impl HasId for List {
    fn id(&self) -> &str {
        &self.id
    }
}

/// Encodes a form body and its content type.
///
/// Shared by `post_form`, `put_form` and the two endpoints that send a repeated key
/// (`choices[]`, `id[]`), so the encoding cannot drift between verbs.
fn form_body(form: &[(&str, String)]) -> (String, Vec<u8>) {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(form.iter().map(|(key, value)| (*key, value.as_str())))
        .finish()
        .into_bytes();
    ("application/x-www-form-urlencoded".to_owned(), body)
}

/// Decodes a response body or turns its status into an [`Error`].
fn decode<R: DeserializeOwned>(response: &HttpResponse) -> Result<R> {
    if !(200..300).contains(&response.status) {
        return Err(status_error(
            response.status,
            response.retry_after_s,
            &response.body,
        ));
    }
    serde_json::from_slice(&response.body)
        .map_err(|e| Error::Decode(format!("{}: {e}", response.status)))
}

/// Maps an unsuccessful response to the error the caller must act on.
///
/// The distinction matters upstream: `Unauthorized` stops sync until the user
/// re-authorises, `RateLimited` schedules a retry on the instance's terms, and
/// anything else is a retry with backoff.
#[must_use]
pub fn status_error(status: u16, retry_after_s: Option<u32>, body: &[u8]) -> Error {
    match status {
        401 | 403 => Error::Unauthorized,
        429 => Error::RateLimited {
            retry_after_s: retry_after_s.unwrap_or(DEFAULT_RETRY_AFTER_S),
        },
        code => Error::Status {
            status: code,
            message: error_message(body),
        },
    }
}

/// Best-effort extraction of an instance's error text.
///
/// Mastodon answers `{"error": "…"}`, but a proxy in front of it may answer HTML;
/// keeping a short slice of whatever arrived is more useful in a log than an
/// empty string, and bounded so a huge body cannot flood it.
#[must_use]
pub fn error_message(body: &[u8]) -> String {
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) {
        if let Some(message) = value.get("error").and_then(serde_json::Value::as_str) {
            return message.to_owned();
        }
    }
    let text = String::from_utf8_lossy(body);
    let trimmed = text.trim();
    if trimmed.len() <= MAX_ERROR_BODY {
        return trimmed.to_owned();
    }
    let mut cut = MAX_ERROR_BODY;
    while cut > 0 && !trimmed.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}…", &trimmed[..cut])
}
