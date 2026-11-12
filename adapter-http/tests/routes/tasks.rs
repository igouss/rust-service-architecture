use crate::support::*;

#[tokio::test]
async fn an_added_task_answers_204() {
    assert_eq!(add(&service(), "write+the+report").await, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn an_added_task_is_open_with_its_title() {
    let service: Service = service();
    add(&service, "write+the+report").await;
    assert_eq!(open(&service), vec!["write the report".to_string()]);
}

#[tokio::test]
async fn two_posts_of_one_title_add_two_tasks() {
    let service: Service = service();
    add(&service, "again").await;
    assert_eq!(add(&service, "again").await, StatusCode::NO_CONTENT);
    assert_eq!(open(&service), vec!["again".to_string(), "again".to_string()]);
}

#[tokio::test]
async fn open_tasks_are_listed_in_the_order_they_were_added() {
    let service: Service = service();
    add(&service, "first").await;
    add(&service, "second").await;
    add(&service, "third").await;
    assert_eq!(open(&service), vec!["first".to_string(), "second".to_string(), "third".to_string()]);
}

#[tokio::test]
async fn an_empty_title_answers_400() {
    assert_eq!(add(&service(), "").await, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_blank_title_answers_400() {
    assert_eq!(add(&service(), "+%09+").await, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_form_without_a_title_answers_400() {
    let response: Response = send(&service().router, post("/tasks", "name=x".to_string())).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_title_at_the_limit_answers_204() {
    assert_eq!(add(&service(), &"é".repeat(Title::LIMIT)).await, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn a_title_over_the_limit_answers_400() {
    assert_eq!(add(&service(), &"é".repeat(Title::LIMIT + 1)).await, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_body_over_the_limit_answers_413() {
    assert_eq!(add(&service(), &"a".repeat(BODY_LIMIT)).await, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn a_refused_title_names_what_was_wrong() {
    let response: Response = send(&service().router, post("/tasks", "title=".to_string())).await;
    assert_eq!(text(response).await, "BlankTitle");
}

#[tokio::test]
async fn a_refused_title_adds_no_task() {
    let service: Service = service();
    add(&service, "").await;
    assert_eq!(open(&service), Vec::<String>::new());
}

#[tokio::test]
async fn completing_an_open_task_answers_204() {
    let service: Service = service();
    add(&service, "report").await;
    assert_eq!(complete(&service, &first_id(&service)).await.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn a_completed_task_moves_from_open_to_done() {
    let service: Service = service();
    add(&service, "report").await;
    complete(&service, &first_id(&service)).await;
    assert_eq!(app::tasks::task_tally(service.store.as_ref()), Ok(Some(Tally { open: 0, done: 1 })));
}

#[tokio::test]
async fn completing_ignores_the_signals_datastar_posts() {
    let service: Service = service();
    add(&service, "report").await;
    let request: Request<Body> = Request::builder()
        .method("POST")
        .uri(format!("/tasks/{}/complete", first_id(&service)))
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .expect("request");
    assert_eq!(send(&service.router, request).await.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn completing_a_done_task_answers_409() {
    let service: Service = service();
    add(&service, "report").await;
    let id: String = first_id(&service);
    complete(&service, &id).await;
    assert_eq!(complete(&service, &id).await.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn completing_a_done_task_names_the_refusal() {
    let service: Service = service();
    add(&service, "report").await;
    let id: String = first_id(&service);
    complete(&service, &id).await;
    assert_eq!(text(complete(&service, &id).await).await, "AlreadyDone");
}

#[tokio::test]
async fn completing_an_unknown_task_names_the_refusal() {
    let response: Response = complete(&service(), "00000000-0000-4000-8000-000000000000").await;
    assert_eq!((response.status(), text(response).await), (StatusCode::CONFLICT, "Absent".to_string()));
}

#[tokio::test]
async fn a_malformed_id_answers_400() {
    let response: Response = complete(&service(), "t-1").await;
    assert_eq!((response.status(), text(response).await), (StatusCode::BAD_REQUEST, "MalformedId".to_string()));
}

#[tokio::test]
async fn a_unit_of_work_runs_off_the_executor_thread() {
    let (witness, router): (Arc<Witness>, Router) = witness();
    send(&router, post("/tasks", "title=report".to_string())).await;
    assert_eq!(calls_and_calls_on_this_thread(&witness), (1, 0));
}

#[tokio::test]
async fn a_store_error_answers_500() {
    let router: Router = router(Arc::new(Broken), Changes::default());
    let response: Response = send(&router, post("/tasks", "title=report".to_string())).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn a_unit_of_work_that_panics_answers_500() {
    let response: Response = send(&router(Arc::new(Panicking), Changes::default()), post("/tasks", "title=report".to_string())).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
