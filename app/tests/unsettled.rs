#![allow(clippy::expect_used, clippy::panic)]

use std::future::Future;
use std::num::NonZeroU32;
use std::pin::pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Waker};

use adapter_sqlite::Sqlite;
use app::{advance, react, relay, Policy};
use domain::{Lifecycle, Never, Reaction, Step, Undelivered};
use ports::{Claimed, Gateway, GatewayError, Journal, Outbox, Recorded, RequestId, StoreError, StreamId, Unit, Version, View};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Caller {
    Waiting,
    Answered,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Echo {
    Silent,
    Heard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Sound {
    Made,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Call {
    Out,
}

impl Lifecycle for Caller {
    const NAME: &'static str = "unsettled_caller";
    type Event = Sound;
    type Refusal = Never;

    fn initial() -> Caller {
        Caller::Waiting
    }

    fn evolve(self, event: &Sound) -> Caller {
        match (self, event) {
            (Caller::Waiting | Caller::Answered, Sound::Made) => Caller::Answered,
        }
    }
}

impl Lifecycle for Echo {
    const NAME: &'static str = "unsettled_echo";
    type Event = Sound;
    type Refusal = Never;

    fn initial() -> Echo {
        Echo::Silent
    }

    fn evolve(self, event: &Sound) -> Echo {
        match (self, event) {
            (Echo::Silent | Echo::Heard, Sound::Made) => Echo::Heard,
        }
    }
}

enum Line {
    Dial,
    Answer,
    Dead,
}

impl From<()> for Line {
    fn from((): ()) -> Line {
        Line::Answer
    }
}

impl From<Undelivered<Call>> for Line {
    fn from(_undelivered: Undelivered<Call>) -> Line {
        Line::Dead
    }
}

impl Reaction for Line {
    type Reads = Caller;
    type Request = Call;

    fn react(self, _reads: &Caller) -> Step<Sound, Call> {
        match self {
            Line::Dial => Step {
                events: Vec::new(),
                requests: vec![Call::Out],
            },
            Line::Answer | Line::Dead => Step {
                events: vec![Sound::Made],
                requests: Vec::new(),
            },
        }
    }
}

struct Repeat;

impl Reaction for Repeat {
    type Reads = Echo;
    type Request = Never;

    fn react(self, _reads: &Echo) -> Step<Sound, Never> {
        Step {
            events: vec![Sound::Made],
            requests: Vec::new(),
        }
    }
}

impl Policy for Caller {
    fn after(unit: &mut Unit<'_>, fact: &Recorded<Caller>) -> Result<(), StoreError> {
        match fact.stream().as_str().starts_with("loop") {
            true => react(unit, &StreamId::<Echo>::new(fact.stream().as_str()), Repeat),
            false => Ok(()),
        }
    }
}

impl Policy for Echo {
    fn after(unit: &mut Unit<'_>, fact: &Recorded<Echo>) -> Result<(), StoreError> {
        react(unit, &StreamId::<Caller>::new(fact.stream().as_str()), Line::Answer)
    }
}

struct Answers(AtomicUsize);

impl Gateway<Call> for Answers {
    type Reply = ();

    async fn perform(&self, _id: RequestId, _stream: &str, _request: &Call) -> Result<(), GatewayError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

struct Refuses(AtomicUsize);

impl Gateway<Call> for Refuses {
    type Reply = ();

    async fn perform(&self, _id: RequestId, _stream: &str, _request: &Call) -> Result<(), GatewayError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(GatewayError::Rejected {
            cause: "no such number".to_string(),
        })
    }
}

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("the future under test awaits nothing that can pend"),
    }
}

fn tries(bound: u32) -> NonZeroU32 {
    NonZeroU32::new(bound).expect("at least one attempt")
}

fn dialled(streams: &[&str]) -> Sqlite {
    let store: Sqlite = Sqlite::in_memory().expect("opens");
    streams
        .iter()
        .for_each(|stream: &&str| advance(&store, &StreamId::new(*stream), Line::Dial).expect("dialled"));
    store
}

fn open(store: &Sqlite) -> Vec<(String, u64)> {
    store
        .claim::<Line>(10)
        .expect("claimed")
        .into_iter()
        .map(|claimed: Claimed<Call>| (claimed.stream, claimed.attempts))
        .collect()
}

fn caller(store: &Sqlite, stream: &str) -> (Version, Caller) {
    store.read(|view: &mut View<'_>| view.state::<Caller>(&StreamId::new(stream))).expect("read")
}

#[test]
fn a_reply_whose_unit_fails_is_counted_and_offered_again() {
    let store: Sqlite = dialled(&["loop-1"]);
    let gateway: Answers = Answers(AtomicUsize::new(0));
    assert_eq!(ready(relay::<Line, Sqlite, Answers>(&store, &gateway, 10, tries(2))), Ok(0));
    assert_eq!(open(&store), vec![("loop-1".to_string(), 1)]);
}

#[test]
fn a_reply_whose_unit_fails_as_often_as_the_bound_parks_the_request_and_tells_nothing() {
    let store: Sqlite = dialled(&["loop-1"]);
    let gateway: Answers = Answers(AtomicUsize::new(0));
    let passes: [Result<usize, StoreError>; 3] = [
        ready(relay::<Line, Sqlite, Answers>(&store, &gateway, 10, tries(2))),
        ready(relay::<Line, Sqlite, Answers>(&store, &gateway, 10, tries(2))),
        ready(relay::<Line, Sqlite, Answers>(&store, &gateway, 10, tries(2))),
    ];
    assert_eq!(passes, [Ok(0), Ok(0), Ok(0)]);
    assert_eq!(gateway.0.load(Ordering::SeqCst), 2);
    assert_eq!(open(&store), Vec::new());
    assert_eq!(caller(&store, "loop-1"), (Version::ZERO, Caller::Waiting));
}

#[test]
fn a_request_behind_one_whose_unit_fails_is_performed_in_the_same_pass() {
    let store: Sqlite = dialled(&["loop-1", "calm-2"]);
    let gateway: Answers = Answers(AtomicUsize::new(0));
    assert_eq!(ready(relay::<Line, Sqlite, Answers>(&store, &gateway, 10, tries(2))), Ok(1));
    assert_eq!(caller(&store, "calm-2"), (Version::from(1), Caller::Answered));
    assert_eq!(open(&store), vec![("loop-1".to_string(), 1)]);
}

#[test]
fn a_rejected_request_whose_lifecycle_cannot_be_told_is_parked_all_the_same() {
    let store: Sqlite = dialled(&["loop-1"]);
    let gateway: Refuses = Refuses(AtomicUsize::new(0));
    assert_eq!(ready(relay::<Line, Sqlite, Refuses>(&store, &gateway, 10, tries(5))), Ok(0));
    assert_eq!(open(&store), Vec::new());
    assert_eq!(caller(&store, "loop-1"), (Version::ZERO, Caller::Waiting));
}

#[test]
fn a_reply_whose_unit_succeeds_is_performed_as_before() {
    let store: Sqlite = dialled(&["calm-1"]);
    let gateway: Answers = Answers(AtomicUsize::new(0));
    assert_eq!(ready(relay::<Line, Sqlite, Answers>(&store, &gateway, 10, tries(2))), Ok(1));
    assert_eq!(open(&store), Vec::new());
}
