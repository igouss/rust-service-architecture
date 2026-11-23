use std::fmt;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use domain::{Command, Lifecycle, Never, Projection, Reaction, Step};
use serde::de::DeserializeOwned;
use serde::Serialize;

pub mod store;

use store::{Key, Reader, Store, StoredFact, StoredRequest, Writer};

pub struct StreamId<S> {
    text: String,
    lifecycle: PhantomData<fn() -> S>,
}

impl<S> StreamId<S> {
    pub fn new(text: impl Into<String>) -> StreamId<S> {
        StreamId {
            text: text.into(),
            lifecycle: PhantomData,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }
}

impl<S> Clone for StreamId<S> {
    fn clone(&self) -> StreamId<S> {
        StreamId::new(self.text.clone())
    }
}

impl<S> PartialEq for StreamId<S> {
    fn eq(&self, other: &StreamId<S>) -> bool {
        self.text == other.text
    }
}

impl<S> Eq for StreamId<S> {}

impl<S> Hash for StreamId<S> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.text.hash(state);
    }
}

impl<S> fmt::Debug for StreamId<S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("StreamId").field(&self.text).finish()
    }
}

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
ordinal!(Moment);

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
    Runaway {
        lifecycle: &'static str,
        outcomes: u32,
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

pub struct Recorded<S: Lifecycle> {
    position: Position,
    stream: StreamId<S>,
    version: Version,
    event: S::Event,
}

impl<S: Lifecycle> Recorded<S> {
    pub fn position(&self) -> Position {
        self.position
    }

    pub fn stream(&self) -> &StreamId<S> {
        &self.stream
    }

    pub fn version(&self) -> Version {
        self.version
    }

    pub fn event(&self) -> &S::Event {
        &self.event
    }
}

#[must_use = "the facts of an applied outcome must reach their lifecycle's policy"]
pub struct Applied<S: Lifecycle> {
    pub answer: Answer<S::Refusal>,
    pub facts: Vec<Recorded<S>>,
}

#[must_use = "the facts of a finished request must reach their lifecycle's policy"]
pub enum Finished<S: Lifecycle> {
    Applied(Applied<S>),
    Already,
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

pub type Fact<P> = Recorded<<P as Projection>::Reads>;

pub fn key<P: Projection>(fact: &Fact<P>) -> Option<String> {
    P::key(fact.stream.as_str(), &fact.event)
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
    applied: u32,
}

impl Unit<'_> {
    pub const OUTCOMES: u32 = 32;

    pub fn apply<S: Lifecycle, R: Serialize>(&mut self, id: &StreamId<S>, decide: impl FnOnce(S) -> Outcome<S, R>) -> Result<Applied<S>, StoreError> {
        if self.applied == Unit::OUTCOMES {
            return Err(StoreError::Runaway {
                lifecycle: S::NAME,
                outcomes: self.applied,
            });
        }
        self.applied += 1;
        let (version, state): (Version, S) = state_in(self.store.state(S::NAME, id.as_str())?)?;
        match decide(state) {
            Outcome::Refused(refusal) => {
                self.store.reject(S::NAME, id.as_str(), &encode("rejections", &refusal)?)?;
                Ok(Applied {
                    answer: Answer::Refused(refusal),
                    facts: Vec::new(),
                })
            }
            Outcome::Accepted(Transition { state, events, requests }) => {
                let mut next: Version = version;
                let mut facts: Vec<Recorded<S>> = Vec::with_capacity(events.len());
                for event in events {
                    next = next.after(1);
                    let position: Position = self.store.append(S::NAME, id.as_str(), next, &encode("facts", &event)?)?;
                    self.last = Some(position);
                    facts.push(Recorded {
                        position,
                        stream: id.clone(),
                        version: next,
                        event,
                    });
                }
                if next != version {
                    self.store.save(S::NAME, id.as_str(), next, &encode("states", &state)?)?;
                }
                for request in &requests {
                    self.store.request(S::NAME, id.as_str(), &encode("outbox", request)?)?;
                }
                Ok(Applied {
                    answer: Answer::Accepted(next),
                    facts,
                })
            }
        }
    }

    pub fn finish<S: Lifecycle, R: Serialize>(&mut self, request: RequestId, decide: impl FnOnce(S) -> Outcome<S, R>) -> Result<Finished<S>, StoreError> {
        let closed: Option<String> = self.store.finish(S::NAME, request)?;
        self.closed(closed, decide)
    }

    pub fn park<S: Lifecycle, R: Serialize>(
        &mut self,
        request: RequestId,
        cause: &str,
        decide: impl FnOnce(S) -> Outcome<S, R>,
    ) -> Result<Finished<S>, StoreError> {
        let closed: Option<String> = self.store.park(S::NAME, request, cause)?;
        self.closed(closed, decide)
    }

    fn closed<S: Lifecycle, R: Serialize>(&mut self, closed: Option<String>, decide: impl FnOnce(S) -> Outcome<S, R>) -> Result<Finished<S>, StoreError> {
        match closed {
            Some(stream) => self.apply(&StreamId::new(stream), decide).map(Finished::Applied),
            None => Ok(Finished::Already),
        }
    }

    pub fn strand<S: Lifecycle>(&mut self, request: RequestId, cause: &str) -> Result<bool, StoreError> {
        Ok(self.store.park(S::NAME, request, cause)?.is_some())
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

    pub fn facts<S: Lifecycle>(&mut self, after: Position, limit: usize) -> Result<Vec<Recorded<S>>, StoreError> {
        let mut facts: Vec<Recorded<S>> = Vec::new();
        self.store.facts(S::NAME, after, limit, &mut |fact: StoredFact<'_>| {
            facts.push(Recorded {
                position: fact.position,
                stream: StreamId::new(fact.stream),
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
    pub fn state<S: Lifecycle>(&mut self, id: &StreamId<S>) -> Result<(Version, S), StoreError> {
        state_in(self.store.state(S::NAME, id.as_str())?)
    }

    pub fn states<S: Lifecycle>(&mut self) -> Result<Vec<(StreamId<S>, S)>, StoreError> {
        let mut states: Vec<(StreamId<S>, S)> = Vec::new();
        self.store.states(S::NAME, &mut |stream: String, state: &str| {
            states.push((StreamId::new(stream), decode("states", state)?));
            Ok(())
        })?;
        Ok(states)
    }

    pub fn row<P: Projection>(&mut self, key: &str) -> Result<Option<P::Row>, StoreError> {
        row_in::<P>(self.store.row(P::NAME, key)?)
    }

    pub fn refusals<S: Lifecycle>(&mut self, id: &StreamId<S>) -> Result<Vec<S::Refusal>, StoreError>
    where
        S::Refusal: DeserializeOwned,
    {
        self.store
            .refusals(S::NAME, id.as_str())?
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
            let mut unit: Unit<'_> = Unit { store, last: None, applied: 0 };
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
    pub stream: String,
    pub request: R,
    pub attempts: u64,
}

pub trait Outbox {
    fn claim<X: Reaction>(&self, limit: usize, now: Moment) -> Result<Vec<Claimed<X::Request>>, StoreError>;
    fn failed(&self, id: RequestId, again: Moment) -> Result<(), StoreError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayError {
    Unavailable { cause: String },
    Rejected { cause: String },
}

pub trait Gateway<R> {
    type Reply;
    fn perform(&self, id: RequestId, stream: &str, request: &R) -> impl Future<Output = Result<Self::Reply, GatewayError>> + Send;
}

impl<K: Store> Outbox for K {
    fn claim<X: Reaction>(&self, limit: usize, now: Moment) -> Result<Vec<Claimed<X::Request>>, StoreError> {
        self.requests(Key::new(), <X::Reads as Lifecycle>::NAME, limit, now)?
            .into_iter()
            .map(|stored: StoredRequest| {
                Ok(Claimed {
                    id: stored.id,
                    stream: stored.stream,
                    request: decode("outbox", &stored.request)?,
                    attempts: stored.attempts,
                })
            })
            .collect()
    }

    fn failed(&self, id: RequestId, again: Moment) -> Result<(), StoreError> {
        self.mark_failed(Key::new(), id, again)
    }
}
