use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use adapter_sqlite::{Durability, Sqlite};
use app::handle;
use app::tasks::task_tally;
use domain::task::{AddTask, Tally, Task};
use domain::Lifecycle;
use ports::{Answer, StoreError, StreamId};
use tokio::runtime::Runtime;
use tokio::task::JoinError;

use crate::commands::add;
use crate::fault::Fault;
use crate::measure::{accepted, count, same, summary, table, title, CALLS, TIMED, WARM_UPS};
use crate::stats::Summary;

async fn hopped(store: &Arc<Sqlite>, call: usize) -> Result<Duration, Fault> {
    let id: StreamId = StreamId(format!("s-{call:02}"));
    let stream: StreamId = id.clone();
    let shared: Arc<Sqlite> = Arc::clone(store);
    let command: AddTask = AddTask { title: title()? };
    let started: Instant = Instant::now();
    let answer: Result<Result<Answer<<Task as Lifecycle>::Refusal>, StoreError>, JoinError> =
        tokio::task::spawn_blocking(move || handle(shared.as_ref(), &stream, command)).await;
    let elapsed: Duration = started.elapsed();
    accepted(answer?, 1, &id)?;
    Ok(elapsed)
}

async fn empty_hop() -> Result<Duration, Fault> {
    let started: Instant = Instant::now();
    let joined: Result<(), JoinError> = tokio::task::spawn_blocking(|| ()).await;
    let elapsed: Duration = started.elapsed();
    joined?;
    Ok(elapsed)
}

async fn hopping(store: Arc<Sqlite>) -> Result<Vec<Summary>, Fault> {
    let mut direct: Vec<Duration> = Vec::with_capacity(CALLS);
    let mut through: Vec<Duration> = Vec::with_capacity(CALLS);
    let mut empty: Vec<Duration> = Vec::with_capacity(CALLS);
    for call in 0..CALLS {
        direct.push(add(&store, "d", call)?);
        through.push(hopped(&store, call).await?);
        empty.push(empty_hop().await?);
    }
    [direct, through, empty].into_iter().map(summary).collect()
}

pub fn hop(directory: &Path, durability: Durability, runtime: &Runtime) -> Result<(), Fault> {
    let store: Arc<Sqlite> = Arc::new(Sqlite::open(&directory.join("hop.sqlite"), durability)?);
    let summaries: Vec<Summary> = runtime.block_on(runtime.spawn(hopping(Arc::clone(&store))))??;
    println!();
    println!("e. The thread hop: handle(AddTask) on one store, called from a task on a worker thread of a multi-thread Tokio");
    println!("   runtime, first directly on that worker and then through tokio::task::spawn_blocking(..).await; the third row");
    println!("   sends an empty closure the same way. The three alternate; {WARM_UPS} warm-ups then {TIMED} timed calls per row; microseconds");
    let labels: [String; 3] = [
        "handle(AddTask) called on the worker thread".to_string(),
        "handle(AddTask) through spawn_blocking".to_string(),
        "an empty closure through spawn_blocking".to_string(),
    ];
    table("variant", &labels, &summaries);
    let calls: u64 = count(CALLS)?;
    same(
        "tally after both variants",
        task_tally(store.as_ref())?,
        Some(Tally { open: 2 * calls, done: 0 }),
    )
}
