#![allow(clippy::expect_used, clippy::panic)]

use std::thread::{self, JoinHandle};

use adapter_postgres::{Durability, Postgres};
use domain::task::{AddTask, Tally, Task, TaskRefusal, TaskTally, Title};
use ports::{Answer, Applied, Journal, Position, Recorded, StoreError, StreamId, Unit};

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
    let applied: Applied<Task> = unit.apply(&StreamId::new(task), |state: Task| ports::decide(AddTask { title }, state))?;
    let fact: &Recorded<Task> = &applied.facts[0];
    let row: Option<Tally> = unit.row::<TaskTally>(TaskTally::KEY)?;
    unit.put(ports::project::<TaskTally>(row, fact))
}

fn writer(schema: &'static str, number: u64) -> JoinHandle<Result<(), StoreError>> {
    thread::spawn(move || {
        let store: Postgres = Postgres::connect(&url(), schema, 2, Durability::Full)?;
        (0..COMMANDS).try_for_each(|command: u64| store.write(|unit| add_and_tally(unit, &format!("t-{number}-{command}"))))
    })
}

#[test]
fn many_writers_on_one_schema_keep_the_tally_equal_to_the_facts() {
    let schema: &'static str = "writers_tally";
    fresh(schema);
    let store: Postgres = Postgres::connect(&url(), schema, 4, Durability::Full).expect("opens");
    let writers: Vec<JoinHandle<Result<(), StoreError>>> = (0..WRITERS).map(|number: u64| writer(schema, number)).collect();
    let outcomes: Vec<Result<(), StoreError>> = writers
        .into_iter()
        .map(|writer: JoinHandle<Result<(), StoreError>>| writer.join().expect("joined"))
        .collect();
    let positions: Vec<Position> = store
        .write(|unit| unit.facts::<Task>(Position::ZERO, 1000))
        .expect("facts")
        .iter()
        .map(Recorded::position)
        .collect();
    let tally: Option<Tally> = store.read(|view| view.row::<TaskTally>(TaskTally::KEY)).expect("tally");
    assert_eq!(
        (outcomes, positions, tally),
        (
            vec![Ok(()); 4],
            (1..=WRITERS * COMMANDS)
                .map(|position: u64| Position::ZERO.after(position))
                .collect::<Vec<Position>>(),
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
    let reader: Postgres = Postgres::connect(&url(), schema, 4, Durability::Full).expect("opens");
    let other: Postgres = Postgres::connect(&url(), schema, 4, Durability::Full).expect("opens");
    let seen: (Option<Tally>, usize) = reader
        .read(|view| {
            let before: Option<Tally> = view.row::<TaskTally>(TaskTally::KEY)?;
            other.write(|unit| add_and_tally(unit, "t-1"))?;
            Ok((before, view.states::<Task>()?.len()))
        })
        .expect("read");
    assert_eq!(seen, (None, 0), "a read saw a commit made after it began");
    assert_eq!(
        reader.read(|view| view.states::<Task>()).map(|tasks: Vec<(StreamId<Task>, Task)>| tasks.len()),
        Ok(1)
    );
}

fn claim(schema: &'static str) -> JoinHandle<Result<Answer<TaskRefusal>, StoreError>> {
    thread::spawn(move || {
        let store: Postgres = Postgres::connect(&url(), schema, 2, Durability::Full)?;
        let title: Title = Title::new("the one").expect("a title");
        let applied: Applied<Task> = store.write(|unit| unit.apply(&StreamId::new("same"), |state: Task| ports::decide(AddTask { title }, state)))?;
        Ok(applied.answer)
    })
}

#[test]
fn writers_claiming_one_stream_leave_exactly_one_accepted() {
    let schema: &'static str = "writers_claim";
    fresh(schema);
    drop(Postgres::connect(&url(), schema, 4, Durability::Full).expect("opens"));
    let claims: Vec<JoinHandle<Result<Answer<TaskRefusal>, StoreError>>> = (0..WRITERS).map(|_writer: u64| claim(schema)).collect();
    let mut answers: Vec<Result<Answer<TaskRefusal>, StoreError>> = claims
        .into_iter()
        .map(|claim: JoinHandle<Result<Answer<TaskRefusal>, StoreError>>| claim.join().expect("joined"))
        .collect();
    answers.sort_by_key(|answer: &Result<Answer<TaskRefusal>, StoreError>| format!("{answer:?}"));
    assert_eq!(
        answers,
        vec![
            Ok(Answer::Accepted(ports::Version::from(1))),
            Ok(Answer::Refused(TaskRefusal::AlreadyExists)),
            Ok(Answer::Refused(TaskRefusal::AlreadyExists)),
            Ok(Answer::Refused(TaskRefusal::AlreadyExists))
        ]
    );
}
