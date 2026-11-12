#![allow(clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use adapter_sqlite::{Durability, Sqlite};
use domain::payment::{Amount, Authorize, Payment, PaymentEvent};
use domain::task::{AddTask, CompleteTask, Tally, Task, TaskEvent, TaskTally, Title};
use ports::{Applied, Journal, Position, Recorded, StoreError, StreamId, Unit, Version, View};

static FILES: AtomicU64 = AtomicU64::new(0);

fn file() -> PathBuf {
    let number: u64 = FILES.fetch_add(1, Ordering::SeqCst);
    let path: PathBuf = std::env::temp_dir().join(format!("adapter-sqlite-{}-{number}.db", std::process::id()));
    let _stale: Result<(), std::io::Error> = std::fs::remove_file(&path);
    path
}

fn id(text: &str) -> StreamId {
    StreamId(text.to_string())
}

fn title(text: &str) -> Title {
    Title::new(text).expect("a title")
}

fn add(store: &Sqlite, task: &str) -> Vec<Recorded<TaskEvent>> {
    let applied: Applied<Task> = store
        .write(|unit| unit.apply(&id(task), |state: Task| ports::decide(AddTask { title: title(task) }, state)))
        .expect("applied");
    applied.facts
}

fn task_facts(store: &Sqlite, after: u64, limit: usize) -> Vec<Recorded<TaskEvent>> {
    store.write(|unit| unit.facts::<Task>(Position(after), limit)).expect("facts")
}

fn added(position: u64, task: &str) -> Recorded<TaskEvent> {
    Recorded {
        position: Position(position),
        stream: id(task),
        version: Version(1),
        event: TaskEvent::Added { title: title(task) },
    }
}

#[test]
fn a_stream_never_written_loads_the_initial_state_at_version_zero() {
    let store: Sqlite = Sqlite::in_memory().expect("opens");
    assert_eq!(store.read(|view| view.state::<Task>(&id("t-1"))), Ok((Version(0), Task::Absent)));
}

#[test]
fn an_applied_outcome_returns_its_recorded_facts() {
    let store: Sqlite = Sqlite::in_memory().expect("opens");
    assert_eq!(add(&store, "t-1"), vec![added(1, "t-1")]);
}

#[test]
fn a_reopened_file_loads_the_same_state_and_version() {
    let path: PathBuf = file();
    let first: Sqlite = Sqlite::open(&path, Durability::Full).expect("opens");
    add(&first, "t-1");
    drop(first);
    let second: Sqlite = Sqlite::open(&path, Durability::Full).expect("reopens");
    assert_eq!(
        second.read(|view| view.state::<Task>(&id("t-1"))),
        Ok((Version(1), Task::Open { title: title("t-1") }))
    );
}

#[test]
fn two_lifecycles_may_share_a_stream_id() {
    let store: Sqlite = Sqlite::in_memory().expect("opens");
    add(&store, "x");
    let amount: Amount = Amount::new(5).expect("positive");
    let _applied: Applied<Payment> = store
        .write(|unit| unit.apply(&id("x"), |state: Payment| ports::decide(Authorize { amount }, state)))
        .expect("applied");
    assert_eq!(
        store.read(|view| view.state::<Payment>(&id("x"))),
        Ok((Version(1), Payment::Authorized { amount }))
    );
    assert_eq!(
        store.read(|view| view.state::<Task>(&id("x"))),
        Ok((Version(1), Task::Open { title: title("x") }))
    );
}

#[test]
fn with_no_facts_the_log_returns_none() {
    let store: Sqlite = Sqlite::in_memory().expect("opens");
    assert_eq!(task_facts(&store, 0, 10), Vec::new());
}

#[test]
fn facts_come_back_in_commit_order_for_their_lifecycle_only() {
    let store: Sqlite = Sqlite::in_memory().expect("opens");
    add(&store, "t-1");
    let amount: Amount = Amount::new(5).expect("positive");
    let _payment: Applied<Payment> = store
        .write(|unit| unit.apply(&id("p-1"), |state: Payment| ports::decide(Authorize { amount }, state)))
        .expect("applied");
    add(&store, "t-2");
    assert_eq!(task_facts(&store, 0, 10), vec![added(1, "t-1"), added(3, "t-2")]);
    assert_eq!(
        store
            .write(|unit| unit.facts::<Payment>(Position(0), 10))
            .map(|facts: Vec<Recorded<PaymentEvent>>| facts.len()),
        Ok(1)
    );
}

#[test]
fn facts_resume_after_a_position_and_honour_the_limit() {
    let store: Sqlite = Sqlite::in_memory().expect("opens");
    add(&store, "t-1");
    add(&store, "t-2");
    add(&store, "t-3");
    assert_eq!(task_facts(&store, 1, 1), vec![added(2, "t-2")]);
}

#[test]
fn a_second_fact_of_a_stream_takes_the_next_version() {
    let store: Sqlite = Sqlite::in_memory().expect("opens");
    add(&store, "t-1");
    let applied: Applied<Task> = store
        .write(|unit| unit.apply(&id("t-1"), |state: Task| ports::decide(CompleteTask, state)))
        .expect("applied");
    assert_eq!(
        applied.facts,
        vec![Recorded {
            position: Position(2),
            stream: id("t-1"),
            version: Version(2),
            event: TaskEvent::Completed
        }]
    );
}

#[test]
fn a_change_at_or_below_the_read_models_position_is_refused() {
    let store: Sqlite = Sqlite::in_memory().expect("opens");
    let fact: Recorded<TaskEvent> = added(1, "t-1");
    store.write(|unit| unit.put(ports::project::<TaskTally>(None, &fact))).expect("first put");
    let second: Result<(), StoreError> = store.write(|unit| unit.put(ports::project::<TaskTally>(None, &fact)));
    assert_eq!(
        second,
        Err(StoreError::OutOfOrder {
            read_model: "task_tally_v1",
            at: Position(1),
            offered: Position(1)
        })
    );
}

#[test]
fn a_cleared_read_model_has_no_rows_and_accepts_position_one_again() {
    let store: Sqlite = Sqlite::in_memory().expect("opens");
    let fact: Recorded<TaskEvent> = added(1, "t-1");
    store.write(|unit| unit.put(ports::project::<TaskTally>(None, &fact))).expect("first put");
    store.write(|unit| unit.clear::<TaskTally>()).expect("cleared");
    assert_eq!(store.read(|view| view.rows::<TaskTally>()), Ok(Vec::new()));
    assert_eq!(store.write(|unit| unit.put(ports::project::<TaskTally>(None, &fact))), Ok(()));
}

#[test]
fn a_put_row_is_listed_under_its_key() {
    let store: Sqlite = Sqlite::in_memory().expect("opens");
    let fact: Recorded<TaskEvent> = added(1, "t-1");
    store.write(|unit| unit.put(ports::project::<TaskTally>(None, &fact))).expect("put");
    assert_eq!(
        store.read(|view| view.rows::<TaskTally>()),
        Ok(vec![("all".to_string(), Tally { open: 1, done: 0 })])
    );
}

#[test]
fn a_corrupt_state_row_is_reported_as_corrupt() {
    let path: PathBuf = file();
    let store: Sqlite = Sqlite::open(&path, Durability::Normal).expect("opens");
    add(&store, "t-1");
    drop(store);
    let raw: rusqlite::Connection = rusqlite::Connection::open(&path).expect("raw");
    raw.execute("UPDATE states SET state = 'not json'", []).expect("corrupted");
    drop(raw);
    let reopened: Sqlite = Sqlite::open(&path, Durability::Normal).expect("reopens");
    let loaded: Result<(Version, Task), StoreError> = reopened.read(|view| view.state::<Task>(&id("t-1")));
    assert!(matches!(loaded, Err(StoreError::Corrupt { table: "states", .. })));
}

#[test]
fn a_unit_that_panics_stores_nothing_and_leaves_the_store_usable() {
    let store: Sqlite = Sqlite::in_memory().expect("opens");
    let panicked: std::thread::Result<Result<(), StoreError>> = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        store.write(|unit| {
            let _applied: Applied<Task> = unit.apply(&id("t-1"), |state: Task| ports::decide(AddTask { title: title("t-1") }, state))?;
            panic!("a bug inside a unit of work")
        })
    }));
    assert!(panicked.is_err());
    assert_eq!(store.read(|view| view.state::<Task>(&id("t-1"))), Ok((Version(0), Task::Absent)));
    assert_eq!(add(&store, "t-2").len(), 1);
}
