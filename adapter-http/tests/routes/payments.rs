use crate::support::*;

const UNKNOWN: &str = "00000000-0000-4000-8000-000000000000";

#[tokio::test]
async fn an_authorization_answers_204() {
    assert_eq!(authorize(&service(), "500").await.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn an_authorization_is_stored_with_its_amount() {
    let service: Service = service();
    authorize(&service, "500").await;
    assert_eq!(
        payments(&service)
            .into_iter()
            .map(|(_id, payment): (StreamId<Payment>, Payment)| payment)
            .collect::<Vec<Payment>>(),
        vec![Payment::Authorized { amount: cents(500) }]
    );
}

#[tokio::test]
async fn two_authorizations_are_two_payments() {
    let service: Service = service();
    authorize(&service, "500").await;
    authorize(&service, "700").await;
    assert_eq!(payments(&service).len(), 2);
}

#[tokio::test]
async fn an_authorization_without_an_amount_answers_400() {
    let response: Response = send(&service().router, post("/payments", "sum=500".to_string())).await;
    assert_eq!(
        (response.status(), text(response).await),
        (StatusCode::BAD_REQUEST, "UnreadableForm".to_string())
    );
}

#[tokio::test]
async fn an_authorization_of_nothing_answers_400() {
    let response: Response = authorize(&service(), "0").await;
    assert_eq!((response.status(), text(response).await), (StatusCode::BAD_REQUEST, "BadAmount".to_string()));
}

#[tokio::test]
async fn an_authorization_of_an_amount_that_is_not_a_number_answers_400() {
    let response: Response = authorize(&service(), "five").await;
    assert_eq!((response.status(), text(response).await), (StatusCode::BAD_REQUEST, "BadAmount".to_string()));
}

#[tokio::test]
async fn a_refused_amount_authorizes_nothing() {
    let service: Service = service();
    authorize(&service, "0").await;
    assert_eq!(payments(&service), Vec::new());
}

#[tokio::test]
async fn a_store_error_on_an_authorization_answers_500() {
    let router: Router = router(Arc::new(Broken), Changes::default());
    assert_eq!(
        send(&router, post("/payments", "amount=500".to_string())).await.status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn capturing_an_authorized_payment_answers_204() {
    let service: Service = service();
    authorize(&service, "500").await;
    assert_eq!(capture(&service, &first_payment(&service), "500").await.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn a_captured_payment_starts_its_fulfilment() {
    let service: Service = service();
    authorize(&service, "500").await;
    let id: String = first_payment(&service);
    capture(&service, &id, "300").await;
    assert_eq!(payments(&service), vec![(StreamId::new(id.clone()), Payment::Captured { amount: cents(300) })]);
    assert_eq!(
        service
            .store
            .read(|view: &mut View<'_>| view.state::<Fulfilment>(&StreamId::new(id)))
            .map(|(_version, fulfilment)| fulfilment),
        Ok(Fulfilment::Reserving)
    );
}

#[tokio::test]
async fn capturing_more_than_was_authorized_answers_204_and_declines_the_payment() {
    let service: Service = service();
    authorize(&service, "500").await;
    let id: String = first_payment(&service);
    assert_eq!(capture(&service, &id, "501").await.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        payments(&service),
        vec![(
            StreamId::new(id),
            Payment::Declined {
                reason: DeclineReason::ExceedsAuthorization
            }
        )]
    );
}

#[tokio::test]
async fn capturing_an_unknown_payment_answers_409_and_names_the_refusal() {
    let response: Response = capture(&service(), UNKNOWN, "500").await;
    assert_eq!((response.status(), text(response).await), (StatusCode::CONFLICT, "NotAuthorized".to_string()));
}

#[tokio::test]
async fn capturing_a_captured_payment_answers_409() {
    let service: Service = service();
    authorize(&service, "500").await;
    let id: String = first_payment(&service);
    capture(&service, &id, "500").await;
    assert_eq!(capture(&service, &id, "500").await.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn capturing_with_a_malformed_id_answers_400() {
    let response: Response = capture(&service(), "p-1", "500").await;
    assert_eq!((response.status(), text(response).await), (StatusCode::BAD_REQUEST, "MalformedId".to_string()));
}

#[tokio::test]
async fn capturing_nothing_answers_400() {
    let response: Response = capture(&service(), UNKNOWN, "0").await;
    assert_eq!((response.status(), text(response).await), (StatusCode::BAD_REQUEST, "BadAmount".to_string()));
}

#[tokio::test]
async fn capturing_without_an_amount_answers_400() {
    let response: Response = send(&service().router, post(&format!("/payments/{UNKNOWN}/capture"), String::new())).await;
    assert_eq!(
        (response.status(), text(response).await),
        (StatusCode::BAD_REQUEST, "UnreadableForm".to_string())
    );
}

#[tokio::test]
async fn a_store_error_on_a_capture_answers_500() {
    let router: Router = router(Arc::new(Broken), Changes::default());
    assert_eq!(
        send(&router, post(&format!("/payments/{UNKNOWN}/capture"), "amount=500".to_string()))
            .await
            .status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn a_payment_command_runs_off_the_executor_thread() {
    let (witness, router): (Arc<Witness>, Router) = witness();
    send(&router, post("/payments", "amount=500".to_string())).await;
    assert_eq!(calls_and_calls_on_this_thread(&witness), (1, 0));
}

#[tokio::test]
async fn the_first_frame_lists_an_authorized_payment_with_a_form_to_capture_it() {
    let service: Service = service();
    authorize(&service, "500").await;
    let id: String = first_payment(&service);
    assert!(first_frame(&service).await.contains(&format!(
        "data: elements <li id=\"payment-{id}\"><span>authorized 500</span> <span>not started</span> <form data-on:submit=\"@post('/payments/{id}/capture', {{contentType: 'form'}})\"><input name=\"amount\" value=\"500\" required inputmode=\"numeric\"> <button>Capture</button></form></li>\n"
    )));
}

#[tokio::test]
async fn the_first_frame_shows_a_captured_payment_and_its_fulfilment() {
    let service: Service = service();
    authorize(&service, "500").await;
    let id: String = first_payment(&service);
    capture(&service, &id, "300").await;
    assert!(first_frame(&service).await.contains(&format!(
        "data: elements <li id=\"payment-{id}\"><span>captured 300</span> <span>reserving stock</span></li>\n"
    )));
}

#[tokio::test]
async fn the_first_frame_shows_a_declined_payment() {
    let service: Service = service();
    authorize(&service, "500").await;
    let id: String = first_payment(&service);
    capture(&service, &id, "501").await;
    assert!(first_frame(&service).await.contains(&format!(
        "data: elements <li id=\"payment-{id}\"><span>declined: exceeds authorization</span> <span>not started</span></li>\n"
    )));
}

#[tokio::test]
async fn the_first_frame_lists_every_payment() {
    let service: Service = service();
    authorize(&service, "500").await;
    authorize(&service, "700").await;
    assert_eq!(first_frame(&service).await.matches("<li id=\"payment-").count(), 2);
}

#[tokio::test]
async fn the_first_frame_with_payments_still_ends_the_event_with_a_blank_line() {
    let service: Service = service();
    authorize(&service, "500").await;
    assert!(first_frame(&service).await.ends_with("data: elements </section>\n\n"));
}

#[tokio::test]
async fn a_frame_with_payments_is_one_read_off_the_executor_thread() {
    let (witness, router): (Arc<Witness>, Router) = witness();
    send(&router, post("/payments", "amount=500".to_string())).await;
    frame(&mut send(&router, get("/events")).await.into_body()).await;
    assert_eq!(calls_and_calls_on_this_thread(&witness), (2, 0));
}

#[tokio::test]
async fn the_page_posts_the_amount_as_a_form() {
    let page: String = text(send(&service().router, get("/")).await).await;
    assert!(page.contains(r#"<form data-on:submit="@post('/payments', {contentType: 'form'})">"#));
}

#[tokio::test]
async fn the_page_has_a_place_for_the_payments() {
    let page: String = text(send(&service().router, get("/")).await).await;
    assert!(page.contains(r#"<section id="payments"></section>"#));
}

#[tokio::test]
async fn a_payment_command_is_logged_with_its_route_name_and_payment() {
    let service: Service = service();
    authorize(&service, "500").await;
    assert!(logged().contains(&format!(
        "request{{route=\"POST /payments\" command=\"Authorize\" payment=\"{}\"",
        first_payment(&service)
    )));
}

#[tokio::test]
async fn a_refused_capture_is_logged_with_the_refusal() {
    capture(&service(), UNKNOWN, "500").await;
    assert!(logged().contains(&format!(
        "request{{route=\"POST /payments/{{id}}/capture\" command=\"Capture\" payment=\"{UNKNOWN}\" outcome=\"refused\" refusal=\"NotAuthorized\"}}"
    )));
}
