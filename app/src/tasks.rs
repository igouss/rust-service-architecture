use domain::task::{Tally, Task, TaskEvent, TaskTally, Title};
use ports::{Journal, Recorded, StoreError, StreamId, Unit, View};

use crate::{project, Policy};

impl Policy for Task {
    fn after<U: Unit>(unit: &mut U, fact: &Recorded<TaskEvent>) -> Result<(), StoreError> {
        project::<U, TaskTally>(unit, fact)
    }
}

fn open_in<V: View>(view: &mut V) -> Result<Vec<(StreamId, Title)>, StoreError> {
    let tasks: Vec<(StreamId, Task)> = view.states::<Task>()?;
    Ok(tasks
        .into_iter()
        .filter_map(|(id, task): (StreamId, Task)| match task {
            Task::Open { title } => Some((id, title)),
            Task::Absent | Task::Done { .. } => None,
        })
        .collect())
}

pub fn open_tasks<J: Journal>(journal: &J) -> Result<Vec<(StreamId, Title)>, StoreError> {
    journal.read(|view: &mut J::View<'_>| open_in(view))
}

pub fn task_tally<J: Journal>(journal: &J) -> Result<Option<Tally>, StoreError> {
    journal.read(|view: &mut J::View<'_>| view.row::<TaskTally>(TaskTally::KEY))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskBoard {
    pub open: Vec<(StreamId, Title)>,
    pub tally: Option<Tally>,
}

pub fn task_board<J: Journal>(journal: &J) -> Result<TaskBoard, StoreError> {
    journal.read(|view: &mut J::View<'_>| {
        Ok(TaskBoard {
            open: open_in(view)?,
            tally: view.row::<TaskTally>(TaskTally::KEY)?,
        })
    })
}
