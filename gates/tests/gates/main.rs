#![allow(clippy::expect_used, clippy::panic)]

use gates::tokens::Source;

mod allows;
mod comments;
mod stored;
mod traits;
mod wildcards;

pub fn source(text: &str) -> Source {
    Source::of(String::from("fixture.rs"), String::from(text)).expect("the fixture lexes")
}
