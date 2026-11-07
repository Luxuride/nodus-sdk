//! Moderation over the REST transport.
//!
//! The paths are `/api/v2/admin` where Mastodon moved reports there and `/api/v1/admin`
//! elsewhere; both are stable, and a client that guessed would fail only for the
//! moderator — who is exactly the person least able to test an alternative.

use crate::actions::AdminActions;
use crate::model::{
    AdminAccount, DomainBlock, DomainSeverity, ModerationAction, Report, ReportState,
};
use crate::rest::{RestInstance, Transport};
use crate::{Error, Result};

impl<T: Transport> AdminActions for RestInstance<T> {
    type Error = Error;

    async fn reports(&self, state: ReportState, limit: u16) -> Result<Vec<Report>> {
        let url = self.url(&format!(
            "/api/v2/admin/reports?state={}&limit={limit}",
            state.as_str()
        ))?;
        self.fetch(self.get(url)).await
    }

    async fn resolve_report(&self, id: &str) -> Result<Report> {
        let url = self.url(&format!("/api/v2/admin/reports/{id}/resolve"))?;
        self.fetch(self.post(url)).await
    }

    async fn reopen_report(&self, id: &str) -> Result<Report> {
        let url = self.url(&format!("/api/v2/admin/reports/{id}/reopen"))?;
        self.fetch(self.post(url)).await
    }

    async fn admin_accounts(&self, local: bool, limit: u16) -> Result<Vec<AdminAccount>> {
        let url = self.url(&format!(
            "/api/v1/admin/accounts?local={local}&limit={limit}"
        ))?;
        self.fetch(self.get(url)).await
    }

    async fn moderate_account(
        &self,
        id: &str,
        action: ModerationAction,
    ) -> Result<AdminAccount> {
        let url = self.url(&format!("/api/v1/admin/accounts/{id}/action"))?;
        let form = vec![("type", action.as_str().to_owned())];
        self.fetch(self.post_form(url, &form)).await
    }

    async fn domain_blocks(&self, limit: u16) -> Result<Vec<DomainBlock>> {
        let url = self.url(&format!("/api/v1/admin/domain_blocks?limit={limit}"))?;
        self.fetch(self.get(url)).await
    }

    async fn create_domain_block(
        &self,
        domain: &str,
        severity: DomainSeverity,
    ) -> Result<DomainBlock> {
        let url = self.url("/api/v1/admin/domain_blocks")?;
        let form = vec![
            ("domain", domain.to_owned()),
            ("severity", severity.as_str().to_owned()),
        ];
        self.fetch(self.post_form(url, &form)).await
    }

    async fn delete_domain_block(&self, id: &str) -> Result<()> {
        let url = self.url(&format!("/api/v1/admin/domain_blocks/{id}"))?;
        // The body of a delete is empty, so it cannot be decoded as the removed block.
        let _: serde_json::Value = self.fetch(self.delete(url)).await?;
        Ok(())
    }
}
