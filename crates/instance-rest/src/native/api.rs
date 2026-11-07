//! `InstanceApi` for the `reqwest` transport: every endpoint Mastodon spells.

use nodus_instance_api::api::InstanceApi;
use nodus_instance_api::model::{
    Account, Context, Draft, LocalId, Notification, Page, PageParams, Relationship, Status,
    StreamKind,
};
use nodus_instance_api::{Error, Result};

use super::HttpInstance;

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
