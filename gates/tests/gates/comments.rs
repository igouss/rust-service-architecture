use gates::comments::flagged;
use gates::Unread;

fn found(text: &str, core: bool) -> Vec<String> {
    flagged("fixture.rs", text, core).expect("the fixture is read")
}

#[test]
fn code_with_no_comment_is_clean() {
    assert_eq!(found("fn f() {}\n", false), Vec::<String>::new());
}

#[test]
fn a_line_comment_is_named_with_its_line() {
    assert_eq!(found("fn f() {}\n// why\n", false), vec![String::from("arch-0d2: fixture.rs:2 // why")]);
}

#[test]
fn a_comment_that_trails_code_is_named() {
    assert_eq!(found("fn f() {} // why\n", false), vec![String::from("arch-0d2: fixture.rs:1 // why")]);
}

#[test]
fn a_doc_comment_is_kept() {
    assert_eq!(found("//! the crate\n/// the item\nfn f() {}\n", false), Vec::<String>::new());
}

#[test]
fn four_slashes_are_not_a_doc_comment() {
    assert_eq!(found("//// loud\nfn f() {}\n", false), vec![String::from("arch-0d2: fixture.rs:1 //// loud")]);
}

#[test]
fn a_block_comment_is_named_by_its_first_line_even_as_a_doc() {
    assert_eq!(
        found("/** one\n two */\nfn f() {}\n", false),
        vec![String::from("arch-0d2: fixture.rs:1 /** one")]
    );
}

#[test]
fn a_comment_after_a_block_of_two_lines_keeps_its_own_line() {
    assert_eq!(
        found("/* one\n two */\n// after\n", false),
        vec![String::from("arch-0d2: fixture.rs:1 /* one"), String::from("arch-0d2: fixture.rs:3 // after")]
    );
}

#[test]
fn a_vouched_effect_is_kept_in_the_core() {
    assert_eq!(found("// fc-allow: the reason\nfn f() {}\n", true), Vec::<String>::new());
}

#[test]
fn a_vouched_effect_is_a_comment_outside_the_core() {
    assert_eq!(
        found("// fc-allow: the reason\nfn f() {}\n", false),
        vec![String::from("arch-0d2: fixture.rs:1 // fc-allow: the reason")]
    );
}

#[test]
fn a_vouched_effect_with_no_reason_is_a_comment() {
    assert_eq!(
        found("// fc-allow: \nfn f() {}\n", true),
        vec![String::from("arch-0d2: fixture.rs:1 // fc-allow: ")]
    );
}

#[test]
fn slashes_in_a_string_are_not_a_comment() {
    assert_eq!(found("const S: &str = \"// no /* no */\";\n", false), Vec::<String>::new());
}

#[test]
fn a_block_comment_that_does_not_end_is_unread() {
    assert_eq!(
        flagged("fixture.rs", "fn f() {}\n/* open\n", false),
        Err(Unread(String::from(
            "fixture.rs:2: a block comment does not end; the scan cannot read this file"
        )))
    );
}
