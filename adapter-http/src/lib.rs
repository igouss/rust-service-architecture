use std::convert::Infallible;
use std::sync::Arc;

use app::tasks::TaskBoard;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use domain::task::{AddTask, CompleteTask, Tally, TaskRefusal, Title, TitleError};
use futures_util::future;
use futures_util::stream::{self, StreamExt};
use ports::{Answer, Journal, Position, StoreError, StreamId};
use serde::Deserialize;
use tokio::sync::watch;
use tokio::task::JoinError;
use tracing::field::Empty;
use tracing::Span;
use uuid::Uuid;

pub const BODY_LIMIT: usize = 4096;

const PATCH_ELEMENTS: &str = "datastar-patch-elements";

const PAGE: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Tasks</title>
<script type="module" src="https://cdn.jsdelivr.net/gh/starfederation/datastar@v1.0.5/bundles/datastar.js"></script>
</head>
<body data-init="@get('/events')">
<h1>Tasks</h1>
<form data-on:submit="@post('/tasks', {contentType: 'form'})">
<input name="title" required maxlength="200" placeholder="What needs doing?">
<button>Add</button>
</form>
<main id="tasks"></main>
</body>
</html>
"#;

#[derive(Clone)]
pub struct Changes {
    commits: watch::Sender<u64>,
    stopped: watch::Sender<bool>,
}

impl Default for Changes {
    fn default() -> Changes {
        Changes {
            commits: watch::Sender::new(0),
            stopped: watch::Sender::new(false),
        }
    }
}

impl Changes {
    pub fn on_commit(&self) -> impl Fn(Position) + Send + Sync + 'static {
        let commits: watch::Sender<u64> = self.commits.clone();
        move |position: Position| commits.send_modify(|seen: &mut u64| *seen = position.0)
    }

    pub fn stop(&self) {
        self.stopped.send_modify(|stopped: &mut bool| *stopped = true);
    }
}

struct Service<J> {
    journal: Arc<J>,
    changes: Changes,
}

impl<J> Clone for Service<J> {
    fn clone(&self) -> Service<J> {
        Service {
            journal: Arc::clone(&self.journal),
            changes: self.changes.clone(),
        }
    }
}

pub fn router<J: Journal + Send + Sync + 'static>(journal: Arc<J>, changes: Changes) -> Router {
    Router::new()
        .route("/", get(page))
        .route("/tasks", post(add_task::<J>))
        .route("/tasks/{id}/complete", post(complete_task::<J>))
        .route("/events", get(events::<J>))
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
        .with_state(Service { journal, changes })
}

fn request(route: &'static str) -> Span {
    tracing::info_span!(
        "request",
        route,
        command = Empty,
        task = Empty,
        outcome = Empty,
        version = Empty,
        refusal = Empty,
        reason = Empty
    )
}

enum Failure {
    Store(StoreError),
    Worker,
}

fn reported(span: &Span, failure: &Failure, message: &'static str) {
    match failure {
        Failure::Store(error @ (StoreError::Unavailable { .. } | StoreError::Corrupt { .. } | StoreError::OutOfOrder { .. })) => {
            tracing::error!(parent: span, error = ?error, "{message}");
        }
        Failure::Worker => tracing::error!(parent: span, error = "the blocking worker did not finish", "{message}"),
    }
}

async fn blocking<J, T>(journal: Arc<J>, work: impl FnOnce(&J) -> Result<T, StoreError> + Send + 'static) -> Result<T, Failure>
where
    J: Send + Sync + 'static,
    T: Send + 'static,
{
    let joined: Result<Result<T, StoreError>, JoinError> = tokio::task::spawn_blocking(move || work(&journal)).await;
    match joined {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(Failure::Store(error)),
        Err(_panicked) => Err(Failure::Worker),
    }
}

enum BadInput {
    UnreadableForm,
    BlankTitle,
    TitleTooLong,
    MalformedId,
}

fn rejected(span: &Span, bad: BadInput) -> Response {
    let name: &'static str = match bad {
        BadInput::UnreadableForm => "UnreadableForm",
        BadInput::BlankTitle => "BlankTitle",
        BadInput::TitleTooLong => "TitleTooLong",
        BadInput::MalformedId => "MalformedId",
    };
    span.record("outcome", "rejected");
    span.record("reason", name);
    (StatusCode::BAD_REQUEST, name).into_response()
}

fn refusal_name(refusal: TaskRefusal) -> &'static str {
    match refusal {
        TaskRefusal::AlreadyExists => "AlreadyExists",
        TaskRefusal::Absent => "Absent",
        TaskRefusal::AlreadyDone => "AlreadyDone",
    }
}

fn failed(span: &Span, failure: &Failure) -> Response {
    span.record("outcome", "error");
    reported(span, failure, "the store failed");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}

fn answered(span: &Span, outcome: Result<Answer<TaskRefusal>, Failure>) -> Response {
    match outcome {
        Ok(Answer::Accepted(version)) => {
            span.record("outcome", "accepted");
            span.record("version", version.0);
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(Answer::Refused(refusal)) => {
            span.record("outcome", "refused");
            span.record("refusal", refusal_name(refusal));
            (StatusCode::CONFLICT, refusal_name(refusal)).into_response()
        }
        Err(failure) => failed(span, &failure),
    }
}

#[derive(Deserialize)]
struct NewTask {
    title: String,
}

fn new_task(body: &[u8]) -> Result<AddTask, BadInput> {
    let form: NewTask = serde_urlencoded::from_bytes(body).map_err(|_unreadable: serde_urlencoded::de::Error| BadInput::UnreadableForm)?;
    Title::new(&form.title).map(|title: Title| AddTask { title }).map_err(bad_title)
}

fn bad_title(error: TitleError) -> BadInput {
    match error {
        TitleError::Blank => BadInput::BlankTitle,
        TitleError::TooLong => BadInput::TitleTooLong,
    }
}

fn task_id(text: &str) -> Result<StreamId, BadInput> {
    Uuid::parse_str(text)
        .map(|id: Uuid| StreamId(id.to_string()))
        .map_err(|_malformed: uuid::Error| BadInput::MalformedId)
}

async fn page() -> Html<&'static str> {
    request("GET /").record("outcome", "served");
    Html(PAGE)
}

async fn add_task<J: Journal + Send + Sync + 'static>(State(service): State<Service<J>>, body: Bytes) -> Response {
    let span: Span = request("POST /tasks");
    span.record("command", "AddTask");
    match new_task(&body) {
        Err(bad) => rejected(&span, bad),
        Ok(command) => {
            let id: StreamId = StreamId(Uuid::now_v7().to_string());
            span.record("task", id.0.as_str());
            answered(&span, blocking(service.journal, move |journal: &J| app::handle(journal, &id, command)).await)
        }
    }
}

async fn complete_task<J: Journal + Send + Sync + 'static>(State(service): State<Service<J>>, Path(id): Path<String>) -> Response {
    let span: Span = request("POST /tasks/{id}/complete");
    span.record("command", "CompleteTask");
    match task_id(&id) {
        Err(bad) => rejected(&span, bad),
        Ok(id) => {
            span.record("task", id.0.as_str());
            answered(
                &span,
                blocking(service.journal, move |journal: &J| app::handle(journal, &id, CompleteTask)).await,
            )
        }
    }
}

fn escape(text: &str) -> String {
    let mut escaped: String = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            '\n' => escaped.push_str("&#10;"),
            '\r' => escaped.push_str("&#13;"),
            other => escaped.push(other),
        }
    }
    escaped
}

fn path_segment(text: &str) -> String {
    text.bytes()
        .map(|byte: u8| match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' => char::from(byte).to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

fn render(board: &TaskBoard) -> String {
    let tally: Tally = board.tally.unwrap_or(Tally { open: 0, done: 0 });
    let items: String = board
        .open
        .iter()
        .map(|(id, title): &(StreamId, Title)| {
            format!(
                "<li id=\"task-{}\"><span>{}</span> <button data-on:click=\"@post('/tasks/{}/complete')\">Done</button></li>\n",
                path_segment(&id.0),
                escape(title.as_str()),
                path_segment(&id.0)
            )
        })
        .collect();
    format!(
        "<main id=\"tasks\">\n<p id=\"tally\">{} open, {} done</p>\n<ul id=\"open\">\n{items}</ul>\n</main>",
        tally.open, tally.done
    )
}

fn patch(html: &str) -> Event {
    let lines: Vec<String> = html.lines().map(|line: &str| format!("elements {line}")).collect();
    Event::default().event(PATCH_ELEMENTS).data(lines.join("\n"))
}

struct Feed<J> {
    journal: Arc<J>,
    commits: watch::Receiver<u64>,
    stopped: watch::Receiver<bool>,
    span: Span,
}

async fn next_patch<J: Journal + Send + Sync + 'static>(mut feed: Feed<J>) -> Option<(Result<Event, Infallible>, Feed<J>)> {
    tokio::select! {
        changed = feed.commits.changed() => changed.ok()?,
        _stopped = feed.stopped.wait_for(|stopped: &bool| *stopped) => return None,
    }
    match blocking(Arc::clone(&feed.journal), app::tasks::task_board::<J>).await {
        Ok(shown) => Some((Ok(patch(&render(&shown))), feed)),
        Err(failure) => {
            reported(&feed.span, &failure, "a store failure ended the event stream");
            None
        }
    }
}

async fn events<J: Journal + Send + Sync + 'static>(State(service): State<Service<J>>) -> Response {
    let feed: Feed<J> = Feed {
        journal: service.journal,
        commits: service.changes.commits.subscribe(),
        stopped: service.changes.stopped.subscribe(),
        span: request("GET /events"),
    };
    match blocking(Arc::clone(&feed.journal), app::tasks::task_board::<J>).await {
        Err(failure) => failed(&feed.span, &failure),
        Ok(shown) => {
            feed.span.record("outcome", "streaming");
            let first: Result<Event, Infallible> = Ok(patch(&render(&shown)));
            Sse::new(stream::once(future::ready(first)).chain(stream::unfold(feed, next_patch::<J>)))
                .keep_alive(KeepAlive::default())
                .into_response()
        }
    }
}
