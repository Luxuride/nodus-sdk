//! The streaming socket: the `wss://` URL and the frames it delivers.

use url::Url;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{BinaryType, MessageEvent, WebSocket};

use crate::api::Streaming;
use crate::model::StreamKind;
use crate::rest::RestInstance;
use crate::wasm::client::{ActiveStream, GlooTransport};
use crate::{Error, Result};

/// The wire spelling of a streaming timeline.
const fn stream_name(kind: StreamKind) -> &'static str {
    match kind {
        StreamKind::User => "user",
        StreamKind::Local => "local",
        StreamKind::Federated => "federated",
    }
}

impl Streaming for RestInstance<GlooTransport> {
    type Error = Error;

    fn subscribe(&self, kind: StreamKind) -> impl Future<Output = Result<()>> {
        core::future::ready(self.open_stream(kind))
    }
}

impl RestInstance<GlooTransport> {
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
            query.append_pair("access_token", self.token().expose());
        }
        Ok(url)
    }

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
        let shared = self.transport().shared().clone();
        let for_sink = shared.clone();
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            let Some(sink) = for_sink.0.borrow().sink.clone() else {
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
        shared.0.borrow_mut().streams.push(stream);
        Ok(())
    }
}
