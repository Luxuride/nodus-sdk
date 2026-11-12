//! The transport seam shared by every HTTP adapter.
//!
//! A protocol adapter builds a [`HttpRequest`]; a platform crate sends it with a
//! [`Transport`]. Keeping the seam here means a Misskey client does not have to
//! depend on the Mastodon crate just to own a `reqwest` client.

use core::future::Future;

use url::Url;

use crate::Result;

/// The verbs the instance surfaces use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// Read.
    Get,
    /// Write.
    Post,
    /// Replace.
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
#[derive(Debug, Clone)]
pub struct HttpResponse {
    /// HTTP status.
    pub status: u16,
    /// `Retry-After`, when the server sent one.
    pub retry_after_s: Option<u32>,
    /// Body bytes.
    pub body: Vec<u8>,
}

/// The seam between a protocol adapter and the platform.
pub trait Transport: Clone + 'static {
    /// Sends one request.
    fn send(&self, request: HttpRequest) -> impl Future<Output = Result<HttpResponse>>;
}
