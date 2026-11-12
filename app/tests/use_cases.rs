#![allow(clippy::expect_used, clippy::panic)]

use std::future::Future;
use std::pin::pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use adapter_sqlite::Sqlite;
use app::tasks::{open_tasks, task_board, task_tally, TaskBoard};
use app::{handle, rebuild, relay};
use domain::fulfilment::{Fulfilment, FulfilmentRequest, Progress};
use domain::payment::{Amount, Authorize, Capture, Payment, PaymentRefusal};
use domain::task::{AddTask, CompleteTask, Tally, Task, TaskRefusal, TaskTally, Title};
use ports::{Answer, Claimed, Gateway, GatewayError, Journal, Outbox, Position, RequestId, StoreError, StreamId, Version, View};

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("the future under test awaits nothing that can pend"),
    }
}

fn store() -> Sqlite {
    Sqlite::in_memory().expect("opens")
}

fn id(text: &str) -> StreamId {
    StreamId(text.to_string())
}

fn title_of(task: &str) -> Title {
    Title::new(&format!("title of {task}")).expect("a title")
}

fn add(store: &Sqlite, task: &str) -> Answer<TaskRefusal> {
    handle(store, &id(task), AddTask { title: title_of(task) }).expect("handled")
}

fn cents(value: u64) -> Amount {
    Amount::new(value).expect("positive")
}

fn captured_payment() -> Sqlite {
    let store: Sqlite = store();
    handle(&store, &id("p-1"), Authorize { amount: cents(500) }).expect("authorized");
    handle(&store, &id("p-1"), Capture { amount: cents(500) }).expect("captured");
    store
}

fn state<S: domain::Lifecycle>(store: &Sqlite, stream: &str) -> (Version, S) {
    store.read(|view| view.state::<S>(&id(stream))).expect("read")
}

struct Recording {
    performed: Mutex<Vec<(RequestId, FulfilmentRequest)>>,
}

impl Gateway<FulfilmentRequest> for Recording {
    async fn perform(&self, id: RequestId, _stream: &StreamId, request: &FulfilmentRequest) -> Result<(), GatewayError> {
        self.performed.lock().expect("lock").push((id, request.clone()));
        Ok(())
    }
}

struct Down;

impl Gateway<FulfilmentRequest> for Down {
    async fn perform(&self, _id: RequestId, _stream: &StreamId, _request: &FulfilmentRequest) -> Result<(), GatewayError> {
        Err(GatewayError { cause: "down".to_string() })
    }
}

#[test]
fn with_no_tasks_there_is_no_tally_and_nothing_open() {
    let store: Sqlite = store();
    assert_eq!(task_tally(&store), Ok(None));
    assert_eq!(open_tasks(&store), Ok(Vec::new()));
}

#[test]
fn the_board_of_no_tasks_is_empty_with_no_tally() {
    assert_eq!(task_board(&store()), Ok(TaskBoard { open: Vec::new(), tally: None }));
}

#[test]
fn the_board_of_one_task_lists_it_and_tallies_it() {
    let store: Sqlite = store();
    add(&store, "t-1");
    assert_eq!(
        task_board(&store),
        Ok(TaskBoard {
            open: vec![(id("t-1"), title_of("t-1"))],
            tally: Some(Tally { open: 1, done: 0 })
        })
    );
}

#[test]
fn the_board_of_many_tasks_lists_the_open_ones_and_tallies_all() {
    let store: Sqlite = store();
    add(&store, "t-1");
    add(&store, "t-2");
    add(&store, "t-3");
    handle(&store, &id("t-2"), CompleteTask).expect("handled");
    assert_eq!(
        task_board(&store),
        Ok(TaskBoard {
            open: vec![(id("t-1"), title_of("t-1")), (id("t-3"), title_of("t-3"))],
            tally: Some(Tally { open: 2, done: 1 })
        })
    );
}

#[test]
fn an_added_task_is_stored_open_at_version_one_and_tallied() {
    let store: Sqlite = store();
    assert_eq!(add(&store, "t-1"), Answer::Accepted(Version(1)));
    assert_eq!(state::<Task>(&store, "t-1"), (Version(1), Task::Open { title: title_of("t-1") }));
    assert_eq!(task_tally(&store), Ok(Some(Tally { open: 1, done: 0 })));
}

#[test]
fn many_added_tasks_are_all_open_and_tallied() {
    let store: Sqlite = store();
    add(&store, "t-1");
    add(&store, "t-2");
    assert_eq!(open_tasks(&store), Ok(vec![(id("t-1"), title_of("t-1")), (id("t-2"), title_of("t-2"))]));
    assert_eq!(task_tally(&store), Ok(Some(Tally { open: 2, done: 0 })));
}

#[test]
fn a_completed_task_leaves_the_open_list_and_moves_in_the_tally() {
    let store: Sqlite = store();
    add(&store, "t-1");
    add(&store, "t-2");
    assert_eq!(handle(&store, &id("t-1"), CompleteTask), Ok(Answer::Accepted(Version(2))));
    assert_eq!(open_tasks(&store), Ok(vec![(id("t-2"), title_of("t-2"))]));
    assert_eq!(task_tally(&store), Ok(Some(Tally { open: 1, done: 1 })));
}

#[test]
fn a_refused_command_is_answered_recorded_and_changes_nothing() {
    let store: Sqlite = store();
    add(&store, "t-1");
    assert_eq!(add(&store, "t-1"), Answer::Refused(TaskRefusal::AlreadyExists));
    assert_eq!(store.rejections("task", &id("t-1")), Ok(vec![r#""AlreadyExists""#.to_string()]));
    assert_eq!(state::<Task>(&store, "t-1"), (Version(1), Task::Open { title: title_of("t-1") }));
    assert_eq!(task_tally(&store), Ok(Some(Tally { open: 1, done: 0 })));
}

#[test]
fn an_accepted_command_records_no_rejection() {
    let store: Sqlite = store();
    add(&store, "t-1");
    assert_eq!(store.rejections("task", &id("t-1")), Ok(Vec::new()));
}

#[test]
fn an_authorization_starts_no_fulfilment() {
    let store: Sqlite = store();
    handle(&store, &id("p-1"), Authorize { amount: cents(500) }).expect("authorized");
    assert_eq!(state::<Fulfilment>(&store, "p-1"), (Version(0), Fulfilment::Idle));
    assert_eq!(store.claim::<Progress>(10), Ok(Vec::new()));
}

#[test]
fn a_capture_starts_the_fulfilment_and_requests_stock_in_the_same_unit() {
    let store: Sqlite = captured_payment();
    assert_eq!(state::<Payment>(&store, "p-1"), (Version(2), Payment::Captured { amount: cents(500) }));
    assert_eq!(state::<Fulfilment>(&store, "p-1"), (Version(1), Fulfilment::Reserving));
    assert_eq!(
        store.claim::<Progress>(10),
        Ok(vec![Claimed {
            id: RequestId(1),
            stream: id("p-1"),
            request: FulfilmentRequest::ReserveStock
        }])
    );
}

#[test]
fn a_refused_capture_starts_nothing() {
    let store: Sqlite = store();
    assert_eq!(
        handle(&store, &id("p-1"), Capture { amount: cents(500) }),
        Ok(Answer::Refused(PaymentRefusal::NotAuthorized))
    );
    assert_eq!(state::<Fulfilment>(&store, "p-1"), (Version(0), Fulfilment::Idle));
}

#[test]
fn the_relay_performs_a_claimed_request_once_and_marks_it_done() {
    let store: Sqlite = captured_payment();
    let gateway: Recording = Recording {
        performed: Mutex::new(Vec::new()),
    };
    assert_eq!(ready(relay::<Progress, Sqlite, Recording>(&store, &gateway, 10)), Ok(1));
    assert_eq!(ready(relay::<Progress, Sqlite, Recording>(&store, &gateway, 10)), Ok(0));
    assert_eq!(*gateway.performed.lock().expect("lock"), vec![(RequestId(1), FulfilmentRequest::ReserveStock)]);
}

#[test]
fn a_request_the_gateway_fails_stays_claimable() {
    let store: Sqlite = captured_payment();
    assert_eq!(ready(relay::<Progress, Sqlite, Down>(&store, &Down, 10)), Ok(0));
    assert_eq!(store.claim::<Progress>(10).map(|claimed| claimed.len()), Ok(1));
}

#[test]
fn the_relay_with_an_empty_outbox_performs_nothing() {
    let store: Sqlite = store();
    assert_eq!(ready(relay::<Progress, Sqlite, Down>(&store, &Down, 10)), Ok(0));
}

#[test]
fn a_rebuilt_read_model_equals_the_live_one() {
    let store: Sqlite = store();
    add(&store, "t-1");
    add(&store, "t-2");
    handle(&store, &id("t-2"), CompleteTask).expect("completed");
    let live: Option<Tally> = task_tally(&store).expect("live");
    assert_eq!(rebuild::<Sqlite, TaskTally>(&store, 2), Ok(3));
    assert_eq!(task_tally(&store), Ok(live));
}

#[test]
fn a_rebuild_over_no_facts_leaves_no_rows() {
    let store: Sqlite = store();
    assert_eq!(rebuild::<Sqlite, TaskTally>(&store, 2), Ok(0));
    assert_eq!(task_tally(&store), Ok(None));
}

#[test]
fn a_commit_tells_the_listener_its_last_position() {
    let seen: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));
    let sink: Arc<AtomicU64> = Arc::clone(&seen);
    let store: Sqlite = store().on_commit(move |position: Position| sink.store(position.0, Ordering::SeqCst));
    handle(&store, &id("p-1"), Authorize { amount: cents(500) }).expect("authorized");
    handle(&store, &id("p-1"), Capture { amount: cents(500) }).expect("captured");
    assert_eq!(seen.load(Ordering::SeqCst), 3);
}

#[test]
fn a_refusal_tells_the_listener_nothing() {
    let seen: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));
    let sink: Arc<AtomicU64> = Arc::clone(&seen);
    let store: Sqlite = store().on_commit(move |position: Position| sink.store(position.0, Ordering::SeqCst));
    handle(&store, &id("t-1"), CompleteTask).expect("handled");
    assert_eq!(seen.load(Ordering::SeqCst), 0);
}

#[test]
fn a_unit_that_fails_after_applying_stores_nothing() {
    let store: Sqlite = store();
    let failed: Result<(), StoreError> = store.write(|unit| {
        app::react(unit, &id("p-1"), Progress::PaymentCaptured)?;
        Err(StoreError::Unavailable { cause: "injected".to_string() })
    });
    assert_eq!(failed, Err(StoreError::Unavailable { cause: "injected".to_string() }));
    assert_eq!(state::<Fulfilment>(&store, "p-1"), (Version(0), Fulfilment::Idle));
    assert_eq!(store.claim::<Progress>(10), Ok(Vec::new()));
}

struct FailsTheFirst;

impl Gateway<FulfilmentRequest> for FailsTheFirst {
    async fn perform(&self, id: RequestId, _stream: &StreamId, _request: &FulfilmentRequest) -> Result<(), GatewayError> {
        match id {
            RequestId(1) => Err(GatewayError { cause: "down".to_string() }),
            RequestId(_) => Ok(()),
        }
    }
}

#[test]
fn a_request_that_keeps_failing_does_not_hold_back_the_next_one() {
    let store: Sqlite = captured_payment();
    handle(&store, &id("p-2"), Authorize { amount: cents(500) }).expect("authorized");
    handle(&store, &id("p-2"), Capture { amount: cents(500) }).expect("captured");
    assert_eq!(ready(relay::<Progress, Sqlite, FailsTheFirst>(&store, &FailsTheFirst, 1)), Ok(0));
    assert_eq!(ready(relay::<Progress, Sqlite, FailsTheFirst>(&store, &FailsTheFirst, 1)), Ok(1));
}

#[test]
fn progress_from_outside_advances_the_process_manager_and_issues_its_request() {
    let store: Sqlite = captured_payment();
    assert_eq!(app::advance(&store, &id("p-1"), Progress::StockReserved), Ok(()));
    assert_eq!(state::<Fulfilment>(&store, "p-1"), (Version(2), Fulfilment::Shipping));
    assert_eq!(store.claim::<Progress>(10).map(|claimed| claimed.len()), Ok(2));
}

#[test]
fn progress_that_does_not_apply_changes_nothing() {
    let store: Sqlite = store();
    assert_eq!(app::advance(&store, &id("p-1"), Progress::Shipped), Ok(()));
    assert_eq!(state::<Fulfilment>(&store, "p-1"), (Version(0), Fulfilment::Idle));
}
