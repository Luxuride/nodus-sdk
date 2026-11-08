//! Moderation over the REST transport.
//!
//! Mastodon splits moderation across two prefixes, and the split is not by version:
//! reports, accounts, blocks, announcements, relays and domain allows live under
//! `/api/v1/admin`, while measures (the dashboard aggregates) live under
//! `/api/v2/admin`. A wrong prefix does not answer 404 as JSON — it falls through to the
//! instance's HTML error page, which reads like "this account cannot moderate" and cost
//! a debugging session, so each path below is spelled out rather than derived.

use std::fmt::Write as _;

use crate::actions::AdminActions;
use crate::model::{
    AdminAccount, AdminMeasure, Announcement, DomainAllow, DomainBlock, DomainSeverity,
    ModerationAction, Relay, Report, ReportState,
};
use crate::rest::{RestInstance, Transport};
use crate::{Error, Result};

impl<T: Transport> AdminActions for RestInstance<T> {
    type Error = Error;

    async fn reports(&self, state: ReportState, limit: u16) -> Result<Vec<Report>> {
        let url = self.url(&format!(
            "/api/v1/admin/reports?state={}&limit={limit}",
            state.as_str()
        ))?;
        self.fetch(self.get(url)).await
    }

    async fn resolve_report(&self, id: &str) -> Result<Report> {
        let url = self.url(&format!("/api/v1/admin/reports/{id}/resolve"))?;
        self.fetch(self.post(url)).await
    }

    async fn reopen_report(&self, id: &str) -> Result<Report> {
        let url = self.url(&format!("/api/v1/admin/reports/{id}/reopen"))?;
        self.fetch(self.post(url)).await
    }

    async fn admin_accounts(&self, local: bool, limit: u16) -> Result<Vec<AdminAccount>> {
        let url = self.url(&format!(
            "/api/v1/admin/accounts?local={local}&limit={limit}"
        ))?;
        self.fetch(self.get(url)).await
    }

    async fn moderate_account(&self, id: &str, action: ModerationAction) -> Result<AdminAccount> {
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

    async fn admin_announcements(&self) -> Result<Vec<Announcement>> {
        let url = self.url("/api/v1/admin/announcements")?;
        self.fetch(self.get(url)).await
    }

    async fn create_announcement(
        &self,
        content: &str,
        starts_at: Option<&str>,
        ends_at: Option<&str>,
    ) -> Result<Announcement> {
        let url = self.url("/api/v1/admin/announcements")?;
        let mut form = vec![("content", content.to_owned())];
        if let Some(starts_at) = starts_at {
            form.push(("starts_at", starts_at.to_owned()));
        }
        if let Some(ends_at) = ends_at {
            form.push(("ends_at", ends_at.to_owned()));
        }
        self.fetch(self.post_form(url, &form)).await
    }

    async fn delete_announcement(&self, id: &str) -> Result<()> {
        let url = self.url(&format!("/api/v1/admin/announcements/{id}"))?;
        self.fetch(self.delete(url)).await
    }

    async fn measures(
        &self,
        keys: &[&str],
        start_at: &str,
        end_at: &str,
    ) -> Result<Vec<AdminMeasure>> {
        let mut path = format!("/api/v2/admin/measures?start_at={start_at}&end_at={end_at}");
        for key in keys {
            let _ = write!(path, "&keys[]={key}");
        }
        let url = self.url(&path)?;
        self.fetch(self.get(url)).await
    }

    async fn relays(&self) -> Result<Vec<Relay>> {
        let url = self.url("/api/v1/admin/relays")?;
        self.fetch(self.get(url)).await
    }

    async fn add_relay(&self, inbox_url: &str) -> Result<Relay> {
        let url = self.url("/api/v1/admin/relays")?;
        let form = vec![("inbox_url", inbox_url.to_owned())];
        self.fetch(self.post_form(url, &form)).await
    }

    async fn remove_relay(&self, id: &str) -> Result<()> {
        let url = self.url(&format!("/api/v1/admin/relays/{id}"))?;
        self.fetch(self.delete(url)).await
    }

    async fn domain_allows(&self, limit: u16) -> Result<Vec<DomainAllow>> {
        let url = self.url(&format!("/api/v1/admin/domain_allows?limit={limit}"))?;
        self.fetch(self.get(url)).await
    }

    async fn create_domain_allow(&self, domain: &str) -> Result<DomainAllow> {
        let url = self.url("/api/v1/admin/domain_allows")?;
        let form = vec![("domain", domain.to_owned())];
        self.fetch(self.post_form(url, &form)).await
    }

    async fn delete_domain_allow(&self, id: &str) -> Result<()> {
        let url = self.url(&format!("/api/v1/admin/domain_allows/{id}"))?;
        self.fetch(self.delete(url)).await
    }
}

#[cfg(test)]
mod tests {
    use core::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::model::ReportState;
    use crate::rest::{HttpRequest, HttpResponse};

    /// Records the URL of every request and answers with an empty list.
    ///
    /// The point of this fake is the *path*, not the payload: a wrong admin prefix
    /// answers with the instance's HTML 404 page, which looks exactly like a permission
    /// problem to whoever is holding the token.
    #[derive(Clone, Default)]
    struct Recording(Rc<RefCell<Vec<String>>>);

    impl Transport for Recording {
        fn send(&self, request: HttpRequest) -> impl core::future::Future<Output = Result<HttpResponse>> {
            self.0.borrow_mut().push(request.url.to_string());
            core::future::ready(Ok(HttpResponse {
                status: 200,
                retry_after_s: None,
                body: b"[]".to_vec(),
            }))
        }
    }

    fn instance() -> (RestInstance<Recording>, Rc<RefCell<Vec<String>>>) {
        let recording = Recording::default();
        let seen = Rc::clone(&recording.0);
        let instance = crate::Instance::parse("https://mastodon.example").expect("valid host");
        (
            RestInstance::with_transport(&instance, "token", recording),
            seen,
        )
    }

    fn block_on<F: core::future::Future>(future: F) -> F::Output {
        let mut future = core::pin::pin!(future);
        let mut context = core::task::Context::from_waker(core::task::Waker::noop());
        match future.as_mut().poll(&mut context) {
            core::task::Poll::Ready(output) => output,
            core::task::Poll::Pending => panic!("the recording transport never pends"),
        }
    }

    #[test]
    fn reports_are_asked_where_mastodon_serves_them() {
        let (instance, seen) = instance();
        block_on(instance.reports(ReportState::Open, 1)).expect("recorded");
        let first = seen.borrow()[0].clone();
        assert!(
            first.starts_with("https://mastodon.example/api/v1/admin/reports?state=open"),
            "reports are `/api/v1/admin/reports`, got {first}"
        );
    }

    #[test]
    fn measures_are_asked_under_v2() {
        let (instance, seen) = instance();
        // The dashboard aggregates are the one piece of moderation Mastodon serves
        // under `/api/v2`, which is the detail this test exists to keep pinned.
        let _ = block_on(instance.measures(&["new_users"], "2024-01-01", "2024-01-08"));
        let first = seen.borrow()[0].clone();
        assert!(
            first.starts_with("https://mastodon.example/api/v2/admin/measures"),
            "measures are `/api/v2/admin/measures`, got {first}"
        );
    }
}
