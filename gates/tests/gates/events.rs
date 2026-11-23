use gates::events::generic;
use gates::Unread;

use crate::lifecycles;

const TASK: &str = "pub enum Task { Absent }
pub enum TaskEvent { Added, Completed }
impl Lifecycle for Task { type Event = TaskEvent; }";

fn found(text: &str) -> Result<Vec<String>, Unread> {
    generic(&lifecycles(text)?)
}

#[test]
fn events_named_for_what_happened_are_clean() {
    assert_eq!(found(TASK), Ok(Vec::new()));
}

#[test]
fn an_event_named_for_a_change_is_named() {
    assert_eq!(
        found(&TASK.replace("Completed", "Updated")),
        Ok(vec![String::from("fixture.rs: TaskEvent::Updated")])
    );
}

#[test]
fn the_lifecycle_name_before_a_generic_verb_is_still_generic() {
    assert_eq!(
        found(&TASK.replace("Completed", "TaskChanged")),
        Ok(vec![String::from("fixture.rs: TaskEvent::TaskChanged")])
    );
}

#[test]
fn every_generic_event_is_named() {
    assert_eq!(
        found(&TASK.replace("Added, Completed", "Saved, Completed, TaskSet")),
        Ok(vec![
            String::from("fixture.rs: TaskEvent::Saved"),
            String::from("fixture.rs: TaskEvent::TaskSet")
        ])
    );
}

#[test]
fn a_workspace_with_no_lifecycle_is_unread() {
    assert_eq!(
        found("pub enum Task { Absent }").map_err(|unread| unread.0),
        Err(String::from(
            "0 `Lifecycle for` in the workspace's src, 0 read as `impl Lifecycle for State { type Event = Name; … }`"
        ))
    );
}

#[test]
fn a_lifecycle_whose_event_is_a_path_is_unread() {
    assert_eq!(
        found(&TASK.replace("type Event = TaskEvent;", "type Event = crate::TaskEvent;")).map_err(|unread| unread.0),
        Err(String::from(
            "1 `Lifecycle for` in the workspace's src, 0 read as `impl Lifecycle for State { type Event = Name; … }`"
        ))
    );
}

#[test]
fn an_event_enum_that_is_not_in_the_crate_is_unread() {
    assert_eq!(
        found(&TASK.replace("pub enum TaskEvent", "pub enum TaskFact")).map_err(|unread| unread.0),
        Err(String::from(
            "TaskEvent is declared 0 times as an enum or a struct in the crate's src; the scan needs exactly one"
        ))
    );
}

#[test]
fn an_event_that_is_a_struct_is_unread() {
    assert_eq!(
        found("pub enum Task { Absent }\npub struct TaskEvent;\nimpl Lifecycle for Task { type Event = TaskEvent; }").map_err(|unread| unread.0),
        Err(String::from("TaskEvent is a struct where the scan needs an enum"))
    );
}
