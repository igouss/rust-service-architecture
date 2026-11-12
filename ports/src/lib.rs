use std::future::Future;

use domain::{Command, Lifecycle, Never, Projection, Reaction, Step};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StreamId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    Unavailable {
        cause: String,
    },
    Corrupt {
        table: &'static str,
        cause: String,
    },
    OutOfOrder {
        read_model: &'static str,
        at: Position,
        offered: Position,
    },
}

pub struct Transition<S: Lifecycle, R> {
    state: S,
    events: Vec<S::Event>,
    requests: Vec<R>,
}

impl<S: Lifecycle, R> Transition<S, R> {
    fn fold(state: S, events: Vec<S::Event>, requests: Vec<R>) -> Self {
        let state: S = events.iter().fold(state, S::evolve);
        Transition { state, events, requests }
    }

    pub fn state(&self) -> &S {
        &self.state
    }

    pub fn into_parts(self) -> (S, Vec<S::Event>, Vec<R>) {
        (self.state, self.events, self.requests)
    }
}

#[must_use = "an outcome that is never applied loses its facts, its requests or its refusal"]
pub enum Outcome<S: Lifecycle, R> {
    Accepted(Transition<S, R>),
    Refused(S::Refusal),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer<J> {
    Accepted(Version),
    Refused(J),
}

pub fn decide<C: Command>(command: C, state: C::Reads) -> Outcome<C::Reads, Never> {
    match command.decide(&state) {
        Ok(events) => Outcome::Accepted(Transition::fold(state, events, Vec::new())),
        Err(rejection) => Outcome::Refused(rejection.into()),
    }
}

pub fn react<X: Reaction>(reaction: X, state: X::Reads) -> Outcome<X::Reads, X::Request> {
    let step: Step<<X::Reads as Lifecycle>::Event, X::Request> = reaction.react(&state);
    Outcome::Accepted(Transition::fold(state, step.events, step.requests))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded<E> {
    pub position: Position,
    pub stream: StreamId,
    pub version: Version,
    pub event: E,
}

#[must_use = "the facts of an applied outcome must reach their lifecycle's policy"]
pub struct Applied<S: Lifecycle> {
    pub answer: Answer<S::Refusal>,
    pub facts: Vec<Recorded<S::Event>>,
}

pub enum Write<P: Projection> {
    Nothing,
    Upsert(String, P::Row),
    Delete(String),
}

#[must_use = "a change that is never put leaves the read model behind its facts"]
pub struct Change<P: Projection> {
    position: Position,
    write: Write<P>,
}

impl<P: Projection> Change<P> {
    pub fn into_parts(self) -> (Position, Write<P>) {
        (self.position, self.write)
    }
}

pub type Fact<P> = Recorded<<<P as Projection>::Reads as Lifecycle>::Event>;

pub fn key<P: Projection>(fact: &Fact<P>) -> Option<String> {
    P::key(&fact.stream.0, &fact.event)
}

pub fn project<P: Projection>(row: Option<P::Row>, fact: &Fact<P>) -> Change<P> {
    let write: Write<P> = match key::<P>(fact) {
        None => Write::Nothing,
        Some(key) => match P::apply(row, &fact.event) {
            Some(row) => Write::Upsert(key, row),
            None => Write::Delete(key),
        },
    };
    Change {
        position: fact.position,
        write,
    }
}

pub trait Unit {
    fn apply<S: Lifecycle, R: serde::Serialize>(&mut self, id: &StreamId, decide: impl FnOnce(S) -> Outcome<S, R>) -> Result<Applied<S>, StoreError>;
    fn row<P: Projection>(&mut self, key: &str) -> Result<Option<P::Row>, StoreError>;
    fn put<P: Projection>(&mut self, change: Change<P>) -> Result<(), StoreError>;
    fn clear<P: Projection>(&mut self) -> Result<(), StoreError>;
    fn facts<S: Lifecycle>(&mut self, after: Position, limit: usize) -> Result<Vec<Recorded<S::Event>>, StoreError>;
}

pub trait View {
    fn state<S: Lifecycle>(&mut self, id: &StreamId) -> Result<(Version, S), StoreError>;
    fn states<S: Lifecycle>(&mut self) -> Result<Vec<(StreamId, S)>, StoreError>;
    fn row<P: Projection>(&mut self, key: &str) -> Result<Option<P::Row>, StoreError>;
    fn rows<P: Projection>(&mut self) -> Result<Vec<(String, P::Row)>, StoreError>;
}

pub trait Journal {
    type Unit<'a>: Unit
    where
        Self: 'a;
    type View<'a>: View
    where
        Self: 'a;
    fn write<T>(&self, work: impl FnOnce(&mut Self::Unit<'_>) -> Result<T, StoreError>) -> Result<T, StoreError>;
    fn read<T>(&self, work: impl FnOnce(&mut Self::View<'_>) -> Result<T, StoreError>) -> Result<T, StoreError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claimed<R> {
    pub id: RequestId,
    pub stream: StreamId,
    pub request: R,
}

pub trait Outbox {
    fn claim<X: Reaction>(&self, limit: usize) -> Result<Vec<Claimed<X::Request>>, StoreError>;
    fn done(&self, id: RequestId) -> Result<(), StoreError>;
    fn failed(&self, id: RequestId) -> Result<(), StoreError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayError {
    pub cause: String,
}

pub trait Gateway<R> {
    fn perform(&self, id: RequestId, stream: &StreamId, request: &R) -> impl Future<Output = Result<(), GatewayError>> + Send;
}
