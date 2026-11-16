use std::future::Future;

use domain::{Command, Lifecycle, Never, Projection, Reaction, Step};
use serde::de::DeserializeOwned;
use serde::Serialize;

pub mod store;

use store::{Key, Reader, Store, StoredFact, StoredRequest, Writer};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StreamId(pub String);

macro_rules! ordinal {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u64);

        impl $name {
            pub const ZERO: $name = $name(0);

            pub fn get(self) -> u64 {
                self.0
            }

            pub fn after(self, more: u64) -> $name {
                $name(self.0.saturating_add(more).min(i64::MAX.unsigned_abs()))
            }

            pub fn stored(self) -> i64 {
                i64::try_from(self.0).unwrap_or(i64::MAX)
            }

            pub fn loaded(value: i64) -> Option<$name> {
                u64::try_from(value).ok().map($name)
            }
        }

        impl From<u32> for $name {
            fn from(value: u32) -> $name {
                $name(u64::from(value))
            }
        }
    };
}

ordinal!(Version);
ordinal!(Position);
ordinal!(RequestId);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    Unavailable {
        cause: String,
    },
    Misconfigured {
        cause: String,
    },
    Corrupt {
        table: &'static str,
        cause: String,
    },
    Unencodable {
        table: &'static str,
        cause: String,
    },
    OutOfOrder {
        read_model: &'static str,
        at: Position,
        offered: Position,
    },
    Trimmed {
        after: Position,
        oldest: Position,
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
        Err(refusal) => Outcome::Refused(refusal),
    }
}

pub fn react<X: Reaction>(reaction: X, state: X::Reads) -> Outcome<X::Reads, X::Request> {
    let step: Step<<X::Reads as Lifecycle>::Event, X::Request> = reaction.react(&state);
    Outcome::Accepted(Transition::fold(state, step.events, step.requests))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded<E> {
    position: Position,
    stream: StreamId,
    version: Version,
    event: E,
}

impl<E> Recorded<E> {
    pub fn position(&self) -> Position {
        self.position
    }

    pub fn stream(&self) -> &StreamId {
        &self.stream
    }

    pub fn version(&self) -> Version {
        self.version
    }

    pub fn event(&self) -> &E {
        &self.event
    }
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

fn encode<T: Serialize>(table: &'static str, value: &T) -> Result<String, StoreError> {
    serde_json::to_string(value).map_err(|error: serde_json::Error| StoreError::Unencodable {
        table,
        cause: error.to_string(),
    })
}

fn decode<T: DeserializeOwned>(table: &'static str, text: &str) -> Result<T, StoreError> {
    serde_json::from_str(text).map_err(|error: serde_json::Error| StoreError::Corrupt {
        table,
        cause: error.to_string(),
    })
}

fn state_in<S: Lifecycle>(found: Option<(Version, String)>) -> Result<(Version, S), StoreError> {
    match found {
        None => Ok((Version::ZERO, S::initial())),
        Some((version, state)) => Ok((version, decode("states", &state)?)),
    }
}

fn row_in<P: Projection>(found: Option<String>) -> Result<Option<P::Row>, StoreError> {
    found.map(|text: String| decode("read_models", &text)).transpose()
}

pub struct Unit<'a> {
    store: &'a mut dyn Writer,
    last: Option<Position>,
}

impl Unit<'_> {
    pub fn apply<S: Lifecycle, R: Serialize>(&mut self, id: &StreamId, decide: impl FnOnce(S) -> Outcome<S, R>) -> Result<Applied<S>, StoreError> {
        let (version, state): (Version, S) = state_in(self.store.state(S::NAME, id)?)?;
        match decide(state) {
            Outcome::Refused(refusal) => {
                self.store.reject(S::NAME, id, &encode("rejections", &refusal)?)?;
                Ok(Applied {
                    answer: Answer::Refused(refusal),
                    facts: Vec::new(),
                })
            }
            Outcome::Accepted(Transition { state, events, requests }) => {
                let mut next: Version = version;
                let mut facts: Vec<Recorded<S::Event>> = Vec::with_capacity(events.len());
                for event in events {
                    next = next.after(1);
                    let position: Position = self.store.append(S::NAME, id, next, &encode("facts", &event)?)?;
                    self.last = Some(position);
                    facts.push(Recorded {
                        position,
                        stream: id.clone(),
                        version: next,
                        event,
                    });
                }
                if next != version {
                    self.store.save(S::NAME, id, next, &encode("states", &state)?)?;
                }
                for request in &requests {
                    self.store.request(S::NAME, id, &encode("outbox", request)?)?;
                }
                Ok(Applied {
                    answer: Answer::Accepted(next),
                    facts,
                })
            }
        }
    }

    pub fn row<P: Projection>(&mut self, key: &str) -> Result<Option<P::Row>, StoreError> {
        row_in::<P>(self.store.row(P::NAME, key)?)
    }

    pub fn put<P: Projection>(&mut self, change: Change<P>) -> Result<(), StoreError> {
        let Change { position, write } = change;
        let at: Position = self.store.position(P::NAME)?;
        if position <= at {
            return Err(StoreError::OutOfOrder {
                read_model: P::NAME,
                at,
                offered: position,
            });
        }
        match write {
            Write::Nothing => {}
            Write::Upsert(key, row) => self.store.upsert(P::NAME, &key, &encode("read_models", &row)?)?,
            Write::Delete(key) => self.store.delete(P::NAME, &key)?,
        }
        self.store.advance(P::NAME, position)
    }

    pub fn clear<P: Projection>(&mut self) -> Result<(), StoreError> {
        self.store.clear(P::NAME)
    }

    pub fn facts<S: Lifecycle>(&mut self, after: Position, limit: usize) -> Result<Vec<Recorded<S::Event>>, StoreError> {
        let mut facts: Vec<Recorded<S::Event>> = Vec::new();
        self.store.facts(S::NAME, after, limit, &mut |fact: StoredFact<'_>| {
            facts.push(Recorded {
                position: fact.position,
                stream: fact.stream,
                version: fact.version,
                event: decode("facts", fact.event)?,
            });
            Ok(())
        })?;
        Ok(facts)
    }
}

pub struct View<'a> {
    store: &'a mut dyn Reader,
}

impl View<'_> {
    pub fn state<S: Lifecycle>(&mut self, id: &StreamId) -> Result<(Version, S), StoreError> {
        state_in(self.store.state(S::NAME, id)?)
    }

    pub fn states<S: Lifecycle>(&mut self) -> Result<Vec<(StreamId, S)>, StoreError> {
        let mut states: Vec<(StreamId, S)> = Vec::new();
        self.store.states(S::NAME, &mut |stream: StreamId, state: &str| {
            states.push((stream, decode("states", state)?));
            Ok(())
        })?;
        Ok(states)
    }

    pub fn row<P: Projection>(&mut self, key: &str) -> Result<Option<P::Row>, StoreError> {
        row_in::<P>(self.store.row(P::NAME, key)?)
    }

    pub fn refusals<S: Lifecycle>(&mut self, id: &StreamId) -> Result<Vec<S::Refusal>, StoreError>
    where
        S::Refusal: DeserializeOwned,
    {
        self.store
            .refusals(S::NAME, id)?
            .iter()
            .map(|refusal: &String| decode("rejections", refusal))
            .collect()
    }

    pub fn rows<P: Projection>(&mut self) -> Result<Vec<(String, P::Row)>, StoreError> {
        let mut rows: Vec<(String, P::Row)> = Vec::new();
        self.store.rows(P::NAME, &mut |key: String, row: &str| {
            rows.push((key, decode("read_models", row)?));
            Ok(())
        })?;
        Ok(rows)
    }
}

pub trait Journal {
    fn write<T>(&self, work: impl FnOnce(&mut Unit<'_>) -> Result<T, StoreError>) -> Result<T, StoreError>;
    fn read<T>(&self, work: impl FnOnce(&mut View<'_>) -> Result<T, StoreError>) -> Result<T, StoreError>;
}

impl<K: Store> Journal for K {
    fn write<T>(&self, work: impl FnOnce(&mut Unit<'_>) -> Result<T, StoreError>) -> Result<T, StoreError> {
        let (value, last): (T, Option<Position>) = self.transact(Key::new(), |store: &mut dyn Writer| {
            let mut unit: Unit<'_> = Unit { store, last: None };
            let value: T = work(&mut unit)?;
            Ok((value, unit.last))
        })?;
        if let Some(position) = last {
            self.committed(Key::new(), position);
        }
        Ok(value)
    }

    fn read<T>(&self, work: impl FnOnce(&mut View<'_>) -> Result<T, StoreError>) -> Result<T, StoreError> {
        self.snapshot(Key::new(), |store: &mut dyn Reader| work(&mut View { store }))
    }
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

impl<K: Store> Outbox for K {
    fn claim<X: Reaction>(&self, limit: usize) -> Result<Vec<Claimed<X::Request>>, StoreError> {
        self.requests(Key::new(), <X::Reads as Lifecycle>::NAME, limit)?
            .into_iter()
            .map(|stored: StoredRequest| {
                Ok(Claimed {
                    id: stored.id,
                    stream: stored.stream,
                    request: decode("outbox", &stored.request)?,
                })
            })
            .collect()
    }

    fn done(&self, id: RequestId) -> Result<(), StoreError> {
        self.mark_done(Key::new(), id)
    }

    fn failed(&self, id: RequestId) -> Result<(), StoreError> {
        self.mark_failed(Key::new(), id)
    }
}
