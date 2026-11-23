#![allow(clippy::expect_used, clippy::panic)]

use gates::tokens::Source;

mod allows;
mod comments;
mod events;
mod names;
mod pins;
mod stored;
mod traits;
mod wildcards;

pub fn lifecycles(text: &str) -> Result<gates::lifecycles::Lifecycles, gates::Unread> {
    gates::lifecycles::Lifecycles::of(vec![gates::declared::Member::of("fixture", vec![source(text)]).expect("the fixture parses")])
}

pub fn source(text: &str) -> Source {
    Source::of(String::from("fixture.rs"), String::from(text)).expect("the fixture lexes")
}
