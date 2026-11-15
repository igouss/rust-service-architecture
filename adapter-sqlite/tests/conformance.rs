#![allow(clippy::expect_used, clippy::panic)]

use adapter_sqlite::Sqlite;

type Subject = Sqlite;

fn subject() -> Subject {
    Sqlite::in_memory().expect("opens")
}

include!("../../ports/tests/conformance/suite.rs");
