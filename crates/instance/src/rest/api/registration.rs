//! The approval queue over the REST transport.
//!
//! Invites are not here on purpose: Mastodon serves them only to its own web UI.

use crate::actions::RegistrationActions;
use crate::model::AdminAccount;
use crate::rest::{RestInstance, Transport};
use crate::{Error, Result};

impl<T: Transport> RegistrationActions for RestInstance<T> {
    type Error = Error;

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
