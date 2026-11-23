use gates::allows::flagged;
use gates::Unread;

use crate::source;

fn found(text: &str, in_tests: bool) -> Vec<String> {
    flagged(&source(text), in_tests).expect("the fixture is read")
}

#[test]
fn code_with_no_attribute_is_clean() {
    assert_eq!(found("fn f() {}", false), Vec::<String>::new());
}

#[test]
fn an_attribute_that_silences_nothing_is_clean() {
    assert_eq!(found("#[derive(Debug)]\n#[must_use]\nstruct A;", false), Vec::<String>::new());
}

#[test]
fn an_allow_is_named_with_its_line_and_its_text() {
    assert_eq!(
        found("fn f() {}\n#[allow(dead_code)]\nfn g() {}", false),
        vec![String::from("arch-tsu: fixture.rs:2 #[allow(dead_code)]")]
    );
}

#[test]
fn an_expect_is_named() {
    assert_eq!(
        found("#[expect(unused)]\nfn g() {}", false),
        vec![String::from("arch-tsu: fixture.rs:1 #[expect(unused)]")]
    );
}

#[test]
fn an_allow_under_cfg_attr_is_named() {
    assert_eq!(
        found("#[cfg_attr(all(), allow(unused))]\nfn g() {}", false),
        vec![String::from("arch-tsu: fixture.rs:1 #[cfg_attr(all(), allow(unused))]")]
    );
}

#[test]
fn an_allow_written_across_lines_is_named_on_one() {
    assert_eq!(
        found("#[ allow ( dead_code,\n  unused ) ]\nfn g() {}", false),
        vec![String::from("arch-tsu: fixture.rs:1 #[allow ( dead_code, unused )]")]
    );
}

#[test]
fn two_allows_are_named_in_the_order_written() {
    assert_eq!(
        found("mod m { #[allow(a)] fn f() {} }\n#[allow(b)]\nfn g() {}", false),
        vec![
            String::from("arch-tsu: fixture.rs:1 #[allow(a)]"),
            String::from("arch-tsu: fixture.rs:2 #[allow(b)]")
        ]
    );
}

#[test]
fn an_allow_inside_a_macro_is_named() {
    assert_eq!(
        found("macro_rules! m { () => { #[allow(a)] fn f() {} }; }", false),
        vec![String::from("arch-tsu: fixture.rs:1 #[allow(a)]")]
    );
}

#[test]
fn the_one_allow_opens_a_test_file() {
    assert_eq!(found("#![allow(clippy::expect_used, clippy::panic)]\nfn f() {}", true), Vec::<String>::new());
}

#[test]
fn a_test_file_may_take_one_of_the_two_lints() {
    assert_eq!(found("#![allow(clippy::panic)]\nfn f() {}", true), Vec::<String>::new());
}

#[test]
fn the_one_allow_is_not_accepted_outside_tests() {
    assert_eq!(
        found("#![allow(clippy::panic)]\nfn f() {}", false),
        vec![String::from("arch-tsu: fixture.rs:1 #![allow(clippy::panic)]")]
    );
}

#[test]
fn a_test_file_may_not_silence_a_third_lint() {
    assert_eq!(
        found("#![allow(clippy::panic, dead_code)]\nfn f() {}", true),
        vec![String::from("arch-tsu: fixture.rs:1 #![allow(clippy::panic, dead_code)]")]
    );
}

#[test]
fn a_test_file_may_not_silence_an_item() {
    assert_eq!(
        found("#[allow(clippy::panic)]\nfn f() {}", true),
        vec![String::from("arch-tsu: fixture.rs:1 #[allow(clippy::panic)]")]
    );
}

#[test]
fn an_allow_in_a_doc_comment_is_text() {
    assert_eq!(found("/// #[allow(dead_code)]\nfn f() {}", false), Vec::<String>::new());
}

#[test]
fn a_call_named_allow_is_unread() {
    assert_eq!(
        flagged(&source("fn f() -> u8 { allow(1) }"), false),
        Err(Unread(String::from("fixture.rs: an `allow(` stands outside every attribute the scan read")))
    );
}
