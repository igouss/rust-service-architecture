use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{Command, Lifecycle, Projection};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct Title(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleError {
    Blank,
    TooLong,
}

impl fmt::Display for TitleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TitleError::Blank => write!(formatter, "a title is not blank"),
            TitleError::TooLong => write!(formatter, "a title is at most {} characters", Title::LIMIT),
        }
    }
}

impl Title {
    pub const LIMIT: usize = 200;

    /// If the text is empty or only whitespace, then `new` shall refuse with `Blank`.
    /// If the text is longer than `LIMIT` characters, then `new` shall refuse with `TooLong`.
    /// When the text is neither, `new` shall return a title that holds the text unchanged.
    pub fn new(text: &str) -> Result<Title, TitleError> {
        if text.trim().is_empty() {
            return Err(TitleError::Blank);
        }
        if text.chars().count() > Title::LIMIT {
            return Err(TitleError::TooLong);
        }
        Ok(Title(text.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Title {
    type Error = TitleError;

    /// When a stored title is loaded, `try_from` shall refuse what `new` refuses.
    fn try_from(text: String) -> Result<Title, TitleError> {
        Title::new(&text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Task {
    Absent,
    Open { title: Title },
    Done { title: Title },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskEvent {
    Added { title: Title },
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskRefusal {
    AlreadyExists,
    Absent,
    AlreadyDone,
}

pub struct AddTask {
    pub title: Title,
}

impl Command for AddTask {
    type Reads = Task;
    type Rejection = TaskRefusal;

    /// While the task is `Absent`, when `AddTask` is decided, `decide` shall emit `Added` with the title.
    /// If the task is `Open` or `Done`, then `decide` shall refuse with `AlreadyExists`.
    fn decide(self, reads: &Task) -> Result<Vec<TaskEvent>, TaskRefusal> {
        match reads {
            Task::Absent => Ok(vec![TaskEvent::Added { title: self.title }]),
            Task::Open { .. } | Task::Done { .. } => Err(TaskRefusal::AlreadyExists),
        }
    }
}

pub struct CompleteTask;

impl Command for CompleteTask {
    type Reads = Task;
    type Rejection = TaskRefusal;

    /// While the task is `Open`, when `CompleteTask` is decided, `decide` shall emit `Completed`.
    /// If the task is `Absent`, then `decide` shall refuse with `Absent`.
    /// If the task is `Done`, then `decide` shall refuse with `AlreadyDone`.
    fn decide(self, reads: &Task) -> Result<Vec<TaskEvent>, TaskRefusal> {
        match reads {
            Task::Open { .. } => Ok(vec![TaskEvent::Completed]),
            Task::Absent => Err(TaskRefusal::Absent),
            Task::Done { .. } => Err(TaskRefusal::AlreadyDone),
        }
    }
}

impl Lifecycle for Task {
    const NAME: &'static str = "task";
    type Event = TaskEvent;
    type Refusal = TaskRefusal;

    fn initial() -> Task {
        Task::Absent
    }

    /// While the task is `Absent`, when `Added` is folded, `evolve` shall return `Open` with the title.
    /// While the task is `Open`, when `Completed` is folded, `evolve` shall return `Done` with the same title.
    /// If an event does not apply to the state, then `evolve` shall return the state unchanged.
    fn evolve(self, event: &TaskEvent) -> Task {
        match (self, event) {
            (Task::Absent, TaskEvent::Added { title }) => Task::Open { title: title.clone() },
            (Task::Open { title }, TaskEvent::Completed) => Task::Done { title },
            (s @ Task::Absent, TaskEvent::Completed)
            | (s @ Task::Open { .. }, TaskEvent::Added { .. })
            | (s @ Task::Done { .. }, TaskEvent::Added { .. } | TaskEvent::Completed) => s,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tally {
    pub open: u64,
    pub done: u64,
}

pub struct TaskTally;

impl TaskTally {
    pub const KEY: &'static str = "all";
}

impl Projection for TaskTally {
    const NAME: &'static str = "task_tally_v1";
    type Reads = Task;
    type Row = Tally;

    fn key(_stream: &str, event: &TaskEvent) -> Option<String> {
        match event {
            TaskEvent::Added { .. } | TaskEvent::Completed => Some(TaskTally::KEY.to_string()),
        }
    }

    /// When `Added` is folded, `apply` shall add one to `open`.
    /// When `Completed` is folded, `apply` shall move one from `open` to `done`.
    /// If there is no row yet, then `apply` shall start from zero open and zero done.
    fn apply(row: Option<Tally>, event: &TaskEvent) -> Option<Tally> {
        let tally: Tally = row.unwrap_or(Tally { open: 0, done: 0 });
        match event {
            TaskEvent::Added { .. } => Some(Tally {
                open: tally.open + 1,
                done: tally.done,
            }),
            TaskEvent::Completed => Some(Tally {
                open: tally.open.saturating_sub(1),
                done: tally.done + 1,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn given(events: &[TaskEvent]) -> Task {
        events.iter().fold(Task::initial(), Task::evolve)
    }

    fn title(text: &str) -> Title {
        Title::new(text).expect("a title")
    }

    fn added() -> TaskEvent {
        TaskEvent::Added {
            title: title("write the report"),
        }
    }

    #[test]
    fn an_empty_title_is_blank() {
        assert_eq!(Title::new(""), Err(TitleError::Blank));
    }

    #[test]
    fn a_title_of_only_whitespace_is_blank() {
        assert_eq!(Title::new(" \t\n"), Err(TitleError::Blank));
    }

    #[test]
    fn a_blank_title_over_the_limit_is_blank() {
        assert_eq!(Title::new(&" ".repeat(Title::LIMIT + 1)), Err(TitleError::Blank));
    }

    #[test]
    fn a_title_of_one_character_holds_it() {
        assert_eq!(title("a").as_str(), "a");
    }

    #[test]
    fn a_title_holds_its_text_unchanged() {
        assert_eq!(title(" write the report ").as_str(), " write the report ");
    }

    #[test]
    fn a_title_of_200_characters_is_accepted() {
        assert_eq!(Title::new(&"é".repeat(200)).map(|title: Title| title.as_str().chars().count()), Ok(200));
    }

    #[test]
    fn a_title_of_201_characters_is_too_long() {
        assert_eq!(Title::new(&"é".repeat(201)), Err(TitleError::TooLong));
    }

    #[test]
    fn a_blank_title_error_says_what_a_title_is() {
        assert_eq!(TitleError::Blank.to_string(), "a title is not blank");
    }

    #[test]
    fn a_too_long_title_error_names_the_limit() {
        assert_eq!(TitleError::TooLong.to_string(), "a title is at most 200 characters");
    }

    #[test]
    fn add_of_an_absent_task_emits_added() {
        assert_eq!(
            AddTask {
                title: title("write the report")
            }
            .decide(&given(&[])),
            Ok(vec![added()])
        );
    }

    #[test]
    fn add_of_an_existing_task_is_refused() {
        assert_eq!(AddTask { title: title("again") }.decide(&given(&[added()])), Err(TaskRefusal::AlreadyExists));
    }

    #[test]
    fn complete_of_an_open_task_emits_completed() {
        assert_eq!(CompleteTask.decide(&given(&[added()])), Ok(vec![TaskEvent::Completed]));
    }

    #[test]
    fn complete_of_an_absent_task_is_refused() {
        assert_eq!(CompleteTask.decide(&given(&[])), Err(TaskRefusal::Absent));
    }

    #[test]
    fn complete_of_a_done_task_is_refused() {
        assert_eq!(CompleteTask.decide(&given(&[added(), TaskEvent::Completed])), Err(TaskRefusal::AlreadyDone));
    }

    #[test]
    fn a_completed_task_keeps_its_title() {
        assert_eq!(
            given(&[added(), TaskEvent::Completed]),
            Task::Done {
                title: title("write the report")
            }
        );
    }

    #[test]
    fn a_completion_of_an_absent_task_leaves_it_absent() {
        assert_eq!(given(&[TaskEvent::Completed]), Task::Absent);
    }

    #[test]
    fn the_first_added_task_opens_the_tally() {
        assert_eq!(TaskTally::apply(None, &added()), Some(Tally { open: 1, done: 0 }));
    }

    #[test]
    fn a_completion_moves_one_task_from_open_to_done() {
        assert_eq!(
            TaskTally::apply(Some(Tally { open: 2, done: 3 }), &TaskEvent::Completed),
            Some(Tally { open: 1, done: 4 })
        );
    }

    #[test]
    fn every_task_event_keys_the_one_tally_row() {
        assert_eq!(TaskTally::key("t-1", &TaskEvent::Completed), Some("all".to_string()));
    }
}
