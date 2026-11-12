//! The browser transport: the platform's own `fetch`, plus the sockets it owns.

use core::fmt;
use std::cell::RefCell;
use std::rc::Rc;

use gloo_net::http::{Headers, Request};
use wasm_bindgen::closure::Closure;
use web_sys::{MessageEvent, WebSocket};

use crate::rest::{HttpRequest, HttpResponse, Method, RestInstance, Transport};
use crate::{Error, Instance, Result};

/// The browser's `fetch`, and the state the page owns rather than the struct:
/// the open streaming sockets and the callback that receives their frames.
///
/// There is no connection pool to carry (the browser has one), so a request is
/// built and sent per call. Cloning shares the sockets on purpose: a socket
/// opened before a clone keeps delivering to the sink registered after it.
#[derive(Clone, Default)]
pub struct GlooTransport {
    shared: Shared,
}

/// State the browser, rather than the struct, owns.
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

impl GlooTransport {
    /// A transport with no sockets yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The shared socket state, for the streaming implementation.
    pub(super) fn shared(&self) -> &Shared {
        &self.shared
    }
}

impl fmt::Debug for GlooTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("GlooTransport")
    }
}

impl Transport for GlooTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse> {
        let HttpRequest {
            method,
            url,
            headers,
            body,
        } = request;
        let builder = match method {
            Method::Get => Request::get(url.as_str()),
            Method::Post => Request::post(url.as_str()),
            Method::Put => Request::put(url.as_str()),
            Method::Patch => Request::patch(url.as_str()),
            Method::Delete => Request::delete(url.as_str()),
        };
        let builder = builder.headers(headers_of(&headers));
        // `gloo-net` splits here: a request with a body is built by `body`, one
        // without by `build`. Both hand back a `Request`, which is what sends.
        let request = match body {
            Some(body) => builder
                .body(body)
                .map_err(|e| Error::Transport(format!("body: {e:?}")))?,
            None => builder
                .build()
                .map_err(|e| Error::Transport(format!("{method:?} {url}: {e}")))?,
        };
        let response = request
            .send()
            .await
            .map_err(|e| Error::Transport(format!("{method:?} {url}: {e}")))?;
        let status = response.status();
        let retry_after_s = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.parse::<u32>().ok());
        let body = response
            .binary()
            .await
            .map_err(|e| Error::Transport(format!("{method:?} {url}: {e}")))?;
        Ok(HttpResponse {
            status,
            retry_after_s,
            body,
        })
    }
}

fn headers_of(entries: &[(String, String)]) -> Headers {
    let headers = Headers::new();
    for (name, value) in entries {
        headers.set(name, value);
    }
    headers
}

/// The browser instance handle: the shared endpoint implementation over [`GlooTransport`].
pub type BrowserInstance = RestInstance<GlooTransport>;

impl RestInstance<GlooTransport> {
    /// Builds a transport for `instance` authenticated as `token`.
    #[must_use]
    pub fn new(instance: &Instance, token: impl Into<String>) -> Self {
        Self::with_transport(instance, token, GlooTransport::new())
    }

    /// Registers the callback that receives each stream frame's raw JSON text.
    ///
    /// A browser socket cannot hand a Rust channel back through
    /// [`Streaming::subscribe`](crate::api::Streaming::subscribe), so the host
    /// supplies a JS function instead. The sink is read by every socket when a
    /// frame arrives, so registering it after a subscription takes effect for
    /// that subscription too.
    #[must_use]
    pub fn with_stream_sink(self, callback: js_sys::Function) -> Self {
        self.transport().shared.0.borrow_mut().sink = Some(callback);
        self
    }
}
