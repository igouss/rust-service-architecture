use std::num::NonZeroU32;

use domain::{Command, Lifecycle, Projection, Reaction, Undelivered};
use ports::{Answer, Applied, Claimed, Finished, Gateway, GatewayError, Journal, Outbox, Position, Recorded, RequestId, StoreError, StreamId, Unit};

pub mod board;
pub mod payments;
pub mod tasks;

pub trait Policy: Lifecycle {
    fn after(unit: &mut Unit<'_>, fact: &Recorded<Self>) -> Result<(), StoreError>;
}

pub fn handle<J: Journal, C: Command>(journal: &J, id: &StreamId<C::Reads>, command: C) -> Result<Answer<<C::Reads as Lifecycle>::Refusal>, StoreError>
where
    C::Reads: Policy,
{
    journal.write(|unit: &mut Unit<'_>| {
        let applied: Applied<C::Reads> = unit.apply(id, |state: C::Reads| ports::decide(command, state))?;
        settle(unit, applied)
    })
}

pub fn react<X: Reaction>(unit: &mut Unit<'_>, id: &StreamId<X::Reads>, progress: X) -> Result<(), StoreError>
where
    X::Reads: Policy,
{
    let applied: Applied<X::Reads> = unit.apply(id, |state: X::Reads| ports::react(progress, state))?;
    settle(unit, applied).map(|_answer: Answer<<X::Reads as Lifecycle>::Refusal>| ())
}

pub fn advance<J: Journal, X: Reaction>(journal: &J, id: &StreamId<X::Reads>, progress: X) -> Result<(), StoreError>
where
    X::Reads: Policy,
{
    journal.write(|unit: &mut Unit<'_>| react(unit, id, progress))
}

pub fn resume<X: Reaction>(unit: &mut Unit<'_>, request: RequestId, progress: X) -> Result<(), StoreError>
where
    X::Reads: Policy,
{
    let finished: Finished<X::Reads> = unit.finish(request, |state: X::Reads| ports::react(progress, state))?;
    closed(unit, finished)
}

pub fn abandon<X: Reaction>(unit: &mut Unit<'_>, request: RequestId, cause: &str, progress: X) -> Result<(), StoreError>
where
    X::Reads: Policy,
{
    let parked: Finished<X::Reads> = unit.park(request, cause, |state: X::Reads| ports::react(progress, state))?;
    closed(unit, parked)
}

fn closed<S: Policy>(unit: &mut Unit<'_>, finished: Finished<S>) -> Result<(), StoreError> {
    match finished {
        Finished::Applied(applied) => settle(unit, applied).map(|_answer: Answer<S::Refusal>| ()),
        Finished::Already => Ok(()),
    }
}

fn settle<S: Policy>(unit: &mut Unit<'_>, applied: Applied<S>) -> Result<Answer<S::Refusal>, StoreError> {
    for fact in &applied.facts {
        S::after(unit, fact)?;
    }
    Ok(applied.answer)
}

pub fn project<P: Projection>(unit: &mut Unit<'_>, fact: &ports::Fact<P>) -> Result<(), StoreError> {
    let row: Option<P::Row> = match ports::key::<P>(fact) {
        Some(key) => unit.row::<P>(&key)?,
        None => None,
    };
    unit.put(ports::project::<P>(row, fact))
}

pub fn rebuild<J: Journal, P: Projection>(journal: &J, batch: usize) -> Result<usize, StoreError> {
    journal.write(|unit: &mut Unit<'_>| {
        unit.clear::<P>()?;
        let mut after: Position = Position::ZERO;
        let mut folded: usize = 0;
        loop {
            let facts: Vec<ports::Fact<P>> = unit.facts::<P::Reads>(after, batch)?;
            let Some(last) = facts.last() else { return Ok(folded) };
            after = last.position();
            folded += facts.len();
            for fact in &facts {
                project::<P>(unit, fact)?;
            }
        }
    })
}

fn stranded<S: Lifecycle, K: Journal>(store: &K, request: RequestId, unsettled: &StoreError) -> Result<(), StoreError> {
    store.write(|unit: &mut Unit<'_>| unit.strand::<S>(request, &format!("{unsettled:?}")).map(|_parked: bool| ()))
}

pub async fn relay<X, K, G>(store: &K, gateway: &G, limit: usize, attempts: NonZeroU32) -> Result<usize, StoreError>
where
    X: Reaction + From<G::Reply> + From<Undelivered<X::Request>>,
    X::Reads: Policy,
    K: Journal + Outbox,
    G: Gateway<X::Request>,
{
    let claimed: Vec<Claimed<X::Request>> = store.claim::<X>(limit)?;
    let mut performed: usize = 0;
    for Claimed {
        id,
        stream,
        request,
        attempts: failures,
    } in claimed
    {
        let again: bool = failures.saturating_add(1) < u64::from(attempts.get());
        let outcome: Result<G::Reply, GatewayError> = gateway.perform(id, &stream, &request).await;
        let (closed, counted): (Result<usize, StoreError>, bool) = match outcome {
            Ok(reply) => (store.write(|unit: &mut Unit<'_>| resume(unit, id, X::from(reply))).map(|()| 1), again),
            Err(GatewayError::Unavailable { .. }) if again => (store.failed(id).map(|()| 0), false),
            Err(GatewayError::Unavailable { cause } | GatewayError::Rejected { cause }) => (
                store
                    .write(|unit: &mut Unit<'_>| abandon(unit, id, &cause, X::from(Undelivered { request })))
                    .map(|()| 0),
                false,
            ),
        };
        match closed {
            Ok(done) => performed += done,
            Err(down @ (StoreError::Unavailable { .. } | StoreError::Misconfigured { .. })) => return Err(down),
            Err(
                StoreError::Corrupt { .. }
                | StoreError::Unencodable { .. }
                | StoreError::OutOfOrder { .. }
                | StoreError::Trimmed { .. }
                | StoreError::Runaway { .. },
            ) if counted => store.failed(id)?,
            Err(
                unsettled @ (StoreError::Corrupt { .. }
                | StoreError::Unencodable { .. }
                | StoreError::OutOfOrder { .. }
                | StoreError::Trimmed { .. }
                | StoreError::Runaway { .. }),
            ) => stranded::<X::Reads, K>(store, id, &unsettled)?,
        }
    }
    Ok(performed)
}
