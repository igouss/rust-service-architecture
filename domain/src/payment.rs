use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

use crate::{Command, Lifecycle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Amount(NonZeroU64);

impl Amount {
    pub fn new(cents: u64) -> Option<Amount> {
        NonZeroU64::new(cents).map(Amount)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeclineReason {
    ExceedsAuthorization,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Payment {
    Absent,
    Authorized { amount: Amount },
    Captured { amount: Amount },
    Declined { reason: DeclineReason },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaymentEvent {
    Authorized { amount: Amount },
    Captured { amount: Amount },
    Declined { reason: DeclineReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaymentRefusal {
    AlreadyAuthorized,
    NotAuthorized,
}

pub struct Authorize {
    pub amount: Amount,
}

impl Command for Authorize {
    type Reads = Payment;
    type Rejection = PaymentRefusal;

    /// While the payment is `Absent`, when `Authorize` is decided, `decide` shall emit `Authorized` with the amount.
    /// If the payment is not `Absent`, then `decide` shall refuse with `AlreadyAuthorized`.
    fn decide(self, reads: &Payment) -> Result<Vec<PaymentEvent>, PaymentRefusal> {
        match reads {
            Payment::Absent => Ok(vec![PaymentEvent::Authorized { amount: self.amount }]),
            Payment::Authorized { .. } | Payment::Captured { .. } | Payment::Declined { .. } => Err(PaymentRefusal::AlreadyAuthorized),
        }
    }
}

pub struct Capture {
    pub amount: Amount,
}

impl Command for Capture {
    type Reads = Payment;
    type Rejection = PaymentRefusal;

    /// While the payment is `Authorized`, when the amount is at most the authorized amount, `decide` shall emit `Captured` with the amount.
    /// While the payment is `Authorized`, when the amount exceeds the authorized amount, `decide` shall emit `Declined` with `ExceedsAuthorization`.
    /// If the payment is not `Authorized`, then `decide` shall refuse with `NotAuthorized`.
    fn decide(self, reads: &Payment) -> Result<Vec<PaymentEvent>, PaymentRefusal> {
        match reads {
            Payment::Authorized { amount } if self.amount <= *amount => Ok(vec![PaymentEvent::Captured { amount: self.amount }]),
            Payment::Authorized { .. } => Ok(vec![PaymentEvent::Declined {
                reason: DeclineReason::ExceedsAuthorization,
            }]),
            Payment::Absent | Payment::Captured { .. } | Payment::Declined { .. } => Err(PaymentRefusal::NotAuthorized),
        }
    }
}

impl Lifecycle for Payment {
    const NAME: &'static str = "payment";
    type Event = PaymentEvent;
    type Refusal = PaymentRefusal;

    fn initial() -> Payment {
        Payment::Absent
    }

    /// While the payment is `Absent`, when `Authorized` is folded, `evolve` shall return `Authorized`.
    /// While the payment is `Authorized`, when `Captured` or `Declined` is folded, `evolve` shall return that state.
    /// If an event does not apply to the state, then `evolve` shall return the state unchanged.
    fn evolve(self, event: &PaymentEvent) -> Payment {
        match (self, event) {
            (Payment::Absent, PaymentEvent::Authorized { amount }) => Payment::Authorized { amount: *amount },
            (Payment::Authorized { .. }, PaymentEvent::Captured { amount }) => Payment::Captured { amount: *amount },
            (Payment::Authorized { .. }, PaymentEvent::Declined { reason }) => Payment::Declined { reason: *reason },
            (s @ Payment::Absent, PaymentEvent::Captured { .. } | PaymentEvent::Declined { .. })
            | (s @ Payment::Authorized { .. }, PaymentEvent::Authorized { .. })
            | (s @ Payment::Captured { .. }, PaymentEvent::Authorized { .. } | PaymentEvent::Captured { .. } | PaymentEvent::Declined { .. })
            | (s @ Payment::Declined { .. }, PaymentEvent::Authorized { .. } | PaymentEvent::Captured { .. } | PaymentEvent::Declined { .. }) => s,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cents(value: u64) -> Amount {
        Amount::new(value).expect("positive")
    }

    fn given(events: &[PaymentEvent]) -> Payment {
        events.iter().fold(Payment::initial(), Payment::evolve)
    }

    #[test]
    fn authorize_of_an_absent_payment_emits_authorized() {
        assert_eq!(
            Authorize { amount: cents(500) }.decide(&given(&[])),
            Ok(vec![PaymentEvent::Authorized { amount: cents(500) }])
        );
    }

    #[test]
    fn authorize_of_an_authorized_payment_is_refused() {
        let state: Payment = given(&[PaymentEvent::Authorized { amount: cents(500) }]);
        assert_eq!(Authorize { amount: cents(500) }.decide(&state), Err(PaymentRefusal::AlreadyAuthorized));
    }

    #[test]
    fn capture_of_the_full_authorization_emits_captured() {
        let state: Payment = given(&[PaymentEvent::Authorized { amount: cents(500) }]);
        assert_eq!(
            Capture { amount: cents(500) }.decide(&state),
            Ok(vec![PaymentEvent::Captured { amount: cents(500) }])
        );
    }

    #[test]
    fn capture_above_the_authorization_emits_declined() {
        let state: Payment = given(&[PaymentEvent::Authorized { amount: cents(500) }]);
        assert_eq!(
            Capture { amount: cents(501) }.decide(&state),
            Ok(vec![PaymentEvent::Declined {
                reason: DeclineReason::ExceedsAuthorization
            }])
        );
    }

    #[test]
    fn capture_of_an_absent_payment_is_refused() {
        assert_eq!(Capture { amount: cents(500) }.decide(&given(&[])), Err(PaymentRefusal::NotAuthorized));
    }

    #[test]
    fn capture_of_a_captured_payment_is_refused() {
        let state: Payment = given(&[PaymentEvent::Authorized { amount: cents(500) }, PaymentEvent::Captured { amount: cents(500) }]);
        assert_eq!(Capture { amount: cents(1) }.decide(&state), Err(PaymentRefusal::NotAuthorized));
    }

    #[test]
    fn a_capture_evolves_an_authorized_payment_to_captured() {
        let state: Payment = given(&[PaymentEvent::Authorized { amount: cents(500) }, PaymentEvent::Captured { amount: cents(300) }]);
        assert_eq!(state, Payment::Captured { amount: cents(300) });
    }

    #[test]
    fn a_decline_evolves_an_authorized_payment_to_declined() {
        let state: Payment = given(&[
            PaymentEvent::Authorized { amount: cents(500) },
            PaymentEvent::Declined {
                reason: DeclineReason::ExceedsAuthorization,
            },
        ]);
        assert_eq!(
            state,
            Payment::Declined {
                reason: DeclineReason::ExceedsAuthorization
            }
        );
    }

    #[test]
    fn an_event_that_does_not_apply_leaves_the_state() {
        assert_eq!(given(&[PaymentEvent::Captured { amount: cents(500) }]), Payment::Absent);
    }
}
