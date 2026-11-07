//! Invites and approvals over the REST transport.

use crate::actions::RegistrationActions;
use crate::model::{AdminAccount, Invite};
use crate::rest::{RestInstance, Transport};
use crate::{Error, Result};

impl<T: Transport> RegistrationActions for RestInstance<T> {
    type Error = Error;

    async fn invites(&self, limit: u16) -> Result<Vec<Invite>> {
        let url = self.url(&format!("/api/v1/invites?limit={limit}"))?;
        self.fetch(self.get(url)).await
    }

    async fn create_invite(
        &self,
        max_uses: Option<u32>,
        expires_in: Option<u64>,
        autofollow: bool,
    ) -> Result<Invite> {
        let url = self.url("/api/v1/invites")?;
        let mut form = vec![("autofollow", autofollow.to_string())];
        if let Some(max_uses) = max_uses {
            form.push(("max_uses", max_uses.to_string()));
        }
        if let Some(expires_in) = expires_in {
            form.push(("expires_in", expires_in.to_string()));
        }
        self.fetch(self.post_form(url, &form)).await
    }

    async fn revoke_invite(&self, id: &str) -> Result<()> {
        let url = self.url(&format!("/api/v1/invites/{id}"))?;
        self.fetch(self.delete(url)).await
    }

    async fn pending_accounts(&self, limit: u16) -> Result<Vec<AdminAccount>> {
        // `status=pending` is only meaningful on a server that requires approval; one that
        // does not answers with an empty list, which is the correct answer here.
        let url = self.url(&format!(
            "/api/v1/admin/accounts?local=true&status=pending&limit={limit}"
        ))?;
        self.fetch(self.get(url)).await
    }

    async fn approve_account(&self, id: &str) -> Result<AdminAccount> {
        self.decide(id, "approve").await
    }

    async fn reject_account(&self, id: &str) -> Result<AdminAccount> {
        self.decide(id, "reject").await
    }
}

impl<T: Transport> RestInstance<T> {
    /// Approve and reject differ only in the last path segment.
    async fn decide(&self, id: &str, decision: &str) -> Result<AdminAccount> {
        let url = self.url(&format!("/api/v1/admin/accounts/{id}/{decision}"))?;
        self.fetch(self.post(url)).await
    }
}
