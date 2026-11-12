use std::env;
use std::future::Future;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use adapter_http::{router, Changes};
use adapter_sqlite::{Durability, Sqlite};
use axum::Router;
use ports::StoreError;
use tokio::net::TcpListener;
use tokio::signal::unix::{signal, Signal, SignalKind};
use tracing_subscriber::fmt::format::FmtSpan;

const DATABASE: &str = "TASKS_DATABASE";
const LISTEN: &str = "TASKS_LISTEN";
const DURABILITY: &str = "TASKS_DURABILITY";

const DEFAULT_DATABASE: &str = "tasks.sqlite";
const DEFAULT_LISTEN: &str = "127.0.0.1:3000";

enum Failure {
    UnknownDurability(String),
    Store(StoreError),
    Listen(io::Error),
    Serve(io::Error),
    Signal(io::Error),
}

fn explained(failure: &Failure) -> String {
    match failure {
        Failure::UnknownDurability(setting) => format!("{DURABILITY} is {setting:?}; it is full or normal"),
        Failure::Store(error) => format!("cannot open the database: {error:?}"),
        Failure::Listen(error) => format!("cannot listen: {error}"),
        Failure::Serve(error) => format!("stopped serving: {error}"),
        Failure::Signal(error) => format!("cannot wait for SIGTERM: {error}"),
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

async fn served(database: PathBuf, address: String, durability: Option<String>) -> Result<(), Failure> {
    let durability: Durability = durability_of(durability.as_deref())?;
    let changes: Changes = Changes::default();
    let store: Sqlite = Sqlite::open(&database, durability).map_err(Failure::Store)?.on_commit(changes.on_commit());
    let routes: Router = router(Arc::new(store), changes.clone());
    let terminate: Signal = signal(SignalKind::terminate()).map_err(Failure::Signal)?;
    let listener: TcpListener = TcpListener::bind(&address).await.map_err(Failure::Listen)?;
    tracing::info!(address = %listener.local_addr().map_err(Failure::Listen)?, "listening");
    axum::serve(listener, routes)
        .with_graceful_shutdown(stopped(either(interrupted(), terminated(terminate)), changes))
        .await
        .map_err(Failure::Serve)
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt().with_writer(io::stderr).with_span_events(FmtSpan::CLOSE).init();
    let database: PathBuf = PathBuf::from(env::var(DATABASE).unwrap_or(DEFAULT_DATABASE.to_string()));
    let address: String = env::var(LISTEN).unwrap_or(DEFAULT_LISTEN.to_string());
    match served(database, address, env::var(DURABILITY).ok()).await {
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
    use std::time::Duration;

    use tokio::time::timeout;

    use super::*;

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
