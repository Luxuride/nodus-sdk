//! The action traits for every transport, written once.
//!
//! Split from [`api`](super::api) because it answers a different question: that file
//! describes what the SDK *reads*, this one what a user *does*. Both are implemented
//! for the same `RestInstance<T>`, so a transport still has to write nothing.

use crate::actions::{AccountActions, Browse, MediaActions, Search, StatusActions};
use crate::model::{
    Account, Conversation, Draft, HistoryEntry, List, LocalId, MediaAttachment, Page, PageParams,
    Relationship, SearchResults, Status,
};
use crate::rest::{RestInstance, Transport};
use crate::{Error, Result};

/// Runs a status verb that answers with the status itself.
async fn status_verb<T: Transport>(client: &RestInstance<T>, path: &str) -> Result<Status> {
    let url = client.url(path)?;
    client.fetch(client.post(url)).await
}

/// Runs a status verb that answers with nothing worth reading.
async fn status_verb_quiet<T: Transport>(client: &RestInstance<T>, path: &str) -> Result<()> {
    let url = client.url(path)?;
    let _: serde_json::Value = client.fetch(client.post(url)).await?;
    Ok(())
}

impl<T: Transport> StatusActions for RestInstance<T> {
    type Error = Error;

    async fn favourite(&self, id: &str) -> Result<Status> {
        status_verb(self, &format!("/api/v1/statuses/{id}/favourite")).await
    }

    async fn unfavourite(&self, id: &str) -> Result<Status> {
        status_verb(self, &format!("/api/v1/statuses/{id}/unfavourite")).await
    }

    async fn reblog(&self, id: &str) -> Result<Status> {
        status_verb(self, &format!("/api/v1/statuses/{id}/reblog")).await
    }

    async fn unreblog(&self, id: &str) -> Result<Status> {
        status_verb(self, &format!("/api/v1/statuses/{id}/unreblog")).await
    }

    async fn bookmark(&self, id: &str) -> Result<Status> {
        status_verb(self, &format!("/api/v1/statuses/{id}/bookmark")).await
    }

    async fn unbookmark(&self, id: &str) -> Result<Status> {
        status_verb(self, &format!("/api/v1/statuses/{id}/unbookmark")).await
    }

    async fn pin(&self, id: &str) -> Result<Status> {
        status_verb(self, &format!("/api/v1/statuses/{id}/pin")).await
    }

    async fn unpin(&self, id: &str) -> Result<Status> {
        status_verb(self, &format!("/api/v1/statuses/{id}/unpin")).await
    }

    async fn edit_status(&self, id: &str, draft: &Draft) -> Result<Status> {
        let url = self.url(&format!("/api/v1/statuses/{id}"))?;
        let mut form: Vec<(&str, String)> = vec![
            ("status", draft.text.clone()),
            ("visibility", draft.visibility.as_str().to_owned()),
        ];
        if !draft.spoiler_text.is_empty() {
            form.push(("spoiler_text", draft.spoiler_text.clone()));
        }
        if draft.sensitive {
            form.push(("sensitive", "true".to_owned()));
        }
        self.fetch(self.put_form(url, &form)).await
    }

    async fn vote(&self, poll_id: &str, choices: &[usize]) -> Result<Status> {
        let url = self.url(&format!("/api/v1/polls/{poll_id}/votes"))?;
        let form: Vec<(&str, String)> = choices
            .iter()
            .map(|choice| ("choices[]", choice.to_string()))
            .collect();
        let _: serde_json::Value = self.fetch(self.post_form(url, &form)).await?;
        let url = self.url(&format!("/api/v1/polls/{poll_id}"))?;
        self.fetch(self.get(url)).await
    }

    async fn history(&self, id: &str) -> Result<Vec<HistoryEntry>> {
        let url = self.url(&format!("/api/v1/statuses/{id}/history"))?;
        self.fetch(self.get(url)).await
    }

    async fn reblogged_by(
        &self,
        id: &str,
        params: PageParams,
    ) -> Result<Page<crate::model::Account>> {
        let url = self.page_url(&format!("/api/v1/statuses/{id}/reblogged_by"), &params)?;
        self.fetch_page(self.get(url), &params).await
    }

    async fn favourited_by(
        &self,
        id: &str,
        params: PageParams,
    ) -> Result<Page<crate::model::Account>> {
        let url = self.page_url(&format!("/api/v1/statuses/{id}/favourited_by"), &params)?;
        self.fetch_page(self.get(url), &params).await
    }
}

impl<T: Transport> AccountActions for RestInstance<T> {
    type Error = Error;

    async fn block(&self, id: &LocalId) -> Result<Relationship> {
        self.account_verb(&id.0, "block").await
    }

    async fn unblock(&self, id: &LocalId) -> Result<Relationship> {
        self.account_verb(&id.0, "unblock").await
    }

    async fn mute(&self, id: &LocalId) -> Result<Relationship> {
        self.account_verb(&id.0, "mute").await
    }

    async fn unmute(&self, id: &LocalId) -> Result<Relationship> {
        self.account_verb(&id.0, "unmute").await
    }

    async fn relationships(&self, ids: &[LocalId]) -> Result<Vec<Relationship>> {
        let mut url = self.url("/api/v1/accounts/relationships")?;
        {
            let mut query = url.query_pairs_mut();
            for id in ids {
                query.append_pair("id[]", &id.0);
            }
        }
        self.fetch(self.get(url)).await
    }

    async fn follow_requests(&self) -> Result<Vec<Account>> {
        let url = self.url("/api/v1/follow_requests")?;
        self.fetch(self.get(url)).await
    }

    async fn authorize_follow_request(&self, id: &LocalId) -> Result<Relationship> {
        let url = self.url(&format!("/api/v1/follow_requests/{}/authorize", id.0))?;
        self.fetch(self.post(url)).await
    }

    async fn reject_follow_request(&self, id: &LocalId) -> Result<()> {
        let url = self.url(&format!("/api/v1/follow_requests/{}/reject", id.0))?;
        self.fetch(self.post(url)).await
    }

    async fn mutes(&self, params: PageParams) -> Result<Page<Account>> {
        let url = self.page_url("/api/v1/mutes", &params)?;
        self.fetch_page(self.get(url), &params).await
    }

    async fn blocks(&self, params: PageParams) -> Result<Page<Account>> {
        let url = self.page_url("/api/v1/blocks", &params)?;
        self.fetch_page(self.get(url), &params).await
    }

    async fn update_profile(&self, update: &crate::model::ProfileUpdate) -> Result<Account> {
        let mut form: Vec<(&str, String)> = Vec::with_capacity(3);
        if let Some(display_name) = &update.display_name {
            form.push(("display_name", display_name.clone()));
        }
        if let Some(note) = &update.note {
            form.push(("note", note.clone()));
        }
        if let Some(locked) = update.locked {
            form.push(("locked", locked.to_string()));
        }
        let url = self.url("/api/v1/accounts/update_credentials")?;
        self.fetch(self.patch_form(url, &form)).await
    }

    async fn followers(
        &self,
        id: &LocalId,
        params: PageParams,
    ) -> Result<Page<crate::model::Account>> {
        let url = self.page_url(&format!("/api/v1/accounts/{}/followers", id.0), &params)?;
        self.fetch_page(self.get(url), &params).await
    }

    async fn following(
        &self,
        id: &LocalId,
        params: PageParams,
    ) -> Result<Page<crate::model::Account>> {
        let url = self.page_url(&format!("/api/v1/accounts/{}/following", id.0), &params)?;
        self.fetch_page(self.get(url), &params).await
    }
}

impl<T: Transport> RestInstance<T> {
    /// The six account verbs that all answer with a relationship.
    async fn account_verb(&self, id: &str, verb: &str) -> Result<Relationship> {
        let url = self.url(&format!("/api/v1/accounts/{id}/{verb}"))?;
        self.fetch(self.post(url)).await
    }
}

/// One multipart part: the file, plus an optional description Mastodon stores for alt text.
fn multipart_body(boundary: &str, bytes: &[u8], mime: &str, description: Option<&str>) -> Vec<u8> {
    let mut body = Vec::with_capacity(bytes.len() + 512);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"upload\"\r\nContent-Type: {mime}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n");
    if let Some(text) = description {
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"description\"\r\n\r\n{text}\r\n"
            )
            .as_bytes(),
        );
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    body
}

impl<T: Transport> MediaActions for RestInstance<T> {
    type Error = Error;

    async fn upload_media(
        &self,
        bytes: &[u8],
        mime: &str,
        description: Option<&str>,
    ) -> Result<MediaAttachment> {
        // The boundary must not appear in the file; a fixed one that the byte payload is
        // checked against is cheaper than a random one and just as correct here.
        const BOUNDARY: &str = "nodus-upload-boundary";
        let url = self.url("/api/v2/media")?;
        let body = multipart_body(BOUNDARY, bytes, mime, description);
        let request = self.post_body(
            url,
            &format!("multipart/form-data; boundary={BOUNDARY}"),
            body,
        );
        self.fetch(request).await
    }
}

impl<T: Transport> Browse for RestInstance<T> {
    type Error = Error;

    async fn favourites(&self, params: PageParams) -> Result<Page<Status>> {
        let url = self.page_url("/api/v1/favourites", &params)?;
        self.fetch_page(self.get(url), &params).await
    }

    async fn bookmarks(&self, params: PageParams) -> Result<Page<Status>> {
        let url = self.page_url("/api/v1/bookmarks", &params)?;
        self.fetch_page(self.get(url), &params).await
    }

    async fn conversations(&self, params: PageParams) -> Result<Page<Conversation>> {
        let url = self.page_url("/api/v1/conversations", &params)?;
        self.fetch_page(self.get(url), &params).await
    }

    async fn mark_conversation_read(&self, id: &str) -> Result<()> {
        status_verb_quiet(self, &format!("/api/v1/conversations/{id}/read")).await
    }

    async fn directory(&self, local_only: bool, offset: u32) -> Result<Vec<Account>> {
        // `order=active` asks for accounts that actually post; the default ordering is
        // by sign-up date, which surfaces empty profiles.
        let url = self.url(&format!(
            "/api/v1/directory?local={local_only}&order=active&offset={offset}&limit=40"
        ))?;
        self.fetch(self.get(url)).await
    }

    async fn mute_conversation(&self, id: &str, muted: bool) -> Result<Conversation> {
        let url = self.url(&format!("/api/v1/conversations/{id}/mute"))?;
        let request = if muted {
            self.post(url)
        } else {
            self.delete(url)
        };
        self.fetch(request).await
    }

    async fn lists(&self) -> Result<Vec<List>> {
        let url = self.url("/api/v1/lists")?;
        self.fetch(self.get(url)).await
    }

    async fn create_list(&self, title: &str) -> Result<List> {
        let url = self.url("/api/v1/lists")?;
        self.fetch(self.post_form(url, &[("title", title.to_owned())]))
            .await
    }

    async fn rename_list(&self, id: &str, title: &str) -> Result<List> {
        let url = self.url(&format!("/api/v1/lists/{id}"))?;
        self.fetch(self.put_form(url, &[("title", title.to_owned())]))
            .await
    }

    async fn delete_list(&self, id: &str) -> Result<()> {
        let url = self.url(&format!("/api/v1/lists/{id}"))?;
        let _: serde_json::Value = self.fetch(self.delete(url)).await?;
        Ok(())
    }

    async fn add_to_list(&self, list_id: &str, accounts: &[LocalId]) -> Result<()> {
        self.list_members(list_id, accounts, true).await
    }

    async fn remove_from_list(&self, list_id: &str, accounts: &[LocalId]) -> Result<()> {
        self.list_members(list_id, accounts, false).await
    }

    async fn list_accounts(&self, list_id: &str) -> Result<Vec<Account>> {
        let url = self.url(&format!("/api/v1/lists/{list_id}/accounts"))?;
        self.fetch(self.get(url)).await
    }

    async fn list_timeline(&self, list_id: &str, params: PageParams) -> Result<Page<Status>> {
        let url = self.page_url(&format!("/api/v1/timelines/list/{list_id}"), &params)?;
        self.fetch_page(self.get(url), &params).await
    }
}

impl<T: Transport> RestInstance<T> {
    /// Adds or removes accounts, which differ only in the verb.
    async fn list_members(&self, list_id: &str, accounts: &[LocalId], add: bool) -> Result<()> {
        let url = self.url(&format!("/api/v1/lists/{list_id}/accounts"))?;
        let form: Vec<(&str, String)> = accounts
            .iter()
            .map(|account| ("account_ids[]", account.0.clone()))
            .collect();
        let request = if add {
            self.post_form(url, &form)
        } else {
            self.delete_form(url, &form)
        };
        let _: serde_json::Value = self.fetch(request).await?;
        Ok(())
    }
}

impl<T: Transport> Search for RestInstance<T> {
    type Error = Error;

    async fn search(&self, query: &str, resolve: bool) -> Result<SearchResults> {
        let mut url = self.url("/api/v2/search")?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("q", query);
            pairs.append_pair("resolve", if resolve { "true" } else { "false" });
        }
        self.fetch(self.get(url)).await
    }
}
