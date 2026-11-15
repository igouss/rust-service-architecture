use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use ports::store::{Key, Reader, Store, StoredFact, StoredRequest, Writer};
use ports::{Position, RequestId, StoreError, StreamId, Version};
use postgres::types::ToSql;
use postgres::{Client, IsolationLevel, NoTls, Row, Statement, Transaction};

mod statements;

use statements::{Statements, SCHEMA};

type Listener = Box<dyn Fn(Position) + Send + Sync>;
type Values<'a> = &'a [&'a (dyn ToSql + Sync)];

struct Connection {
    client: Client,
    statements: Statements,
}

pub struct Postgres {
    writer: Mutex<Connection>,
    readers: Vec<Mutex<Connection>>,
    next: AtomicUsize,
    lock: i64,
    listener: Option<Listener>,
}

fn unavailable(error: postgres::Error) -> StoreError {
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

fn number(table: &'static str, row: &Row, column: usize) -> Result<u64, StoreError> {
    loaded(table, row.try_get(column).map_err(unavailable)?)
}

fn text(row: &Row, column: usize) -> Result<&str, StoreError> {
    row.try_get(column).map_err(unavailable)
}

fn named(schema: &str) -> Result<&str, StoreError> {
    let plain: bool = schema
        .chars()
        .all(|letter: char| letter.is_ascii_lowercase() || letter.is_ascii_digit() || letter == '_');
    match plain && schema.starts_with(|letter: char| letter.is_ascii_lowercase()) {
        true => Ok(schema),
        false => Err(StoreError::Unavailable {
            cause: format!("schema name {schema:?} must be lower-case letters, digits and underscores, starting with a letter"),
        }),
    }
}

fn connection(url: &str, schema: &str) -> Result<Connection, StoreError> {
    let mut client: Client = Client::connect(url, NoTls).map_err(unavailable)?;
    client.batch_execute(&format!("SET search_path TO {schema}")).map_err(unavailable)?;
    let statements: Statements = Statements::prepare(&mut client).map_err(unavailable)?;
    Ok(Connection { client, statements })
}

impl Postgres {
    pub fn connect(url: &str, schema: &str, readers: usize) -> Result<Postgres, StoreError> {
        let schema: &str = named(schema)?;
        let mut setup: Client = Client::connect(url, NoTls).map_err(unavailable)?;
        setup
            .batch_execute(&format!("CREATE SCHEMA IF NOT EXISTS {schema}; SET search_path TO {schema}; {SCHEMA}"))
            .map_err(unavailable)?;
        let lock: i64 = setup
            .query_one("SELECT to_regnamespace($1)::oid::bigint", &[&schema])
            .map_err(unavailable)?
            .try_get(0)
            .map_err(unavailable)?;
        drop(setup);
        let readers: Vec<Mutex<Connection>> = (0..readers.max(1))
            .map(|_reader: usize| connection(url, schema).map(Mutex::new))
            .collect::<Result<Vec<Mutex<Connection>>, StoreError>>()?;
        Ok(Postgres {
            writer: Mutex::new(connection(url, schema)?),
            readers,
            next: AtomicUsize::new(0),
            lock,
            listener: None,
        })
    }

    pub fn on_commit(self, listener: impl Fn(Position) + Send + Sync + 'static) -> Postgres {
        Postgres {
            listener: Some(Box::new(listener)),
            ..self
        }
    }

    pub fn rejections(&self, lifecycle: &str, stream: &StreamId) -> Result<Vec<String>, StoreError> {
        let mut reader: MutexGuard<'_, Connection> = self.reader();
        let Connection { client, statements } = &mut *reader;
        client
            .query(&statements.rejections, &[&lifecycle, &stream.0])
            .map_err(unavailable)?
            .iter()
            .map(|row: &Row| text(row, 0).map(str::to_string))
            .collect()
    }

    fn writer(&self) -> MutexGuard<'_, Connection> {
        self.writer.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn reader(&self) -> MutexGuard<'_, Connection> {
        let turn: usize = self.next.fetch_add(1, Ordering::Relaxed) % self.readers.len();
        self.readers[turn].lock().unwrap_or_else(PoisonError::into_inner)
    }
}

struct Session<'a> {
    transaction: Transaction<'a>,
    statements: &'a Statements,
    last: Option<Position>,
}

impl Session<'_> {
    fn execute(&mut self, statement: &Statement, values: Values<'_>) -> Result<(), StoreError> {
        self.transaction.execute(statement, values).map(|_changed: u64| ()).map_err(unavailable)
    }
}

impl Reader for Session<'_> {
    fn state(&mut self, lifecycle: &'static str, stream: &StreamId) -> Result<Option<(Version, String)>, StoreError> {
        match self
            .transaction
            .query_opt(&self.statements.state, &[&lifecycle, &stream.0])
            .map_err(unavailable)?
        {
            None => Ok(None),
            Some(row) => Ok(Some((Version(number("states", &row, 0)?), text(&row, 1)?.to_string()))),
        }
    }

    fn states(&mut self, lifecycle: &'static str, each: &mut dyn FnMut(StreamId, &str) -> Result<(), StoreError>) -> Result<(), StoreError> {
        self.transaction
            .query(&self.statements.states, &[&lifecycle])
            .map_err(unavailable)?
            .iter()
            .try_for_each(|row: &Row| each(StreamId(text(row, 0)?.to_string()), text(row, 1)?))
    }

    fn row(&mut self, read_model: &'static str, key: &str) -> Result<Option<String>, StoreError> {
        match self.transaction.query_opt(&self.statements.row, &[&read_model, &key]).map_err(unavailable)? {
            None => Ok(None),
            Some(row) => Ok(Some(text(&row, 0)?.to_string())),
        }
    }

    fn rows(&mut self, read_model: &'static str, each: &mut dyn FnMut(String, &str) -> Result<(), StoreError>) -> Result<(), StoreError> {
        self.transaction
            .query(&self.statements.rows, &[&read_model])
            .map_err(unavailable)?
            .iter()
            .try_for_each(|row: &Row| each(text(row, 0)?.to_string(), text(row, 1)?))
    }
}

impl Writer for Session<'_> {
    fn append(&mut self, lifecycle: &'static str, stream: &StreamId, version: Version, event: &str) -> Result<Position, StoreError> {
        let row: Row = self
            .transaction
            .query_one(&self.statements.append, &[&lifecycle, &stream.0, &stored(version.0)?, &event])
            .map_err(unavailable)?;
        let position: Position = Position(number("facts", &row, 0)?);
        self.last = Some(position);
        Ok(position)
    }

    fn save(&mut self, lifecycle: &'static str, stream: &StreamId, version: Version, state: &str) -> Result<(), StoreError> {
        let statement: Statement = self.statements.save.clone();
        self.execute(&statement, &[&lifecycle, &stream.0, &stored(version.0)?, &state])
    }

    fn reject(&mut self, lifecycle: &'static str, stream: &StreamId, refusal: &str) -> Result<(), StoreError> {
        let statement: Statement = self.statements.reject.clone();
        self.execute(&statement, &[&lifecycle, &stream.0, &refusal])
    }

    fn request(&mut self, lifecycle: &'static str, stream: &StreamId, request: &str) -> Result<(), StoreError> {
        let statement: Statement = self.statements.request.clone();
        self.execute(&statement, &[&lifecycle, &stream.0, &request])
    }

    fn facts(
        &mut self,
        lifecycle: &'static str,
        after: Position,
        limit: usize,
        each: &mut dyn FnMut(StoredFact<'_>) -> Result<(), StoreError>,
    ) -> Result<(), StoreError> {
        let limit: i64 = i64::try_from(limit).unwrap_or(i64::MAX);
        self.transaction
            .query(&self.statements.facts, &[&lifecycle, &stored(after.0)?, &limit])
            .map_err(unavailable)?
            .iter()
            .try_for_each(|row: &Row| {
                each(StoredFact {
                    position: Position(number("facts", row, 0)?),
                    stream: StreamId(text(row, 1)?.to_string()),
                    version: Version(number("facts", row, 2)?),
                    event: text(row, 3)?,
                })
            })
    }

    fn position(&mut self, read_model: &'static str) -> Result<Position, StoreError> {
        match self.transaction.query_opt(&self.statements.position, &[&read_model]).map_err(unavailable)? {
            None => Ok(Position(0)),
            Some(row) => Ok(Position(number("read_model_positions", &row, 0)?)),
        }
    }

    fn advance(&mut self, read_model: &'static str, position: Position) -> Result<(), StoreError> {
        let statement: Statement = self.statements.advance.clone();
        self.execute(&statement, &[&read_model, &stored(position.0)?])
    }

    fn upsert(&mut self, read_model: &'static str, key: &str, row: &str) -> Result<(), StoreError> {
        let statement: Statement = self.statements.upsert.clone();
        self.execute(&statement, &[&read_model, &key, &row])
    }

    fn delete(&mut self, read_model: &'static str, key: &str) -> Result<(), StoreError> {
        let statement: Statement = self.statements.delete.clone();
        self.execute(&statement, &[&read_model, &key])
    }

    fn clear(&mut self, read_model: &'static str) -> Result<(), StoreError> {
        let rows: Statement = self.statements.clear_rows.clone();
        let position: Statement = self.statements.clear_position.clone();
        self.execute(&rows, &[&read_model])?;
        self.execute(&position, &[&read_model])
    }
}

impl Store for Postgres {
    fn transact<T>(&self, _key: Key, work: impl FnOnce(&mut dyn Writer) -> Result<T, StoreError>) -> Result<T, StoreError> {
        let mut writer: MutexGuard<'_, Connection> = self.writer();
        let (value, last): (T, Option<Position>) = {
            let Connection { client, statements } = &mut *writer;
            let mut session: Session<'_> = Session {
                transaction: client.transaction().map_err(unavailable)?,
                statements,
                last: None,
            };
            session.transaction.execute(&statements.lock, &[&self.lock]).map_err(unavailable)?;
            let value: T = work(&mut session)?;
            let last: Option<Position> = session.last;
            session.transaction.commit().map_err(unavailable)?;
            (value, last)
        };
        drop(writer);
        if let (Some(position), Some(listener)) = (last, &self.listener) {
            listener(position);
        }
        Ok(value)
    }

    fn snapshot<T>(&self, _key: Key, work: impl FnOnce(&mut dyn Reader) -> Result<T, StoreError>) -> Result<T, StoreError> {
        let mut reader: MutexGuard<'_, Connection> = self.reader();
        let Connection { client, statements } = &mut *reader;
        let transaction: Transaction<'_> = client
            .build_transaction()
            .isolation_level(IsolationLevel::RepeatableRead)
            .read_only(true)
            .start()
            .map_err(unavailable)?;
        let mut session: Session<'_> = Session {
            transaction,
            statements,
            last: None,
        };
        let outcome: Result<T, StoreError> = work(&mut session);
        drop(session);
        outcome
    }

    fn requests(&self, _key: Key, lifecycle: &'static str, limit: usize) -> Result<Vec<StoredRequest>, StoreError> {
        let mut reader: MutexGuard<'_, Connection> = self.reader();
        let Connection { client, statements } = &mut *reader;
        let limit: i64 = i64::try_from(limit).unwrap_or(i64::MAX);
        client
            .query(&statements.requests, &[&lifecycle, &limit])
            .map_err(unavailable)?
            .iter()
            .map(|row: &Row| {
                Ok(StoredRequest {
                    id: RequestId(number("outbox", row, 0)?),
                    stream: StreamId(text(row, 1)?.to_string()),
                    request: text(row, 2)?.to_string(),
                })
            })
            .collect()
    }

    fn mark_done(&self, _key: Key, id: RequestId) -> Result<(), StoreError> {
        let mut writer: MutexGuard<'_, Connection> = self.writer();
        let Connection { client, statements } = &mut *writer;
        client.execute(&statements.done, &[&stored(id.0)?]).map(|_changed: u64| ()).map_err(unavailable)
    }

    fn mark_failed(&self, _key: Key, id: RequestId) -> Result<(), StoreError> {
        let mut writer: MutexGuard<'_, Connection> = self.writer();
        let Connection { client, statements } = &mut *writer;
        client
            .execute(&statements.failed, &[&stored(id.0)?])
            .map(|_changed: u64| ())
            .map_err(unavailable)
    }
}
