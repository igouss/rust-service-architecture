#![allow(clippy::expect_used, clippy::panic)]

use adapter_memory::{Held, Kept, Memory, Parked};
use domain::fulfilment::{Fulfilment, FulfilmentRequest, Progress};
use domain::task::{AddTask, CompleteTask, Task, TaskRefusal, Title};
use ports::{Applied, Claimed, Finished, Journal, Moment, Outbox, Position, Recorded, RequestId, StoreError, StreamId, View};

fn id<S>(text: &str) -> StreamId<S> {
    StreamId::new(text)
}

fn store(facts: usize, refusals: usize) -> Memory {
    Memory::new(Kept { facts, refusals, parked: 2 })
}

fn add(store: &Memory, task: &str) {
    let title: Title = Title::new(task).expect("a title");
    let _applied: Applied<Task> = store
        .write(|unit| unit.apply(&id(task), |state: Task| ports::decide(AddTask { title }, state)))
        .expect("applied");
}

fn refuse(store: &Memory, task: &str) {
    let _applied: Applied<Task> = store
        .write(|unit| unit.apply(&id(task), |state: Task| ports::decide(CompleteTask, state)))
        .expect("applied");
}

fn request(store: &Memory, stream: &str) {
    let _applied: Applied<Fulfilment> = store
        .write(|unit| unit.apply(&id(stream), |state: Fulfilment| ports::react(Progress::PaymentCaptured, state)))
        .expect("applied");
}

fn positions(store: &Memory, after: u64) -> Result<Vec<Position>, StoreError> {
    store
        .write(|unit| unit.facts::<Task>(Position::ZERO.after(after), 10))
        .map(|facts: Vec<Recorded<Task>>| facts.iter().map(Recorded::position).collect())
}

#[test]
fn a_new_store_holds_nothing() {
    assert_eq!(store(2, 2).held(), Held::default());
}

#[test]
fn with_as_many_facts_as_are_kept_none_is_trimmed() {
    let store: Memory = store(2, 2);
    add(&store, "t-1");
    add(&store, "t-2");
    assert_eq!(positions(&store, 0), Ok(vec![Position::from(1), Position::from(2)]));
}

#[test]
fn facts_beyond_the_kept_number_are_dropped_and_positions_keep_counting() {
    let store: Memory = store(2, 2);
    add(&store, "t-1");
    add(&store, "t-2");
    add(&store, "t-3");
    assert_eq!(store.held().facts, 2);
    assert_eq!(positions(&store, 1), Ok(vec![Position::from(2), Position::from(3)]));
}

#[test]
fn a_read_of_facts_that_were_trimmed_is_refused() {
    let store: Memory = store(2, 2);
    add(&store, "t-1");
    add(&store, "t-2");
    add(&store, "t-3");
    assert_eq!(
        positions(&store, 0),
        Err(StoreError::Trimmed {
            after: Position::ZERO,
            oldest: Position::from(2)
        })
    );
}

#[test]
fn a_state_outlives_its_trimmed_facts() {
    let store: Memory = store(1, 2);
    add(&store, "t-1");
    add(&store, "t-2");
    assert_eq!(store.held().states, 2);
}

#[test]
fn refusals_beyond_the_kept_number_drop_the_oldest() {
    let store: Memory = store(2, 1);
    refuse(&store, "t-1");
    refuse(&store, "t-2");
    assert_eq!(store.held().refusals, 1);
    assert_eq!(store.read(|view: &mut View<'_>| view.refusals::<Task>(&id("t-1"))), Ok(Vec::new()));
    assert_eq!(
        store.read(|view: &mut View<'_>| view.refusals::<Task>(&id("t-2"))),
        Ok(vec![TaskRefusal::Absent])
    );
}

#[test]
fn a_done_request_is_no_longer_held() {
    let store: Memory = store(2, 2);
    request(&store, "p-1");
    let claimed: Vec<Claimed<FulfilmentRequest>> = store.claim::<Progress>(10, Moment::ZERO, Moment::ZERO).expect("claimed");
    let _finished: Finished<Fulfilment> = store
        .write(|unit| unit.finish(claimed[0].id, |state: Fulfilment| ports::react(Progress::StockUnavailable, state)))
        .expect("finished");
    assert_eq!(store.held().requests, 0);
}

#[test]
fn a_failed_request_is_still_held() {
    let store: Memory = store(2, 2);
    request(&store, "p-1");
    let claimed: Vec<Claimed<FulfilmentRequest>> = store.claim::<Progress>(10, Moment::ZERO, Moment::ZERO).expect("claimed");
    store.failed(claimed[0].id, Moment::ZERO).expect("failed");
    assert_eq!(store.held().requests, 1);
}

#[test]
fn a_parked_request_is_held_as_parked_with_its_cause_and_its_attempts() {
    let store: Memory = store(2, 2);
    request(&store, "p-1");
    store.failed(RequestId::from(1), Moment::ZERO).expect("failed");
    let _parked: Finished<Fulfilment> = store
        .write(|unit| {
            unit.park(RequestId::from(1), "no such warehouse", |state: Fulfilment| {
                ports::react(Progress::ReservationUndelivered, state)
            })
        })
        .expect("parked");
    assert_eq!((store.held().requests, store.held().parked), (0, 1));
    assert_eq!(
        store.parked(),
        vec![Parked {
            id: RequestId::from(1),
            lifecycle: "fulfilment",
            stream: "p-1".to_string(),
            request: "\"ReserveStock\"".to_string(),
            attempts: 2,
            cause: "no such warehouse".to_string()
        }]
    );
}

#[test]
fn a_finished_request_is_not_held_as_parked() {
    let store: Memory = store(2, 2);
    request(&store, "p-1");
    let _finished: Finished<Fulfilment> = store
        .write(|unit| unit.finish(RequestId::from(1), |state: Fulfilment| ports::react(Progress::StockUnavailable, state)))
        .expect("finished");
    assert_eq!(store.parked(), Vec::new());
}

#[test]
fn a_stranded_request_is_held_as_parked_with_its_cause() {
    let store: Memory = store(2, 2);
    request(&store, "p-1");
    assert_eq!(store.write(|unit| unit.strand::<Fulfilment>(RequestId::from(1), "cannot be told")), Ok(true));
    assert_eq!(
        store
            .parked()
            .into_iter()
            .map(|parked: Parked| (parked.id, parked.cause))
            .collect::<Vec<(RequestId, String)>>(),
        vec![(RequestId::from(1), "cannot be told".to_string())]
    );
}

fn park(store: &Memory, stream: &str, number: u32) {
    request(store, stream);
    assert_eq!(
        store.write(|unit| unit.strand::<Fulfilment>(RequestId::from(number), "cannot be told")),
        Ok(true)
    );
}

fn parked_ids(store: &Memory) -> Vec<RequestId> {
    store.parked().into_iter().map(|parked: Parked| parked.id).collect()
}

#[test]
fn with_as_many_parked_requests_as_are_kept_none_is_dropped() {
    let store: Memory = store(2, 2);
    park(&store, "p-1", 1);
    park(&store, "p-2", 2);
    assert_eq!(parked_ids(&store), vec![RequestId::from(1), RequestId::from(2)]);
    assert_eq!((store.held().parked, store.held().forgotten), (2, 0));
}

#[test]
fn parked_requests_beyond_the_kept_number_drop_the_oldest_and_are_counted() {
    let store: Memory = store(2, 2);
    park(&store, "p-1", 1);
    park(&store, "p-2", 2);
    park(&store, "p-3", 3);
    assert_eq!(parked_ids(&store), vec![RequestId::from(2), RequestId::from(3)]);
    assert_eq!((store.held().parked, store.held().forgotten), (2, 1));
}
