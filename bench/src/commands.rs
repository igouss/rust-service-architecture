use std::path::{Path, PathBuf};
use std::time::Duration;

use adapter_sqlite::{Durability, Sqlite, SqliteView};
use app::payments::fulfilment_of;
use app::tasks::task_tally;
use app::{handle, rebuild};
use domain::fulfilment::{Fulfilment, Progress};
use domain::payment::{Amount, Authorize, Capture, Payment};
use domain::task::{AddTask, CompleteTask, Tally, Task, TaskEvent, TaskTally};
use domain::{Lifecycle, Projection};
use ports::{Answer, Journal, Outbox, StoreError, StreamId, Version, View};
use rusqlite::{Connection, Row, Statement, Transaction};

use crate::fault::Fault;
use crate::measure::{accepted, count, rounds, same, signed, table, timed, title, CALLS, TIMED, WARM_UPS};
use crate::stats::{micros, ratio, Summary};

const HISTORIES: [u64; 3] = [1, 100, 10_000];
const CENTS: u64 = 500;
const REBUILD_BATCH: usize = 1_000;

struct Seeded {
    streams: u64,
    facts: u64,
}

fn journal_mode(path: &Path) -> Result<String, Fault> {
    let connection: Connection = Connection::open(path)?;
    Ok(connection.query_row("PRAGMA journal_mode", [], |row: &Row<'_>| row.get::<_, String>(0))?)
}

fn history_stream(length: u64, call: usize) -> StreamId {
    StreamId(format!("h-{length:05}-{call:02}"))
}

fn seed_histories(path: &Path) -> Result<Seeded, Fault> {
    let mut connection: Connection = Connection::open(path)?;
    let transaction: Transaction<'_> = connection.transaction()?;
    let added: String = serde_json::to_string(&TaskEvent::Added { title: title()? })?;
    let open: String = serde_json::to_string(&Task::Open { title: title()? })?;
    let mut seeded: Seeded = Seeded { streams: 0, facts: 0 };
    {
        let mut fact: Statement<'_> = transaction.prepare("INSERT INTO facts (lifecycle, stream, version, event) VALUES (?1, ?2, ?3, ?4)")?;
        let mut state: Statement<'_> = transaction.prepare("INSERT INTO states (lifecycle, stream, version, state) VALUES (?1, ?2, ?3, ?4)")?;
        for length in HISTORIES {
            for call in 0..CALLS {
                let id: StreamId = history_stream(length, call);
                for version in 1..=length {
                    fact.execute((Task::NAME, &id.0, signed(version)?, &added))?;
                }
                state.execute((Task::NAME, &id.0, signed(length)?, &open))?;
                seeded.streams += 1;
                seeded.facts += length;
            }
        }
    }
    let last: i64 = transaction.query_row("SELECT max(position) FROM facts", [], |row: &Row<'_>| row.get::<_, i64>(0))?;
    let tally: String = serde_json::to_string(&Tally { open: seeded.facts, done: 0 })?;
    transaction.execute(
        "INSERT INTO read_models (name, key, row) VALUES (?1, ?2, ?3)",
        (TaskTally::NAME, TaskTally::KEY, &tally),
    )?;
    transaction.execute("INSERT INTO read_model_positions (name, position) VALUES (?1, ?2)", (TaskTally::NAME, last))?;
    transaction.commit()?;
    Ok(seeded)
}

fn complete(store: &Sqlite, length: u64, call: usize) -> Result<Duration, Fault> {
    let id: StreamId = history_stream(length, call);
    let (elapsed, answer): (Duration, Result<Answer<<Task as Lifecycle>::Refusal>, StoreError>) = timed(|| handle(store, &id, CompleteTask));
    accepted(answer, length + 1, &id)?;
    Ok(elapsed)
}

fn history_table(summaries: &[Summary]) {
    println!("{:>7} | {:>10} {:>10} {:>10} | {:>14}", "N", "min", "p50", "p90", "p50 over N=1");
    for (length, summary) in HISTORIES.iter().zip(summaries) {
        let base: &Summary = summaries.first().unwrap_or(summary);
        println!(
            "{:>7} | {:>10.1} {:>10.1} {:>10.1} | {:>14.2}",
            length,
            micros(summary.min),
            micros(summary.p50),
            micros(summary.p90),
            ratio(summary.p50, base.p50)
        );
    }
}

pub fn histories(directory: &Path, durability: Durability) -> Result<(), Fault> {
    let path: PathBuf = directory.join("history.sqlite");
    drop(Sqlite::open(&path, durability)?);
    let seeded: Seeded = seed_histories(&path)?;
    let store: Sqlite = Sqlite::open(&path, durability)?;
    println!("journal_mode read back from a second connection: {}", journal_mode(&path)?);
    println!();
    println!("a. app::handle(CompleteTask) on a task whose history already holds N facts. The command loads the state row and");
    println!("   writes one fact, the state row and the tally row. No command accumulates N facts on one task, so the histories");
    println!("   are seeded by raw SQL in one transaction: N Added facts per task (versions 1 to N), its state row Open at");
    println!("   version N (the fold of those facts), and the tally row and its position as app::rebuild folds those facts");
    println!(
        "   (one open per Added fact, so the tally counts facts here, not tasks). {} tasks, {} facts.",
        seeded.streams, seeded.facts
    );
    println!("   Each call on its own task; the three N alternate; {WARM_UPS} warm-ups then {TIMED} timed calls per cell; microseconds;");
    println!("   percentiles by nearest rank");
    let [one, hundred, many]: [u64; 3] = HISTORIES;
    let summaries: Vec<Summary> = rounds(&mut [
        &mut |call: usize| complete(&store, one, call),
        &mut |call: usize| complete(&store, hundred, call),
        &mut |call: usize| complete(&store, many, call),
    ])?;
    history_table(&summaries);
    let live: Option<Tally> = Some(Tally {
        open: seeded.facts - seeded.streams,
        done: seeded.streams,
    });
    same("tally after every seeded task was completed", task_tally(&store)?, live)?;
    let folded: u64 = count(rebuild::<Sqlite, TaskTally>(&store, REBUILD_BATCH)?)?;
    same("facts folded by a rebuild of the tally", folded, seeded.facts + seeded.streams)?;
    same("tally rebuilt from the seeded and the written facts", task_tally(&store)?, live)?;
    println!("   checked after the cell: the live tally equals app::rebuild over all {folded} facts");
    Ok(())
}

pub fn add(store: &Sqlite, prefix: &str, call: usize) -> Result<Duration, Fault> {
    let id: StreamId = StreamId(format!("{prefix}-{call:02}"));
    let command: AddTask = AddTask { title: title()? };
    let (elapsed, answer): (Duration, Result<Answer<<Task as Lifecycle>::Refusal>, StoreError>) = timed(|| handle(store, &id, command));
    accepted(answer, 1, &id)?;
    Ok(elapsed)
}

fn payment(call: usize) -> StreamId {
    StreamId(format!("p-{call:02}"))
}

fn amount() -> Result<Amount, Fault> {
    Amount::new(CENTS).ok_or_else(|| Fault::Unexpected("the benchmark amount is zero".to_string()))
}

fn authorize(store: &Sqlite) -> Result<(), Fault> {
    for call in 0..CALLS {
        let id: StreamId = payment(call);
        accepted(handle(store, &id, Authorize { amount: amount()? }), 1, &id)?;
    }
    Ok(())
}

fn capture(store: &Sqlite, call: usize) -> Result<Duration, Fault> {
    let id: StreamId = payment(call);
    let command: Capture = Capture { amount: amount()? };
    let (elapsed, answer): (Duration, Result<Answer<<Payment as Lifecycle>::Refusal>, StoreError>) = timed(|| handle(store, &id, command));
    accepted(answer, 2, &id)?;
    let fulfilment: (Version, Fulfilment) = store.read(|view: &mut SqliteView<'_>| view.state::<Fulfilment>(&fulfilment_of(&id)))?;
    same("fulfilment after the capture", fulfilment, (Version(1), Fulfilment::Reserving))?;
    Ok(elapsed)
}

pub fn commands(directory: &Path, durability: Durability) -> Result<(), Fault> {
    let adding: Sqlite = Sqlite::open(&directory.join("add.sqlite"), durability)?;
    let capture_path: PathBuf = directory.join("capture.sqlite");
    let authorized: Sqlite = Sqlite::open(&capture_path, durability)?;
    authorize(&authorized)?;
    drop(authorized);
    let capturing: Sqlite = Sqlite::open(&capture_path, durability)?;
    println!();
    println!("b, c. One command per call, each on its own stream, each command in its own store; the two alternate;");
    println!("   the payments were authorized through handle(Authorize) before the clock, then the store reopened;");
    println!("   {WARM_UPS} warm-ups then {TIMED} timed calls per row; microseconds");
    let summaries: Vec<Summary> = rounds(&mut [&mut |call: usize| add(&adding, "a", call), &mut |call: usize| capture(&capturing, call)])?;
    let labels: [String; 2] = [
        "b. handle(AddTask): 1 fact, 1 state row, tally row".to_string(),
        "c. handle(Capture): 2 facts, 2 state rows, 1 outbox row".to_string(),
    ];
    table("command and what its unit of work writes", &labels, &summaries);
    let calls: u64 = count(CALLS)?;
    same("tally after the added tasks", task_tally(&adding)?, Some(Tally { open: calls, done: 0 }))?;
    same("outbox requests after the captures", capturing.claim::<Progress>(CALLS + 1)?.len(), CALLS)
}
