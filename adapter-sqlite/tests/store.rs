#![allow(clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use adapter_sqlite::{Durability, Sqlite};
use domain::fulfilment::{Fulfilment, Progress};
use domain::task::{AddTask, Task, Title};
use ports::{Applied, Finished, Journal, Outbox, RequestId, StoreError, StreamId, Version};

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

fn add(store: &Sqlite, task: &str) {
    let _applied: Applied<Task> = store
        .write(|unit| unit.apply(&id(task), |state: Task| ports::decide(AddTask { title: title(task) }, state)))
        .expect("applied");
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
        Ok((Version::from(1), Task::Open { title: title("t-1") }))
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

fn parked(path: &Path) -> Vec<(String, String, i64, String)> {
    let raw: rusqlite::Connection = rusqlite::Connection::open(path).expect("raw");
    let mut statement: rusqlite::Statement<'_> = raw
        .prepare("SELECT outbox.lifecycle, outbox.stream, outbox.attempts, parked.cause FROM parked JOIN outbox USING (id) ORDER BY id")
        .expect("prepared");
    let rows = statement
        .query_map([], |row: &rusqlite::Row<'_>| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
        .expect("queried");
    rows.collect::<Result<Vec<(String, String, i64, String)>, rusqlite::Error>>().expect("read")
}

fn requested(path: &Path) -> Sqlite {
    let store: Sqlite = Sqlite::open(path, Durability::Normal).expect("opens");
    let _applied: Applied<Fulfilment> = store
        .write(|unit| unit.apply(&id("p-1"), |state: Fulfilment| ports::react(Progress::PaymentCaptured, state)))
        .expect("applied");
    store
}

#[test]
fn with_no_request_parked_the_parked_table_is_empty() {
    let path: PathBuf = file();
    let store: Sqlite = requested(&path);
    store.failed(RequestId::from(1)).expect("failed");
    drop(store);
    assert_eq!(parked(&path), Vec::new());
}

#[test]
fn a_parked_request_is_read_with_sql_with_its_cause_and_its_attempts() {
    let path: PathBuf = file();
    let store: Sqlite = requested(&path);
    store.failed(RequestId::from(1)).expect("failed");
    let _parked: Finished<Fulfilment> = store
        .write(|unit| {
            unit.park(RequestId::from(1), "no such warehouse", |state: Fulfilment| {
                ports::react(Progress::ReservationUndelivered, state)
            })
        })
        .expect("parked");
    drop(store);
    assert_eq!(
        parked(&path),
        vec![("fulfilment".to_string(), "p-1".to_string(), 2, "no such warehouse".to_string())]
    );
}

#[test]
fn a_finished_request_is_not_listed_as_parked() {
    let path: PathBuf = file();
    let store: Sqlite = requested(&path);
    let _finished: Finished<Fulfilment> = store
        .write(|unit| unit.finish(RequestId::from(1), |state: Fulfilment| ports::react(Progress::StockUnavailable, state)))
        .expect("finished");
    drop(store);
    assert_eq!(parked(&path), Vec::new());
}

#[test]
fn a_title_stored_under_the_rule_of_its_day_loads_whatever_the_rule_for_new_titles_is_now() {
    let path: PathBuf = file();
    drop(Sqlite::open(&path, Durability::Normal).expect("opens"));
    let text: String = "é".repeat(200);
    let raw: rusqlite::Connection = rusqlite::Connection::open(&path).expect("raw");
    raw.execute(
        "INSERT INTO states (lifecycle, stream, version, state) VALUES ('task', 't-1', 1, ?1)",
        [format!(r#"{{"Open":{{"title":"{text}"}}}}"#)],
    )
    .expect("stored");
    drop(raw);
    let reopened: Sqlite = Sqlite::open(&path, Durability::Normal).expect("reopens");
    let loaded: Result<Option<String>, StoreError> = reopened
        .read(|view| view.state::<Task>(&id("t-1")))
        .map(|(_version, task): (Version, Task)| match task {
            Task::Open { title } => Some(title.as_str().to_string()),
            Task::Absent | Task::Done { .. } => None,
        });
    assert_eq!(loaded, Ok(Some(text)));
}
