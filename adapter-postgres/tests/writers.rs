#![allow(clippy::expect_used, clippy::panic)]

use std::thread::{self, JoinHandle};

use adapter_postgres::Postgres;
use domain::task::{AddTask, Tally, Task, TaskEvent, TaskTally, Title};
use ports::{Applied, Journal, Position, Recorded, StoreError, StreamId, Unit};

const WRITERS: u64 = 4;
const COMMANDS: u64 = 25;

fn url() -> String {
    std::env::var("STORE_POSTGRES_URL").expect("STORE_POSTGRES_URL names a Postgres database these tests may write to")
}

fn fresh(schema: &str) {
    let mut setup: postgres::Client = postgres::Client::connect(&url(), postgres::NoTls).expect("connects");
    setup.batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE")).expect("dropped");
}

fn add_and_tally(unit: &mut Unit<'_>, task: &str) -> Result<(), StoreError> {
    let title: Title = Title::new(task).expect("a title");
    let applied: Applied<Task> = unit.apply(&StreamId(task.to_string()), |state: Task| ports::decide(AddTask { title }, state))?;
    let fact: &Recorded<TaskEvent> = &applied.facts[0];
    let row: Option<Tally> = unit.row::<TaskTally>(TaskTally::KEY)?;
    unit.put(ports::project::<TaskTally>(row, fact))
}

fn writer(schema: &'static str, number: u64) -> JoinHandle<Result<(), StoreError>> {
    thread::spawn(move || {
        let store: Postgres = Postgres::connect(&url(), schema, 1)?;
        (0..COMMANDS).try_for_each(|command: u64| store.write(|unit| add_and_tally(unit, &format!("t-{number}-{command}"))))
    })
}

#[test]
fn many_writers_on_one_schema_keep_the_tally_equal_to_the_facts() {
    let schema: &'static str = "writers_tally";
    fresh(schema);
    let store: Postgres = Postgres::connect(&url(), schema, 1).expect("opens");
    let writers: Vec<JoinHandle<Result<(), StoreError>>> = (0..WRITERS).map(|number: u64| writer(schema, number)).collect();
    let outcomes: Vec<Result<(), StoreError>> = writers
        .into_iter()
        .map(|writer: JoinHandle<Result<(), StoreError>>| writer.join().expect("joined"))
        .collect();
    let positions: Vec<Position> = store
        .write(|unit| unit.facts::<Task>(Position(0), 1000))
        .expect("facts")
        .iter()
        .map(Recorded::position)
        .collect();
    let tally: Option<Tally> = store.read(|view| view.row::<TaskTally>(TaskTally::KEY)).expect("tally");
    assert_eq!(
        (outcomes, positions, tally),
        (
            vec![Ok(()); 4],
            (1..=WRITERS * COMMANDS).map(Position).collect::<Vec<Position>>(),
            Some(Tally {
                open: WRITERS * COMMANDS,
                done: 0
            })
        ),
        "diverged under concurrent writers"
    );
}

#[test]
fn a_read_keeps_one_snapshot_while_another_writer_commits() {
    let schema: &'static str = "writers_snapshot";
    fresh(schema);
    let reader: Postgres = Postgres::connect(&url(), schema, 1).expect("opens");
    let other: Postgres = Postgres::connect(&url(), schema, 1).expect("opens");
    let seen: (Option<Tally>, usize) = reader
        .read(|view| {
            let before: Option<Tally> = view.row::<TaskTally>(TaskTally::KEY)?;
            other.write(|unit| add_and_tally(unit, "t-1"))?;
            Ok((before, view.states::<Task>()?.len()))
        })
        .expect("read");
    assert_eq!(seen, (None, 0), "a read saw a commit made after it began");
    assert_eq!(reader.read(|view| view.states::<Task>()).map(|tasks: Vec<(StreamId, Task)>| tasks.len()), Ok(1));
}
