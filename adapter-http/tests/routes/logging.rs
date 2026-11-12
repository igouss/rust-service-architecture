use crate::support::*;

#[tokio::test]
async fn a_command_is_logged_with_its_route_name_and_task() {
    let service: Service = service();
    add(&service, "report").await;
    assert!(logged().contains(&format!("request{{route=\"POST /tasks\" command=\"AddTask\" task=\"{}\"", first_id(&service))));
}

#[tokio::test]
async fn an_accepted_command_is_logged_with_its_version() {
    add(&service(), "report").await;
    assert!(logged().contains("outcome=\"accepted\" version=1}"));
}

#[tokio::test]
async fn a_refused_command_is_logged_with_the_refusal() {
    complete(&service(), "00000000-0000-4000-8000-000000000000").await;
    assert!(logged().contains(
        "request{route=\"POST /tasks/{id}/complete\" command=\"CompleteTask\" task=\"00000000-0000-4000-8000-000000000000\" outcome=\"refused\" refusal=\"Absent\"}"
    ));
}

#[tokio::test]
async fn bad_input_is_logged_with_its_reason() {
    add(&service(), "").await;
    assert!(logged().contains("request{route=\"POST /tasks\" command=\"AddTask\" outcome=\"rejected\" reason=\"BlankTitle\"}"));
}

#[tokio::test]
async fn a_store_error_is_logged_with_its_cause() {
    send(&router(Arc::new(Broken), Changes::default()), post("/tasks", "title=report".to_string())).await;
    assert!(logged().contains("the store failed error=Unavailable { cause: \"injected\" }"));
}

#[tokio::test]
async fn a_store_error_is_logged_as_the_outcome_of_its_request() {
    send(&router(Arc::new(Broken), Changes::default()), get("/events")).await;
    assert!(logged().contains("request{route=\"GET /events\" outcome=\"error\"}"));
}

#[tokio::test]
async fn a_store_error_that_ends_an_event_stream_is_logged() {
    let changes: Changes = Changes::default();
    let journal: Arc<Exhausted> = Arc::new(Exhausted {
        store: Sqlite::in_memory().expect("opens"),
        reads_left: AtomicUsize::new(1),
    });
    let mut stream: Body = send(&router(journal, changes.clone()), get("/events")).await.into_body();
    frame(&mut stream).await;
    changes.on_commit()(Position(1));
    tokio::time::timeout(Duration::from_secs(5), stream.frame())
        .await
        .expect("an end within five seconds");
    assert!(logged().contains(
        "request{route=\"GET /events\" outcome=\"streaming\"}: adapter_http: a store failure ended the event stream error=Unavailable { cause: \"injected\" }"
    ));
}

#[tokio::test]
async fn a_unit_of_work_that_panics_is_logged() {
    send(&router(Arc::new(Panicking), Changes::default()), post("/tasks", "title=report".to_string())).await;
    assert!(logged().contains("the store failed error=\"the blocking worker did not finish\""));
}

#[tokio::test]
async fn an_accepted_title_is_not_logged() {
    add(&service(), "sentinel").await;
    assert_eq!((logged().contains("accepted"), logged().contains("sentinel")), (true, false));
}

#[tokio::test]
async fn a_refused_title_is_not_logged() {
    add(&service(), &"sentinel".repeat(Title::LIMIT)).await;
    assert_eq!((logged().contains("TitleTooLong"), logged().contains("sentinel")), (true, false));
}

#[tokio::test]
async fn an_unreadable_form_is_not_logged() {
    send(&service().router, post("/tasks", "sentinel=sentinel".to_string())).await;
    assert_eq!((logged().contains("UnreadableForm"), logged().contains("sentinel")), (true, false));
}
