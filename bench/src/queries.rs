use std::path::{Path, PathBuf};
use std::time::Duration;

use adapter_sqlite::{Durability, Sqlite};
use app::handle;
use app::tasks::{open_tasks, task_board, task_tally, TaskBoard};
use domain::task::{AddTask, CompleteTask, Tally, Title};
use ports::{StoreError, StreamId};

use crate::fault::Fault;
use crate::measure::{accepted, count, heading, rounds, row, same, timed, title, TIMED, WARM_UPS};
use crate::stats::Summary;

const POPULATIONS: [u64; 5] = [10, 100, 1_000, 10_000, 100_000];

fn populate(path: &Path, tasks: u64) -> Result<(), Fault> {
    let store: Sqlite = Sqlite::open(path, Durability::Normal)?;
    for number in 0..tasks {
        let id: StreamId = StreamId(format!("t-{number:06}"));
        accepted(handle(&store, &id, AddTask { title: title()? }), 1, &id)?;
        if number % 2 == 1 {
            accepted(handle(&store, &id, CompleteTask), 2, &id)?;
        }
    }
    Ok(())
}

fn list(store: &Sqlite, open: u64) -> Result<Duration, Fault> {
    let (elapsed, listed): (Duration, Result<Vec<(StreamId, Title)>, StoreError>) = timed(|| open_tasks(store));
    same("open tasks listed", count(listed?.len())?, open)?;
    Ok(elapsed)
}

fn tally(store: &Sqlite, expected: Tally) -> Result<Duration, Fault> {
    let (elapsed, found): (Duration, Result<Option<Tally>, StoreError>) = timed(|| task_tally(store));
    same("task tally", found?, Some(expected))?;
    Ok(elapsed)
}

fn board(store: &Sqlite, expected: Tally) -> Result<Duration, Fault> {
    let (elapsed, found): (Duration, Result<TaskBoard, StoreError>) = timed(|| task_board(store));
    let found: TaskBoard = found?;
    same("open tasks on the board", count(found.open.len())?, expected.open)?;
    same("tally on the board", found.tally, Some(expected))?;
    Ok(elapsed)
}

pub fn queries(directory: &Path, durability: Durability) -> Result<(), Fault> {
    println!();
    println!("d. Queries over a store of T tasks, every second one completed (T/2 open, T/2 done), each T in its own file.");
    println!("   Seeded through app::handle(AddTask) and app::handle(CompleteTask) under Durability::Normal, one unit of work");
    println!("   per command, then the store closed and reopened under the durability measured.");
    println!("   open_tasks reads and decodes every Task state row (JSON) and keeps the open ones; task_tally reads one");
    println!("   read-model row; task_board does both in one snapshot. The three alternate;");
    println!("   {WARM_UPS} warm-ups then {TIMED} timed calls per row; microseconds");
    heading("query");
    for tasks in POPULATIONS {
        let path: PathBuf = directory.join(format!("tasks-{tasks}.sqlite"));
        populate(&path, tasks)?;
        let store: Sqlite = Sqlite::open(&path, durability)?;
        let done: u64 = tasks / 2;
        let expected: Tally = Tally { open: tasks - done, done };
        let summaries: Vec<Summary> = rounds(&mut [
            &mut |_: usize| list(&store, expected.open),
            &mut |_: usize| tally(&store, expected),
            &mut |_: usize| board(&store, expected),
        ])?;
        let labels: [String; 3] = [
            format!("open_tasks, {tasks} tasks"),
            format!("task_tally, {tasks} tasks"),
            format!("task_board, {tasks} tasks"),
        ];
        for (label, summary) in labels.iter().zip(&summaries) {
            row(label, summary);
        }
    }
    Ok(())
}
