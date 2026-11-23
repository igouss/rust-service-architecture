use app::payments::PaymentStatus;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::Response;
use domain::fulfilment::Fulfilment;
use domain::payment::{Amount, Authorize, Capture, DeclineReason, Payment, PaymentRefusal};
use ports::{Journal, StreamId};
use serde::Deserialize;
use tracing::Span;

use crate::{answered, blocking, created, path_segment, rejected, request, stream_id, BadInput, Service};

#[derive(Deserialize)]
struct Sum {
    amount: String,
}

fn amount(body: &[u8]) -> Result<Amount, BadInput> {
    let form: Sum = serde_urlencoded::from_bytes(body).map_err(|_unreadable: serde_urlencoded::de::Error| BadInput::UnreadableForm)?;
    form.amount.parse::<u64>().ok().and_then(Amount::new).ok_or(BadInput::BadAmount)
}

fn payment_refusal(refusal: PaymentRefusal) -> &'static str {
    match refusal {
        PaymentRefusal::AlreadyAuthorized => "AlreadyAuthorized",
        PaymentRefusal::NotAuthorized => "NotAuthorized",
    }
}

pub async fn authorize<J: Journal + Send + Sync + 'static>(State(service): State<Service<J>>, headers: HeaderMap, body: Bytes) -> Response {
    let span: Span = request("POST /payments");
    span.record("command", "Authorize");
    match amount(&body).and_then(|amount: Amount| created::<Payment>(&headers).map(|id: StreamId<Payment>| (id, amount))) {
        Err(bad) => rejected(&span, bad),
        Ok((id, amount)) => {
            span.record("payment", id.as_str());
            answered(
                &span,
                blocking(service.journal, move |journal: &J| app::handle(journal, &id, Authorize { amount })).await,
                payment_refusal,
            )
        }
    }
}

pub async fn capture<J: Journal + Send + Sync + 'static>(State(service): State<Service<J>>, Path(id): Path<String>, body: Bytes) -> Response {
    let span: Span = request("POST /payments/{id}/capture");
    span.record("command", "Capture");
    match stream_id::<Payment>(&id).and_then(|id: StreamId<Payment>| amount(&body).map(|amount: Amount| (id, amount))) {
        Err(bad) => rejected(&span, bad),
        Ok((id, amount)) => {
            span.record("payment", id.as_str());
            answered(
                &span,
                blocking(service.journal, move |journal: &J| app::handle(journal, &id, Capture { amount })).await,
                payment_refusal,
            )
        }
    }
}

fn progress(fulfilment: &Fulfilment) -> &'static str {
    match fulfilment {
        Fulfilment::Idle => "not started",
        Fulfilment::Reserving => "reserving stock",
        Fulfilment::Shipping => "shipping",
        Fulfilment::Done => "shipped",
        Fulfilment::Abandoned => "abandoned",
    }
}

fn item(status: &PaymentStatus) -> String {
    let id: String = path_segment(status.id.as_str());
    let fulfilment: &str = progress(&status.fulfilment);
    match &status.payment {
        Payment::Absent => String::new(),
        Payment::Authorized { amount } => format!(
            "<li id=\"payment-{id}\"><span>authorized {cents}</span> <span>{fulfilment}</span> <form data-on:submit=\"@post('/payments/{id}/capture', {{contentType: 'form'}})\"><input name=\"amount\" value=\"{cents}\" required inputmode=\"numeric\"> <button>Capture</button></form></li>\n",
            cents = amount.cents()
        ),
        Payment::Captured { amount } => format!("<li id=\"payment-{id}\"><span>captured {}</span> <span>{fulfilment}</span></li>\n", amount.cents()),
        Payment::Declined {
            reason: DeclineReason::ExceedsAuthorization,
        } => format!("<li id=\"payment-{id}\"><span>declined: exceeds authorization</span> <span>{fulfilment}</span></li>\n"),
    }
}

pub fn render(payments: &[PaymentStatus]) -> String {
    match payments {
        [] => String::new(),
        listed => format!(
            "\n<section id=\"payments\">\n<ul>\n{}</ul>\n</section>",
            listed.iter().map(item).collect::<String>()
        ),
    }
}
