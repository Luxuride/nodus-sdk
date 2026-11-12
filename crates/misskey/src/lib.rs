//! Misskey's API, mapped onto the same [`InstanceApi`] the Mastodon crate implements.
//!
//! Misskey is not a Mastodon dialect: every call is a JSON POST, ids are opaque
//! strings, visibility has five values, and notes carry MFM rather than HTML. The
//! mapping lives here so the SDK state machine and the views never branch on the
//! server software.

#![allow(
    clippy::needless_pass_by_value,
    clippy::unnecessary_wraps,
    clippy::format_push_string,
    clippy::cast_possible_truncation,
    clippy::unused_self,
    reason = "Misskey wire values are consumed by the mapper, and the port signatures are the Mastodon ones"
)]

pub mod convert;

use core::fmt;

use nodus_client_core::model::{
    Account, Context, CredentialAccount, Draft, LocalId, Notification, Page, PageParams,
    Relationship, Status, StreamKind,
};
use nodus_client_core::rest::{HttpRequest, HttpResponse, Method, Transport};
use nodus_client_core::{Error, Instance, Result};
use serde_json::{Value, json};
use url::Url;

pub use convert::note_html;

/// One Misskey instance, one token, one transport.
pub struct MisskeyInstance<T: Transport> {
    base: Url,
    token: String,
    transport: T,
}

impl<T: Transport> fmt::Debug for MisskeyInstance<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MisskeyInstance")
            .field("base", &self.base.as_str())
            .field("token", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl<T: Transport> Clone for MisskeyInstance<T> {
    fn clone(&self) -> Self {
        Self {
            base: self.base.clone(),
            token: self.token.clone(),
            transport: self.transport.clone(),
        }
    }
}

impl<T: Transport> MisskeyInstance<T> {
    /// Build from an already-parsed instance and a transport.
    #[must_use]
    pub fn with_transport(instance: &Instance, token: impl Into<String>, transport: T) -> Self {
        Self {
            base: instance.base.clone(),
            token: token.into(),
            transport,
        }
    }

    /// The instance this client talks to.
    #[must_use]
    pub const fn base(&self) -> &Url {
        &self.base
    }

    /// POST one JSON call. Misskey has no GET endpoints.
    async fn call(&self, path: &str, body: Value) -> Result<Value> {
        let url = self.url(path)?;
        let mut headers = vec![("Content-Type".to_owned(), "application/json".to_owned())];
        if !self.token.is_empty() {
            headers.push(("Authorization".to_owned(), format!("Bearer {}", self.token)));
        }
        let request = HttpRequest {
            method: Method::Post,
            url,
            headers,
            body: Some(body.to_string().into_bytes()),
        };
        let response = self.transport.send(request).await?;
        self.finish(response)
    }

    fn finish(&self, response: HttpResponse) -> Result<Value> {
        match response.status {
            200..=299 => serde_json::from_slice(&response.body)
                .map_err(|error| Error::Decode(error.to_string())),
            401 | 403 => Err(Error::Unauthorized),
            429 => Err(Error::RateLimited {
                retry_after_s: response.retry_after_s.unwrap_or(300),
            }),
            status => {
                let body = String::from_utf8_lossy(&response.body);
                let message = serde_json::from_slice::<Value>(&response.body)
                    .ok()
                    .and_then(|value| {
                        value
                            .pointer("/error/message")
                            .or_else(|| value.get("message"))
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    })
                    .unwrap_or_else(|| body.chars().take(200).collect());
                Err(Error::Status { status, message })
            }
        }
    }

    fn url(&self, path: &str) -> Result<Url> {
        let joined = format!("{}{path}", self.base.as_str().trim_end_matches('/'));
        Url::parse(&joined).map_err(|error| Error::Transport(format!("{joined}: {error}")))
    }

    fn body(params: &PageParams) -> Value {
        let mut body = serde_json::Map::new();
        if let Some(limit) = params.limit {
            body.insert("limit".to_owned(), json!(limit));
        }
        if let Some(until) = &params.max_id {
            body.insert("untilId".to_owned(), json!(until));
        }
        if let Some(since) = &params.since_id {
            body.insert("sinceId".to_owned(), json!(since));
        }
        Value::Object(body)
    }

    async fn page<U>(
        &self,
        path: &str,
        params: &PageParams,
        map: impl Fn(Value) -> Option<U>,
    ) -> Result<Page<U>> {
        let value = self.call(path, Self::body(params)).await?;
        let array = value.as_array().cloned().unwrap_or_default();
        let items: Vec<U> = array.into_iter().filter_map(map).collect();
        let next = items_next(&value);
        Ok(Page::new(items).with_next(next))
    }

    async fn relation(&self, id: &LocalId) -> Result<Relationship> {
        let value = self
            .call("/api/users/relation", json!({ "userId": id.0 }))
            .await?;
        convert::relationship(&value, id)
            .ok_or_else(|| Error::Decode("a relation without the other account".to_owned()))
    }
}

fn items_next(value: &Value) -> Option<String> {
    value
        .as_array()
        .and_then(|items| items.last())
        .and_then(|item| item.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

impl<T: Transport> nodus_client_core::InstanceApi for MisskeyInstance<T> {
    type Error = Error;

    async fn verify_credentials(&self) -> Result<Account> {
        let value = self.call("/api/i", json!({})).await?;
        convert::account(&value).ok_or_else(|| Error::Decode("i is not an account".to_owned()))
    }

    async fn credential_account(&self) -> Result<CredentialAccount> {
        let value = self.call("/api/i", json!({})).await?;
        let account = convert::account(&value)
            .ok_or_else(|| Error::Decode("i is not an account".to_owned()))?;
        Ok(CredentialAccount {
            account,
            source: convert::source(&value),
        })
    }

    async fn timeline(&self, kind: StreamKind, params: PageParams) -> Result<Page<Status>> {
        let path = match kind {
            StreamKind::User => "/api/notes/timeline",
            StreamKind::Local => "/api/notes/local-timeline",
            StreamKind::Federated => "/api/notes/global-timeline",
        };
        self.page(path, &params, convert::status).await
    }

    async fn tag_timeline(&self, tag: &str, params: PageParams) -> Result<Page<Status>> {
        let mut body = Self::body(&params);
        body["tag"] = json!(tag);
        let value = self.call("/api/notes/search", body).await?;
        let items = value
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(convert::status)
            .collect();
        Ok(Page::new(items).with_next(items_next(&value)))
    }

    async fn status(&self, id: &str) -> Result<Status> {
        let value = self
            .call("/api/notes/show", json!({ "noteId": id }))
            .await?;
        convert::status(value).ok_or_else(|| Error::Decode("note is not a status".to_owned()))
    }

    async fn status_context(&self, id: &str) -> Result<Context> {
        let conversation = self
            .call(
                "/api/notes/conversation",
                json!({ "noteId": id, "limit": 40 }),
            )
            .await
            .unwrap_or(Value::Array(Vec::new()));
        let children = self
            .call("/api/notes/children", json!({ "noteId": id, "limit": 40 }))
            .await
            .unwrap_or(Value::Array(Vec::new()));
        Ok(Context {
            ancestors: conversation
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(convert::status)
                .collect(),
            descendants: children
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(convert::status)
                .collect(),
        })
    }

    async fn notifications(&self, params: PageParams) -> Result<Page<Notification>> {
        let value = self
            .call("/api/i/notifications", Self::body(&params))
            .await?;
        let items = value
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(convert::notification)
            .collect();
        Ok(Page::new(items).with_next(items_next(&value)))
    }

    async fn account(&self, id: &LocalId) -> Result<Account> {
        let value = self
            .call("/api/users/show", json!({ "userId": id.0 }))
            .await?;
        convert::account(&value).ok_or_else(|| Error::Decode("user is not an account".to_owned()))
    }

    async fn account_lookup(&self, acct: &str) -> Result<Account> {
        let (username, host) = acct
            .split_once('@')
            .map_or((acct, None), |(user, host)| (user, Some(host)));
        let mut body = json!({ "username": username });
        if let Some(host) = host {
            body["host"] = json!(host);
        }
        let value = self.call("/api/users/show", body).await?;
        convert::account(&value).ok_or_else(|| Error::Decode("user is not an account".to_owned()))
    }

    async fn account_followers(&self, id: &LocalId, params: PageParams) -> Result<Page<Account>> {
        let mut body = Self::body(&params);
        body["userId"] = json!(id.0);
        let value = self.call("/api/users/followers", body).await?;
        let items = value
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| convert::account(value.get("follower").unwrap_or(&value)))
            .collect();
        Ok(Page::new(items).with_next(items_next(&value)))
    }

    async fn account_following(&self, id: &LocalId, params: PageParams) -> Result<Page<Account>> {
        let mut body = Self::body(&params);
        body["userId"] = json!(id.0);
        let value = self.call("/api/users/following", body).await?;
        let items = value
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| convert::account(value.get("followee").unwrap_or(&value)))
            .collect();
        Ok(Page::new(items).with_next(items_next(&value)))
    }

    async fn account_statuses(&self, id: &LocalId, params: PageParams) -> Result<Page<Status>> {
        let mut body = Self::body(&params);
        body["userId"] = json!(id.0);
        let value = self.call("/api/users/notes", body).await?;
        let items = value
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(convert::status)
            .collect();
        Ok(Page::new(items).with_next(items_next(&value)))
    }

    async fn relationship(&self, id: &LocalId) -> Result<Relationship> {
        self.relation(id).await
    }

    async fn follow(&self, id: &LocalId) -> Result<Relationship> {
        self.call("/api/following/create", json!({ "userId": id.0 }))
            .await?;
        self.relation(id).await
    }

    async fn unfollow(&self, id: &LocalId) -> Result<Relationship> {
        self.call("/api/following/delete", json!({ "userId": id.0 }))
            .await?;
        self.relation(id).await
    }

    async fn post_status(&self, draft: &Draft) -> Result<Status> {
        let mut body = json!({
            "text": draft.text,
            "cw": if draft.spoiler_text.is_empty() { Value::Null } else { json!(draft.spoiler_text) },
            "visibility": convert::visibility_out(draft.visibility),
        });
        if let Some(reply) = &draft.in_reply_to_id {
            body["replyId"] = json!(reply);
        }
        if !draft.media_ids.is_empty() {
            body["fileIds"] = json!(draft.media_ids);
        }
        if let Some(poll) = &draft.poll {
            body["poll"] = json!({
                "choices": poll.options,
                "multiple": poll.multiple,
                "expiresAt": (chrono_now_ms() + u64::from(poll.expires_in) * 1000),
            });
        }
        let value = self.call("/api/notes/create", body).await?;
        let note = value.get("createdNote").cloned().unwrap_or(value);
        convert::status(note)
            .ok_or_else(|| Error::Decode("created note is not a status".to_owned()))
    }

    async fn delete_status(&self, id: &str) -> Result<()> {
        self.call("/api/notes/delete", json!({ "noteId": id }))
            .await
            .map(|_| ())
    }
}

fn chrono_now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}
