#![allow(clippy::expect_used, clippy::panic)]

use adapter_memory::Memory;

type Subject = Memory;

fn subject() -> Subject {
    Memory::new()
}

include!("../../ports/tests/conformance/suite.rs");
