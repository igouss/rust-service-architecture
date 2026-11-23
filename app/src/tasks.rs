use domain::task::{Tally, Task, TaskEvent, TaskTally, Title};
use ports::{Journal, Recorded, StoreError, StreamId, Unit, View};

use crate::{project, Policy};

impl Policy for Task {
    fn after(unit: &mut Unit<'_>, fact: &Recorded<TaskEvent>) -> Result<(), StoreError> {
        project::<TaskTally>(unit, fact)
    }
}

fn open_in(view: &mut View<'_>) -> Result<Vec<(StreamId, Title)>, StoreError> {
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
    journal.read(|view: &mut View<'_>| open_in(view))
}

pub fn task_tally<J: Journal>(journal: &J) -> Result<Option<Tally>, StoreError> {
    journal.read(|view: &mut View<'_>| view.row::<TaskTally>(TaskTally::KEY))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskBoard {
    pub open: Vec<(StreamId, Title)>,
    pub tally: Option<Tally>,
}

pub(crate) fn task_board_in(view: &mut View<'_>) -> Result<TaskBoard, StoreError> {
    Ok(TaskBoard {
        open: open_in(view)?,
        tally: view.row::<TaskTally>(TaskTally::KEY)?,
    })
}

pub fn task_board<J: Journal>(journal: &J) -> Result<TaskBoard, StoreError> {
    journal.read(|view: &mut View<'_>| task_board_in(view))
}
