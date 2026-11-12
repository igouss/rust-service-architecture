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

#[test]
fn task_events_keep_their_stored_form() {
    assert_eq!(stored(&TaskEvent::Added { title: title("t") }), r#"{"Added":{"title":"t"}}"#);
    assert_eq!(stored(&TaskEvent::Completed), r#""Completed""#);
}

#[test]
fn task_states_keep_their_stored_form() {
    assert_eq!(stored(&Task::Absent), r#""Absent""#);
    assert_eq!(stored(&Task::Open { title: title("t") }), r#"{"Open":{"title":"t"}}"#);
    assert_eq!(stored(&Task::Done { title: title("t") }), r#"{"Done":{"title":"t"}}"#);
}

#[test]
fn task_refusals_and_tally_keep_their_stored_form() {
    assert_eq!(stored(&TaskRefusal::AlreadyExists), r#""AlreadyExists""#);
    assert_eq!(stored(&TaskRefusal::Absent), r#""Absent""#);
    assert_eq!(stored(&TaskRefusal::AlreadyDone), r#""AlreadyDone""#);
    assert_eq!(stored(&Tally { open: 1, done: 2 }), r#"{"open":1,"done":2}"#);
}

#[test]
fn payment_events_keep_their_stored_form() {
    assert_eq!(stored(&PaymentEvent::Authorized { amount: cents(500) }), r#"{"Authorized":{"amount":500}}"#);
    assert_eq!(stored(&PaymentEvent::Captured { amount: cents(500) }), r#"{"Captured":{"amount":500}}"#);
    assert_eq!(
        stored(&PaymentEvent::Declined {
            reason: DeclineReason::ExceedsAuthorization
        }),
        r#"{"Declined":{"reason":"ExceedsAuthorization"}}"#
    );
}

#[test]
fn payment_states_and_refusals_keep_their_stored_form() {
    assert_eq!(stored(&Payment::Absent), r#""Absent""#);
    assert_eq!(stored(&Payment::Authorized { amount: cents(500) }), r#"{"Authorized":{"amount":500}}"#);
    assert_eq!(stored(&Payment::Captured { amount: cents(500) }), r#"{"Captured":{"amount":500}}"#);
    assert_eq!(
        stored(&Payment::Declined {
            reason: DeclineReason::ExceedsAuthorization
        }),
        r#"{"Declined":{"reason":"ExceedsAuthorization"}}"#
    );
    assert_eq!(stored(&PaymentRefusal::AlreadyAuthorized), r#""AlreadyAuthorized""#);
    assert_eq!(stored(&PaymentRefusal::NotAuthorized), r#""NotAuthorized""#);
}

#[test]
fn fulfilment_keeps_its_stored_form() {
    assert_eq!(stored(&Fulfilment::Idle), r#""Idle""#);
    assert_eq!(stored(&Fulfilment::Reserving), r#""Reserving""#);
    assert_eq!(stored(&Fulfilment::Shipping), r#""Shipping""#);
    assert_eq!(stored(&Fulfilment::Done), r#""Done""#);
    assert_eq!(stored(&Fulfilment::Abandoned), r#""Abandoned""#);
    assert_eq!(stored(&FulfilmentEvent::ReservationRequested), r#""ReservationRequested""#);
    assert_eq!(stored(&FulfilmentEvent::ShipmentRequested), r#""ShipmentRequested""#);
    assert_eq!(stored(&FulfilmentEvent::Completed), r#""Completed""#);
    assert_eq!(stored(&FulfilmentEvent::Abandoned), r#""Abandoned""#);
    assert_eq!(stored(&FulfilmentRequest::ReserveStock), r#""ReserveStock""#);
    assert_eq!(stored(&FulfilmentRequest::Ship), r#""Ship""#);
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
