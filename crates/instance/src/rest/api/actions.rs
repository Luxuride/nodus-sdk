//! The action traits for every transport, written once.
//!
//! Split from [`api`](super::api) because it answers a different question: that file
//! describes what the SDK *reads*, this one what a user *does*. Both are implemented
//! for the same `RestInstance<T>`, so a transport still has to write nothing.

use crate::actions::{AccountActions, Browse, Search, StatusActions};
use crate::model::{
    Conversation, Draft, HistoryEntry, List, LocalId, Page, PageParams, Relationship,
    SearchResults, Status,
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

    async fn delete_status(&self, id: &str) -> Result<()> {
        let url = self.url(&format!("/api/v1/statuses/{id}"))?;
        let _: Status = self.fetch(self.delete(url)).await?;
        Ok(())
    }

    async fn edit_status(&self, id: &str, draft: &Draft) -> Result<Status> {
        let url = self.url(&format!("/api/v1/statuses/{id}"))?;
        let form = [
            ("status", draft.text.clone()),
            ("visibility", draft.visibility.to_string()),
        ];
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

    async fn reblogged_by(&self, id: &str, params: PageParams) -> Result<Page<crate::model::Account>> {
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

    async fn follow(&self, id: &LocalId) -> Result<Relationship> {
        self.account_verb(&id.0, "follow").await
    }

    async fn unfollow(&self, id: &LocalId) -> Result<Relationship> {
        self.account_verb(&id.0, "unfollow").await
    }

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

    async fn followers(&self, id: &LocalId, params: PageParams) -> Result<Page<crate::model::Account>> {
        let url = self.page_url(&format!("/api/v1/accounts/{}/followers", id.0), &params)?;
        self.fetch_page(self.get(url), &params).await
    }

    async fn following(&self, id: &LocalId, params: PageParams) -> Result<Page<crate::model::Account>> {
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
