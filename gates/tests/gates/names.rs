use gates::names::{named, shared, Named};
use gates::Unread;

use crate::source;

fn read(text: &str) -> Result<Vec<Named>, Unread> {
    let source = source(text);
    named(&source, &source.parsed().expect("the fixture parses"))
}

fn one(kind: &str, name: &str, holder: &str) -> Named {
    Named {
        kind: String::from(kind),
        name: String::from(name),
        holder: String::from(holder),
    }
}

#[test]
fn a_file_with_no_lifecycle_names_nothing() {
    assert_eq!(read("struct A; impl A { const NAME: &'static str = \"a\"; }"), Ok(Vec::new()));
}

#[test]
fn a_lifecycle_is_named_by_its_literal() {
    assert_eq!(
        read("impl Lifecycle for Task { const NAME: &'static str = \"task\"; }"),
        Ok(vec![one("Lifecycle", "task", "Task (fixture.rs)")])
    );
}

#[test]
fn a_lifecycle_and_a_projection_are_each_named() {
    assert_eq!(
        read("impl Lifecycle for Task { const NAME: &'static str = \"task\"; }\nimpl Projection for Tally { const NAME : & 'static  str = \"tally\" ; }"),
        Ok(vec![
            one("Lifecycle", "task", "Task (fixture.rs)"),
            one("Projection", "tally", "Tally (fixture.rs)")
        ])
    );
}

#[test]
fn a_name_that_is_not_a_literal_is_unread() {
    assert_eq!(
        read("impl Lifecycle for Task { const NAME: &'static str = concat!(\"ta\", \"sk\"); }"),
        Err(Unread(String::from(
            "fixture.rs: impl Lifecycle for Task holds 0 NAME written as a string literal; the scan needs exactly one"
        )))
    );
}

#[test]
fn a_name_written_with_an_escape_or_as_a_raw_string_is_unread() {
    assert_eq!(
        read("impl Lifecycle for Task { const NAME: &'static str = \"ta\\u{73}k\"; }\nimpl Projection for Tally { const NAME: &'static str = r\"tally\"; }")
            .map_err(|unread| unread.0),
        Err(String::from(
            "fixture.rs: impl Lifecycle for Task holds 0 NAME written as a string literal; the scan needs exactly one"
        ))
    );
}

#[test]
fn names_that_differ_are_not_shared() {
    assert_eq!(
        shared(&[one("Lifecycle", "task", "Task (a.rs)"), one("Lifecycle", "payment", "Payment (b.rs)")]),
        Vec::<String>::new()
    );
}

#[test]
fn a_lifecycle_and_a_projection_may_share_a_name() {
    assert_eq!(
        shared(&[one("Lifecycle", "task", "Task (a.rs)"), one("Projection", "task", "Tally (a.rs)")]),
        Vec::<String>::new()
    );
}

#[test]
fn two_lifecycles_with_one_name_are_named_together() {
    assert_eq!(
        shared(&[
            one("Lifecycle", "payment", "Fulfilment (a.rs)"),
            one("Lifecycle", "task", "Task (c.rs)"),
            one("Lifecycle", "payment", "Payment (b.rs)")
        ]),
        vec![String::from(
            "arch-1bmf: two lifecycles are named \"payment\": Fulfilment (a.rs) and Payment (b.rs)"
        )]
    );
}

#[test]
fn two_projections_with_one_name_are_named_together() {
    assert_eq!(
        shared(&[one("Projection", "tally", "Other (a.rs)"), one("Projection", "tally", "Tally (a.rs)")]),
        vec![String::from("arch-1bmf: two projections are named \"tally\": Other (a.rs) and Tally (a.rs)")]
    );
}
