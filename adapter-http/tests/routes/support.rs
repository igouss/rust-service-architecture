pub use std::cell::RefCell;
pub use std::io;
pub use std::sync::atomic::{AtomicUsize, Ordering};
pub use std::sync::{Arc, Mutex, Once};
pub use std::thread::{self, ThreadId};
pub use std::time::Duration;

pub use adapter_http::{router, Changes, BODY_LIMIT};
pub use adapter_sqlite::{Sqlite, SqliteUnit, SqliteView};
pub use axum::body::Body;
pub use axum::http::{Request, StatusCode};
pub use axum::response::Response;
pub use axum::Router;
pub use domain::task::{Tally, Title};
pub use http_body_util::BodyExt;
pub use ports::{Journal, Position, StoreError, StreamId};
pub use tower::ServiceExt;
pub use tracing_subscriber::fmt::format::FmtSpan;

pub struct Service {
    pub store: Arc<Sqlite>,
    pub router: Router,
}

pub fn service() -> Service {
    let changes: Changes = Changes::default();
    let store: Arc<Sqlite> = Arc::new(Sqlite::in_memory().expect("opens").on_commit(changes.on_commit()));
    let router: Router = router(Arc::clone(&store), changes);
    Service { store, router }
}

pub fn get(path: &str) -> Request<Body> {
    Request::builder().uri(path).body(Body::empty()).expect("request")
}

pub fn post(path: &str, body: String) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .expect("request")
}

thread_local! {
    static LOG: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

pub static LOGGING: Once = Once::new();

pub struct ThreadLog;

impl io::Write for ThreadLog {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        LOG.with_borrow_mut(|log: &mut Vec<u8>| log.extend_from_slice(bytes));
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub fn log_to_the_calling_thread() {
    tracing_subscriber::fmt()
        .with_writer(|| ThreadLog)
        .without_time()
        .with_span_events(FmtSpan::CLOSE)
        .init();
}

pub fn logged() -> String {
    LOG.with_borrow(|log: &Vec<u8>| String::from_utf8(log.clone()).expect("utf-8"))
}

pub async fn send(router: &Router, request: Request<Body>) -> Response {
    LOGGING.call_once(log_to_the_calling_thread);
    router.clone().oneshot(request).await.expect("answered")
}

pub async fn add(service: &Service, title: &str) -> StatusCode {
    send(&service.router, post("/tasks", format!("title={title}"))).await.status()
}

pub fn open(service: &Service) -> Vec<String> {
    app::tasks::open_tasks(service.store.as_ref())
        .expect("queried")
        .into_iter()
        .map(|(_id, title): (StreamId, Title)| title.as_str().to_string())
        .collect()
}

pub fn first_id(service: &Service) -> String {
    app::tasks::open_tasks(service.store.as_ref())
        .expect("queried")
        .into_iter()
        .map(|(id, _title): (StreamId, Title)| id.0)
        .next()
        .expect("one open task")
}

pub async fn complete(service: &Service, id: &str) -> Response {
    send(&service.router, post(&format!("/tasks/{id}/complete"), String::new())).await
}

pub async fn text(response: Response) -> String {
    let bytes: Vec<u8> = response.into_body().collect().await.expect("body").to_bytes().to_vec();
    String::from_utf8(bytes).expect("utf-8")
}

pub async fn frame(body: &mut Body) -> String {
    let data: Vec<u8> = tokio::time::timeout(Duration::from_secs(5), body.frame())
        .await
        .expect("a frame within five seconds")
        .expect("an open stream")
        .expect("a readable frame")
        .into_data()
        .expect("a data frame")
        .to_vec();
    String::from_utf8(data).expect("utf-8")
}

pub async fn first_frame(service: &Service) -> String {
    frame(&mut send(&service.router, get("/events")).await.into_body()).await
}

pub struct Witness {
    pub store: Sqlite,
    pub threads: Mutex<Vec<ThreadId>>,
}

impl Witness {
    pub fn note(&self) {
        self.threads.lock().expect("unpoisoned").push(thread::current().id());
    }
}

impl Journal for Witness {
    type Unit<'a> = SqliteUnit<'a>;
    type View<'a> = SqliteView<'a>;

    fn write<T>(&self, work: impl FnOnce(&mut SqliteUnit<'_>) -> Result<T, StoreError>) -> Result<T, StoreError> {
        self.note();
        self.store.write(work)
    }

    fn read<T>(&self, work: impl FnOnce(&mut SqliteView<'_>) -> Result<T, StoreError>) -> Result<T, StoreError> {
        self.note();
        self.store.read(work)
    }
}

pub fn witness() -> (Arc<Witness>, Router) {
    let witness: Arc<Witness> = Arc::new(Witness {
        store: Sqlite::in_memory().expect("opens"),
        threads: Mutex::new(Vec::new()),
    });
    let router: Router = router(Arc::clone(&witness), Changes::default());
    (witness, router)
}

pub fn calls_and_calls_on_this_thread(witness: &Witness) -> (usize, usize) {
    let here: ThreadId = thread::current().id();
    let threads: Vec<ThreadId> = witness.threads.lock().expect("unpoisoned").clone();
    (threads.len(), threads.iter().filter(|thread: &&ThreadId| **thread == here).count())
}

pub struct Broken;

impl Journal for Broken {
    type Unit<'a> = SqliteUnit<'a>;
    type View<'a> = SqliteView<'a>;

    fn write<T>(&self, _work: impl FnOnce(&mut SqliteUnit<'_>) -> Result<T, StoreError>) -> Result<T, StoreError> {
        Err(StoreError::Unavailable { cause: "injected".to_string() })
    }

    fn read<T>(&self, _work: impl FnOnce(&mut SqliteView<'_>) -> Result<T, StoreError>) -> Result<T, StoreError> {
        Err(StoreError::Unavailable { cause: "injected".to_string() })
    }
}

pub struct Exhausted {
    pub store: Sqlite,
    pub reads_left: AtomicUsize,
}

impl Journal for Exhausted {
    type Unit<'a> = SqliteUnit<'a>;
    type View<'a> = SqliteView<'a>;

    fn write<T>(&self, work: impl FnOnce(&mut SqliteUnit<'_>) -> Result<T, StoreError>) -> Result<T, StoreError> {
        self.store.write(work)
    }

    fn read<T>(&self, work: impl FnOnce(&mut SqliteView<'_>) -> Result<T, StoreError>) -> Result<T, StoreError> {
        self.reads_left
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left: usize| left.checked_sub(1))
            .map_err(|_none_left: usize| StoreError::Unavailable { cause: "injected".to_string() })?;
        self.store.read(work)
    }
}

pub struct Panicking;

impl Journal for Panicking {
    type Unit<'a> = SqliteUnit<'a>;
    type View<'a> = SqliteView<'a>;

    fn write<T>(&self, _work: impl FnOnce(&mut SqliteUnit<'_>) -> Result<T, StoreError>) -> Result<T, StoreError> {
        panic!("injected")
    }

    fn read<T>(&self, _work: impl FnOnce(&mut SqliteView<'_>) -> Result<T, StoreError>) -> Result<T, StoreError> {
        panic!("injected")
    }
}
