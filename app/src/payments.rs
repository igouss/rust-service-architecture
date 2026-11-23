use domain::fulfilment::{Fulfilment, FulfilmentEvent, Progress};
use domain::payment::{Payment, PaymentEvent};
use ports::{Journal, Recorded, StoreError, StreamId, Unit, View};

use crate::{react, Policy};

pub fn fulfilment_of(payment: &StreamId<Payment>) -> StreamId<Fulfilment> {
    StreamId::new(payment.as_str())
}

impl Policy for Payment {
    fn after(unit: &mut Unit<'_>, fact: &Recorded<Payment>) -> Result<(), StoreError> {
        match fact.event() {
            PaymentEvent::Captured { .. } => react(unit, &fulfilment_of(fact.stream()), Progress::PaymentCaptured),
            PaymentEvent::Authorized { .. } | PaymentEvent::Declined { .. } => Ok(()),
        }
    }
}

impl Policy for Fulfilment {
    fn after(_unit: &mut Unit<'_>, fact: &Recorded<Fulfilment>) -> Result<(), StoreError> {
        match fact.event() {
            FulfilmentEvent::ReservationRequested | FulfilmentEvent::ShipmentRequested | FulfilmentEvent::Completed | FulfilmentEvent::Abandoned => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentStatus {
    pub id: StreamId<Payment>,
    pub payment: Payment,
    pub fulfilment: Fulfilment,
}

pub(crate) fn payments_in(view: &mut View<'_>) -> Result<Vec<PaymentStatus>, StoreError> {
    let payments: Vec<(StreamId<Payment>, Payment)> = view.states::<Payment>()?;
    payments
        .into_iter()
        .map(|(id, payment): (StreamId<Payment>, Payment)| {
            let (_version, fulfilment): (ports::Version, Fulfilment) = view.state::<Fulfilment>(&fulfilment_of(&id))?;
            Ok(PaymentStatus { id, payment, fulfilment })
        })
        .collect()
}

pub fn payments<J: Journal>(journal: &J) -> Result<Vec<PaymentStatus>, StoreError> {
    journal.read(|view: &mut View<'_>| payments_in(view))
}
