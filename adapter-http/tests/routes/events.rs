use crate::support::*;

#[tokio::test]
async fn the_first_frame_is_a_datastar_element_patch() {
    assert!(first_frame(&service())
        .await
        .starts_with("event: datastar-patch-elements\ndata: elements <main id=\"tasks\">\n"));
}

#[tokio::test]
async fn the_first_frame_ends_the_event_with_a_blank_line() {
    assert!(first_frame(&service()).await.ends_with("data: elements </main>\n\n"));
}

#[tokio::test]
async fn the_first_frame_of_an_empty_list_shows_a_zero_tally() {
    assert_eq!(
        first_frame(&service()).await,
        "event: datastar-patch-elements\ndata: elements <main id=\"tasks\">\ndata: elements <p id=\"tally\">0 open, 0 done</p>\ndata: elements <ul id=\"open\">\ndata: elements </ul>\ndata: elements </main>\n\n"
    );
}

#[tokio::test]
async fn the_first_frame_lists_one_open_task() {
    let service: Service = service();
    add(&service, "report").await;
    let id: String = first_id(&service);
    assert!(first_frame(&service).await.contains(&format!(
        "data: elements <li id=\"task-{id}\"><span>report</span> <button data-on:click=\"@post('/tasks/{id}/complete')\">Done</button></li>\n"
    )));
}

#[tokio::test]
async fn the_first_frame_lists_every_open_task() {
    let service: Service = service();
    add(&service, "report").await;
    add(&service, "review").await;
    assert_eq!(first_frame(&service).await.matches("<li id=\"task-").count(), 2);
}

#[tokio::test]
async fn the_first_frame_tallies_open_and_done() {
    let service: Service = service();
    add(&service, "report").await;
    add(&service, "review").await;
    complete(&service, &first_id(&service)).await;
    assert!(first_frame(&service).await.contains("data: elements <p id=\"tally\">1 open, 1 done</p>\n"));
}

#[tokio::test]
async fn a_commit_produces_a_second_frame() {
    let service: Service = service();
    let mut stream: Body = send(&service.router, get("/events")).await.into_body();
    frame(&mut stream).await;
    add(&service, "report").await;
    assert!(frame(&mut stream).await.contains("<span>report</span>"));
}

#[tokio::test]
async fn a_stopped_service_ends_its_event_streams() {
    let changes: Changes = Changes::default();
    let router: Router = router(Arc::new(Sqlite::in_memory().expect("opens")), changes.clone());
    let mut stream: Body = send(&router, get("/events")).await.into_body();
    frame(&mut stream).await;
    changes.stop();
    let ended: bool = tokio::time::timeout(Duration::from_secs(5), stream.frame())
        .await
        .expect("an end within five seconds")
        .is_none();
    assert!(ended);
}

#[tokio::test]
async fn html_in_a_title_is_escaped() {
    let service: Service = service();
    add(&service, "%3Cscript%3Ealert(%221%27%26%22)%3C/script%3E").await;
    assert!(first_frame(&service)
        .await
        .contains("<span>&lt;script&gt;alert(&quot;1&#39;&amp;&quot;)&lt;/script&gt;</span>"));
}

#[tokio::test]
async fn a_line_break_in_a_title_stays_inside_its_element() {
    let service: Service = service();
    add(&service, "a%0D%0Adata:+mode+remove").await;
    assert!(first_frame(&service).await.contains("<span>a&#13;&#10;data: mode remove</span>"));
}

#[tokio::test]
async fn a_frame_is_one_read_off_the_executor_thread() {
    let (witness, router): (Arc<Witness>, Router) = witness();
    frame(&mut send(&router, get("/events")).await.into_body()).await;
    assert_eq!(calls_and_calls_on_this_thread(&witness), (1, 0));
}

#[tokio::test]
async fn a_store_error_on_opening_the_event_stream_answers_500() {
    let router: Router = router(Arc::new(Broken), Changes::default());
    assert_eq!(send(&router, get("/events")).await.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn a_store_error_after_the_first_frame_ends_the_event_stream() {
    let changes: Changes = Changes::default();
    let journal: Arc<Exhausted> = Arc::new(Exhausted {
        store: Sqlite::in_memory().expect("opens"),
        reads_left: AtomicUsize::new(1),
    });
    let mut stream: Body = send(&router(journal, changes.clone()), get("/events")).await.into_body();
    frame(&mut stream).await;
    changes.on_commit()(Position(1));
    let ended: bool = tokio::time::timeout(Duration::from_secs(5), stream.frame())
        .await
        .expect("an end within five seconds")
        .is_none();
    assert!(ended);
}
