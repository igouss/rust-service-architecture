use std::env;
use std::future::Future;
use std::io;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::pin::pin;
use std::process::ExitCode;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use adapter_http::{router, Changes};
use adapter_sqlite::{Durability, Sqlite};
use axum::Router;
use domain::fulfilment::{FulfilmentReply, FulfilmentRequest, Progress};
use ports::{Gateway, Journal, Outbox, StoreError};
use tokio::net::TcpListener;
use tokio::runtime::{Builder, Runtime};
use tokio::signal::unix::{signal, Signal, SignalKind};
use tracing_subscriber::fmt::format::FmtSpan;

mod warehouse;

use warehouse::AlwaysInStock;

const DATABASE: &str = "TASKS_DATABASE";
const LISTEN: &str = "TASKS_LISTEN";
const DURABILITY: &str = "TASKS_DURABILITY";
const ATTEMPTS: &str = "TASKS_RELAY_ATTEMPTS";

const DEFAULT_DATABASE: &str = "tasks.sqlite";
const DEFAULT_LISTEN: &str = "127.0.0.1:3000";
const DEFAULT_ATTEMPTS: NonZeroU32 = match NonZeroU32::new(5) {
    Some(attempts) => attempts,
    None => NonZeroU32::MIN,
};

const RELAY_BATCH: usize = 16;
const RELAY_EVERY: Duration = Duration::from_millis(500);

enum Failure {
    UnknownDurability(String),
    UnknownAttempts(String),
    Store(StoreError),
    Listen(io::Error),
    Serve(io::Error),
    Signal(io::Error),
    Relay(io::Error),
    RelayPanicked,
}

fn explained(failure: &Failure) -> String {
    match failure {
        Failure::UnknownDurability(setting) => format!("{DURABILITY} is {setting:?}; it is full or normal"),
        Failure::UnknownAttempts(setting) => format!("{ATTEMPTS} is {setting:?}; it is a whole number of at least 1"),
        Failure::Store(error) => format!("cannot open the database: {error:?}"),
        Failure::Listen(error) => format!("cannot listen: {error}"),
        Failure::Serve(error) => format!("stopped serving: {error}"),
        Failure::Signal(error) => format!("cannot wait for SIGTERM: {error}"),
        Failure::Relay(error) => format!("cannot start the relay: {error}"),
        Failure::RelayPanicked => "the relay panicked".to_string(),
    }
}

fn attempts_of(setting: Option<&str>) -> Result<NonZeroU32, Failure> {
    match setting {
        None => Ok(DEFAULT_ATTEMPTS),
        Some(text) => text
            .parse::<NonZeroU32>()
            .map_err(|_unreadable: std::num::ParseIntError| Failure::UnknownAttempts(text.to_string())),
    }
}

fn durability_of(setting: Option<&str>) -> Result<Durability, Failure> {
    match setting {
        None | Some("full") => Ok(Durability::Full),
        Some("normal") => Ok(Durability::Normal),
        Some(unknown) => Err(Failure::UnknownDurability(unknown.to_string())),
    }
}

async fn interrupted() {
    match tokio::signal::ctrl_c().await {
        Ok(()) => tracing::info!(signal = "SIGINT", "stopping"),
        Err(error) => tracing::error!(%error, "cannot wait for ctrl-c, stopping"),
    }
}

async fn terminated(mut signal: Signal) {
    let _received: Option<()> = signal.recv().await;
    tracing::info!(signal = "SIGTERM", "stopping");
}

async fn either(first: impl Future<Output = ()>, second: impl Future<Output = ()>) {
    tokio::select! {
        () = first => (),
        () = second => (),
    }
}

async fn stopped(asked: impl Future<Output = ()>, changes: Changes) {
    asked.await;
    changes.stop();
}

/// When started, `relayed` shall make one pass over the outbox, and another after each pause, until it is asked to stop.
/// If a pass cannot reach the store, then `relayed` shall log the error and pass again after the pause.
async fn relayed<K, G>(store: &K, gateway: &G, attempts: NonZeroU32, every: Duration, asked: impl Future<Output = ()>)
where
    K: Journal + Outbox,
    G: Gateway<FulfilmentRequest, Reply = FulfilmentReply>,
{
    let mut asked = pin!(asked);
    loop {
        match app::relay::<Progress, K, G>(store, gateway, RELAY_BATCH, attempts).await {
            Ok(performed) => tracing::debug!(performed, "relayed"),
            Err(error) => tracing::error!(?error, "the relay could not reach the store"),
        }
        tokio::select! {
            () = &mut asked => return,
            () = tokio::time::sleep(every) => (),
        }
    }
}

fn relaying(store: Arc<Sqlite>, attempts: NonZeroU32, changes: Changes) -> Result<JoinHandle<()>, Failure> {
    let runtime: Runtime = Builder::new_current_thread().enable_time().build().map_err(Failure::Relay)?;
    thread::Builder::new()
        .name("relay".to_string())
        .spawn(move || runtime.block_on(relayed(store.as_ref(), &AlwaysInStock, attempts, RELAY_EVERY, changes.stopped())))
        .map_err(Failure::Relay)
}

struct Settings {
    database: PathBuf,
    address: String,
    durability: Option<String>,
    attempts: Option<String>,
}

async fn served(settings: Settings) -> Result<(), Failure> {
    let durability: Durability = durability_of(settings.durability.as_deref())?;
    let attempts: NonZeroU32 = attempts_of(settings.attempts.as_deref())?;
    let changes: Changes = Changes::default();
    let store: Arc<Sqlite> = Arc::new(
        Sqlite::open(&settings.database, durability)
            .map_err(Failure::Store)?
            .on_commit(changes.on_commit()),
    );
    let routes: Router = router(Arc::clone(&store), changes.clone());
    let terminate: Signal = signal(SignalKind::terminate()).map_err(Failure::Signal)?;
    let listener: TcpListener = TcpListener::bind(&settings.address).await.map_err(Failure::Listen)?;
    tracing::info!(address = %listener.local_addr().map_err(Failure::Listen)?, "listening");
    let relay: JoinHandle<()> = relaying(store, attempts, changes.clone())?;
    let serving: Result<(), Failure> = axum::serve(listener, routes)
        .with_graceful_shutdown(stopped(either(interrupted(), terminated(terminate)), changes.clone()))
        .await
        .map_err(Failure::Serve);
    changes.stop();
    let relayed: Result<(), Failure> = tokio::task::spawn_blocking(move || relay.join())
        .await
        .map_err(|_cancelled: tokio::task::JoinError| Failure::RelayPanicked)
        .and_then(|joined: thread::Result<()>| joined.map_err(|_panic: Box<dyn std::any::Any + Send>| Failure::RelayPanicked));
    serving.and(relayed)
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt().with_writer(io::stderr).with_span_events(FmtSpan::CLOSE).init();
    let settings: Settings = Settings {
        database: PathBuf::from(env::var(DATABASE).unwrap_or(DEFAULT_DATABASE.to_string())),
        address: env::var(LISTEN).unwrap_or(DEFAULT_LISTEN.to_string()),
        durability: env::var(DURABILITY).ok(),
        attempts: env::var(ATTEMPTS).ok(),
    };
    match served(settings).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            tracing::error!("{}", explained(&failure));
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use std::future::{pending, ready};

    use domain::fulfilment::Fulfilment;
    use domain::payment::{Amount, Authorize, Capture};
    use ports::{StreamId, Version, View};
    use tokio::time::timeout;

    use super::*;

    fn tries(bound: u32) -> NonZeroU32 {
        NonZeroU32::new(bound).expect("at least one attempt")
    }

    fn captured() -> Sqlite {
        let store: Sqlite = Sqlite::in_memory().expect("opens");
        let amount: Amount = Amount::new(500).expect("positive");
        app::handle(&store, &StreamId::new("p-1"), Authorize { amount }).expect("authorized");
        app::handle(&store, &StreamId::new("p-1"), Capture { amount }).expect("captured");
        store
    }

    fn fulfilment(store: &Sqlite) -> (Version, Fulfilment) {
        store.read(|view: &mut View<'_>| view.state::<Fulfilment>(&StreamId::new("p-1"))).expect("read")
    }

    #[tokio::test]
    async fn with_nothing_in_the_outbox_the_relay_stops_when_asked() {
        let store: Sqlite = Sqlite::in_memory().expect("opens");
        let stopped: Result<(), tokio::time::error::Elapsed> = timeout(
            Duration::from_secs(5),
            relayed(&store, &AlwaysInStock, tries(3), Duration::from_secs(3600), ready(())),
        )
        .await;
        assert_eq!(stopped, Ok(()));
    }

    #[tokio::test]
    async fn a_relay_asked_to_stop_at_once_still_makes_one_pass() {
        let store: Sqlite = captured();
        relayed(&store, &AlwaysInStock, tries(3), Duration::from_secs(3600), ready(())).await;
        assert_eq!(fulfilment(&store), (Version::from(2), Fulfilment::Shipping));
    }

    #[tokio::test]
    async fn a_relay_left_running_passes_again_until_the_fulfilment_is_done() {
        let store: Sqlite = captured();
        relayed(
            &store,
            &AlwaysInStock,
            tries(3),
            Duration::from_millis(1),
            tokio::time::sleep(Duration::from_millis(250)),
        )
        .await;
        assert_eq!(fulfilment(&store), (Version::from(3), Fulfilment::Done));
    }

    #[tokio::test]
    async fn a_relay_that_is_not_asked_to_stop_keeps_running() {
        let store: Sqlite = Sqlite::in_memory().expect("opens");
        assert!(timeout(
            Duration::from_millis(50),
            relayed(&store, &AlwaysInStock, tries(3), Duration::from_millis(1), pending())
        )
        .await
        .is_err());
    }

    #[test]
    fn the_relay_gives_a_request_five_attempts_by_default() {
        assert_eq!(attempts_of(None).map_err(|failure: Failure| explained(&failure)), Ok(tries(5)));
    }

    #[test]
    fn one_attempt_is_named() {
        assert_eq!(attempts_of(Some("1")).map_err(|failure: Failure| explained(&failure)), Ok(tries(1)));
    }

    #[test]
    fn many_attempts_are_named() {
        assert_eq!(attempts_of(Some("12")).map_err(|failure: Failure| explained(&failure)), Ok(tries(12)));
    }

    #[test]
    fn no_attempt_at_all_stops_the_start_and_names_the_setting() {
        assert_eq!(
            attempts_of(Some("0")).map_err(|failure: Failure| explained(&failure)),
            Err("TASKS_RELAY_ATTEMPTS is \"0\"; it is a whole number of at least 1".to_string())
        );
    }

    #[test]
    fn attempts_that_are_not_a_number_stop_the_start() {
        assert_eq!(
            attempts_of(Some("many")).map_err(|failure: Failure| explained(&failure)),
            Err("TASKS_RELAY_ATTEMPTS is \"many\"; it is a whole number of at least 1".to_string())
        );
    }

    #[tokio::test]
    async fn the_first_request_to_stop_is_enough() {
        assert_eq!(timeout(Duration::from_secs(5), either(ready(()), pending())).await, Ok(()));
    }

    #[tokio::test]
    async fn the_second_request_to_stop_is_enough() {
        assert_eq!(timeout(Duration::from_secs(5), either(pending(), ready(()))).await, Ok(()));
    }

    #[tokio::test]
    async fn with_no_request_to_stop_the_server_keeps_serving() {
        assert!(timeout(Duration::from_millis(50), either(pending(), pending())).await.is_err());
    }

    #[test]
    fn durability_defaults_to_full() {
        assert_eq!(durability_of(None).map_err(|failure: Failure| explained(&failure)), Ok(Durability::Full));
    }

    #[test]
    fn full_is_named() {
        assert_eq!(
            durability_of(Some("full")).map_err(|failure: Failure| explained(&failure)),
            Ok(Durability::Full)
        );
    }

    #[test]
    fn normal_is_named() {
        assert_eq!(
            durability_of(Some("normal")).map_err(|failure: Failure| explained(&failure)),
            Ok(Durability::Normal)
        );
    }

    #[test]
    fn an_unknown_durability_stops_the_start_and_names_the_setting() {
        assert_eq!(
            durability_of(Some("fast")).map_err(|failure: Failure| explained(&failure)),
            Err("TASKS_DURABILITY is \"fast\"; it is full or normal".to_string())
        );
    }
}
