use gates::refined::{flagged, loaders, marked, Loader};
use gates::Unread;

use crate::source;

const TITLE: &str = "#[derive(Deserialize)]\n#[serde(try_from = \"String\")]\npub struct Title(String);\n";

fn judged(text: &str) -> Result<Vec<String>, Unread> {
    let source = source(text);
    let held: Vec<Loader> = loaders(&source)?;
    flagged("domain", &marked(&source)?, &held)
}

fn with(body: &str) -> String {
    format!("{TITLE}impl TryFrom<String> for Title {{ type Error = E; fn try_from(text: String) -> Result<Title, E> {{ {body} }} }}")
}

#[test]
fn a_file_with_no_marked_type_flags_nothing() {
    assert_eq!(
        judged("pub struct Title(String); impl Title { pub fn new() -> Title { Title(String::new()) } }"),
        Ok(Vec::new())
    );
}

#[test]
fn a_loader_that_writes_its_own_rule_is_not_flagged() {
    assert_eq!(judged(&with("if text.is_empty() { Err(E) } else { Ok(Title(text)) }")), Ok(Vec::new()));
}

#[test]
fn a_loader_that_calls_the_constructor_by_the_type_is_flagged() {
    assert_eq!(
        judged(&with("Title::new(&text)")),
        Ok(vec![String::from(
            "arch-v9j1: domain: Title loads its stored values through TryFrom, which calls Title::new"
        )])
    );
}

#[test]
fn a_loader_that_calls_the_constructor_by_self_inside_a_closure_is_flagged() {
    assert_eq!(
        judged(&with("Some(text).map(|text| Self::new(&text)).ok_or(E)?")).map(|lines| lines.len()),
        Ok(1)
    );
}

#[test]
fn of_many_marked_types_only_the_ones_that_call_their_constructor_are_flagged() {
    let text: String = format!(
        "{}\n#[serde(try_from = \"u64\")]\nenum Count {{ One }}\nimpl TryFrom<u64> for Count {{ fn try_from(n: u64) -> Result<Count, E> {{ Count::new(n) }} }}",
        with("Ok(Title(text))")
    );
    assert_eq!(
        judged(&text),
        Ok(vec![String::from(
            "arch-v9j1: domain: Count loads its stored values through TryFrom, which calls Count::new"
        )])
    );
}

#[test]
fn a_constructor_of_another_type_is_not_the_types_own() {
    assert_eq!(judged(&with("Ok(Title(String::new() + &text))")), Ok(Vec::new()));
}

#[test]
fn a_marked_type_with_no_loader_is_unread() {
    assert_eq!(
        judged(TITLE),
        Err(Unread(String::from(
            "domain: Title is marked `try_from` and no `impl TryFrom<…> for Title` is read in the member"
        )))
    );
}

#[test]
fn a_mark_that_is_on_no_struct_or_enum_is_unread() {
    assert_eq!(
        judged("struct Task { #[serde(try_from = \"String\")] title: Title }"),
        Err(Unread(String::from(
            "fixture.rs: 1 `try_from =` written, 0 read as `#[serde(try_from = …)]` on a struct or an enum"
        )))
    );
}

#[test]
fn a_loader_for_a_type_with_generics_is_unread() {
    assert_eq!(
        judged("impl<T> TryFrom<String> for Wrapped<T> { }"),
        Err(Unread(String::from(
            "fixture.rs: 1 `TryFrom<…> for` written, 0 read as `impl TryFrom<…> for Type { … }`"
        )))
    );
}

#[test]
fn a_mark_behind_other_attributes_and_a_visibility_is_read() {
    assert_eq!(
        marked(&source("#[serde(try_from = \"String\")]\n#[derive(Debug)]\npub(crate) struct Title(String);")),
        Ok(vec![String::from("Title")])
    );
}

#[test]
fn a_bound_that_names_the_trait_is_not_a_loader() {
    assert_eq!(
        judged("fn load<T: TryFrom<String>>(text: String) -> Option<T> { T::try_from(text).ok() }"),
        Ok(Vec::new())
    );
}
