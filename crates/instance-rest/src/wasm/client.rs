//! The transport handle: the instance, its token, and the state the browser owns.

use core::fmt;
use std::cell::RefCell;
use std::rc::Rc;

use gloo_net::http::{Headers, Request, RequestBuilder};
use url::Url;
use wasm_bindgen::closure::Closure;
use web_sys::{MessageEvent, WebSocket};

use nodus_instance_api::model::PageParams;
use nodus_instance_api::{Error, Instance, Result};

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
    pub(super) token: Token,
    pub(super) shared: Shared,
}

/// The bearer token, kept in a newtype so it cannot be printed by accident.
///
/// `Debug` is redacted and there is no `Display`, which makes `{:?}` in a log
/// line — the common way a token leaks — harmless. `expose` is the one way out,
/// and the streaming URL is the one place it is written down.
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
pub(super) struct Shared(pub(super) Rc<RefCell<SharedState>>);

/// The contents of [`Shared`].
#[derive(Default)]
pub(super) struct SharedState {
    /// Where raw stream frames go, once the host has registered a callback.
    pub(super) sink: Option<js_sys::Function>,
    /// Sockets this transport keeps open.
    pub(super) streams: Vec<ActiveStream>,
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
pub(super) struct ActiveStream {
    pub(super) socket: WebSocket,
    pub(super) on_message: Closure<dyn FnMut(MessageEvent)>,
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
    /// [`Streaming::subscribe`](nodus_instance_api::api::Streaming::subscribe),
    /// so the host supplies a JS function instead. The sink is read by every
    /// socket when a frame arrives, so registering it after a subscription takes
    /// effect for that subscription too.
    #[must_use]
    pub fn with_stream_sink(self, callback: js_sys::Function) -> Self {
        self.shared.0.borrow_mut().sink = Some(callback);
        self
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
    pub(super) fn headers(&self) -> Headers {
        let headers = Headers::new();
        headers.set("Authorization", &self.authorization());
        headers
    }

    /// A GET with the bearer token attached.
    pub(super) fn get(&self, url: &Url) -> RequestBuilder {
        Request::get(url.as_str()).headers(self.headers())
    }

    /// A POST with the bearer token attached.
    pub(super) fn post(&self, url: &Url) -> RequestBuilder {
        Request::post(url.as_str()).headers(self.headers())
    }

    /// A DELETE with the bearer token attached.
    pub(super) fn delete(&self, url: &Url) -> RequestBuilder {
        Request::delete(url.as_str()).headers(self.headers())
    }
}
