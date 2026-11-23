use gates::traits::{declared, Declared};
use gates::Unread;

use crate::source;

#[test]
fn code_with_no_trait_declares_none() {
    assert_eq!(declared(&source("struct A; impl A { fn f(&self) {} }")), Ok(Vec::new()));
}

#[test]
fn a_trait_is_named_with_its_line() {
    assert_eq!(
        declared(&source("struct A;\npub trait Board { fn open(&self); }")),
        Ok(vec![Declared {
            line: 2,
            name: String::from("Board")
        }])
    );
}

#[test]
fn two_traits_are_named_in_the_order_written() {
    assert_eq!(
        declared(&source("mod inner { pub trait Deep {} }\ntrait Shallow {}")),
        Ok(vec![
            Declared {
                line: 1,
                name: String::from("Deep")
            },
            Declared {
                line: 2,
                name: String::from("Shallow")
            }
        ])
    );
}

#[test]
fn a_trait_inside_a_macro_is_named() {
    assert_eq!(
        declared(&source("macro_rules! m { () => { trait Hidden {} }; }")),
        Ok(vec![Declared {
            line: 1,
            name: String::from("Hidden")
        }])
    );
}

#[test]
fn the_word_in_a_string_or_a_name_is_not_a_trait() {
    assert_eq!(declared(&source("const S: &str = \"trait A\"; fn traits() {}")), Ok(Vec::new()));
}

#[test]
fn a_trait_with_no_name_after_it_is_unread() {
    assert_eq!(
        declared(&source("macro_rules! m { ($n:ident) => { trait $n {} }; }")),
        Err(Unread(String::from("fixture.rs: a `trait` the scan could not name")))
    );
}
