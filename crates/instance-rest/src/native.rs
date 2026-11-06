//! The `reqwest` transport, for the native targets (the calculator, integration
//! tests, any CLI).
//!
//! This is the only place in the SDK that knows how Mastodon spells its URLs,
//! maps its error statuses, or reads its pagination. The browser transport
//! (`gloo-net`) implements the same two traits with the same mapping, so a
//! crawl and a scroll cannot disagree about what "page two" means.
//!
//! Enable with the `native` feature.

use core::fmt;
use std::time::Duration;

use reqwest::{Client, RequestBuilder, Response, StatusCode};
use serde::de::DeserializeOwned;
use url::Url;

use nodus_instance_api::api::InstanceApi;
use nodus_instance_api::model::{
    Account, Context, Draft, LocalId, Notification, Page, PageParams, Relationship, Status,
    StreamKind,
};
use nodus_instance_api::sync_api::SyncApi;
use nodus_instance_api::{Error, Instance, Result};

/// Seconds to wait when an instance rate-limits us without saying for how long.
///
/// Mastodon always sends `Retry-After` on a 429, but a reverse proxy in front of
/// it may not; 300 s is the documented window for its own limits, so it is the
/// safe guess rather than an aggressive one.
const DEFAULT_RETRY_AFTER_S: u32 = 300;

/// How much of an unexpected error body is kept in the error message.
const MAX_ERROR_BODY: usize = 200;

/// One instance, one token, one HTTP client.
///
/// Cheap to clone (the client is reference-counted internally), so the calculator
/// builds one per job and the pool is shared process-wide.
#[derive(Clone)]
pub struct HttpInstance {
    base: Url,
    token: Token,
    http: Client,
}

/// The bearer token, kept in a newtype so it cannot be printed by accident.
///
/// `Debug` is redacted and there is no `Display`, which makes `{:?}` in a log
/// line — the common way a token leaks — harmless. `expose` is the one way out.
#[derive(Clone)]
struct Token(String);

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

impl Token {
    fn expose(&self) -> &str {
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
    fn url(&self, path: &str) -> Result<Url> {
        let joined = format!("{}{path}", self.base.as_str().trim_end_matches('/'));
        Url::parse(&joined).map_err(|e| Error::Transport(format!("{joined}: {e}")))
    }

    /// An absolute URL with the pagination parameters applied as a query string.
    fn page_url(&self, path: &str, params: &PageParams) -> Result<Url> {
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

    /// A GET with the bearer token attached.
    fn get(&self, url: Url) -> RequestBuilder {
        self.http.get(url).bearer_auth(self.token.expose())
    }

    /// A POST with the bearer token attached.
    fn post(&self, url: Url) -> RequestBuilder {
        self.http.post(url).bearer_auth(self.token.expose())
    }

    /// A DELETE with the bearer token attached.
    fn delete(&self, url: Url) -> RequestBuilder {
        self.http.delete(url).bearer_auth(self.token.expose())
    }

    /// Sends a request and decodes the body, mapping statuses to [`Error`].
    async fn fetch<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T> {
        let response = request
            .send()
            .await
            .map_err(|e| Error::Transport(e.to_string()))?;
        decode(response).await
    }

    /// Fetches a page, deriving the older-cursor from the last item's id.
    ///
    /// Mastodon has no `next` link on these endpoints; the cursor is the id of the
    /// last item *only* when the page came back full. Using a short page's last id
    /// would silently stop the crawl, so the length is what decides.
    async fn fetch_page<T: HasId + DeserializeOwned>(
        &self,
        request: RequestBuilder,
        params: &PageParams,
    ) -> Result<Page<T>> {
        let items: Vec<T> = self.fetch(request).await?;
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
trait HasId {
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

/// Decodes a response body or turns its status into an [`Error`].
async fn decode<T: DeserializeOwned>(response: Response) -> Result<T> {
    let status = response.status();
    let retry_after = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u32>().ok());
    let body = response
        .bytes()
        .await
        .map_err(|e| Error::Transport(e.to_string()))?;

    if !status.is_success() {
        return Err(status_error(status, retry_after, &body));
    }

    serde_json::from_slice(&body).map_err(|e| Error::Decode(format!("{status}: {e}")))
}

/// Maps an unsuccessful response to the error the caller must act on.
///
/// The distinction matters upstream: `Unauthorized` stops sync until the user
/// re-authorises, `RateLimited` schedules a retry on the instance's terms, and
/// anything else is a retry with backoff.
fn status_error(status: StatusCode, retry_after: Option<u32>, body: &[u8]) -> Error {
    match status.as_u16() {
        401 | 403 => Error::Unauthorized,
        429 => Error::RateLimited {
            retry_after_s: retry_after.unwrap_or(DEFAULT_RETRY_AFTER_S),
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
fn error_message(body: &[u8]) -> String {
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

impl InstanceApi for HttpInstance {
    type Error = Error;

    async fn verify_credentials(&self) -> Result<Account> {
        let url = self.url("/api/v1/accounts/verify_credentials")?;
        self.fetch(self.get(url)).await
    }

    async fn timeline(&self, kind: StreamKind, params: PageParams) -> Result<Page<Status>> {
        let (path, local) = match kind {
            StreamKind::User => ("/api/v1/timelines/home", false),
            StreamKind::Local => ("/api/v1/timelines/public", true),
            StreamKind::Federated => ("/api/v1/timelines/public", false),
        };
        let mut url = self.page_url(path, &params)?;
        if local {
            url.query_pairs_mut().append_pair("local", "true");
        }
        self.fetch_page(self.get(url), &params).await
    }

    async fn tag_timeline(&self, tag: &str, params: PageParams) -> Result<Page<Status>> {
        let url = self.page_url(&format!("/api/v1/timelines/tag/{tag}"), &params)?;
        self.fetch_page(self.get(url), &params).await
    }

    async fn status(&self, id: &str) -> Result<Status> {
        let url = self.url(&format!("/api/v1/statuses/{id}"))?;
        self.fetch(self.get(url)).await
    }

    async fn status_context(&self, id: &str) -> Result<Context> {
        let url = self.url(&format!("/api/v1/statuses/{id}/context"))?;
        self.fetch(self.get(url)).await
    }

    async fn notifications(&self, params: PageParams) -> Result<Page<Notification>> {
        let url = self.page_url("/api/v1/notifications", &params)?;
        self.fetch_page(self.get(url), &params).await
    }

    async fn account(&self, id: &LocalId) -> Result<Account> {
        let url = self.url(&format!("/api/v1/accounts/{}", id.0))?;
        self.fetch(self.get(url)).await
    }

    async fn account_statuses(&self, id: &LocalId, params: PageParams) -> Result<Page<Status>> {
        let url = self.page_url(&format!("/api/v1/accounts/{}/statuses", id.0), &params)?;
        self.fetch_page(self.get(url), &params).await
    }

    async fn relationship(&self, id: &LocalId) -> Result<Relationship> {
        let url = self.url("/api/v1/accounts/relationships")?;
        let mut url = url;
        url.query_pairs_mut().append_pair("id[]", &id.0);
        let mut found: Vec<Relationship> = self.fetch(self.get(url)).await?;
        found
            .pop()
            .ok_or_else(|| Error::Decode("empty relationship list".to_owned()))
    }

    async fn follow(&self, id: &LocalId) -> Result<Relationship> {
        let url = self.url(&format!("/api/v1/accounts/{}/follow", id.0))?;
        self.fetch(self.post(url)).await
    }

    async fn unfollow(&self, id: &LocalId) -> Result<Relationship> {
        let url = self.url(&format!("/api/v1/accounts/{}/unfollow", id.0))?;
        self.fetch(self.post(url)).await
    }

    async fn post_status(&self, draft: &Draft) -> Result<Status> {
        let url = self.url("/api/v1/statuses")?;
        let mut form: Vec<(&str, String)> = vec![
            ("status", draft.text.clone()),
            ("visibility", draft.visibility.as_str().to_owned()),
        ];
        if let Some(reply_to) = &draft.in_reply_to_id {
            form.push(("in_reply_to_id", reply_to.clone()));
        }
        // `reqwest`'s `form` helper is behind a feature we do not need; the url
        // crate already carries the encoder and is a dependency either way.
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(form.iter().map(|(key, value)| (*key, value.as_str())))
            .finish();
        let mut request = self
            .post(url)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body);
        // Idempotency is what stops a retried compose from posting twice; a draft
        // without a key is a caller bug, not something to paper over here.
        if let Some(key) = &draft.idempotency_key {
            request = request.header("Idempotency-Key", key);
        }
        self.fetch(request).await
    }

    async fn delete_status(&self, id: &str) -> Result<()> {
        let url = self.url(&format!("/api/v1/statuses/{id}"))?;
        let _: serde_json::Value = self.fetch(self.delete(url)).await?;
        Ok(())
    }
}

impl SyncApi for HttpInstance {
    type Error = Error;

    async fn following(&self, id: &LocalId, params: PageParams) -> Result<Page<Account>> {
        let url = self.page_url(&format!("/api/v1/accounts/{}/following", id.0), &params)?;
        self.fetch_page(self.get(url), &params).await
    }

    async fn favourites(&self, params: PageParams) -> Result<Page<Status>> {
        let url = self.page_url("/api/v1/favourites", &params)?;
        self.fetch_page(self.get(url), &params).await
    }

    async fn bookmarks(&self, params: PageParams) -> Result<Page<Status>> {
        let url = self.page_url("/api/v1/bookmarks", &params)?;
        self.fetch_page(self.get(url), &params).await
    }
}

#[cfg(test)]
mod tests {
    use super::{error_message, status_error};
    use nodus_instance_api::Error;
    use reqwest::StatusCode;

    #[test]
    fn a_401_stops_sync() {
        assert!(matches!(
            status_error(StatusCode::UNAUTHORIZED, None, b""),
            Error::Unauthorized
        ));
        assert!(matches!(
            status_error(StatusCode::FORBIDDEN, None, b""),
            Error::Unauthorized
        ));
    }

    #[test]
    fn a_429_carries_the_instances_delay() {
        match status_error(StatusCode::TOO_MANY_REQUESTS, Some(120), b"") {
            Error::RateLimited { retry_after_s } => assert_eq!(retry_after_s, 120),
            other => panic!("expected a rate limit, got {other:?}"),
        }
    }

    #[test]
    fn a_429_without_a_header_falls_back_to_the_documented_window() {
        match status_error(StatusCode::TOO_MANY_REQUESTS, None, b"") {
            Error::RateLimited { retry_after_s } => assert_eq!(retry_after_s, 300),
            other => panic!("expected a rate limit, got {other:?}"),
        }
    }

    #[test]
    fn an_error_body_is_read_from_the_error_field() {
        assert_eq!(
            error_message(br#"{"error":"Record not found"}"#),
            "Record not found"
        );
    }

    #[test]
    fn an_html_error_body_is_trimmed() {
        let body = format!("<html>{}</html>", "x".repeat(500));
        let message = error_message(body.as_bytes());
        // Counted in chars, not bytes: the ellipsis is three bytes of UTF-8, and a
        // byte length would report a body that is already short enough.
        assert_eq!(message.chars().count(), super::MAX_ERROR_BODY + 1);
        assert!(message.ends_with('…'));
    }

    #[test]
    fn a_503_becomes_a_status_error() {
        match status_error(StatusCode::SERVICE_UNAVAILABLE, None, b"down") {
            Error::Status { status, message } => {
                assert_eq!(status, 503);
                assert_eq!(message, "down");
            }
            other => panic!("expected a status error, got {other:?}"),
        }
    }
}
