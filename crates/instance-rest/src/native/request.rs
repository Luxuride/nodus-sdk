//! Sending a request, decoding its body, and mapping a status to an [`Error`].

use reqwest::{RequestBuilder, Response, StatusCode};
use serde::de::DeserializeOwned;
use url::Url;

use nodus_instance_api::model::{Account, Notification, Page, PageParams, Status};
use nodus_instance_api::{Error, Result};

use super::HttpInstance;

/// Seconds to wait when an instance rate-limits us without saying for how long.
///
/// Mastodon always sends `Retry-After` on a 429, but a reverse proxy in front of
/// it may not; 300 s is the documented window for its own limits, so it is the
/// safe guess rather than an aggressive one.
pub(super) const DEFAULT_RETRY_AFTER_S: u32 = 300;

/// How much of an unexpected error body is kept in the error message.
pub(super) const MAX_ERROR_BODY: usize = 200;

impl HttpInstance {
    /// A GET with the bearer token attached.
    pub(super) fn get(&self, url: Url) -> RequestBuilder {
        self.http.get(url).bearer_auth(self.token.expose())
    }

    /// A POST with the bearer token attached.
    pub(super) fn post(&self, url: Url) -> RequestBuilder {
        self.http.post(url).bearer_auth(self.token.expose())
    }

    /// A DELETE with the bearer token attached.
    pub(super) fn delete(&self, url: Url) -> RequestBuilder {
        self.http.delete(url).bearer_auth(self.token.expose())
    }

    /// Sends a request and decodes the body, mapping statuses to [`Error`].
    pub(super) async fn fetch<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T> {
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
    pub(super) async fn fetch_page<T: HasId + DeserializeOwned>(
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
pub(super) trait HasId {
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
pub(super) fn status_error(status: StatusCode, retry_after: Option<u32>, body: &[u8]) -> Error {
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
pub(super) fn error_message(body: &[u8]) -> String {
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
