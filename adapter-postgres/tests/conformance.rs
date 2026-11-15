#![allow(clippy::expect_used, clippy::panic)]

use std::sync::atomic::{AtomicU64, Ordering};

use adapter_postgres::Postgres;

type Subject = Postgres;

static SCHEMAS: AtomicU64 = AtomicU64::new(0);

fn subject() -> Subject {
    let url: String = std::env::var("STORE_POSTGRES_URL").expect("STORE_POSTGRES_URL names a Postgres database these tests may write to");
    let schema: String = format!("suite_{}", SCHEMAS.fetch_add(1, Ordering::SeqCst));
    let mut setup: postgres::Client = postgres::Client::connect(&url, postgres::NoTls).expect("connects");
    setup.batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE")).expect("dropped");
    Postgres::connect(&url, &schema, 4).expect("opens")
}

include!("../../ports/tests/conformance/suite.rs");
