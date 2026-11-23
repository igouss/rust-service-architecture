use gates::wildcards::{read, Read};
use gates::Unread;

use crate::source;

fn loose(text: &str) -> Vec<String> {
    read(&source(text)).expect("the fixture is read").loose
}

#[test]
fn code_with_no_match_reads_no_arm() {
    assert_eq!(read(&source("fn f() -> bool { true }")), Ok(Read { arms: 0, loose: Vec::new() }));
}

#[test]
fn a_match_that_spells_out_every_pair_is_clean() {
    assert_eq!(
        read(&source(
            "fn f(x: (bool, bool)) -> u8 { match x { (true, true | false) => 1, (false, true) | (false, false) => 2 } }"
        )),
        Ok(Read { arms: 2, loose: Vec::new() })
    );
}

#[test]
fn a_wildcard_element_is_named_as_written() {
    assert_eq!(
        loose("fn f(x: (A, B)) -> u8 { match x { (s @ A::Done { .. },   _) => 1, (A::Open, B::Go) => 2 } }"),
        vec![String::from("(s @ A::Done { .. }, _)")]
    );
}

#[test]
fn a_bare_name_and_a_rest_are_wildcards() {
    assert_eq!(
        loose("fn f(x: (A, B)) -> u8 { match x { (A::Done, other) => 1, (A::Open, ..) => 2 } }"),
        vec![String::from("(A::Done, other)"), String::from("(A::Open, ..)")]
    );
}

#[test]
fn only_the_loose_alternative_of_an_arm_is_named() {
    assert_eq!(
        loose("fn f(x: (A, B)) -> u8 { match x { (A::Done, B::Go) | (A::Open, _) => 1 } }"),
        vec![String::from("(A::Open, _)")]
    );
}

#[test]
fn a_wildcard_is_found_in_a_nested_tuple_and_behind_a_reference_or_a_binding() {
    assert_eq!(
        loose("fn f(x: &((bool, bool), bool)) -> u8 { match x { ((true, _), true) => 1, &(all @ (false, rest), true) => 2, (all @ _, false) => 3 } }"),
        vec![
            String::from("((true, _), true)"),
            String::from("&(all @ (false, rest), true)"),
            String::from("(all @ _, false)")
        ]
    );
}

#[test]
fn a_literal_a_variant_and_a_wildcard_inside_a_variant_are_not_loose() {
    assert_eq!(
        loose("fn f(x: (bool, Option<u8>)) -> u8 { match x { (true, Some(_)) => 1, (false, None) => 2, (true | false, Some(_) | None) => 3 } }"),
        Vec::<String>::new()
    );
}

#[test]
fn a_name_that_takes_the_whole_value_is_not_an_element() {
    assert_eq!(loose("fn f(x: u8) -> u8 { match x { 0 => 1, n => n } }"), Vec::<String>::new());
}

#[test]
fn a_comma_in_a_generic_does_not_hide_the_next_arm() {
    assert_eq!(
        loose("fn f(x: (bool, bool)) -> Vec<(u8, u8)> { match x { (true, true | false) => Vec::<(u8, u8)>::new(), (false, _) => Vec::new() } }"),
        vec![String::from("(false, _)")]
    );
}

#[test]
fn a_guard_is_not_part_of_the_pattern() {
    assert_eq!(
        loose("fn f(x: (bool, u8)) -> u8 { match x { (true, n) if n > 1 => 1, (true | false, 0..=255) => 2 } }"),
        vec![String::from("(true, n)")]
    );
}

#[test]
fn a_match_passed_to_a_macro_is_read() {
    assert_eq!(
        loose("fn f(x: (bool, bool)) -> String { format!(\"{}\", match x { (true, _) => 1, (false, true | false) => 2 }) }"),
        vec![String::from("(true, _)")]
    );
}

#[test]
fn an_arrow_the_parser_did_not_read_as_an_arm_is_unread() {
    assert_eq!(
        read(&source(
            "macro_rules! twice { ($x:expr) => { $x + $x }; }\nfn f(x: u8) -> u8 { match x { 0 => 1, 1..=255 => 2 } }"
        )),
        Err(Unread(String::from("fixture.rs: 3 `=>` and 2 match arms read")))
    );
}

#[test]
fn a_guard_on_an_arm_of_alternatives_hides_none_of_them() {
    assert_eq!(
        loose("fn f(x: (bool, u8)) -> u8 { match x { (true, 0) | (false, n) if x.1 > 1 => 1, (true | false, 0..=255) => 2 } }"),
        vec![String::from("(false, n)")]
    );
}
