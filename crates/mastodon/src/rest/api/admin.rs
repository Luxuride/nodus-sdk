//! Moderation over the REST transport.
//!
//! Mastodon splits moderation across two prefixes, and the split is not by version:
//! reports, accounts, blocks, relays and domain allows live under
//! `/api/v1/admin`, while measures (the dashboard aggregates) live under
//! `/api/v2/admin`. A wrong prefix does not answer 404 as JSON — it falls through to the
//! instance's HTML error page, which reads like "this account cannot moderate" and cost
//! a debugging session, so each path below is spelled out rather than derived.

use crate::actions::AdminActions;
use crate::model::{
    AdminAccount, AdminMeasure, Announcement, AnnouncementDraft, AuditEntry, DomainAllow,
    DomainBlock, DomainSeverity, ModerationAction, Report, ReportState,
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

    async fn measures(
        &self,
        keys: &[&str],
        start_at: &str,
        end_at: &str,
    ) -> Result<Vec<AdminMeasure>> {
        // `POST /api/v1/admin/measures` with a **JSON body**, not a GET with a query:
        // the GET form answers the instance's HTML 404 page, and so does v2. The keys
        // are an array in the body, which is also why there is no `keys[]` escaping here.
        let url = self.url("/api/v1/admin/measures")?;
        let body = serde_json::json!({
            "keys": keys,
            "start_at": start_at,
            "end_at": end_at,
        });
        let request = self.post_body(url, "application/json", body.to_string().into_bytes());
        self.fetch(request).await
    }

    async fn audit(&self, limit: u16) -> Result<Vec<AuditEntry>> {
        let url = self.url(&format!("/api/v1/admin/audit?limit={limit}"))?;
        self.fetch(self.get(url)).await
    }

    async fn admin_announcements(&self) -> Result<Vec<Announcement>> {
        let url = self.url("/api/v1/admin/announcements")?;
        self.fetch(self.get(url)).await
    }

    async fn create_announcement(&self, draft: AnnouncementDraft) -> Result<Announcement> {
        let url = self.url("/api/v1/admin/announcements")?;
        self.fetch(self.post_form(url, &announcement_form(&draft)))
            .await
    }

    async fn update_announcement(
        &self,
        id: &str,
        draft: AnnouncementDraft,
    ) -> Result<Announcement> {
        let url = self.url(&format!("/api/v1/admin/announcements/{id}"))?;
        self.fetch(self.put_form(url, &announcement_form(&draft)))
            .await
    }

    async fn delete_announcement(&self, id: &str) -> Result<()> {
        let url = self.url(&format!("/api/v1/admin/announcements/{id}"))?;
        let _: serde_json::Value = self.fetch(self.delete(url)).await?;
        Ok(())
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

/// Encode the editable fields; absent dates are omitted rather than sent empty.
fn announcement_form(draft: &AnnouncementDraft) -> Vec<(&'static str, String)> {
    let mut form = vec![
        ("content", draft.content.clone()),
        ("published", draft.published.to_string()),
        ("all_day", draft.all_day.to_string()),
    ];
    if let Some(start) = &draft.starts_at {
        form.push(("starts_at", start.clone()));
    }
    if let Some(end) = &draft.ends_at {
        form.push(("ends_at", end.clone()));
    }
    form
}

#[cfg(test)]
mod tests {
    use core::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::model::ReportState;
    use crate::rest::{HttpRequest, HttpResponse};

    /// One recorded request: verb, absolute URL, decoded body.
    type Sent = (String, String, Option<String>);

    /// Records `(verb, url, body)` of every request and answers with an empty list.
    ///
    /// The point of this fake is the *path*, not the payload: a wrong admin prefix
    /// answers with the instance's HTML 404 page, which looks exactly like a permission
    /// problem to whoever is holding the token.
    #[derive(Clone, Default)]
    struct Recording(Rc<RefCell<Vec<Sent>>>);

    impl Transport for Recording {
        fn send(
            &self,
            request: HttpRequest,
        ) -> impl core::future::Future<Output = Result<HttpResponse>> {
            self.0.borrow_mut().push((
                request.method.as_str().to_owned(),
                request.url.to_string(),
                request
                    .body
                    .as_ref()
                    .map(|bytes| String::from_utf8_lossy(bytes).into_owned()),
            ));
            core::future::ready(Ok(HttpResponse {
                status: 200,
                retry_after_s: None,
                body: b"[]".to_vec(),
            }))
        }
    }

    fn instance() -> (RestInstance<Recording>, Rc<RefCell<Vec<Sent>>>) {
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
        let first = seen.borrow()[0].1.clone();
        assert!(
            first.starts_with("https://mastodon.example/api/v1/admin/reports?state=open"),
            "reports are `/api/v1/admin/reports`, got {first}"
        );
    }

    #[test]
    fn measures_are_posted_as_json_to_v1() {
        let (instance, seen) = instance();
        // A GET with `?keys[]=` answers the instance's HTML 404 page: the dashboard
        // aggregates are a POST with a JSON body. This test pins both halves.
        let mut future = core::pin::pin!(instance.measures(
            &["new_users", "interactions"],
            "2024-01-01",
            "2024-01-08"
        ));
        let mut context = core::task::Context::from_waker(core::task::Waker::noop());
        let _ = future.as_mut().poll(&mut context);
        let requests = seen.borrow().clone();
        let (method, url, body) = &requests[0];
        assert_eq!(method, "POST", "measures are a POST");
        assert!(
            url.starts_with("https://mastodon.example/api/v1/admin/measures"),
            "measures are `/api/v1/admin/measures`, got {url}"
        );
        let body = body.clone().unwrap_or_default();
        assert!(
            body.contains("\"new_users\""),
            "keys go in the body: {body}"
        );
        assert!(body.contains("\"start_at\":\"2024-01-01\""), "{body}");
    }
}
