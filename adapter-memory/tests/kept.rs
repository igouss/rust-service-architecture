#![allow(clippy::expect_used, clippy::panic)]

use adapter_memory::{Held, Kept, Memory};
use domain::fulfilment::{Fulfilment, Progress};
use domain::task::{AddTask, CompleteTask, Task, TaskEvent, Title};
use ports::{Applied, Claimed, Journal, Outbox, Position, Recorded, StoreError, StreamId};

fn id(text: &str) -> StreamId {
    StreamId(text.to_string())
}

fn store(facts: usize, refusals: usize) -> Memory {
    Memory::new(Kept { facts, refusals })
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
        .write(|unit| unit.facts::<Task>(Position(after), 10))
        .map(|facts: Vec<Recorded<TaskEvent>>| facts.iter().map(Recorded::position).collect())
}

#[test]
fn a_new_store_holds_nothing() {
    assert_eq!(
        store(2, 2).held(),
        Held {
            states: 0,
            facts: 0,
            refusals: 0,
            requests: 0,
            rows: 0
        }
    );
}

#[test]
fn with_as_many_facts_as_are_kept_none_is_trimmed() {
    let store: Memory = store(2, 2);
    add(&store, "t-1");
    add(&store, "t-2");
    assert_eq!(positions(&store, 0), Ok(vec![Position(1), Position(2)]));
}

#[test]
fn facts_beyond_the_kept_number_are_dropped_and_positions_keep_counting() {
    let store: Memory = store(2, 2);
    add(&store, "t-1");
    add(&store, "t-2");
    add(&store, "t-3");
    assert_eq!(store.held().facts, 2);
    assert_eq!(positions(&store, 1), Ok(vec![Position(2), Position(3)]));
}

#[test]
fn a_read_of_facts_that_were_trimmed_is_refused() {
    let store: Memory = store(2, 2);
    add(&store, "t-1");
    add(&store, "t-2");
    add(&store, "t-3");
    assert!(matches!(positions(&store, 0), Err(StoreError::Unavailable { .. })));
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
    assert_eq!(store.rejections("task", &id("t-1")), Ok(Vec::new()));
    assert_eq!(store.rejections("task", &id("t-2")).map(|refusals: Vec<String>| refusals.len()), Ok(1));
}

#[test]
fn a_done_request_is_no_longer_held() {
    let store: Memory = store(2, 2);
    request(&store, "p-1");
    let claimed: Vec<Claimed<<Progress as domain::Reaction>::Request>> = store.claim::<Progress>(10).expect("claimed");
    store.done(claimed[0].id).expect("done");
    assert_eq!(store.held().requests, 0);
}

#[test]
fn a_failed_request_is_still_held() {
    let store: Memory = store(2, 2);
    request(&store, "p-1");
    let claimed: Vec<Claimed<<Progress as domain::Reaction>::Request>> = store.claim::<Progress>(10).expect("claimed");
    store.failed(claimed[0].id).expect("failed");
    assert_eq!(store.held().requests, 1);
}
