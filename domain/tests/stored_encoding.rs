#![allow(clippy::expect_used, clippy::panic)]

use domain::fulfilment::{Fulfilment, FulfilmentEvent, FulfilmentRequest};
use domain::payment::{Amount, DeclineReason, Payment, PaymentEvent, PaymentRefusal};
use domain::task::{Tally, Task, TaskEvent, TaskRefusal, Title};
use serde::de::DeserializeOwned;
use serde::Serialize;

fn stored<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("encodes")
}

fn loaded<T: DeserializeOwned>(text: &str) -> T {
    serde_json::from_str(text).expect("decodes")
}

fn title(text: &str) -> Title {
    Title::new(text).expect("a title")
}

fn cents(value: u64) -> Amount {
    Amount::new(value).expect("positive")
}

#[track_caller]
fn pinned<T: Serialize>(value: &T, form: fn(&T) -> &'static str) {
    assert_eq!(stored(value), form(value));
}

fn task(state: &Task) -> &'static str {
    match state {
        Task::Absent => r#""Absent""#,
        Task::Open { .. } => r#"{"Open":{"title":"t"}}"#,
        Task::Done { .. } => r#"{"Done":{"title":"t"}}"#,
    }
}

fn task_event(event: &TaskEvent) -> &'static str {
    match event {
        TaskEvent::Added { .. } => r#"{"Added":{"title":"t"}}"#,
        TaskEvent::Completed => r#""Completed""#,
    }
}

fn task_refusal(refusal: &TaskRefusal) -> &'static str {
    match refusal {
        TaskRefusal::AlreadyExists => r#""AlreadyExists""#,
        TaskRefusal::Absent => r#""Absent""#,
        TaskRefusal::AlreadyDone => r#""AlreadyDone""#,
    }
}

fn tally(_row: &Tally) -> &'static str {
    r#"{"open":1,"done":2}"#
}

fn payment(state: &Payment) -> &'static str {
    match state {
        Payment::Absent => r#""Absent""#,
        Payment::Authorized { .. } => r#"{"Authorized":{"amount":500}}"#,
        Payment::Captured { .. } => r#"{"Captured":{"amount":500}}"#,
        Payment::Declined { .. } => r#"{"Declined":{"reason":"ExceedsAuthorization"}}"#,
    }
}

fn payment_event(event: &PaymentEvent) -> &'static str {
    match event {
        PaymentEvent::Authorized { .. } => r#"{"Authorized":{"amount":500}}"#,
        PaymentEvent::Captured { .. } => r#"{"Captured":{"amount":500}}"#,
        PaymentEvent::Declined { .. } => r#"{"Declined":{"reason":"ExceedsAuthorization"}}"#,
    }
}

fn payment_refusal(refusal: &PaymentRefusal) -> &'static str {
    match refusal {
        PaymentRefusal::AlreadyAuthorized => r#""AlreadyAuthorized""#,
        PaymentRefusal::NotAuthorized => r#""NotAuthorized""#,
    }
}

fn decline_reason(reason: &DeclineReason) -> &'static str {
    match reason {
        DeclineReason::ExceedsAuthorization => r#""ExceedsAuthorization""#,
    }
}

fn fulfilment(state: &Fulfilment) -> &'static str {
    match state {
        Fulfilment::Idle => r#""Idle""#,
        Fulfilment::Reserving => r#""Reserving""#,
        Fulfilment::Shipping => r#""Shipping""#,
        Fulfilment::Done => r#""Done""#,
        Fulfilment::Abandoned => r#""Abandoned""#,
    }
}

fn fulfilment_event(event: &FulfilmentEvent) -> &'static str {
    match event {
        FulfilmentEvent::ReservationRequested => r#""ReservationRequested""#,
        FulfilmentEvent::ShipmentRequested => r#""ShipmentRequested""#,
        FulfilmentEvent::Completed => r#""Completed""#,
        FulfilmentEvent::Abandoned => r#""Abandoned""#,
    }
}

fn fulfilment_request(request: &FulfilmentRequest) -> &'static str {
    match request {
        FulfilmentRequest::ReserveStock => r#""ReserveStock""#,
        FulfilmentRequest::Ship => r#""Ship""#,
    }
}

#[test]
fn task_events_keep_their_stored_form() {
    pinned(&TaskEvent::Added { title: title("t") }, task_event);
    pinned(&TaskEvent::Completed, task_event);
}

#[test]
fn task_states_keep_their_stored_form() {
    pinned(&Task::Absent, task);
    pinned(&Task::Open { title: title("t") }, task);
    pinned(&Task::Done { title: title("t") }, task);
}

#[test]
fn task_refusals_and_tally_keep_their_stored_form() {
    pinned(&TaskRefusal::AlreadyExists, task_refusal);
    pinned(&TaskRefusal::Absent, task_refusal);
    pinned(&TaskRefusal::AlreadyDone, task_refusal);
    pinned(&Tally { open: 1, done: 2 }, tally);
}

#[test]
fn payment_events_keep_their_stored_form() {
    pinned(&PaymentEvent::Authorized { amount: cents(500) }, payment_event);
    pinned(&PaymentEvent::Captured { amount: cents(500) }, payment_event);
    pinned(
        &PaymentEvent::Declined {
            reason: DeclineReason::ExceedsAuthorization,
        },
        payment_event,
    );
    pinned(&DeclineReason::ExceedsAuthorization, decline_reason);
}

#[test]
fn payment_states_and_refusals_keep_their_stored_form() {
    pinned(&Payment::Absent, payment);
    pinned(&Payment::Authorized { amount: cents(500) }, payment);
    pinned(&Payment::Captured { amount: cents(500) }, payment);
    pinned(
        &Payment::Declined {
            reason: DeclineReason::ExceedsAuthorization,
        },
        payment,
    );
    pinned(&PaymentRefusal::AlreadyAuthorized, payment_refusal);
    pinned(&PaymentRefusal::NotAuthorized, payment_refusal);
}

#[test]
fn fulfilment_keeps_its_stored_form() {
    pinned(&Fulfilment::Idle, fulfilment);
    pinned(&Fulfilment::Reserving, fulfilment);
    pinned(&Fulfilment::Shipping, fulfilment);
    pinned(&Fulfilment::Done, fulfilment);
    pinned(&Fulfilment::Abandoned, fulfilment);
    pinned(&FulfilmentEvent::ReservationRequested, fulfilment_event);
    pinned(&FulfilmentEvent::ShipmentRequested, fulfilment_event);
    pinned(&FulfilmentEvent::Completed, fulfilment_event);
    pinned(&FulfilmentEvent::Abandoned, fulfilment_event);
    pinned(&FulfilmentRequest::ReserveStock, fulfilment_request);
    pinned(&FulfilmentRequest::Ship, fulfilment_request);
}

#[test]
fn a_stored_fact_loads_back_as_the_same_event() {
    let event: TaskEvent = loaded(r#"{"Added":{"title":"t"}}"#);
    assert_eq!(event, TaskEvent::Added { title: title("t") });
}

#[test]
fn a_stored_title_at_the_limit_loads() {
    let stored_title: Title = loaded(&format!("\"{}\"", "é".repeat(200)));
    assert_eq!(stored_title, title(&"é".repeat(200)));
}

#[test]
fn a_stored_blank_title_is_refused_on_load() {
    let event: Result<TaskEvent, serde_json::Error> = serde_json::from_str(r#"{"Added":{"title":" "}}"#);
    assert!(event.is_err());
}

#[test]
fn a_stored_over_long_title_is_refused_on_load() {
    let state: Result<Task, serde_json::Error> = serde_json::from_str(&format!(r#"{{"Open":{{"title":"{}"}}}}"#, "é".repeat(201)));
    assert!(state.is_err());
}

#[test]
fn a_stored_zero_amount_is_refused_on_load() {
    let amount: Result<Amount, serde_json::Error> = serde_json::from_str("0");
    assert!(amount.is_err());
}
