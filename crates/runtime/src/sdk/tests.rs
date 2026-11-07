//! Sync consent is state like any other: a command, not a view-local flag.

use nodus_shared::{AccountFeed, EventBatch, MeResponse, Meta, PostFeed};
use nodus_state::{Command, Discovery, SyncConsent};

use nodus_persist::MemoryPersistence;

use crate::clock::SystemClock;
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
    assert!(!sdk.state().sync.get().enabled);

    let consent = block_on(sdk.load_sync_consent()).expect("me answers");

    assert!(consent.enabled);
    assert!(
        sdk.state().sync.get().enabled,
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

    assert_eq!(sdk.state().sync.get(), SyncConsent::default());
}
