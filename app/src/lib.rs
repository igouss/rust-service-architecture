use domain::{Command, Lifecycle, Projection, Reaction};
use ports::{Answer, Applied, Claimed, Gateway, GatewayError, Journal, Outbox, Position, Recorded, StoreError, StreamId, Unit};

pub mod payments;
pub mod tasks;

pub trait Policy: Lifecycle {
    fn after<U: Unit>(unit: &mut U, fact: &Recorded<Self::Event>) -> Result<(), StoreError>;
}

pub fn handle<J: Journal, C: Command>(journal: &J, id: &StreamId, command: C) -> Result<Answer<<C::Reads as Lifecycle>::Refusal>, StoreError>
where
    C::Reads: Policy,
{
    journal.write(|unit: &mut J::Unit<'_>| {
        let applied: Applied<C::Reads> = unit.apply(id, |state: C::Reads| ports::decide(command, state))?;
        settle(unit, applied)
    })
}

pub fn react<U: Unit, X: Reaction>(unit: &mut U, id: &StreamId, progress: X) -> Result<(), StoreError>
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
    journal.write(|unit: &mut J::Unit<'_>| react(unit, id, progress))
}

fn settle<U: Unit, S: Policy>(unit: &mut U, applied: Applied<S>) -> Result<Answer<S::Refusal>, StoreError> {
    for fact in &applied.facts {
        S::after(unit, fact)?;
    }
    Ok(applied.answer)
}

pub fn project<U: Unit, P: Projection>(unit: &mut U, fact: &ports::Fact<P>) -> Result<(), StoreError> {
    let row: Option<P::Row> = match ports::key::<P>(fact) {
        Some(key) => unit.row::<P>(&key)?,
        None => None,
    };
    unit.put(ports::project::<P>(row, fact))
}

pub fn rebuild<J: Journal, P: Projection>(journal: &J, batch: usize) -> Result<usize, StoreError> {
    journal.write(|unit: &mut J::Unit<'_>| {
        unit.clear::<P>()?;
        let mut after: Position = Position(0);
        let mut folded: usize = 0;
        loop {
            let facts: Vec<ports::Fact<P>> = unit.facts::<P::Reads>(after, batch)?;
            let Some(last) = facts.last() else { return Ok(folded) };
            after = last.position;
            folded += facts.len();
            for fact in &facts {
                project::<J::Unit<'_>, P>(unit, fact)?;
            }
        }
    })
}

pub async fn relay<X: Reaction, O: Outbox, G: Gateway<X::Request>>(outbox: &O, gateway: &G, limit: usize) -> Result<usize, StoreError> {
    let claimed: Vec<Claimed<X::Request>> = outbox.claim::<X>(limit)?;
    let mut performed: usize = 0;
    for request in &claimed {
        let outcome: Result<(), GatewayError> = gateway.perform(request.id, &request.stream, &request.request).await;
        match outcome {
            Ok(()) => {
                outbox.done(request.id)?;
                performed += 1;
            }
            Err(GatewayError { .. }) => outbox.failed(request.id)?,
        }
    }
    Ok(performed)
}
