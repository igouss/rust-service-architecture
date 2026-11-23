#![allow(clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use adapter_postgres::{Durability, Postgres};
use app::{handle, relay};
use domain::fulfilment::{Fulfilment, FulfilmentReply, FulfilmentRequest, Progress};
use domain::payment::{Amount, Authorize, Capture};
use domain::task::{AddTask, TaskRefusal, Title};
use ports::{Answer, Finished, Gateway, GatewayError, Journal, Moment, Outbox, RequestId, StoreError, StreamId, Version};
use tokio::runtime::{Builder, Runtime};

fn url() -> String {
    std::env::var("STORE_POSTGRES_URL").expect("STORE_POSTGRES_URL names a Postgres database these tests may write to")
}

fn fresh(schema: &str) {
    let mut setup: postgres::Client = postgres::Client::connect(&url(), postgres::NoTls).expect("connects");
    setup.batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE")).expect("dropped");
}

fn store(schema: &str) -> Postgres {
    fresh(schema);
    Postgres::connect(&url(), schema, 4, Durability::Full).expect("opens")
}

fn id<S>(text: &str) -> StreamId<S> {
    StreamId::new(text)
}

fn add(store: &Postgres, task: &str) -> Result<Answer<TaskRefusal>, StoreError> {
    handle(
        store,
        &id(task),
        AddTask {
            title: Title::new(task).expect("a title"),
        },
    )
}

struct Recording {
    performed: Mutex<Vec<FulfilmentRequest>>,
}

impl Gateway<FulfilmentRequest> for Recording {
    type Reply = FulfilmentReply;

    async fn perform(&self, _id: RequestId, _stream: &str, request: &FulfilmentRequest) -> Result<FulfilmentReply, GatewayError> {
        tokio::task::yield_now().await;
        self.performed.lock().expect("lock").push(request.clone());
        Ok(FulfilmentReply::StockUnavailable)
    }
}

#[test]
fn a_unit_of_work_runs_through_spawn_blocking_on_a_multi_thread_runtime() {
    let store: Arc<Postgres> = Arc::new(store("runtime_blocking"));
    let runtime: Runtime = Builder::new_multi_thread().worker_threads(2).build().expect("runtime");
    let worker: Arc<Postgres> = Arc::clone(&store);
    let answer: Result<Answer<TaskRefusal>, StoreError> =
        runtime.block_on(async move { tokio::task::spawn_blocking(move || add(&worker, "t-1")).await.expect("joined") });
    assert_eq!(answer, Ok(Answer::Accepted(Version::from(1))));
}

#[test]
fn the_relay_runs_on_a_runtime_of_its_own_with_the_store_as_its_outbox() {
    let store: Postgres = store("runtime_relay");
    let amount: Amount = Amount::new(500).expect("positive");
    handle(&store, &id("p-1"), Authorize { amount }).expect("authorized");
    handle(&store, &id("p-1"), Capture { amount }).expect("captured");
    let gateway: Recording = Recording {
        performed: Mutex::new(Vec::new()),
    };
    let runtime: Runtime = Builder::new_current_thread().build().expect("runtime");
    assert_eq!(
        runtime.block_on(relay::<Progress, Postgres, Recording>(
            &store,
            &gateway,
            10,
            &app::Retry {
                attempts: std::num::NonZeroU32::MIN,
                first: std::time::Duration::ZERO,
                longest: std::time::Duration::ZERO,
                lease: std::time::Duration::ZERO,
            },
            Moment::ZERO
        )),
        Ok(1)
    );
    assert_eq!(*gateway.performed.lock().expect("lock"), vec![FulfilmentRequest::ReserveStock]);
}

#[test]
fn the_store_may_be_dropped_inside_a_runtime() {
    let store: Postgres = store("runtime_drop");
    let runtime: Runtime = Builder::new_multi_thread().worker_threads(2).build().expect("runtime");
    runtime.block_on(async move { drop(store) });
}

#[test]
fn the_store_may_be_opened_and_used_inside_a_runtime() {
    let mut setup: postgres::Client = postgres::Client::connect(&url(), postgres::NoTls).expect("connects");
    setup.batch_execute("DROP SCHEMA IF EXISTS runtime_open CASCADE").expect("dropped");
    drop(setup);
    let runtime: Runtime = Builder::new_current_thread().build().expect("runtime");
    let answer: Result<Answer<TaskRefusal>, StoreError> =
        runtime.block_on(async { add(&Postgres::connect(&url(), "runtime_open", 2, Durability::Full)?, "t-1") });
    assert_eq!(answer, Ok(Answer::Accepted(Version::from(1))));
}

#[test]
fn a_store_whose_connections_the_server_closed_recovers() {
    fresh("runtime_recover");
    let store: Postgres = Postgres::connect(&url(), "runtime_recover", 2, Durability::Full).expect("opens");
    add(&store, "t-0").expect("handled");
    let mut admin: postgres::Client = postgres::Client::connect(&url(), postgres::NoTls).expect("connects");
    let closed: postgres::Row = admin
        .query_one(
            "SELECT count(pg_terminate_backend(pid)) FROM pg_stat_activity WHERE application_name = 'store_runtime_recover'",
            &[],
        )
        .expect("terminated");
    assert_eq!(closed.get::<_, i64>(0), 2);
    assert_eq!(add(&store, "t-1"), Ok(Answer::Accepted(Version::from(1))));
}

#[test]
fn a_store_reads_on_one_connection_while_a_unit_is_open_on_another() {
    let store: Postgres = store("runtime_pool");
    let seen: Result<usize, StoreError> = store.write(|unit| {
        let _applied: ports::Applied<domain::task::Task> = unit.apply(&id("t-1"), |state: domain::task::Task| {
            ports::decide(
                AddTask {
                    title: Title::new("t-1").expect("a title"),
                },
                state,
            )
        })?;
        store.read(|view| Ok(view.states::<domain::task::Task>()?.len()))
    });
    assert_eq!(seen, Ok(0));
}

#[test]
fn a_store_opened_as_full_commits_synchronously() {
    fresh("runtime_full");
    let store: Postgres = Postgres::connect(&url(), "runtime_full", 1, Durability::Full).expect("opens");
    assert_eq!(store.durability(), Ok(Durability::Full));
}

#[test]
fn a_store_opened_as_normal_does_not_wait_for_the_disk() {
    fresh("runtime_normal");
    let store: Postgres = Postgres::connect(&url(), "runtime_normal", 1, Durability::Normal).expect("opens");
    assert_eq!(store.durability(), Ok(Durability::Normal));
    assert_eq!(add(&store, "t-1"), Ok(Answer::Accepted(Version::from(1))));
}

#[test]
fn a_schema_name_that_is_not_plain_is_refused_as_a_misconfiguration() {
    let opened: Result<Postgres, StoreError> = Postgres::connect(&url(), "Tasks; DROP", 1, Durability::Full);
    assert!(matches!(opened.map(|_store: Postgres| ()), Err(StoreError::Misconfigured { .. })));
}

fn parked(schema: &str) -> Vec<(String, String, i64, String)> {
    let mut raw: postgres::Client = postgres::Client::connect(&url(), postgres::NoTls).expect("connects");
    raw.query(
        &format!("SELECT o.lifecycle, o.stream, o.attempts, p.cause FROM {schema}.parked p JOIN {schema}.outbox o USING (id) ORDER BY id"),
        &[],
    )
    .expect("queried")
    .iter()
    .map(|row: &postgres::Row| (row.get(0), row.get(1), row.get(2), row.get(3)))
    .collect()
}

fn captured(schema: &str) -> Postgres {
    let store: Postgres = store(schema);
    let amount: Amount = Amount::new(500).expect("positive");
    handle(&store, &id("p-1"), Authorize { amount }).expect("authorized");
    handle(&store, &id("p-1"), Capture { amount }).expect("captured");
    store
}

#[test]
fn with_no_request_parked_the_parked_table_is_empty() {
    let store: Postgres = captured("parked_none");
    store.failed(RequestId::from(1), Moment::ZERO).expect("failed");
    assert_eq!(parked("parked_none"), Vec::new());
}

#[test]
fn a_parked_request_is_read_with_sql_with_its_cause_and_its_attempts() {
    let store: Postgres = captured("parked_one");
    store.failed(RequestId::from(1), Moment::ZERO).expect("failed");
    let _parked: Finished<Fulfilment> = store
        .write(|unit| {
            unit.park(RequestId::from(1), "no such warehouse", |state: Fulfilment| {
                ports::react(Progress::ReservationUndelivered, state)
            })
        })
        .expect("parked");
    assert_eq!(
        parked("parked_one"),
        vec![("fulfilment".to_string(), "p-1".to_string(), 2, "no such warehouse".to_string())]
    );
}
