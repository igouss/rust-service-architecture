use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use domain::{Lifecycle, Projection, Reaction};
use ports::{Answer, Applied, Change, Claimed, Journal, Outbox, Outcome, Position, Recorded, RequestId, StoreError, StreamId, Unit, Version, View, Write};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::de::DeserializeOwned;
use serde::Serialize;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS states (
    lifecycle TEXT NOT NULL,
    stream TEXT NOT NULL,
    version INTEGER NOT NULL,
    state TEXT NOT NULL,
    PRIMARY KEY (lifecycle, stream)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS facts (
    position INTEGER PRIMARY KEY AUTOINCREMENT,
    lifecycle TEXT NOT NULL,
    stream TEXT NOT NULL,
    version INTEGER NOT NULL,
    event TEXT NOT NULL,
    recorded_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (lifecycle, stream, version)
);
CREATE TABLE IF NOT EXISTS rejections (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    lifecycle TEXT NOT NULL,
    stream TEXT NOT NULL,
    refusal TEXT NOT NULL,
    recorded_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS outbox (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    lifecycle TEXT NOT NULL,
    stream TEXT NOT NULL,
    request TEXT NOT NULL,
    done INTEGER NOT NULL DEFAULT 0,
    attempts INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS read_models (
    name TEXT NOT NULL,
    key TEXT NOT NULL,
    row TEXT NOT NULL,
    PRIMARY KEY (name, key)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS read_model_positions (
    name TEXT PRIMARY KEY,
    position INTEGER NOT NULL
) WITHOUT ROWID;
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Durability {
    Full,
    Normal,
}

type Listener = Box<dyn Fn(Position) + Send + Sync>;

pub struct Sqlite {
    connection: Mutex<Connection>,
    listener: Option<Listener>,
}

fn unavailable(error: rusqlite::Error) -> StoreError {
    StoreError::Unavailable { cause: error.to_string() }
}

fn corrupt(table: &'static str) -> impl Fn(serde_json::Error) -> StoreError {
    move |error: serde_json::Error| StoreError::Corrupt {
        table,
        cause: error.to_string(),
    }
}

fn stored(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::Unavailable {
        cause: format!("{value} exceeds the store's integer range"),
    })
}

fn loaded(table: &'static str, value: i64) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| StoreError::Corrupt {
        table,
        cause: format!("negative integer {value}"),
    })
}

fn encode<T: Serialize>(table: &'static str, value: &T) -> Result<String, StoreError> {
    serde_json::to_string(value).map_err(corrupt(table))
}

fn decode<T: DeserializeOwned>(table: &'static str, text: &str) -> Result<T, StoreError> {
    serde_json::from_str(text).map_err(corrupt(table))
}

impl Sqlite {
    fn prepared(connection: Connection, durability: Durability) -> Result<Sqlite, StoreError> {
        let synchronous: &str = match durability {
            Durability::Full => "FULL",
            Durability::Normal => "NORMAL",
        };
        connection.pragma_update(None, "journal_mode", "WAL").map_err(unavailable)?;
        connection.pragma_update(None, "synchronous", synchronous).map_err(unavailable)?;
        connection.execute_batch(SCHEMA).map_err(unavailable)?;
        Ok(Sqlite {
            connection: Mutex::new(connection),
            listener: None,
        })
    }

    pub fn open(path: &Path, durability: Durability) -> Result<Sqlite, StoreError> {
        Sqlite::prepared(Connection::open(path).map_err(unavailable)?, durability)
    }

    pub fn in_memory() -> Result<Sqlite, StoreError> {
        Sqlite::prepared(Connection::open_in_memory().map_err(unavailable)?, Durability::Normal)
    }

    pub fn on_commit(self, listener: impl Fn(Position) + Send + Sync + 'static) -> Sqlite {
        Sqlite {
            connection: self.connection,
            listener: Some(Box::new(listener)),
        }
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, StoreError> {
        Ok(self.connection.lock().unwrap_or_else(PoisonError::into_inner))
    }

    pub fn rejections(&self, lifecycle: &str, stream: &StreamId) -> Result<Vec<String>, StoreError> {
        let connection: MutexGuard<'_, Connection> = self.connection()?;
        let mut statement = connection
            .prepare_cached("SELECT refusal FROM rejections WHERE lifecycle = ?1 AND stream = ?2 ORDER BY id")
            .map_err(unavailable)?;
        let rows = statement
            .query_map(params![lifecycle, stream.0], |row: &rusqlite::Row<'_>| row.get::<_, String>(0))
            .map_err(unavailable)?;
        rows.collect::<Result<Vec<String>, rusqlite::Error>>().map_err(unavailable)
    }
}

fn state_of<S: Lifecycle>(connection: &Connection, id: &StreamId) -> Result<(Version, S), StoreError> {
    let found: Option<(i64, String)> = connection
        .prepare_cached("SELECT version, state FROM states WHERE lifecycle = ?1 AND stream = ?2")
        .map_err(unavailable)?
        .query_row(params![S::NAME, id.0], |row: &rusqlite::Row<'_>| Ok((row.get(0)?, row.get(1)?)))
        .optional()
        .map_err(unavailable)?;
    match found {
        None => Ok((Version(0), S::initial())),
        Some((version, state)) => Ok((Version(loaded("states", version)?), decode("states", &state)?)),
    }
}

fn row_of<P: Projection>(connection: &Connection, key: &str) -> Result<Option<P::Row>, StoreError> {
    let found: Option<String> = connection
        .prepare_cached("SELECT row FROM read_models WHERE name = ?1 AND key = ?2")
        .map_err(unavailable)?
        .query_row(params![P::NAME, key], |row: &rusqlite::Row<'_>| row.get(0))
        .optional()
        .map_err(unavailable)?;
    found.map(|text: String| decode("read_models", &text)).transpose()
}

pub struct SqliteUnit<'a> {
    transaction: &'a Transaction<'a>,
    last: Option<Position>,
}

impl SqliteUnit<'_> {
    fn position_of(&self, read_model: &'static str) -> Result<Position, StoreError> {
        let found: Option<i64> = self
            .transaction
            .prepare_cached("SELECT position FROM read_model_positions WHERE name = ?1")
            .map_err(unavailable)?
            .query_row(params![read_model], |row: &rusqlite::Row<'_>| row.get(0))
            .optional()
            .map_err(unavailable)?;
        Ok(Position(loaded("read_model_positions", found.unwrap_or(0))?))
    }
}

impl Unit for SqliteUnit<'_> {
    fn apply<S: Lifecycle, R: Serialize>(&mut self, id: &StreamId, decide: impl FnOnce(S) -> Outcome<S, R>) -> Result<Applied<S>, StoreError> {
        let (version, state): (Version, S) = state_of(self.transaction, id)?;
        match decide(state) {
            Outcome::Refused(refusal) => {
                self.transaction
                    .prepare_cached("INSERT INTO rejections (lifecycle, stream, refusal) VALUES (?1, ?2, ?3)")
                    .map_err(unavailable)?
                    .execute(params![S::NAME, id.0, encode("rejections", &refusal)?])
                    .map_err(unavailable)?;
                Ok(Applied {
                    answer: Answer::Refused(refusal),
                    facts: Vec::new(),
                })
            }
            Outcome::Accepted(transition) => {
                let (state, events, requests) = transition.into_parts();
                let mut next: Version = version;
                let mut facts: Vec<Recorded<S::Event>> = Vec::with_capacity(events.len());
                for event in events {
                    next = Version(next.0 + 1);
                    self.transaction
                        .prepare_cached("INSERT INTO facts (lifecycle, stream, version, event) VALUES (?1, ?2, ?3, ?4)")
                        .map_err(unavailable)?
                        .execute(params![S::NAME, id.0, stored(next.0)?, encode("facts", &event)?])
                        .map_err(unavailable)?;
                    let position: Position = Position(loaded("facts", self.transaction.last_insert_rowid())?);
                    self.last = Some(position);
                    facts.push(Recorded {
                        position,
                        stream: id.clone(),
                        version: next,
                        event,
                    });
                }
                if next != version {
                    self.transaction
                        .prepare_cached(
                            "INSERT INTO states (lifecycle, stream, version, state) VALUES (?1, ?2, ?3, ?4)
                             ON CONFLICT (lifecycle, stream) DO UPDATE SET version = excluded.version, state = excluded.state",
                        )
                        .map_err(unavailable)?
                        .execute(params![S::NAME, id.0, stored(next.0)?, encode("states", &state)?])
                        .map_err(unavailable)?;
                }
                for request in &requests {
                    self.transaction
                        .prepare_cached("INSERT INTO outbox (lifecycle, stream, request) VALUES (?1, ?2, ?3)")
                        .map_err(unavailable)?
                        .execute(params![S::NAME, id.0, encode("outbox", request)?])
                        .map_err(unavailable)?;
                }
                Ok(Applied {
                    answer: Answer::Accepted(next),
                    facts,
                })
            }
        }
    }

    fn row<P: Projection>(&mut self, key: &str) -> Result<Option<P::Row>, StoreError> {
        row_of::<P>(self.transaction, key)
    }

    fn put<P: Projection>(&mut self, change: Change<P>) -> Result<(), StoreError> {
        let (position, write) = change.into_parts();
        let at: Position = self.position_of(P::NAME)?;
        if position <= at {
            return Err(StoreError::OutOfOrder {
                read_model: P::NAME,
                at,
                offered: position,
            });
        }
        match write {
            Write::Nothing => {}
            Write::Upsert(key, row) => {
                self.transaction
                    .prepare_cached(
                        "INSERT INTO read_models (name, key, row) VALUES (?1, ?2, ?3)
                         ON CONFLICT (name, key) DO UPDATE SET row = excluded.row",
                    )
                    .map_err(unavailable)?
                    .execute(params![P::NAME, key, encode("read_models", &row)?])
                    .map_err(unavailable)?;
            }
            Write::Delete(key) => {
                self.transaction
                    .prepare_cached("DELETE FROM read_models WHERE name = ?1 AND key = ?2")
                    .map_err(unavailable)?
                    .execute(params![P::NAME, key])
                    .map_err(unavailable)?;
            }
        }
        self.transaction
            .prepare_cached(
                "INSERT INTO read_model_positions (name, position) VALUES (?1, ?2)
                 ON CONFLICT (name) DO UPDATE SET position = excluded.position",
            )
            .map_err(unavailable)?
            .execute(params![P::NAME, stored(position.0)?])
            .map_err(unavailable)?;
        Ok(())
    }

    fn clear<P: Projection>(&mut self) -> Result<(), StoreError> {
        self.transaction
            .execute("DELETE FROM read_models WHERE name = ?1", params![P::NAME])
            .map_err(unavailable)?;
        self.transaction
            .execute("DELETE FROM read_model_positions WHERE name = ?1", params![P::NAME])
            .map_err(unavailable)?;
        Ok(())
    }

    fn facts<S: Lifecycle>(&mut self, after: Position, limit: usize) -> Result<Vec<Recorded<S::Event>>, StoreError> {
        let limit: i64 = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut statement = self
            .transaction
            .prepare_cached("SELECT position, stream, version, event FROM facts WHERE lifecycle = ?1 AND position > ?2 ORDER BY position LIMIT ?3")
            .map_err(unavailable)?;
        let rows = statement
            .query_map(params![S::NAME, stored(after.0)?, limit], |row: &rusqlite::Row<'_>| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, String>(3)?))
            })
            .map_err(unavailable)?;
        let mut facts: Vec<Recorded<S::Event>> = Vec::new();
        for row in rows {
            let (position, stream, version, event) = row.map_err(unavailable)?;
            facts.push(Recorded {
                position: Position(loaded("facts", position)?),
                stream: StreamId(stream),
                version: Version(loaded("facts", version)?),
                event: decode("facts", &event)?,
            });
        }
        Ok(facts)
    }
}

pub struct SqliteView<'a> {
    connection: &'a Connection,
}

impl View for SqliteView<'_> {
    fn state<S: Lifecycle>(&mut self, id: &StreamId) -> Result<(Version, S), StoreError> {
        state_of(self.connection, id)
    }

    fn states<S: Lifecycle>(&mut self) -> Result<Vec<(StreamId, S)>, StoreError> {
        let mut statement = self
            .connection
            .prepare_cached("SELECT stream, state FROM states WHERE lifecycle = ?1 ORDER BY stream")
            .map_err(unavailable)?;
        let rows = statement
            .query_map(params![S::NAME], |row: &rusqlite::Row<'_>| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(unavailable)?;
        let mut states: Vec<(StreamId, S)> = Vec::new();
        for row in rows {
            let (stream, state) = row.map_err(unavailable)?;
            states.push((StreamId(stream), decode("states", &state)?));
        }
        Ok(states)
    }

    fn row<P: Projection>(&mut self, key: &str) -> Result<Option<P::Row>, StoreError> {
        row_of::<P>(self.connection, key)
    }

    fn rows<P: Projection>(&mut self) -> Result<Vec<(String, P::Row)>, StoreError> {
        let mut statement = self
            .connection
            .prepare_cached("SELECT key, row FROM read_models WHERE name = ?1 ORDER BY key")
            .map_err(unavailable)?;
        let rows = statement
            .query_map(params![P::NAME], |row: &rusqlite::Row<'_>| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(unavailable)?;
        let mut found: Vec<(String, P::Row)> = Vec::new();
        for row in rows {
            let (key, text) = row.map_err(unavailable)?;
            found.push((key, decode("read_models", &text)?));
        }
        Ok(found)
    }
}

impl Journal for Sqlite {
    type Unit<'a> = SqliteUnit<'a>;
    type View<'a> = SqliteView<'a>;

    fn write<T>(&self, work: impl FnOnce(&mut SqliteUnit<'_>) -> Result<T, StoreError>) -> Result<T, StoreError> {
        let mut connection: MutexGuard<'_, Connection> = self.connection()?;
        let transaction: Transaction<'_> = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(unavailable)?;
        let mut unit: SqliteUnit<'_> = SqliteUnit {
            transaction: &transaction,
            last: None,
        };
        let value: T = work(&mut unit)?;
        let last: Option<Position> = unit.last;
        transaction.commit().map_err(unavailable)?;
        drop(connection);
        if let (Some(position), Some(listener)) = (last, &self.listener) {
            listener(position);
        }
        Ok(value)
    }

    fn read<T>(&self, work: impl FnOnce(&mut SqliteView<'_>) -> Result<T, StoreError>) -> Result<T, StoreError> {
        let connection: MutexGuard<'_, Connection> = self.connection()?;
        let mut view: SqliteView<'_> = SqliteView { connection: &connection };
        work(&mut view)
    }
}

impl Outbox for Sqlite {
    fn claim<X: Reaction>(&self, limit: usize) -> Result<Vec<Claimed<X::Request>>, StoreError> {
        let connection: MutexGuard<'_, Connection> = self.connection()?;
        let limit: i64 = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut statement = connection
            .prepare_cached("SELECT id, stream, request FROM outbox WHERE lifecycle = ?1 AND done = 0 ORDER BY attempts, id LIMIT ?2")
            .map_err(unavailable)?;
        let rows = statement
            .query_map(params![<X::Reads as Lifecycle>::NAME, limit], |row: &rusqlite::Row<'_>| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
            })
            .map_err(unavailable)?;
        let mut claimed: Vec<Claimed<X::Request>> = Vec::new();
        for row in rows {
            let (id, stream, request) = row.map_err(unavailable)?;
            claimed.push(Claimed {
                id: RequestId(loaded("outbox", id)?),
                stream: StreamId(stream),
                request: decode("outbox", &request)?,
            });
        }
        Ok(claimed)
    }

    fn done(&self, id: RequestId) -> Result<(), StoreError> {
        let connection: MutexGuard<'_, Connection> = self.connection()?;
        connection
            .execute("UPDATE outbox SET done = 1 WHERE id = ?1", params![stored(id.0)?])
            .map_err(unavailable)?;
        Ok(())
    }

    fn failed(&self, id: RequestId) -> Result<(), StoreError> {
        let connection: MutexGuard<'_, Connection> = self.connection()?;
        connection
            .execute("UPDATE outbox SET attempts = attempts + 1 WHERE id = ?1", params![stored(id.0)?])
            .map_err(unavailable)?;
        Ok(())
    }
}
