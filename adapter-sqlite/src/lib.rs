use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use ports::store::{Key, Listener, Reader, Store, StoredFact, StoredRequest, Writer};
use ports::{Position, RequestId, StoreError, StreamId, Version};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

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

pub struct Sqlite {
    connection: Mutex<Connection>,
    listener: Option<Listener>,
}

fn unavailable(error: rusqlite::Error) -> StoreError {
    StoreError::Unavailable { cause: error.to_string() }
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
}

fn text<'a>(table: &'static str, row: &'a rusqlite::Row<'_>, column: usize) -> Result<&'a str, StoreError> {
    row.get_ref(column)
        .map_err(unavailable)?
        .as_str()
        .map_err(|error: rusqlite::types::FromSqlError| StoreError::Corrupt {
            table,
            cause: error.to_string(),
        })
}

struct Session<'a> {
    transaction: &'a Transaction<'a>,
}

impl Session<'_> {
    fn execute(&self, sql: &str, values: impl rusqlite::Params) -> Result<(), StoreError> {
        self.transaction
            .prepare_cached(sql)
            .map_err(unavailable)?
            .execute(values)
            .map(|_changed: usize| ())
            .map_err(unavailable)
    }
}

impl Reader for Session<'_> {
    fn state(&mut self, lifecycle: &'static str, stream: &StreamId) -> Result<Option<(Version, String)>, StoreError> {
        let found: Option<(i64, String)> = self
            .transaction
            .prepare_cached("SELECT version, state FROM states WHERE lifecycle = ?1 AND stream = ?2")
            .map_err(unavailable)?
            .query_row(params![lifecycle, stream.0], |row: &rusqlite::Row<'_>| Ok((row.get(0)?, row.get(1)?)))
            .optional()
            .map_err(unavailable)?;
        match found {
            None => Ok(None),
            Some((version, state)) => Ok(Some((Version(loaded("states", version)?), state))),
        }
    }

    fn states(&mut self, lifecycle: &'static str, each: &mut dyn FnMut(StreamId, &str) -> Result<(), StoreError>) -> Result<(), StoreError> {
        let mut statement = self
            .transaction
            .prepare_cached("SELECT stream, state FROM states WHERE lifecycle = ?1 ORDER BY stream")
            .map_err(unavailable)?;
        let mut rows = statement.query(params![lifecycle]).map_err(unavailable)?;
        while let Some(row) = rows.next().map_err(unavailable)? {
            each(StreamId(row.get(0).map_err(unavailable)?), text("states", row, 1)?)?;
        }
        Ok(())
    }

    fn row(&mut self, read_model: &'static str, key: &str) -> Result<Option<String>, StoreError> {
        self.transaction
            .prepare_cached("SELECT row FROM read_models WHERE name = ?1 AND key = ?2")
            .map_err(unavailable)?
            .query_row(params![read_model, key], |row: &rusqlite::Row<'_>| row.get(0))
            .optional()
            .map_err(unavailable)
    }

    fn rows(&mut self, read_model: &'static str, each: &mut dyn FnMut(String, &str) -> Result<(), StoreError>) -> Result<(), StoreError> {
        let mut statement = self
            .transaction
            .prepare_cached("SELECT key, row FROM read_models WHERE name = ?1 ORDER BY key")
            .map_err(unavailable)?;
        let mut rows = statement.query(params![read_model]).map_err(unavailable)?;
        while let Some(row) = rows.next().map_err(unavailable)? {
            each(row.get(0).map_err(unavailable)?, text("read_models", row, 1)?)?;
        }
        Ok(())
    }

    fn refusals(&mut self, lifecycle: &'static str, stream: &StreamId) -> Result<Vec<String>, StoreError> {
        let mut statement = self
            .transaction
            .prepare_cached("SELECT refusal FROM rejections WHERE lifecycle = ?1 AND stream = ?2 ORDER BY id")
            .map_err(unavailable)?;
        let rows = statement
            .query_map(params![lifecycle, stream.0], |row: &rusqlite::Row<'_>| row.get::<_, String>(0))
            .map_err(unavailable)?;
        rows.collect::<Result<Vec<String>, rusqlite::Error>>().map_err(unavailable)
    }
}

impl Writer for Session<'_> {
    fn append(&mut self, lifecycle: &'static str, stream: &StreamId, version: Version, event: &str) -> Result<Position, StoreError> {
        self.execute(
            "INSERT INTO facts (lifecycle, stream, version, event) VALUES (?1, ?2, ?3, ?4)",
            params![lifecycle, stream.0, stored(version.0)?, event],
        )?;
        Ok(Position(loaded("facts", self.transaction.last_insert_rowid())?))
    }

    fn save(&mut self, lifecycle: &'static str, stream: &StreamId, version: Version, state: &str) -> Result<(), StoreError> {
        self.execute(
            "INSERT INTO states (lifecycle, stream, version, state) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (lifecycle, stream) DO UPDATE SET version = excluded.version, state = excluded.state",
            params![lifecycle, stream.0, stored(version.0)?, state],
        )
    }

    fn reject(&mut self, lifecycle: &'static str, stream: &StreamId, refusal: &str) -> Result<(), StoreError> {
        self.execute(
            "INSERT INTO rejections (lifecycle, stream, refusal) VALUES (?1, ?2, ?3)",
            params![lifecycle, stream.0, refusal],
        )
    }

    fn request(&mut self, lifecycle: &'static str, stream: &StreamId, request: &str) -> Result<(), StoreError> {
        self.execute(
            "INSERT INTO outbox (lifecycle, stream, request) VALUES (?1, ?2, ?3)",
            params![lifecycle, stream.0, request],
        )
    }

    fn facts(
        &mut self,
        lifecycle: &'static str,
        after: Position,
        limit: usize,
        each: &mut dyn FnMut(StoredFact<'_>) -> Result<(), StoreError>,
    ) -> Result<(), StoreError> {
        let limit: i64 = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut statement = self
            .transaction
            .prepare_cached("SELECT position, stream, version, event FROM facts WHERE lifecycle = ?1 AND position > ?2 ORDER BY position LIMIT ?3")
            .map_err(unavailable)?;
        let mut rows = statement.query(params![lifecycle, stored(after.0)?, limit]).map_err(unavailable)?;
        while let Some(row) = rows.next().map_err(unavailable)? {
            each(StoredFact {
                position: Position(loaded("facts", row.get(0).map_err(unavailable)?)?),
                stream: StreamId(row.get(1).map_err(unavailable)?),
                version: Version(loaded("facts", row.get(2).map_err(unavailable)?)?),
                event: text("facts", row, 3)?,
            })?;
        }
        Ok(())
    }

    fn position(&mut self, read_model: &'static str) -> Result<Position, StoreError> {
        let found: Option<i64> = self
            .transaction
            .prepare_cached("SELECT position FROM read_model_positions WHERE name = ?1")
            .map_err(unavailable)?
            .query_row(params![read_model], |row: &rusqlite::Row<'_>| row.get(0))
            .optional()
            .map_err(unavailable)?;
        Ok(Position(loaded("read_model_positions", found.unwrap_or(0))?))
    }

    fn advance(&mut self, read_model: &'static str, position: Position) -> Result<(), StoreError> {
        self.execute(
            "INSERT INTO read_model_positions (name, position) VALUES (?1, ?2)
             ON CONFLICT (name) DO UPDATE SET position = excluded.position",
            params![read_model, stored(position.0)?],
        )
    }

    fn upsert(&mut self, read_model: &'static str, key: &str, row: &str) -> Result<(), StoreError> {
        self.execute(
            "INSERT INTO read_models (name, key, row) VALUES (?1, ?2, ?3)
             ON CONFLICT (name, key) DO UPDATE SET row = excluded.row",
            params![read_model, key, row],
        )
    }

    fn delete(&mut self, read_model: &'static str, key: &str) -> Result<(), StoreError> {
        self.execute("DELETE FROM read_models WHERE name = ?1 AND key = ?2", params![read_model, key])
    }

    fn clear(&mut self, read_model: &'static str) -> Result<(), StoreError> {
        self.execute("DELETE FROM read_models WHERE name = ?1", params![read_model])?;
        self.execute("DELETE FROM read_model_positions WHERE name = ?1", params![read_model])
    }
}

impl Store for Sqlite {
    fn transact<T>(&self, _key: Key, work: impl FnOnce(&mut dyn Writer) -> Result<T, StoreError>) -> Result<T, StoreError> {
        let mut connection: MutexGuard<'_, Connection> = self.connection()?;
        let transaction: Transaction<'_> = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(unavailable)?;
        let mut writer: Session<'_> = Session { transaction: &transaction };
        let value: T = work(&mut writer)?;
        transaction.commit().map_err(unavailable)?;
        Ok(value)
    }

    fn snapshot<T>(&self, _key: Key, work: impl FnOnce(&mut dyn Reader) -> Result<T, StoreError>) -> Result<T, StoreError> {
        let mut connection: MutexGuard<'_, Connection> = self.connection()?;
        let transaction: Transaction<'_> = connection.transaction().map_err(unavailable)?;
        work(&mut Session { transaction: &transaction })
    }

    fn committed(&self, _key: Key, last: Position) {
        if let Some(listener) = &self.listener {
            listener(last);
        }
    }

    fn requests(&self, _key: Key, lifecycle: &'static str, limit: usize) -> Result<Vec<StoredRequest>, StoreError> {
        let connection: MutexGuard<'_, Connection> = self.connection()?;
        let limit: i64 = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut statement = connection
            .prepare_cached("SELECT id, stream, request FROM outbox WHERE lifecycle = ?1 AND done = 0 ORDER BY attempts, id LIMIT ?2")
            .map_err(unavailable)?;
        let rows = statement
            .query_map(params![lifecycle, limit], |row: &rusqlite::Row<'_>| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
            })
            .map_err(unavailable)?;
        let mut requests: Vec<StoredRequest> = Vec::new();
        for row in rows {
            let (id, stream, request) = row.map_err(unavailable)?;
            requests.push(StoredRequest {
                id: RequestId(loaded("outbox", id)?),
                stream: StreamId(stream),
                request,
            });
        }
        Ok(requests)
    }

    fn mark_done(&self, _key: Key, id: RequestId) -> Result<(), StoreError> {
        let connection: MutexGuard<'_, Connection> = self.connection()?;
        connection
            .execute("UPDATE outbox SET done = 1 WHERE id = ?1", params![stored(id.0)?])
            .map_err(unavailable)?;
        Ok(())
    }

    fn mark_failed(&self, _key: Key, id: RequestId) -> Result<(), StoreError> {
        let connection: MutexGuard<'_, Connection> = self.connection()?;
        connection
            .execute("UPDATE outbox SET attempts = attempts + 1 WHERE id = ?1", params![stored(id.0)?])
            .map_err(unavailable)?;
        Ok(())
    }
}
