//! `SyncApi` for the `reqwest` transport: the pages the crawler walks.

use nodus_instance_api::model::{Account, LocalId, Page, PageParams, Status};
use nodus_instance_api::sync_api::SyncApi;
use nodus_instance_api::{Error, Result};

use super::HttpInstance;

impl SyncApi for HttpInstance {
    type Error = Error;

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
