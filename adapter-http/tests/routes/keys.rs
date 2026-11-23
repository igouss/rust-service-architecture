use crate::support::*;

const KEY: &str = "0198c0de-0000-7000-8000-000000000001";
const OTHER: &str = "0198c0de-0000-7000-8000-000000000002";

#[tokio::test]
async fn a_task_posted_under_a_key_answers_204_and_is_stored_under_that_key() {
    let service: Service = service();
    let response: Response = send(&service.router, keyed("/tasks", KEY, "title=report")).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(first_id(&service), KEY);
}

#[tokio::test]
async fn a_task_posted_again_under_its_key_adds_no_second_task() {
    let service: Service = service();
    send(&service.router, keyed("/tasks", KEY, "title=report")).await;
    let again: Response = send(&service.router, keyed("/tasks", KEY, "title=report")).await;
    assert_eq!((again.status(), text(again).await), (StatusCode::CONFLICT, "AlreadyExists".to_string()));
    assert_eq!(open(&service), vec!["report".to_string()]);
}

#[tokio::test]
async fn tasks_posted_under_two_keys_are_two_tasks() {
    let service: Service = service();
    send(&service.router, keyed("/tasks", KEY, "title=report")).await;
    send(&service.router, keyed("/tasks", OTHER, "title=report")).await;
    assert_eq!(open(&service), vec!["report".to_string(), "report".to_string()]);
}

#[tokio::test]
async fn a_task_posted_under_a_key_that_is_not_a_uuid_answers_400_and_adds_nothing() {
    let service: Service = service();
    let response: Response = send(&service.router, keyed("/tasks", "again", "title=report")).await;
    assert_eq!((response.status(), text(response).await), (StatusCode::BAD_REQUEST, "MalformedKey".to_string()));
    assert_eq!(open(&service), Vec::<String>::new());
}

#[tokio::test]
async fn a_payment_authorized_under_a_key_is_stored_under_that_key() {
    let service: Service = service();
    let response: Response = send(&service.router, keyed("/payments", KEY, "amount=500")).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(first_payment(&service), KEY);
}

#[tokio::test]
async fn a_payment_authorized_again_under_its_key_authorizes_no_second_payment() {
    let service: Service = service();
    send(&service.router, keyed("/payments", KEY, "amount=500")).await;
    let again: Response = send(&service.router, keyed("/payments", KEY, "amount=500")).await;
    assert_eq!((again.status(), text(again).await), (StatusCode::CONFLICT, "AlreadyAuthorized".to_string()));
    assert_eq!(payments(&service).len(), 1);
}

#[tokio::test]
async fn a_payment_authorized_under_a_key_that_is_not_a_uuid_answers_400() {
    let response: Response = send(&service().router, keyed("/payments", "again", "amount=500")).await;
    assert_eq!((response.status(), text(response).await), (StatusCode::BAD_REQUEST, "MalformedKey".to_string()));
}

#[tokio::test]
async fn one_key_names_a_task_and_a_payment_apart() {
    let service: Service = service();
    send(&service.router, keyed("/tasks", KEY, "title=report")).await;
    let response: Response = send(&service.router, keyed("/payments", KEY, "amount=500")).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}
