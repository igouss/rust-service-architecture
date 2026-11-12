use domain::fulfilment::{Fulfilment, FulfilmentEvent, Progress};
use domain::payment::{Payment, PaymentEvent};
use ports::{Recorded, StoreError, StreamId, Unit};

use crate::{react, Policy};

pub fn fulfilment_of(payment: &StreamId) -> StreamId {
    StreamId(payment.0.clone())
}

impl Policy for Payment {
    fn after<U: Unit>(unit: &mut U, fact: &Recorded<PaymentEvent>) -> Result<(), StoreError> {
        match fact.event {
            PaymentEvent::Captured { .. } => react(unit, &fulfilment_of(&fact.stream), Progress::PaymentCaptured),
            PaymentEvent::Authorized { .. } | PaymentEvent::Declined { .. } => Ok(()),
        }
    }
}

impl Policy for Fulfilment {
    fn after<U: Unit>(_unit: &mut U, fact: &Recorded<FulfilmentEvent>) -> Result<(), StoreError> {
        match fact.event {
            FulfilmentEvent::ReservationRequested | FulfilmentEvent::ShipmentRequested | FulfilmentEvent::Completed | FulfilmentEvent::Abandoned => Ok(()),
        }
    }
}
