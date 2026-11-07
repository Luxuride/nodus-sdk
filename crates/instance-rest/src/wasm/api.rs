//! `InstanceApi` for the browser transport: every endpoint Mastodon spells.

use gloo_net::http::Request;

use nodus_instance_api::api::InstanceApi;
use nodus_instance_api::model::{
    Account, Context, Draft, LocalId, Notification, Page, PageParams, Relationship, Status,
    StreamKind,
};
use nodus_instance_api::{Error, Result};

use super::BrowserInstance;
use super::request::{fetch, fetch_page, send};

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
