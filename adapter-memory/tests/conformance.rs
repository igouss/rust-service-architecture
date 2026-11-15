#![allow(clippy::expect_used, clippy::panic)]

use adapter_memory::{Kept, Memory};

type Subject = Memory;

fn subject() -> Subject {
    Memory::new(Kept { facts: 1000, refusals: 100 })
}

include!("../../ports/tests/conformance/suite.rs");
