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
        // `/api/v2/admin/accounts` and not v1: v1 has boolean switches (`local`,
        // `silenced`, …) but no `status` filter, so `?status=pending` there is silently
        // ignored and the queue lists **every** local account — approved ones included,
        // which is what a moderator reported seeing.
        //
        // `status=pending` is only meaningful on a server that requires approval; one that
        // does not answers with an empty list, which is the correct answer here.
        let url = self.url(&format!(
            "/api/v2/admin/accounts?origin=local&status=pending&limit={limit}"
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

#[cfg(test)]
mod tests {
    use core::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::rest::{HttpRequest, HttpResponse};

    /// Records every request URL and answers with an empty list.
    #[derive(Clone, Default)]
    struct Recording(Rc<RefCell<Vec<String>>>);

    impl Transport for Recording {
        fn send(
            &self,
            request: HttpRequest,
        ) -> impl core::future::Future<Output = Result<HttpResponse>> {
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
    fn the_approval_queue_asks_v2_where_status_is_a_filter() {
        let (instance, seen) = instance();
        block_on(instance.pending_accounts(20)).expect("recorded");
        let first = seen.borrow()[0].clone();
        assert!(
            first.starts_with(
                "https://mastodon.example/api/v2/admin/accounts?origin=local&status=pending"
            ),
            "the queue needs the v2 `status` filter, got {first}"
        );
    }
}
