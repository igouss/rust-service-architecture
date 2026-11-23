#![allow(clippy::expect_used, clippy::panic)]

use std::future::Future;
use std::num::NonZeroU32;
use std::pin::pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use adapter_sqlite::Sqlite;
use app::{handle, relay, Retry};
use domain::fulfilment::{Fulfilment, FulfilmentReply, FulfilmentRequest, Progress};
use domain::payment::{Amount, Authorize, Capture};
use ports::{Claimed, Gateway, GatewayError, Journal, Moment, Outbox, RequestId, StoreError, StreamId, View};

struct Down(AtomicUsize);

impl Gateway<FulfilmentRequest> for Down {
    type Reply = FulfilmentReply;

    async fn perform(&self, _id: RequestId, _stream: &str, _request: &FulfilmentRequest) -> Result<FulfilmentReply, GatewayError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(GatewayError::Unavailable { cause: "down".to_string() })
    }
}

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("the future under test awaits nothing that can pend"),
    }
}

fn retry(attempts: u32) -> Retry {
    Retry {
        attempts: NonZeroU32::new(attempts).expect("at least one attempt"),
        first: Duration::from_secs(1),
        longest: Duration::from_secs(300),
        lease: Duration::from_secs(60),
    }
}

fn captured() -> Sqlite {
    let store: Sqlite = Sqlite::in_memory().expect("opens");
    let amount: Amount = Amount::new(500).expect("positive");
    handle(&store, &StreamId::new("p-1"), Authorize { amount }).expect("authorized");
    handle(&store, &StreamId::new("p-1"), Capture { amount }).expect("captured");
    store
}

fn passed(store: &Sqlite, gateway: &Down, attempts: u32, at: &[u64]) -> usize {
    at.iter().for_each(|millis: &u64| {
        assert_eq!(
            ready(relay::<Progress, Sqlite, Down>(
                store,
                gateway,
                10,
                &retry(attempts),
                Moment::ZERO.after(*millis)
            )),
            Ok::<usize, StoreError>(0)
        );
    });
    gateway.0.load(Ordering::SeqCst)
}

fn fulfilment(store: &Sqlite) -> Fulfilment {
    store
        .read(|view: &mut View<'_>| view.state::<Fulfilment>(&StreamId::new("p-1")))
        .expect("read")
        .1
}

#[test]
fn before_any_failure_there_is_no_wait() {
    assert_eq!(retry(5).wait(0), Duration::ZERO);
}

#[test]
fn the_wait_after_the_first_failure_is_the_first_wait() {
    assert_eq!(retry(5).wait(1), Duration::from_secs(1));
}

#[test]
fn each_further_failure_doubles_the_wait() {
    assert_eq!((retry(5).wait(2), retry(5).wait(4)), (Duration::from_secs(2), Duration::from_secs(8)));
}

#[test]
fn the_wait_is_never_longer_than_the_longest() {
    assert_eq!(
        (retry(5).wait(10), retry(5).wait(u64::MAX)),
        (Duration::from_secs(300), Duration::from_secs(300))
    );
}

#[test]
fn a_failed_request_is_not_performed_again_before_its_wait_is_over() {
    let store: Sqlite = captured();
    assert_eq!(passed(&store, &Down(AtomicUsize::new(0)), 5, &[0, 999]), 1);
}

#[test]
fn a_failed_request_is_performed_again_once_its_wait_is_over() {
    let store: Sqlite = captured();
    assert_eq!(passed(&store, &Down(AtomicUsize::new(0)), 5, &[0, 1000]), 2);
}

#[test]
fn a_request_that_failed_twice_waits_twice_as_long() {
    let store: Sqlite = captured();
    assert_eq!(passed(&store, &Down(AtomicUsize::new(0)), 5, &[0, 1000, 2999]), 2);
}

#[test]
fn five_passes_half_a_second_apart_leave_a_fulfilment_reserving() {
    let store: Sqlite = captured();
    assert_eq!(passed(&store, &Down(AtomicUsize::new(0)), 5, &[0, 500, 1000, 1500, 2000]), 2);
    assert_eq!(fulfilment(&store), Fulfilment::Reserving);
}

#[test]
fn a_request_that_fails_after_every_wait_is_parked_at_the_bound() {
    let store: Sqlite = captured();
    assert_eq!(passed(&store, &Down(AtomicUsize::new(0)), 3, &[0, 1000, 3000, 7000]), 3);
    assert_eq!(fulfilment(&store), Fulfilment::Abandoned);
}

struct Peeks<'a> {
    store: &'a Sqlite,
    open: AtomicUsize,
}

impl Gateway<FulfilmentRequest> for Peeks<'_> {
    type Reply = FulfilmentReply;

    async fn perform(&self, _id: RequestId, _stream: &str, _request: &FulfilmentRequest) -> Result<FulfilmentReply, GatewayError> {
        let open: Vec<Claimed<FulfilmentRequest>> = self.store.claim::<Progress>(10, Moment::ZERO, Moment::ZERO).expect("claimed");
        self.open.fetch_add(open.len(), Ordering::SeqCst);
        Ok(FulfilmentReply::StockUnavailable)
    }
}

fn peeked(lease: Duration) -> usize {
    let store: Sqlite = captured();
    let gateway: Peeks<'_> = Peeks {
        store: &store,
        open: AtomicUsize::new(0),
    };
    let held: Retry = Retry { lease, ..retry(5) };
    assert_eq!(ready(relay::<Progress, Sqlite, Peeks<'_>>(&store, &gateway, 10, &held, Moment::ZERO)), Ok(1));
    gateway.open.load(Ordering::SeqCst)
}

#[test]
fn while_a_relay_performs_a_request_under_its_lease_no_other_claim_is_handed_it() {
    assert_eq!(peeked(Duration::from_secs(60)), 0);
}

#[test]
fn with_no_lease_a_second_claim_is_handed_the_request_being_performed() {
    assert_eq!(peeked(Duration::ZERO), 1);
}

#[test]
fn a_request_claimed_by_a_relay_that_died_is_not_performed_before_the_lease_ends() {
    let store: Sqlite = captured();
    let died: usize = store.claim::<Progress>(10, Moment::ZERO, Moment::ZERO.after(60_000)).expect("claimed").len();
    assert_eq!((died, passed(&store, &Down(AtomicUsize::new(0)), 5, &[59_999])), (1, 0));
}

#[test]
fn a_request_claimed_by_a_relay_that_died_is_performed_when_the_lease_ends() {
    let store: Sqlite = captured();
    let died: usize = store.claim::<Progress>(10, Moment::ZERO, Moment::ZERO.after(60_000)).expect("claimed").len();
    assert_eq!((died, passed(&store, &Down(AtomicUsize::new(0)), 5, &[60_000])), (1, 1));
}
