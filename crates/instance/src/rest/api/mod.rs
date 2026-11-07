//! `InstanceApi` for every transport, written once.

mod actions;
mod meta;
mod admin;
mod filters;
mod highlights;
mod schedule;
mod settings;

use crate::api::InstanceApi;
use crate::model::{
    Account, Context, Draft, LocalId, Notification, Page, PageParams, Relationship, Status,
    StreamKind,
};
use crate::rest::{RestInstance, Transport};
use crate::{Error, Result};

impl<T: Transport> InstanceApi for RestInstance<T> {
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
        let mut url = self.url("/api/v1/accounts/relationships")?;
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
        for media_id in &draft.media_ids {
            form.push(("media_ids[]", media_id.clone()));
        }
        if !draft.spoiler_text.is_empty() {
            form.push(("spoiler_text", draft.spoiler_text.clone()));
        }
        if draft.sensitive {
            form.push(("sensitive", "true".to_owned()));
        }
        let mut request = self.post_form(url, &form);
        // Idempotency is what stops a retried compose from posting twice; a draft
        // without a key is a caller bug, not something to paper over here.
        if let Some(key) = &draft.idempotency_key {
            request
                .headers
                .push(("Idempotency-Key".to_owned(), key.clone()));
        }
        self.fetch(request).await
    }

    async fn delete_status(&self, id: &str) -> Result<()> {
        let url = self.url(&format!("/api/v1/statuses/{id}"))?;
        let _: serde_json::Value = self.fetch(self.delete(url)).await?;
        Ok(())
    }
}

#[cfg(feature = "native")]
mod sync_impl {
    use crate::Result;
    use crate::model::{Account, LocalId, Page, PageParams, Status};
    use crate::rest::{RestInstance, Transport};
    use crate::sync_api::SyncApi;

    impl<T: Transport> SyncApi for RestInstance<T> {
        type Error = crate::Error;

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
}
