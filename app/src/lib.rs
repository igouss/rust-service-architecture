use domain::{Command, Lifecycle, Projection, Reaction};
use ports::{Answer, Applied, Claimed, Finished, Gateway, GatewayError, Journal, Outbox, Position, Recorded, RequestId, StoreError, StreamId, Unit};

pub mod payments;
pub mod tasks;

pub trait Policy: Lifecycle {
    fn after(unit: &mut Unit<'_>, fact: &Recorded<Self::Event>) -> Result<(), StoreError>;
}

pub fn handle<J: Journal, C: Command>(journal: &J, id: &StreamId, command: C) -> Result<Answer<<C::Reads as Lifecycle>::Refusal>, StoreError>
where
    C::Reads: Policy,
{
    journal.write(|unit: &mut Unit<'_>| {
        let applied: Applied<C::Reads> = unit.apply(id, |state: C::Reads| ports::decide(command, state))?;
        settle(unit, applied)
    })
}

pub fn react<X: Reaction>(unit: &mut Unit<'_>, id: &StreamId, progress: X) -> Result<(), StoreError>
where
    X::Reads: Policy,
{
    let applied: Applied<X::Reads> = unit.apply(id, |state: X::Reads| ports::react(progress, state))?;
    settle(unit, applied).map(|_answer: Answer<<X::Reads as Lifecycle>::Refusal>| ())
}

pub fn advance<J: Journal, X: Reaction>(journal: &J, id: &StreamId, progress: X) -> Result<(), StoreError>
where
    X::Reads: Policy,
{
    journal.write(|unit: &mut Unit<'_>| react(unit, id, progress))
}

pub fn resume<X: Reaction>(unit: &mut Unit<'_>, request: RequestId, progress: X) -> Result<(), StoreError>
where
    X::Reads: Policy,
{
    match unit.finish(request, |state: X::Reads| ports::react(progress, state))? {
        Finished::Applied(applied) => settle(unit, applied).map(|_answer: Answer<<X::Reads as Lifecycle>::Refusal>| ()),
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

pub async fn relay<X, K, G>(store: &K, gateway: &G, limit: usize) -> Result<usize, StoreError>
where
    X: Reaction + From<G::Reply>,
    X::Reads: Policy,
    K: Journal + Outbox,
    G: Gateway<X::Request>,
{
    let claimed: Vec<Claimed<X::Request>> = store.claim::<X>(limit)?;
    let mut performed: usize = 0;
    for request in &claimed {
        let outcome: Result<G::Reply, GatewayError> = gateway.perform(request.id, &request.stream, &request.request).await;
        match outcome {
            Ok(reply) => {
                store.write(|unit: &mut Unit<'_>| resume(unit, request.id, X::from(reply)))?;
                performed += 1;
            }
            Err(GatewayError { .. }) => store.failed(request.id)?,
        }
    }
    Ok(performed)
}
