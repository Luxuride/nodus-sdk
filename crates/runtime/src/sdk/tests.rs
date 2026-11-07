//! Sync consent is state like any other: a command, not a view-local flag.

use nodus_instance::model::{
    Account, Context, Draft, LocalId, Notification, Page, PageParams, Relationship, Status,
};
use nodus_instance::{InstanceApi, StreamKind};
use nodus_shared::{AccountFeed, EventBatch, MeResponse, Meta, PostFeed};
use nodus_state::{Command, Discovery, SyncConsent, TimelineRequest};

use nodus_persist::MemoryPersistence;

use crate::clock::{FixedClock, SystemClock};
use crate::sdk::Sdk;
use crate::shell::{Unavailable, UnavailableBackend};

/// Answers `me` with a fixed consent and fails everything else.
struct Consented(SyncConsent);

/// A finished failure: the stub answers on the first poll, like the shell's.
fn fail<T>() -> core::future::Ready<Result<T, Unavailable>> {
    core::future::ready(Err(Unavailable))
}

impl Discovery for Consented {
    type Error = Unavailable;

    fn meta(&self) -> impl core::future::Future<Output = Result<Meta, Self::Error>> {
        fail::<Meta>()
    }

    fn me(&self) -> impl core::future::Future<Output = Result<MeResponse, Self::Error>> {
        let consent = self.0;
        core::future::ready(Ok(MeResponse {
            acct: "someone@remote.test".to_owned(),
            instance_host: "remote.test".to_owned(),
            sync_enabled: consent.enabled,
            telemetry_opt_in: consent.telemetry_opt_in,
            contract: nodus_shared::CONTRACT_VERSION.to_owned(),
        }))
    }

    fn feed(
        &self,
        _cursor: Option<&str>,
    ) -> impl core::future::Future<Output = Result<AccountFeed, Self::Error>> {
        fail::<AccountFeed>()
    }

    fn posts(
        &self,
        _cursor: Option<&str>,
    ) -> impl core::future::Future<Output = Result<PostFeed, Self::Error>> {
        fail::<PostFeed>()
    }

    fn refresh(
        &self,
        _feed_id: &str,
    ) -> impl core::future::Future<Output = Result<AccountFeed, Self::Error>> {
        fail::<AccountFeed>()
    }

    fn send_events(
        &self,
        _batch: &EventBatch,
    ) -> impl core::future::Future<Output = Result<(), Self::Error>> {
        fail::<()>()
    }

    fn set_sync(
        &self,
        _enable: bool,
        _telemetry_opt_in: bool,
    ) -> impl core::future::Future<Output = Result<(), Self::Error>> {
        fail::<()>()
    }
}

fn sdk_with(
    consent: SyncConsent,
) -> Sdk<UnavailableBackend, Consented, MemoryPersistence, SystemClock> {
    Sdk::new(
        UnavailableBackend,
        Consented(consent),
        MemoryPersistence::new(),
        SystemClock,
    )
}

/// Polls once: every future here is ready on the first poll.
fn block_on<F: core::future::Future>(future: F) -> F::Output {
    use core::task::{Context, Poll, Waker};
    let mut future = core::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("the port under test never pends"),
    }
}

#[test]
fn the_server_is_what_answers_whether_sync_is_on() {
    let sdk = sdk_with(SyncConsent {
        enabled: true,
        telemetry_opt_in: false,
    });
    assert!(
        sdk.state().sync.get().is_none(),
        "nothing is known before the server answers"
    );

    let consent = block_on(sdk.load_sync_consent()).expect("me answers");

    assert!(consent.enabled);
    assert_eq!(
        sdk.state().sync.get().map(|consent| consent.enabled),
        Some(true),
        "the answer is state, not a local flag"
    );
}

#[test]
fn signing_out_forgets_the_consent() {
    let sdk = sdk_with(SyncConsent {
        enabled: true,
        telemetry_opt_in: true,
    });
    block_on(sdk.load_sync_consent()).expect("me answers");

    sdk.dispatch(Command::SignedOut);

    assert_eq!(sdk.state().sync.get(), None);
}

/// Serves prepared pages of statuses, and counts how many times it was asked.
struct Pages {
    calls: core::cell::Cell<usize>,
    pages: Vec<Vec<Status>>,
}

/// A status with only the fields the timeline logic reads.
fn status(id: &str) -> Status {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "created_at": "2026-11-07T00:00:00.000Z",
        "content": "<p>hello</p>",
        "visibility": "public",
        "account": {
            "id": format!("account-{id}"),
            "acct": "someone@remote.test",
            "display_name": "Someone"
        }
    }))
    .expect("a status the model accepts")
}

impl Pages {
    fn of(pages: Vec<Vec<&str>>) -> Self {
        Self {
            calls: core::cell::Cell::new(0),
            pages: pages
                .into_iter()
                .map(|ids| ids.into_iter().map(status).collect())
                .collect(),
        }
    }
}

impl InstanceApi for Pages {
    type Error = Unavailable;

    fn verify_credentials(
        &self,
    ) -> impl core::future::Future<Output = Result<Account, Self::Error>> {
        fail::<Account>()
    }

    fn timeline(
        &self,
        _kind: StreamKind,
        _params: PageParams,
    ) -> impl core::future::Future<Output = Result<Page<Status>, Self::Error>> {
        let index = self.calls.get();
        self.calls.set(index + 1);
        let items = self.pages.get(index).cloned().unwrap_or_default();
        let next = items.last().map(|status| status.id.clone());
        core::future::ready(Ok(Page::new(items).with_next(next)))
    }

    fn tag_timeline(
        &self,
        _tag: &str,
        _params: PageParams,
    ) -> impl core::future::Future<Output = Result<Page<Status>, Self::Error>> {
        fail::<Page<Status>>()
    }

    fn status(&self, _id: &str) -> impl core::future::Future<Output = Result<Status, Self::Error>> {
        fail::<Status>()
    }

    fn status_context(
        &self,
        _id: &str,
    ) -> impl core::future::Future<Output = Result<Context, Self::Error>> {
        fail::<Context>()
    }

    fn notifications(
        &self,
        _params: PageParams,
    ) -> impl core::future::Future<Output = Result<Page<Notification>, Self::Error>> {
        fail::<Page<Notification>>()
    }

    fn account(
        &self,
        _id: &LocalId,
    ) -> impl core::future::Future<Output = Result<Account, Self::Error>> {
        fail::<Account>()
    }

    fn account_statuses(
        &self,
        _id: &LocalId,
        _params: PageParams,
    ) -> impl core::future::Future<Output = Result<Page<Status>, Self::Error>> {
        fail::<Page<Status>>()
    }

    fn relationship(
        &self,
        _id: &LocalId,
    ) -> impl core::future::Future<Output = Result<Relationship, Self::Error>> {
        fail::<Relationship>()
    }

    fn follow(
        &self,
        _id: &LocalId,
    ) -> impl core::future::Future<Output = Result<Relationship, Self::Error>> {
        fail::<Relationship>()
    }

    fn unfollow(
        &self,
        _id: &LocalId,
    ) -> impl core::future::Future<Output = Result<Relationship, Self::Error>> {
        fail::<Relationship>()
    }

    fn post_status(
        &self,
        _draft: &Draft,
    ) -> impl core::future::Future<Output = Result<Status, Self::Error>> {
        fail::<Status>()
    }

    fn delete_status(
        &self,
        _id: &str,
    ) -> impl core::future::Future<Output = Result<(), Self::Error>> {
        fail::<()>()
    }
}

/// One request, one page, and the page is readable without another request.
#[test]
fn a_loaded_page_is_readable_without_asking_again() {
    let instance = Pages::of(vec![vec!["1", "2"]]);
    instance.calls.set(0);
    let sdk = Sdk::new(
        instance,
        Consented(SyncConsent::default()),
        MemoryPersistence::new(),
        FixedClock::at(1_000),
    );

    let request = TimelineRequest::Home;
    block_on(sdk.ensure_timeline(request.clone(), None, 20, 60_000)).expect("a page arrives");

    let (statuses, cursor) = sdk.read_timeline(&request);
    assert_eq!(
        statuses
            .iter()
            .map(|status| status.id.clone())
            .collect::<Vec<_>>(),
        vec!["1".to_owned(), "2".to_owned()],
    );
    assert_eq!(cursor.as_deref(), Some("2"));
}

/// Inside its TTL a held page is not fetched again.
#[test]
fn ensure_timeline_trusts_a_page_inside_its_ttl() {
    let instance = Pages::of(vec![vec!["1"], vec!["9"]]);
    let sdk = Sdk::new(
        instance,
        Consented(SyncConsent::default()),
        MemoryPersistence::new(),
        FixedClock::at(1_000),
    );

    let request = TimelineRequest::Home;
    block_on(sdk.ensure_timeline(request.clone(), None, 20, 60_000)).expect("first page");
    let second = block_on(sdk.ensure_timeline(request.clone(), None, 20, 60_000)).expect("read");

    assert!(second.is_none(), "nothing was stale, so nothing was asked");
    assert_eq!(sdk.read_timeline(&request).0.len(), 1);
}

/// A cursor adds to what is held instead of replacing it.
#[test]
fn an_older_page_is_appended_not_substituted() {
    let instance = Pages::of(vec![vec!["1", "2"], vec!["3"]]);
    let sdk = Sdk::new(
        instance,
        Consented(SyncConsent::default()),
        MemoryPersistence::new(),
        FixedClock::at(1_000),
    );

    let request = TimelineRequest::Home;
    block_on(sdk.load_timeline(request.clone(), None, 20)).expect("newest page");
    block_on(sdk.load_timeline(request.clone(), Some("2"), 20)).expect("older page");

    let ids: Vec<String> = sdk
        .read_timeline(&request)
        .0
        .into_iter()
        .map(|status| status.id)
        .collect();
    assert_eq!(ids, vec!["1".to_owned(), "2".to_owned(), "3".to_owned()]);
}
