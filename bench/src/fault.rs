use std::fmt;

use ports::StoreError;
use tokio::task::JoinError;

#[derive(Debug)]
pub enum Fault {
    Io(std::io::Error),
    Store(StoreError),
    Sql(rusqlite::Error),
    Json(serde_json::Error),
    Join(JoinError),
    VolatileFilesystem(String),
    Unexpected(String),
}

impl fmt::Display for Fault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Fault::Io(error) => write!(formatter, "i/o: {error}"),
            Fault::Store(error) => write!(formatter, "store: {error:?}"),
            Fault::Sql(error) => write!(formatter, "sqlite: {error}"),
            Fault::Json(error) => write!(formatter, "json: {error}"),
            Fault::Join(error) => write!(formatter, "tokio: {error}"),
            Fault::VolatileFilesystem(line) => write!(formatter, "refusing to measure on a volatile filesystem: {line}"),
            Fault::Unexpected(what) => write!(formatter, "unexpected: {what}"),
        }
    }
}

impl From<std::io::Error> for Fault {
    fn from(error: std::io::Error) -> Fault {
        Fault::Io(error)
    }
}

impl From<StoreError> for Fault {
    fn from(error: StoreError) -> Fault {
        Fault::Store(error)
    }
}

impl From<rusqlite::Error> for Fault {
    fn from(error: rusqlite::Error) -> Fault {
        Fault::Sql(error)
    }
}

impl From<serde_json::Error> for Fault {
    fn from(error: serde_json::Error) -> Fault {
        Fault::Json(error)
    }
}

impl From<JoinError> for Fault {
    fn from(error: JoinError) -> Fault {
        Fault::Join(error)
    }
}
