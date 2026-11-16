use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvError, SendError, Sender, SyncSender};
use std::sync::Arc;
use std::thread;

use ports::store::{Key, Listener, Reader, Store, StoredFact, StoredRequest, Writer};
use ports::{Position, RequestId, StoreError, StreamId, Version};
use postgres::error::DbError;
use postgres::{Client, NoTls, Row};
use r2d2::{ManageConnection, Pool, PooledConnection};

mod statements;

use statements::{Statements, SCHEMA};

type Job = Box<dyn FnOnce(&mut Client, &Statements) + Send>;
type Pipe<T> = (SyncSender<T>, Receiver<T>);

type Checked = PooledConnection<Connections>;

struct Worker {
    jobs: Sender<Job>,
    broken: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Durability {
    Full,
    Normal,
}

impl Durability {
    fn setting(self) -> &'static str {
        match self {
            Durability::Full => "on",
            Durability::Normal => "off",
        }
    }
}

struct Connections {
    url: String,
    schema: String,
    durability: Durability,
}

#[derive(Debug)]
struct Refused(String);

pub struct Postgres {
    pool: Pool<Connections>,
    lock: i64,
    listener: Option<Listener>,
}

impl fmt::Display for Refused {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for Refused {}

fn refused(error: StoreError) -> Refused {
    Refused(format!("{error:?}"))
}

impl ManageConnection for Connections {
    type Connection = Worker;
    type Error = Refused;

    fn connect(&self) -> Result<Worker, Refused> {
        Worker::start(&self.url, &self.schema, self.durability).map_err(refused)
    }

    fn is_valid(&self, worker: &mut Worker) -> Result<(), Refused> {
        worker.run("SELECT 1").map_err(refused)
    }

    fn has_broken(&self, worker: &mut Worker) -> bool {
        worker.broken.load(Ordering::SeqCst)
    }
}

fn unavailable(error: postgres::Error) -> StoreError {
    StoreError::Unavailable { cause: error.to_string() }
}

fn fatal(error: &postgres::Error) -> bool {
    error.is_closed()
        || error
            .as_db_error()
            .is_some_and(|server: &DbError| matches!(server.severity(), "FATAL" | "PANIC"))
}

fn stopped() -> StoreError {
    StoreError::Unavailable {
        cause: "the store's connection thread has stopped".to_string(),
    }
}

fn number<T>(table: &'static str, row: &Row, column: usize, read: fn(i64) -> Option<T>) -> Result<T, StoreError> {
    let corrupt = |cause: String| StoreError::Corrupt { table, cause };
    let value: i64 = row.try_get(column).map_err(|error: postgres::Error| corrupt(error.to_string()))?;
    read(value).ok_or_else(|| corrupt(format!("negative integer {value}")))
}

fn text<'a>(table: &'static str, row: &'a Row, column: usize) -> Result<&'a str, StoreError> {
    row.try_get(column).map_err(|error: postgres::Error| StoreError::Corrupt {
        table,
        cause: error.to_string(),
    })
}

fn named(schema: &str) -> Result<&str, StoreError> {
    let plain: bool = schema
        .chars()
        .all(|letter: char| letter.is_ascii_lowercase() || letter.is_ascii_digit() || letter == '_');
    match plain && schema.starts_with(|letter: char| letter.is_ascii_lowercase()) {
        true => Ok(schema),
        false => Err(StoreError::Misconfigured {
            cause: format!("schema name {schema:?} must be lower-case letters, digits and underscores, starting with a letter"),
        }),
    }
}

fn prepared(url: &str, schema: &str) -> Result<i64, postgres::Error> {
    let mut client: Client = Client::connect(url, NoTls)?;
    client.batch_execute(&format!(
        "SELECT pg_advisory_lock(hashtext('{schema}')); CREATE SCHEMA IF NOT EXISTS {schema}; SET search_path TO {schema}; {SCHEMA}"
    ))?;
    client.query_one("SELECT to_regnamespace($1)::oid::bigint", &[&schema])?.try_get(0)
}

fn connection(url: &str, schema: &str, durability: Durability) -> Result<(Client, Statements), postgres::Error> {
    let mut client: Client = Client::connect(url, NoTls)?;
    let commit: &str = durability.setting();
    client.batch_execute(&format!(
        "SET search_path TO {schema}; SET application_name TO 'store_{schema}'; SET synchronous_commit TO {commit}"
    ))?;
    let statements: Statements = Statements::prepare(&mut client)?;
    Ok((client, statements))
}

fn serve(url: &str, schema: &str, durability: Durability, ready: &SyncSender<Result<(), StoreError>>, inbox: Receiver<Job>) {
    match connection(url, schema, durability) {
        Err(error) => {
            let _unheard: Result<(), SendError<Result<(), StoreError>>> = ready.send(Err(unavailable(error)));
        }
        Ok((mut client, statements)) => {
            let _unheard: Result<(), SendError<Result<(), StoreError>>> = ready.send(Ok(()));
            let _closed: Option<()> = inbox.into_iter().try_for_each(|job: Job| {
                job(&mut client, &statements);
                (!client.is_closed()).then_some(())
            });
        }
    }
}

impl Worker {
    fn start(url: &str, schema: &str, durability: Durability) -> Result<Worker, StoreError> {
        let (jobs, inbox): (Sender<Job>, Receiver<Job>) = mpsc::channel();
        let (ready, started): Pipe<Result<(), StoreError>> = mpsc::sync_channel(1);
        let (url, schema): (String, String) = (url.to_string(), schema.to_string());
        let broken: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
        let ended: Arc<AtomicBool> = Arc::clone(&broken);
        thread::Builder::new()
            .name("postgres-store".to_string())
            .spawn(move || {
                serve(&url, &schema, durability, &ready, inbox);
                ended.store(true, Ordering::SeqCst);
            })
            .map_err(|error: std::io::Error| StoreError::Unavailable { cause: error.to_string() })?;
        started.recv().map_err(|_: RecvError| stopped())??;
        Ok(Worker { jobs, broken })
    }

    fn call<R: Send + 'static>(&self, job: impl FnOnce(&mut Client, &Statements) -> Result<R, postgres::Error> + Send + 'static) -> Result<R, StoreError> {
        let (reply, answer): Pipe<Result<R, postgres::Error>> = mpsc::sync_channel(1);
        let broken: Arc<AtomicBool> = Arc::clone(&self.broken);
        self.jobs
            .send(Box::new(move |client: &mut Client, statements: &Statements| {
                let outcome: Result<R, postgres::Error> = job(client, statements);
                broken.fetch_or(client.is_closed() || outcome.as_ref().is_err_and(fatal), Ordering::SeqCst);
                let _unheard: Result<(), SendError<Result<R, postgres::Error>>> = reply.send(outcome);
            }))
            .map_err(|_: SendError<Job>| stopped())?;
        answer.recv().map_err(|_: RecvError| stopped())?.map_err(unavailable)
    }

    fn run(&self, sql: &'static str) -> Result<(), StoreError> {
        self.call(move |client: &mut Client, _statements: &Statements| client.batch_execute(sql))
    }
}

impl Postgres {
    pub fn connect(url: &str, schema: &str, connections: u32, durability: Durability) -> Result<Postgres, StoreError> {
        let schema: &str = named(schema)?;
        let (address, name): (String, String) = (url.to_string(), schema.to_string());
        let lock: i64 = thread::spawn(move || prepared(&address, &name))
            .join()
            .map_err(|_panic: Box<dyn std::any::Any + Send>| stopped())?
            .map_err(unavailable)?;
        let pool: Pool<Connections> = Pool::builder()
            .max_size(connections.max(1))
            .test_on_check_out(false)
            .build(Connections {
                url: url.to_string(),
                schema: schema.to_string(),
                durability,
            })
            .map_err(|error: r2d2::Error| StoreError::Unavailable { cause: error.to_string() })?;
        Ok(Postgres { pool, lock, listener: None })
    }

    pub fn on_commit(self, listener: impl Fn(Position) + Send + Sync + 'static) -> Postgres {
        Postgres {
            listener: Some(Box::new(listener)),
            ..self
        }
    }

    pub fn durability(&self) -> Result<Durability, StoreError> {
        let row: Row = self
            .checked()?
            .call(|client: &mut Client, _statements: &Statements| client.query_one("SELECT current_setting('synchronous_commit')", &[]))?;
        match text("pg_settings", &row, 0)? {
            "off" => Ok(Durability::Normal),
            _synchronous => Ok(Durability::Full),
        }
    }

    fn checked(&self) -> Result<Checked, StoreError> {
        self.pool
            .get()
            .map_err(|error: r2d2::Error| StoreError::Unavailable { cause: error.to_string() })
    }

    fn begun(&self, sql: &'static str) -> Result<Checked, StoreError> {
        let mut refusal: StoreError = stopped();
        for _attempt in 0..=self.pool.max_size() {
            let connection: Checked = self.checked()?;
            match connection.run(sql) {
                Ok(()) => return Ok(connection),
                Err(error) if connection.broken.load(Ordering::SeqCst) => refusal = error,
                Err(error) => return Err(error),
            }
        }
        Err(refusal)
    }
}

struct Session<'a> {
    worker: &'a Worker,
    open: bool,
}

impl<'a> Session<'a> {
    fn over(worker: &'a Worker) -> Session<'a> {
        Session { worker, open: true }
    }

    fn commit(mut self) -> Result<(), StoreError> {
        self.open = false;
        self.worker.run("COMMIT")
    }
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
        if self.open {
            let _unanswered: Result<(), StoreError> = self.worker.run("ROLLBACK");
        }
    }
}

impl Reader for Session<'_> {
    fn state(&mut self, lifecycle: &'static str, stream: &StreamId) -> Result<Option<(Version, String)>, StoreError> {
        let stream: String = stream.0.clone();
        match self
            .worker
            .call(move |client: &mut Client, statements: &Statements| client.query_opt(&statements.state, &[&lifecycle, &stream]))?
        {
            None => Ok(None),
            Some(row) => Ok(Some((number("states", &row, 0, Version::loaded)?, text("states", &row, 1)?.to_string()))),
        }
    }

    fn states(&mut self, lifecycle: &'static str, each: &mut dyn FnMut(StreamId, &str) -> Result<(), StoreError>) -> Result<(), StoreError> {
        self.worker
            .call(move |client: &mut Client, statements: &Statements| client.query(&statements.states, &[&lifecycle]))?
            .iter()
            .try_for_each(|row: &Row| each(StreamId(text("states", row, 0)?.to_string()), text("states", row, 1)?))
    }

    fn row(&mut self, read_model: &'static str, key: &str) -> Result<Option<String>, StoreError> {
        let key: String = key.to_string();
        match self
            .worker
            .call(move |client: &mut Client, statements: &Statements| client.query_opt(&statements.row, &[&read_model, &key]))?
        {
            None => Ok(None),
            Some(row) => Ok(Some(text("read_models", &row, 0)?.to_string())),
        }
    }

    fn rows(&mut self, read_model: &'static str, each: &mut dyn FnMut(String, &str) -> Result<(), StoreError>) -> Result<(), StoreError> {
        self.worker
            .call(move |client: &mut Client, statements: &Statements| client.query(&statements.rows, &[&read_model]))?
            .iter()
            .try_for_each(|row: &Row| each(text("read_models", row, 0)?.to_string(), text("read_models", row, 1)?))
    }

    fn refusals(&mut self, lifecycle: &'static str, stream: &StreamId) -> Result<Vec<String>, StoreError> {
        let stream: String = stream.0.clone();
        self.worker
            .call(move |client: &mut Client, statements: &Statements| client.query(&statements.rejections, &[&lifecycle, &stream]))?
            .iter()
            .map(|row: &Row| text("rejections", row, 0).map(str::to_string))
            .collect()
    }
}

impl Writer for Session<'_> {
    fn append(&mut self, lifecycle: &'static str, stream: &StreamId, version: Version, event: &str) -> Result<Position, StoreError> {
        let (stream, version, event): (String, i64, String) = (stream.0.clone(), version.stored(), event.to_string());
        let row: Row = self
            .worker
            .call(move |client: &mut Client, statements: &Statements| client.query_one(&statements.append, &[&lifecycle, &stream, &version, &event]))?;
        number("facts", &row, 0, Position::loaded)
    }

    fn save(&mut self, lifecycle: &'static str, stream: &StreamId, version: Version, state: &str) -> Result<(), StoreError> {
        let (stream, version, state): (String, i64, String) = (stream.0.clone(), version.stored(), state.to_string());
        self.worker
            .call(move |client: &mut Client, statements: &Statements| client.execute(&statements.save, &[&lifecycle, &stream, &version, &state]))
            .map(|_changed: u64| ())
    }

    fn reject(&mut self, lifecycle: &'static str, stream: &StreamId, refusal: &str) -> Result<(), StoreError> {
        let (stream, refusal): (String, String) = (stream.0.clone(), refusal.to_string());
        self.worker
            .call(move |client: &mut Client, statements: &Statements| client.execute(&statements.reject, &[&lifecycle, &stream, &refusal]))
            .map(|_changed: u64| ())
    }

    fn request(&mut self, lifecycle: &'static str, stream: &StreamId, request: &str) -> Result<(), StoreError> {
        let (stream, request): (String, String) = (stream.0.clone(), request.to_string());
        self.worker
            .call(move |client: &mut Client, statements: &Statements| client.execute(&statements.request, &[&lifecycle, &stream, &request]))
            .map(|_changed: u64| ())
    }

    fn facts(
        &mut self,
        lifecycle: &'static str,
        after: Position,
        limit: usize,
        each: &mut dyn FnMut(StoredFact<'_>) -> Result<(), StoreError>,
    ) -> Result<(), StoreError> {
        let (after, limit): (i64, i64) = (after.stored(), i64::try_from(limit).unwrap_or(i64::MAX));
        self.worker
            .call(move |client: &mut Client, statements: &Statements| client.query(&statements.facts, &[&lifecycle, &after, &limit]))?
            .iter()
            .try_for_each(|row: &Row| {
                each(StoredFact {
                    position: number("facts", row, 0, Position::loaded)?,
                    stream: StreamId(text("facts", row, 1)?.to_string()),
                    version: number("facts", row, 2, Version::loaded)?,
                    event: text("facts", row, 3)?,
                })
            })
    }

    fn position(&mut self, read_model: &'static str) -> Result<Position, StoreError> {
        match self
            .worker
            .call(move |client: &mut Client, statements: &Statements| client.query_opt(&statements.position, &[&read_model]))?
        {
            None => Ok(Position::ZERO),
            Some(row) => Ok(number("read_model_positions", &row, 0, Position::loaded)?),
        }
    }

    fn advance(&mut self, read_model: &'static str, position: Position) -> Result<(), StoreError> {
        let position: i64 = position.stored();
        self.worker
            .call(move |client: &mut Client, statements: &Statements| client.execute(&statements.advance, &[&read_model, &position]))
            .map(|_changed: u64| ())
    }

    fn upsert(&mut self, read_model: &'static str, key: &str, row: &str) -> Result<(), StoreError> {
        let (key, row): (String, String) = (key.to_string(), row.to_string());
        self.worker
            .call(move |client: &mut Client, statements: &Statements| client.execute(&statements.upsert, &[&read_model, &key, &row]))
            .map(|_changed: u64| ())
    }

    fn delete(&mut self, read_model: &'static str, key: &str) -> Result<(), StoreError> {
        let key: String = key.to_string();
        self.worker
            .call(move |client: &mut Client, statements: &Statements| client.execute(&statements.delete, &[&read_model, &key]))
            .map(|_changed: u64| ())
    }

    fn clear(&mut self, read_model: &'static str) -> Result<(), StoreError> {
        self.worker
            .call(move |client: &mut Client, statements: &Statements| {
                client.execute(&statements.clear_rows, &[&read_model])?;
                client.execute(&statements.clear_position, &[&read_model])
            })
            .map(|_changed: u64| ())
    }
}

impl Store for Postgres {
    fn transact<T>(&self, _key: Key, work: impl FnOnce(&mut dyn Writer) -> Result<T, StoreError>) -> Result<T, StoreError> {
        let connection: Checked = self.begun("BEGIN")?;
        let lock: i64 = self.lock;
        let mut session: Session<'_> = Session::over(&connection);
        connection
            .call(move |client: &mut Client, statements: &Statements| client.execute(&statements.lock, &[&lock]))
            .map(|_locked: u64| ())?;
        let value: T = work(&mut session)?;
        session.commit()?;
        Ok(value)
    }

    fn snapshot<T>(&self, _key: Key, work: impl FnOnce(&mut dyn Reader) -> Result<T, StoreError>) -> Result<T, StoreError> {
        let connection: Checked = self.begun("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")?;
        let mut session: Session<'_> = Session::over(&connection);
        work(&mut session)
    }

    fn committed(&self, _key: Key, last: Position) {
        if let Some(listener) = &self.listener {
            listener(last);
        }
    }

    fn requests(&self, _key: Key, lifecycle: &'static str, limit: usize) -> Result<Vec<StoredRequest>, StoreError> {
        let limit: i64 = i64::try_from(limit).unwrap_or(i64::MAX);
        self.checked()?
            .call(move |client: &mut Client, statements: &Statements| client.query(&statements.requests, &[&lifecycle, &limit]))?
            .iter()
            .map(|row: &Row| {
                Ok(StoredRequest {
                    id: number("outbox", row, 0, RequestId::loaded)?,
                    stream: StreamId(text("outbox", row, 1)?.to_string()),
                    request: text("outbox", row, 2)?.to_string(),
                })
            })
            .collect()
    }

    fn mark_done(&self, _key: Key, id: RequestId) -> Result<(), StoreError> {
        let id: i64 = id.stored();
        self.checked()?
            .call(move |client: &mut Client, statements: &Statements| client.execute(&statements.done, &[&id]))
            .map(|_changed: u64| ())
    }

    fn mark_failed(&self, _key: Key, id: RequestId) -> Result<(), StoreError> {
        let id: i64 = id.stored();
        self.checked()?
            .call(move |client: &mut Client, statements: &Statements| client.execute(&statements.failed, &[&id]))
            .map(|_changed: u64| ())
    }
}
