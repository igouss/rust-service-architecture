use serde::{Deserialize, Serialize};

use crate::{Lifecycle, Never, Reaction, Step, Undelivered};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fulfilment {
    Idle,
    Reserving,
    Shipping,
    Done,
    Abandoned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FulfilmentEvent {
    ReservationRequested,
    ShipmentRequested,
    Completed,
    Abandoned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FulfilmentRequest {
    ReserveStock,
    Ship,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    PaymentCaptured,
    StockReserved,
    StockUnavailable,
    Shipped,
    ReservationUndelivered,
    ShipmentUndelivered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FulfilmentReply {
    StockReserved,
    StockUnavailable,
    Shipped,
}

impl From<FulfilmentReply> for Progress {
    /// When a gateway replies to a request, `from` shall return the progress that reply names.
    fn from(reply: FulfilmentReply) -> Progress {
        match reply {
            FulfilmentReply::StockReserved => Progress::StockReserved,
            FulfilmentReply::StockUnavailable => Progress::StockUnavailable,
            FulfilmentReply::Shipped => Progress::Shipped,
        }
    }
}

impl From<Undelivered<FulfilmentRequest>> for Progress {
    /// When a request will not be performed, `from` shall return the progress that names that request as undelivered.
    fn from(undelivered: Undelivered<FulfilmentRequest>) -> Progress {
        match undelivered.request {
            FulfilmentRequest::ReserveStock => Progress::ReservationUndelivered,
            FulfilmentRequest::Ship => Progress::ShipmentUndelivered,
        }
    }
}

impl Lifecycle for Fulfilment {
    const NAME: &'static str = "fulfilment";
    type Event = FulfilmentEvent;
    type Refusal = Never;

    fn initial() -> Fulfilment {
        Fulfilment::Idle
    }

    /// When a requested step is folded in the state that requested it, `evolve` shall return the next state.
    /// While the fulfilment is `Reserving` or `Shipping`, when `Abandoned` is folded, `evolve` shall return `Abandoned`.
    /// If an event does not apply to the state, then `evolve` shall return the state unchanged.
    fn evolve(self, event: &FulfilmentEvent) -> Fulfilment {
        match (self, event) {
            (Fulfilment::Idle, FulfilmentEvent::ReservationRequested) => Fulfilment::Reserving,
            (Fulfilment::Reserving, FulfilmentEvent::ShipmentRequested) => Fulfilment::Shipping,
            (Fulfilment::Reserving, FulfilmentEvent::Abandoned) => Fulfilment::Abandoned,
            (Fulfilment::Shipping, FulfilmentEvent::Abandoned) => Fulfilment::Abandoned,
            (Fulfilment::Shipping, FulfilmentEvent::Completed) => Fulfilment::Done,
            (s @ Fulfilment::Idle, FulfilmentEvent::ShipmentRequested | FulfilmentEvent::Completed | FulfilmentEvent::Abandoned)
            | (s @ Fulfilment::Reserving, FulfilmentEvent::ReservationRequested | FulfilmentEvent::Completed)
            | (s @ Fulfilment::Shipping, FulfilmentEvent::ReservationRequested | FulfilmentEvent::ShipmentRequested)
            | (
                s @ (Fulfilment::Done | Fulfilment::Abandoned),
                FulfilmentEvent::ReservationRequested | FulfilmentEvent::ShipmentRequested | FulfilmentEvent::Completed | FulfilmentEvent::Abandoned,
            ) => s,
        }
    }
}

impl Reaction for Progress {
    type Reads = Fulfilment;
    type Request = FulfilmentRequest;

    /// While the fulfilment is `Idle`, when the payment is captured, `react` shall emit `ReservationRequested` and request `ReserveStock`.
    /// While the fulfilment is `Reserving`, when stock is reserved, `react` shall emit `ShipmentRequested` and request `Ship`.
    /// While the fulfilment is `Reserving`, when stock is unavailable, `react` shall emit `Abandoned` and request nothing.
    /// While the fulfilment is `Shipping`, when the order is shipped, `react` shall emit `Completed`.
    /// While the fulfilment is `Reserving`, when the reservation is undelivered, `react` shall emit `Abandoned` and request nothing.
    /// While the fulfilment is `Shipping`, when the shipment is undelivered, `react` shall emit `Abandoned` and request nothing.
    /// If progress does not apply to the state, then `react` shall emit and request nothing.
    fn react(self, reads: &Fulfilment) -> Step<FulfilmentEvent, FulfilmentRequest> {
        match (reads, self) {
            (Fulfilment::Idle, Progress::PaymentCaptured) => Step {
                events: vec![FulfilmentEvent::ReservationRequested],
                requests: vec![FulfilmentRequest::ReserveStock],
            },
            (Fulfilment::Reserving, Progress::StockReserved) => Step {
                events: vec![FulfilmentEvent::ShipmentRequested],
                requests: vec![FulfilmentRequest::Ship],
            },
            (Fulfilment::Reserving, Progress::StockUnavailable | Progress::ReservationUndelivered) | (Fulfilment::Shipping, Progress::ShipmentUndelivered) => {
                Step {
                    events: vec![FulfilmentEvent::Abandoned],
                    requests: Vec::new(),
                }
            }
            (Fulfilment::Shipping, Progress::Shipped) => Step {
                events: vec![FulfilmentEvent::Completed],
                requests: Vec::new(),
            },
            (
                Fulfilment::Idle,
                Progress::StockReserved | Progress::StockUnavailable | Progress::Shipped | Progress::ReservationUndelivered | Progress::ShipmentUndelivered,
            )
            | (Fulfilment::Reserving, Progress::PaymentCaptured | Progress::Shipped | Progress::ShipmentUndelivered)
            | (Fulfilment::Shipping, Progress::PaymentCaptured | Progress::StockReserved | Progress::StockUnavailable | Progress::ReservationUndelivered)
            | (
                Fulfilment::Done | Fulfilment::Abandoned,
                Progress::PaymentCaptured
                | Progress::StockReserved
                | Progress::StockUnavailable
                | Progress::Shipped
                | Progress::ReservationUndelivered
                | Progress::ShipmentUndelivered,
            ) => Step::nothing(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_captured_payment_starts_the_reservation() {
        assert_eq!(
            Progress::PaymentCaptured.react(&Fulfilment::Idle),
            Step {
                events: vec![FulfilmentEvent::ReservationRequested],
                requests: vec![FulfilmentRequest::ReserveStock]
            }
        );
    }

    #[test]
    fn reserved_stock_starts_the_shipment() {
        assert_eq!(
            Progress::StockReserved.react(&Fulfilment::Reserving),
            Step {
                events: vec![FulfilmentEvent::ShipmentRequested],
                requests: vec![FulfilmentRequest::Ship]
            }
        );
    }

    #[test]
    fn unavailable_stock_abandons_and_requests_nothing() {
        assert_eq!(
            Progress::StockUnavailable.react(&Fulfilment::Reserving),
            Step {
                events: vec![FulfilmentEvent::Abandoned],
                requests: Vec::new()
            }
        );
    }

    #[test]
    fn a_shipment_completes_the_fulfilment() {
        assert_eq!(
            Progress::Shipped.react(&Fulfilment::Shipping),
            Step {
                events: vec![FulfilmentEvent::Completed],
                requests: Vec::new()
            }
        );
    }

    #[test]
    fn progress_that_does_not_apply_does_nothing() {
        assert_eq!(Progress::PaymentCaptured.react(&Fulfilment::Reserving), Step::nothing());
    }

    #[test]
    fn a_reply_that_stock_is_reserved_is_that_progress() {
        assert_eq!(Progress::from(FulfilmentReply::StockReserved), Progress::StockReserved);
    }

    #[test]
    fn a_reply_that_stock_is_unavailable_is_that_progress() {
        assert_eq!(Progress::from(FulfilmentReply::StockUnavailable), Progress::StockUnavailable);
    }

    #[test]
    fn a_reply_that_the_order_shipped_is_that_progress() {
        assert_eq!(Progress::from(FulfilmentReply::Shipped), Progress::Shipped);
    }

    #[test]
    fn an_undelivered_reservation_is_that_progress() {
        assert_eq!(
            Progress::from(Undelivered {
                request: FulfilmentRequest::ReserveStock
            }),
            Progress::ReservationUndelivered
        );
    }

    #[test]
    fn an_undelivered_shipment_is_that_progress() {
        assert_eq!(
            Progress::from(Undelivered {
                request: FulfilmentRequest::Ship
            }),
            Progress::ShipmentUndelivered
        );
    }

    #[test]
    fn an_undelivered_reservation_abandons_and_requests_nothing() {
        assert_eq!(
            Progress::ReservationUndelivered.react(&Fulfilment::Reserving),
            Step {
                events: vec![FulfilmentEvent::Abandoned],
                requests: Vec::new()
            }
        );
    }

    #[test]
    fn an_undelivered_shipment_abandons_and_requests_nothing() {
        assert_eq!(
            Progress::ShipmentUndelivered.react(&Fulfilment::Shipping),
            Step {
                events: vec![FulfilmentEvent::Abandoned],
                requests: Vec::new()
            }
        );
    }

    #[test]
    fn an_undelivered_shipment_does_nothing_while_reserving() {
        assert_eq!(Progress::ShipmentUndelivered.react(&Fulfilment::Reserving), Step::nothing());
    }

    #[test]
    fn an_abandonment_evolves_shipping_to_abandoned() {
        assert_eq!(Fulfilment::Shipping.evolve(&FulfilmentEvent::Abandoned), Fulfilment::Abandoned);
    }

    #[test]
    fn a_reservation_request_evolves_idle_to_reserving() {
        assert_eq!(Fulfilment::Idle.evolve(&FulfilmentEvent::ReservationRequested), Fulfilment::Reserving);
    }

    #[test]
    fn an_event_that_does_not_apply_leaves_the_fulfilment() {
        assert_eq!(Fulfilment::Done.evolve(&FulfilmentEvent::Abandoned), Fulfilment::Done);
    }
}
