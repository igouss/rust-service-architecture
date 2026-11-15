#![allow(clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use adapter_sqlite::{Durability, Sqlite};
use domain::task::{AddTask, Task, Title};
use ports::{Applied, Journal, StoreError, StreamId, Version};

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
        Ok((Version(1), Task::Open { title: title("t-1") }))
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
