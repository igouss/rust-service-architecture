use std::fmt;
use std::time::{Duration, Instant};

use domain::task::Title;
use ports::{Answer, StoreError, StreamId, Version};

use crate::fault::Fault;
use crate::stats::{after_warm_ups, micros, summarize, Summary};

pub const WARM_UPS: usize = 5;
pub const TIMED: usize = 30;
pub const CALLS: usize = WARM_UPS + TIMED;
const TITLE: &str = "buy milk";

pub type Call<'a> = &'a mut dyn FnMut(usize) -> Result<Duration, Fault>;

pub fn title() -> Result<Title, Fault> {
    Title::new(TITLE).map_err(|error: domain::task::TitleError| Fault::Unexpected(error.to_string()))
}

pub fn timed<T>(work: impl FnOnce() -> T) -> (Duration, T) {
    let started: Instant = Instant::now();
    let value: T = work();
    (started.elapsed(), value)
}

pub fn signed(value: u64) -> Result<i64, Fault> {
    i64::try_from(value).map_err(|_: std::num::TryFromIntError| Fault::Unexpected(format!("{value} does not fit a column")))
}

pub fn count(value: usize) -> Result<u64, Fault> {
    u64::try_from(value).map_err(|_: std::num::TryFromIntError| Fault::Unexpected(format!("{value} does not fit a count")))
}

pub fn same<T: PartialEq + fmt::Debug>(what: &str, found: T, expected: T) -> Result<(), Fault> {
    if found == expected {
        return Ok(());
    }
    Err(Fault::Unexpected(format!("{what}: found {found:?}, expected {expected:?}")))
}

pub fn accepted<J: PartialEq + fmt::Debug>(answer: Result<Answer<J>, StoreError>, version: u64, id: &StreamId) -> Result<(), Fault> {
    same(&id.0, answer, Ok(Answer::Accepted(Version(version))))
}

pub fn summary(samples: Vec<Duration>) -> Result<Summary, Fault> {
    summarize(after_warm_ups(samples, WARM_UPS)).ok_or_else(|| Fault::Unexpected("no samples".to_string()))
}

pub fn rounds(cells: &mut [Call<'_>]) -> Result<Vec<Summary>, Fault> {
    let mut samples: Vec<Vec<Duration>> = cells.iter().map(|_: &Call<'_>| Vec::with_capacity(CALLS)).collect();
    for round in 0..CALLS {
        for (cell, taken) in cells.iter_mut().zip(samples.iter_mut()) {
            taken.push(cell(round)?);
        }
    }
    samples.into_iter().map(summary).collect()
}

pub fn heading(label: &str) {
    println!("{:<58} | {:>10} {:>10} {:>10}", label, "min", "p50", "p90");
}

pub fn row(label: &str, summary: &Summary) {
    println!(
        "{:<58} | {:>10.1} {:>10.1} {:>10.1}",
        label,
        micros(summary.min),
        micros(summary.p50),
        micros(summary.p90)
    );
}

pub fn table(label: &str, labels: &[String], summaries: &[Summary]) {
    heading(label);
    for (label, summary) in labels.iter().zip(summaries) {
        row(label, summary);
    }
}
