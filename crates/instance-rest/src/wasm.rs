//! The browser transport (`gloo-net` + `web-sys`), for `wasm32`.
//!
//! # Why two transports
//!
//! In the browser the client *is* the person: it talks to an instance directly,
//! with the token that person granted it, over the shared CORS channel every web
//! app uses. That is the product's primary data path, so this transport is not a
//! build variant of the native one — it is the one the web client runs on, and
//! `native` exists for the parts of the product that have no browser: the
//! background calculator and the integration tests. Both are driven by the same
//! two traits in [`nodus_instance_api::api`], so an endpoint spelling, an error mapping or a
//! pagination rule cannot drift between them.
//!
//! Enabled by the `wasm` feature, and only compiled for `wasm32`; see
//! `crate::native` for the `reqwest` twin.

use core::fmt;
use std::cell::RefCell;
use std::rc::Rc;

use gloo_net::http::{Headers, Request, RequestBuilder, Response};
use serde::de::DeserializeOwned;
use url::Url;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{BinaryType, MessageEvent, WebSocket};

use nodus_instance_api::api::{InstanceApi, Streaming};
use nodus_instance_api::model::{
    Account, Context, Draft, LocalId, Notification, Page, PageParams, Relationship, Status,
    StreamKind,
};
use nodus_instance_api::{Error, Instance, Result};

/// Seconds to wait when an instance rate-limits us without saying for how long.
///
/// Mastodon always sends `Retry-After` on a 429, but a reverse proxy in front of
/// it may not; 300 s is the documented window for its own limits, so it is the
/// safe guess rather than an aggressive one.
const DEFAULT_RETRY_AFTER_S: u32 = 300;

/// How much of an unexpected error body is kept in the error message.
const MAX_ERROR_BODY: usize = 200;

/// One instance, one token, reached through the browser's own `fetch`.
///
/// Cheap to clone: cloning shares the stream sockets and the frame sink, which
/// is what a UI wants when it hands the transport to several components.
///
/// There is no client handle to carry, unlike `reqwest`'s `Client`: the browser
/// owns the connection pool, so a request is built and sent through the platform
/// `fetch` on every call.
#[derive(Clone)]
pub struct BrowserInstance {
    base: Url,
    token: Token,
    shared: Shared,
}

/// The bearer token, kept in a newtype so it cannot be printed by accident.
///
/// `Debug` is redacted and there is no `Display`, which makes `{:?}` in a log
/// line — the common way a token leaks — harmless. `expose` is the one way out,
/// and the streaming URL is the one place it is written down.
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

impl fmt::Debug for BrowserInstance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BrowserInstance")
            .field("base", &self.base.as_str())
            .field("token", &self.token)
            .finish_non_exhaustive()
    }
}

/// State the browser, rather than the struct, owns: the open sockets and the
/// callback that receives their frames.
///
/// Kept behind `Rc<RefCell<…>>` because `InstanceApi` and `Streaming` take
/// `&self`; it is shared by clones on purpose, so a socket opened before a clone
/// keeps delivering to the sink registered after it.
#[derive(Clone, Default)]
struct Shared(Rc<RefCell<SharedState>>);

/// The contents of [`Shared`].
#[derive(Default)]
struct SharedState {
    /// Where raw stream frames go, once the host has registered a callback.
    sink: Option<js_sys::Function>,
    /// Sockets this transport keeps open.
    streams: Vec<ActiveStream>,
}

impl Drop for SharedState {
    fn drop(&mut self) {
        for stream in &self.streams {
            let _ = stream.socket.close();
        }
    }
}

/// One open stream socket plus the handler attached to it.
///
/// The closure is owned here instead of `forget`-ten, so dropping the transport
/// drops the socket and its handler together rather than leaking the callback
/// for the lifetime of the page.
struct ActiveStream {
    socket: WebSocket,
    on_message: Closure<dyn FnMut(MessageEvent)>,
}

impl BrowserInstance {
    /// Builds a transport for `instance` authenticated as `token`.
    #[must_use]
    pub fn new(instance: &Instance, token: impl Into<String>) -> Self {
        Self {
            base: instance.base.clone(),
            token: Token(token.into()),
            shared: Shared::default(),
        }
    }

    /// The instance this transport talks to.
    #[must_use]
    pub const fn base(&self) -> &Url {
        &self.base
    }

    /// Registers the callback that receives each stream frame's raw JSON text.
    ///
    /// A browser socket cannot hand a Rust channel back through
    /// [`Streaming::subscribe`], so the host supplies a JS function instead. The
    /// sink is read by every socket when a frame arrives, so registering it
    /// after a subscription takes effect for that subscription too.
    #[must_use]
    pub fn with_stream_sink(self, callback: js_sys::Function) -> Self {
        self.shared.0.borrow_mut().sink = Some(callback);
        self
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

    /// The `Authorization` header value.
    ///
    /// Built per request rather than stored, so the only string holding the
    /// token is the one the platform copies into the request immediately.
    fn authorization(&self) -> String {
        format!("Bearer {}", self.token.expose())
    }

    /// The headers every request carries, with the token copied in.
    ///
    /// A `Headers` value is mutated through `&self`, so a caller can add its own
    /// entries before the builder takes it.
    fn headers(&self) -> Headers {
        let headers = Headers::new();
        headers.set("Authorization", &self.authorization());
        headers
    }

    /// A GET with the bearer token attached.
    fn get(&self, url: &Url) -> RequestBuilder {
        Request::get(url.as_str()).headers(self.headers())
    }

    /// A POST with the bearer token attached.
    fn post(&self, url: &Url) -> RequestBuilder {
        Request::post(url.as_str()).headers(self.headers())
    }

    /// A DELETE with the bearer token attached.
    fn delete(&self, url: &Url) -> RequestBuilder {
        Request::delete(url.as_str()).headers(self.headers())
    }

    /// The `wss://` URL for a streaming timeline.
    ///
    /// The token rides in the query string because a browser `WebSocket` cannot
    /// send headers; this is the one place the token is written into a URL, and
    /// no error or log line in this module ever includes that query string.
    fn stream_url(&self, kind: StreamKind) -> Result<Url> {
        let mut url = self.url("/api/v1/streaming")?;
        url.set_scheme("wss").map_err(|()| {
            Error::Transport("/api/v1/streaming: the instance URL has no websocket form".to_owned())
        })?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("stream", stream_name(kind));
            query.append_pair("access_token", self.token.expose());
        }
        Ok(url)
    }
}

/// Sends a built request and decodes the body, mapping statuses to [`Error`].
///
/// `method` and `path` exist only for the error message: the path is passed
/// without its query string, which is where a streaming token would live.
async fn send<T: DeserializeOwned>(request: Request, method: &str, path: &str) -> Result<T> {
    let response = request
        .send()
        .await
        .map_err(|e| Error::Transport(format!("{method} {path}: {e}")))?;
    decode(&response).await
}

/// Builds `request` and hands it to [`send`].
async fn fetch<T: DeserializeOwned>(
    request: RequestBuilder,
    method: &str,
    path: &str,
) -> Result<T> {
    // The builder can only fail on an invalid URL or header, and both are
    // already token-free by construction.
    let built = request
        .build()
        .map_err(|e| Error::Transport(format!("{method} {path}: {e}")))?;
    send(built, method, path).await
}

/// Fetches a page, deriving the older-cursor from the last item's id.
///
/// Mastodon has no `next` link on these endpoints; the cursor is the id of the
/// last item *only* when the page came back full. Using a short page's last id
/// would silently stop the crawl, so the length is what decides. This is the
/// native transport's rule verbatim.
async fn fetch_page<T: HasId + DeserializeOwned>(
    request: RequestBuilder,
    params: &PageParams,
    method: &str,
    path: &str,
) -> Result<Page<T>> {
    let items: Vec<T> = fetch(request, method, path).await?;
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

/// The wire spelling of a streaming timeline.
const fn stream_name(kind: StreamKind) -> &'static str {
    match kind {
        StreamKind::User => "user",
        StreamKind::Local => "local",
        StreamKind::Federated => "federated",
    }
}

/// Decodes a response body or turns its status into an [`Error`].
async fn decode<T: DeserializeOwned>(response: &Response) -> Result<T> {
    let code = response.status();
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|value| value.trim().parse::<u32>().ok());
    let body = response
        .text()
        .await
        .map_err(|e| Error::Transport(format!("reading the response body: {e}")))?;

    if !(200..300).contains(&code) {
        return Err(status_error(code, retry_after, &body));
    }

    serde_json::from_str(&body).map_err(|e| Error::Decode(format!("{code}: {e}")))
}

/// Maps an unsuccessful response to the error the caller must act on.
///
/// The distinction matters upstream: `Unauthorized` stops sync until the user
/// re-authorises, `RateLimited` schedules a retry on the instance's terms, and
/// anything else is a retry with backoff.
fn status_error(code: u16, retry_after: Option<u32>, body: &str) -> Error {
    match code {
        401 | 403 => Error::Unauthorized,
        429 => Error::RateLimited {
            retry_after_s: retry_after.unwrap_or(DEFAULT_RETRY_AFTER_S),
        },
        other => Error::Status {
            status: other,
            message: error_message(body),
        },
    }
}

/// Best-effort extraction of an instance's error text.
///
/// Mastodon answers `{"error": "…"}`, but a proxy in front of it may answer
/// HTML; keeping a short slice of whatever arrived is more useful in a log than
/// an empty string, and bounded so a huge body cannot flood it.
fn error_message(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(message) = value.get("error").and_then(serde_json::Value::as_str) {
            return message.to_owned();
        }
    }
    let trimmed = body.trim();
    if trimmed.len() <= MAX_ERROR_BODY {
        return trimmed.to_owned();
    }
    let mut cut = MAX_ERROR_BODY;
    while cut > 0 && !trimmed.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}…", &trimmed[..cut])
}

impl InstanceApi for BrowserInstance {
    type Error = Error;

    async fn verify_credentials(&self) -> Result<Account> {
        let path = "/api/v1/accounts/verify_credentials";
        let url = self.url(path)?;
        fetch(self.get(&url), "GET", path).await
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
        fetch_page(self.get(&url), &params, "GET", path).await
    }

    async fn tag_timeline(&self, tag: &str, params: PageParams) -> Result<Page<Status>> {
        let path = format!("/api/v1/timelines/tag/{tag}");
        let url = self.page_url(&path, &params)?;
        fetch_page(self.get(&url), &params, "GET", &path).await
    }

    async fn status(&self, id: &str) -> Result<Status> {
        let path = format!("/api/v1/statuses/{id}");
        let url = self.url(&path)?;
        fetch(self.get(&url), "GET", &path).await
    }

    async fn status_context(&self, id: &str) -> Result<Context> {
        let path = format!("/api/v1/statuses/{id}/context");
        let url = self.url(&path)?;
        fetch(self.get(&url), "GET", &path).await
    }

    async fn notifications(&self, params: PageParams) -> Result<Page<Notification>> {
        let path = "/api/v1/notifications";
        let url = self.page_url(path, &params)?;
        fetch_page(self.get(&url), &params, "GET", path).await
    }

    async fn account(&self, id: &LocalId) -> Result<Account> {
        let path = format!("/api/v1/accounts/{}", id.0);
        let url = self.url(&path)?;
        fetch(self.get(&url), "GET", &path).await
    }

    async fn account_statuses(&self, id: &LocalId, params: PageParams) -> Result<Page<Status>> {
        let path = format!("/api/v1/accounts/{}/statuses", id.0);
        let url = self.page_url(&path, &params)?;
        fetch_page(self.get(&url), &params, "GET", &path).await
    }

    async fn relationship(&self, id: &LocalId) -> Result<Relationship> {
        let path = "/api/v1/accounts/relationships";
        let mut url = self.url(path)?;
        url.query_pairs_mut().append_pair("id[]", &id.0);
        let mut found: Vec<Relationship> = fetch(self.get(&url), "GET", path).await?;
        found
            .pop()
            .ok_or_else(|| Error::Decode("empty relationship list".to_owned()))
    }

    async fn follow(&self, id: &LocalId) -> Result<Relationship> {
        let path = format!("/api/v1/accounts/{}/follow", id.0);
        let url = self.url(&path)?;
        fetch(self.post(&url), "POST", &path).await
    }

    async fn unfollow(&self, id: &LocalId) -> Result<Relationship> {
        let path = format!("/api/v1/accounts/{}/unfollow", id.0);
        let url = self.url(&path)?;
        fetch(self.post(&url), "POST", &path).await
    }

    async fn post_status(&self, draft: &Draft) -> Result<Status> {
        let path = "/api/v1/statuses";
        let url = self.url(path)?;
        let mut form: Vec<(&str, String)> = vec![
            ("status", draft.text.clone()),
            ("visibility", draft.visibility.as_str().to_owned()),
        ];
        if let Some(reply_to) = &draft.in_reply_to_id {
            form.push(("in_reply_to_id", reply_to.clone()));
        }
        // Reusing the `url` crate's encoder keeps the browser and native forms
        // identical without another dependency.
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(form.iter().map(|(key, value)| (*key, value.as_str())))
            .finish();
        let headers = self.headers();
        headers.set("Content-Type", "application/x-www-form-urlencoded");
        // Idempotency is what stops a retried compose from posting twice; a
        // draft without a key is a caller bug, not something to paper over here.
        if let Some(key) = &draft.idempotency_key {
            headers.set("Idempotency-Key", key);
        }
        // `body` finishes the builder — it hands back the `Request` that `send`
        // takes — so every header has to be in place before this call.
        let request = Request::post(url.as_str())
            .headers(headers)
            .body(body)
            .map_err(|e| Error::Transport(format!("POST {path}: {e}")))?;
        send(request, "POST", path).await
    }

    async fn delete_status(&self, id: &str) -> Result<()> {
        let path = format!("/api/v1/statuses/{id}");
        let url = self.url(&path)?;
        let _: serde_json::Value = fetch(self.delete(&url), "DELETE", &path).await?;
        Ok(())
    }
}

impl Streaming for BrowserInstance {
    type Error = Error;

    fn subscribe(&self, kind: StreamKind) -> impl Future<Output = Result<()>> {
        core::future::ready(self.open_stream(kind))
    }
}

impl BrowserInstance {
    /// Opens the streaming socket and registers it.
    ///
    /// Kept out of [`Streaming::subscribe`] so that method can stay a plain
    /// function: the browser opens a socket synchronously, and an `async fn`
    /// whose body never awaits trips `clippy::unused_async`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Transport`] when the socket URL cannot be built or the
    /// browser rejects it. The rejection text is deliberately not forwarded:
    /// the browser quotes the URL it refused, and that URL carries the token.
    fn open_stream(&self, kind: StreamKind) -> Result<()> {
        let url = self.stream_url(kind)?;
        // The error is deliberately discarded: the browser's own message quotes
        // the URL it rejected, and that URL carries the access token.
        let socket = WebSocket::new(url.as_str()).map_err(|_| {
            Error::Transport("/api/v1/streaming: the browser rejected the socket URL".to_owned())
        })?;
        socket.set_binary_type(BinaryType::Arraybuffer);
        let shared = self.shared.clone();
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            let Some(sink) = shared.0.borrow().sink.clone() else {
                return;
            };
            if let Some(text) = event.data().as_string() {
                let _ = sink.call1(&JsValue::NULL, &JsValue::from_str(&text));
            }
        });
        let stream = ActiveStream { socket, on_message };
        stream
            .socket
            .set_onmessage(Some(stream.on_message.as_ref().unchecked_ref()));
        self.shared.0.borrow_mut().streams.push(stream);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_ERROR_BODY, error_message, status_error};
    use nodus_instance_api::Error;

    #[test]
    fn a_401_stops_sync() {
        assert!(matches!(status_error(401, None, ""), Error::Unauthorized));
        assert!(matches!(status_error(403, None, ""), Error::Unauthorized));
    }

    #[test]
    fn a_429_carries_the_instances_delay() {
        match status_error(429, Some(120), "") {
            Error::RateLimited { retry_after_s } => assert_eq!(retry_after_s, 120),
            other => panic!("expected a rate limit, got {other:?}"),
        }
    }

    #[test]
    fn a_429_without_a_header_falls_back_to_the_documented_window() {
        match status_error(429, None, "") {
            Error::RateLimited { retry_after_s } => assert_eq!(retry_after_s, 300),
            other => panic!("expected a rate limit, got {other:?}"),
        }
    }

    #[test]
    fn an_error_body_is_read_from_the_error_field() {
        assert_eq!(
            error_message(r#"{"error":"Record not found"}"#),
            "Record not found"
        );
    }

    #[test]
    fn an_html_error_body_is_trimmed() {
        let body = format!("<html>{}</html>", "x".repeat(500));
        let message = error_message(&body);
        // Counted in chars, not bytes: the ellipsis is three bytes of UTF-8, and
        // a byte length would report a body that is already short enough.
        assert_eq!(message.chars().count(), MAX_ERROR_BODY + 1);
        assert!(message.ends_with('…'));
    }

    #[test]
    fn a_503_becomes_a_status_error() {
        match status_error(503, None, "down") {
            Error::Status { status, message } => {
                assert_eq!(status, 503);
                assert_eq!(message, "down");
            }
            other => panic!("expected a status error, got {other:?}"),
        }
    }
}
