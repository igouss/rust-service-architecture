#![allow(clippy::expect_used, clippy::panic)]

use gates::tokens::{fat_arrows, followed, squeezed, Source};
use gates::Unread;

fn source(text: &str) -> Source {
    Source::of(String::from("fixture.rs"), String::from(text)).expect("the fixture lexes")
}

#[test]
fn a_file_with_no_match_holds_no_fat_arrow() {
    assert_eq!(fat_arrows(&source("fn f() -> bool { 1 >= 0 }").tokens), 0);
}

#[test]
fn a_fat_arrow_in_a_string_or_a_comment_is_not_counted() {
    assert_eq!(fat_arrows(&source("/// a => b\nfn f() -> &'static str { \"=>\" }").tokens), 0);
}

#[test]
fn every_arm_of_a_match_is_one_fat_arrow() {
    assert_eq!(fat_arrows(&source("fn f(x: u8) -> u8 { match x { 0 => 1, n => n } }").tokens), 2);
}

#[test]
fn a_fat_arrow_inside_a_macro_is_counted() {
    assert_eq!(fat_arrows(&source("macro_rules! m { () => { 1 }; }").tokens), 1);
}

#[test]
fn an_impl_is_counted_by_its_trait_and_for() {
    assert_eq!(
        followed(
            &source("impl Lifecycle for A {} impl<T> Lifecycle for B<T> {} fn lifecycle_for() {}").tokens,
            "Lifecycle",
            "for"
        ),
        2
    );
}

#[test]
fn a_span_gives_back_the_source_as_written() {
    let read: Source = source("fn f(x: (u8,   u8)) {}");
    let parsed: syn::File = read.parsed().expect("the fixture parses");
    let syn::Item::Fn(function) = &parsed.items[0] else {
        panic!("the fixture is a function")
    };
    let syn::FnArg::Typed(argument) = &function.sig.inputs[0] else {
        panic!("the fixture takes an argument")
    };
    assert_eq!(read.slice(syn::spanned::Spanned::span(&*argument.ty)), "(u8,   u8)");
    assert_eq!(squeezed(read.slice(syn::spanned::Spanned::span(&*argument.ty))), "(u8, u8)");
}

#[test]
fn a_string_that_does_not_end_is_unread() {
    assert!(matches!(
        Source::of(String::from("fixture.rs"), String::from("fn f() { \"open }")),
        Err(Unread(_))
    ));
}
