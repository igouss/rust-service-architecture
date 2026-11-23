use gates::declared::Member;
use gates::stored::{pinned, unpinned, wanted};
use gates::Unread;

use crate::source;

const LIFECYCLE: &str = "pub enum Task { Absent, Open { title: Title } }
pub struct Title(String);
pub enum TaskEvent { Added { kind: Kind }, Completed }
pub enum Kind { Small, Large }
pub enum TaskRefusal { Blank }
impl Lifecycle for Task { type Event = TaskEvent; type Refusal = TaskRefusal; }";

fn member(text: &str) -> Member {
    Member::of("fixture", vec![source(text)]).expect("the fixture parses")
}

fn samples(names: &[&str]) -> Vec<String> {
    names.iter().map(ToString::to_string).collect()
}

#[test]
fn a_member_with_no_stored_type_wants_no_sample() {
    assert_eq!(wanted(&member("pub enum Free { A, B }")), Ok(Vec::new()));
}

#[test]
fn a_lifecycle_wants_every_variant_of_its_state_its_event_its_refusal_and_the_enums_they_hold() {
    assert_eq!(
        wanted(&member(LIFECYCLE)),
        Ok(samples(&[
            "Kind::Small",
            "Kind::Large",
            "Task::Absent",
            "Task::Open",
            "TaskEvent::Added",
            "TaskEvent::Completed",
            "TaskRefusal::Blank"
        ]))
    );
}

#[test]
fn a_struct_an_impl_names_is_wanted_and_one_held_as_a_field_is_not() {
    assert_eq!(
        wanted(&member(
            "pub struct Tally { open: Count }\npub struct Count(u64);\nimpl Projection for Tally { type Row = Tally; }"
        )),
        Ok(samples(&["Tally"]))
    );
}

#[test]
fn a_request_is_wanted() {
    assert_eq!(
        wanted(&member("pub enum Ask { Ship }\nimpl Reaction for Progress { type Request = Ask; }")),
        Ok(samples(&["Ask::Ship"]))
    );
}

#[test]
fn a_slot_written_as_a_path_is_unread() {
    assert_eq!(
        wanted(&member(&LIFECYCLE.replace("type Event = TaskEvent;", "type Event = crate::TaskEvent;"))).map_err(|unread| unread.0),
        Err(String::from(
            "fixture: `impl Lifecycle for Task` does not name its Event and Refusal as one type"
        ))
    );
}

#[test]
fn an_impl_for_a_generic_type_is_unread() {
    assert_eq!(
        wanted(&member("pub struct W<T>(T);\nimpl<T> Lifecycle for W<T> { type Event = T; type Refusal = T; }")),
        Err(Unread(String::from(
            "fixture: an impl of Lifecycle, Reaction, Projection the scan could not read"
        )))
    );
}

#[test]
fn a_stored_type_declared_elsewhere_is_unread() {
    assert_eq!(
        wanted(&member(&LIFECYCLE.replace("pub enum TaskEvent", "pub enum TaskFact"))),
        Err(Unread(String::from(
            "TaskEvent is declared 0 times as an enum or a struct in the crate's src; the scan needs exactly one"
        )))
    );
}

#[test]
fn a_stored_type_declared_twice_is_unread() {
    assert_eq!(
        wanted(&member(&format!("{LIFECYCLE}\nmod shadow {{ pub enum Kind {{ A }} }}"))),
        Err(Unread(String::from(
            "Kind is declared 2 times as an enum or a struct in the crate's src; the scan needs exactly one"
        )))
    );
}

#[test]
fn a_test_that_pins_nothing_holds_no_path() {
    assert_eq!(pinned(&source("fn f() { assert!(true); }").tokens), Vec::<Vec<String>>::new());
}

#[test]
fn each_pinned_call_gives_the_path_it_opens_with() {
    assert_eq!(
        pinned(&source("fn f() { pinned(&TaskEvent::Added { title: title(\"t\") }, form); pinned(&Tally::default(), row); pinned(other, form); }").tokens),
        vec![samples(&["TaskEvent", "Added"]), samples(&["Tally", "default"])]
    );
}

#[test]
fn a_sample_is_pinned_by_a_path_that_opens_with_it() {
    assert_eq!(
        unpinned(
            &samples(&["Task::Open", "Task::Absent", "Tally", "Tall"]),
            &[samples(&["Task", "Open"]), samples(&["Tally", "default"])]
        ),
        samples(&["Task::Absent", "Tall"])
    );
}
#[test]
fn a_stored_type_with_generics_is_unread() {
    assert_eq!(
        wanted(&member(&LIFECYCLE.replace("pub enum TaskEvent {", "pub enum TaskEvent<T = ()> {"))),
        Err(Unread(String::from(
            "TaskEvent is declared 0 times as an enum or a struct in the crate's src; the scan needs exactly one"
        )))
    );
}

#[test]
fn a_generic_type_held_by_a_stored_one_is_unread() {
    assert_eq!(
        wanted(&member(
            &format!("{LIFECYCLE}\npub struct Wrapped<T>(T);").replace("Added { kind: Kind }", "Added { kind: Wrapped<Kind> }")
        )),
        Err(Unread(String::from(
            "Wrapped is declared 0 times as an enum or a struct in the crate's src; the scan needs exactly one"
        )))
    );
}

#[test]
fn a_negative_impl_is_unread() {
    assert_eq!(
        wanted(&member("pub struct Task;\nimpl !Lifecycle for Task {}")),
        Err(Unread(String::from(
            "fixture: an impl of Lifecycle, Reaction, Projection the scan could not read"
        )))
    );
}
