---
prefix: arch
---

# Ring 1: ports

The `ports` crate, role `port-and-adapter`: the typed unit of work the shell calls, the raw
store an adapter implements, and the types that cross them. It is synchronous except for
gateways to other systems.

## `arch-s6q` A unit of work shall have exactly one way to change a lifecycle, which takes the whole outcome

`Unit::apply` loads the state, hands it to the decision and writes the outcome, inside the
unit's transaction. There is no `save(state)`, no `append(event)`, no `Outbox::push` and no
`Rejections::record`: with a second method, "call both" is a sentence, and a sentence is what
agents violate. An invariant the API can make impossible to violate lives in the API's shape.

```rust
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
```

`Unit` and `View` are structs in `ports`, not traits an adapter implements. `Unit::apply` is
the one place that numbers versions, serializes, and builds a `Recorded` fact:

```rust
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
}
```

```rust
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
```

What the shape rules out, each seen red against a mutant ([`arch-30c`](invariants.req.md),
[`arch-oif`](invariants.req.md)): a state written without a decision (E0308); a state that
disagrees with its facts, since `Transition`'s fields are private (E0451) and its only
constructor is the private fold through `evolve` (E0624); a fact that was never written,
since `Recorded`'s fields are private too (E0451); an outcome or a change that is computed
and dropped (`unused_must_use`); a read-model row the projection did not compute (E0451). A
decision made on stale state cannot be written, because the state it reads and the write share
the transaction: there is no expected version to pass and no conflict to retry.

### Acceptance

- `arch-s6q.1` The criteria of [`arch-30c`](invariants.req.md) and [`arch-0yc`](invariants.req.md).

## `arch-iv3d` A store adapter shall implement only the raw store, over text, versions and positions

An adapter implements `Store`, `Writer` and `Reader`: one statement per method, over JSON text.
It names no domain type, serializes nothing, numbers no version and checks no read model's
position; `ports` does each of those once, for every store. A rule that each adapter
implements again is a rule one of them gets wrong, and an adapter that handles typed facts has
to be able to build one, which lets everything else build one.

Every method of `Store` takes a `Key` that only `ports` can make, and `Unit` and `View` hold
their writer and reader in a private field, so nothing outside `ports` reaches a raw write: the
shell cannot call `save` or `append`, although the adapter implements them.

```rust
use crate::{Position, RequestId, StoreError, StreamId, Version};

pub struct Key(());

impl Key {
    pub(crate) fn new() -> Key {
        Key(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFact<'a> {
    pub position: Position,
    pub stream: StreamId,
    pub version: Version,
    pub event: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRequest {
    pub id: RequestId,
    pub stream: StreamId,
    pub request: String,
}

pub type Listener = Box<dyn Fn(Position) + Send + Sync>;

pub trait Reader {
    fn state(&mut self, lifecycle: &'static str, stream: &StreamId) -> Result<Option<(Version, String)>, StoreError>;
    fn states(&mut self, lifecycle: &'static str, each: &mut dyn FnMut(StreamId, &str) -> Result<(), StoreError>) -> Result<(), StoreError>;
    fn row(&mut self, read_model: &'static str, key: &str) -> Result<Option<String>, StoreError>;
    fn rows(&mut self, read_model: &'static str, each: &mut dyn FnMut(String, &str) -> Result<(), StoreError>) -> Result<(), StoreError>;
    fn refusals(&mut self, lifecycle: &'static str, stream: &StreamId) -> Result<Vec<String>, StoreError>;
}

pub trait Writer: Reader {
    fn append(&mut self, lifecycle: &'static str, stream: &StreamId, version: Version, event: &str) -> Result<Position, StoreError>;
    fn save(&mut self, lifecycle: &'static str, stream: &StreamId, version: Version, state: &str) -> Result<(), StoreError>;
    fn reject(&mut self, lifecycle: &'static str, stream: &StreamId, refusal: &str) -> Result<(), StoreError>;
    fn request(&mut self, lifecycle: &'static str, stream: &StreamId, request: &str) -> Result<(), StoreError>;
    fn facts(
        &mut self,
        lifecycle: &'static str,
        after: Position,
        limit: usize,
        each: &mut dyn FnMut(StoredFact<'_>) -> Result<(), StoreError>,
    ) -> Result<(), StoreError>;
    fn position(&mut self, read_model: &'static str) -> Result<Position, StoreError>;
    fn advance(&mut self, read_model: &'static str, position: Position) -> Result<(), StoreError>;
    fn upsert(&mut self, read_model: &'static str, key: &str, row: &str) -> Result<(), StoreError>;
    fn delete(&mut self, read_model: &'static str, key: &str) -> Result<(), StoreError>;
    fn clear(&mut self, read_model: &'static str) -> Result<(), StoreError>;
}

pub trait Store {
    fn transact<T>(&self, key: Key, work: impl FnOnce(&mut dyn Writer) -> Result<T, StoreError>) -> Result<T, StoreError>;
    fn snapshot<T>(&self, key: Key, work: impl FnOnce(&mut dyn Reader) -> Result<T, StoreError>) -> Result<T, StoreError>;
    fn committed(&self, key: Key, last: Position);
    fn requests(&self, key: Key, lifecycle: &'static str, limit: usize) -> Result<Vec<StoredRequest>, StoreError>;
    fn mark_done(&self, key: Key, id: RequestId) -> Result<(), StoreError>;
    fn mark_failed(&self, key: Key, id: RequestId) -> Result<(), StoreError>;
}
```

A `Writer` method's contract is its name and the conformance suite, which every adapter runs
([adapters](adapters.req.md)).

### Acceptance

- `arch-iv3d.1` The key to the raw store cannot be built or minted outside `ports` (E0603, E0624; mutants C11 and C12).
- `arch-iv3d.2` A `Unit` cannot be built around a writer outside `ports` (E0451; mutant C13).
- `arch-iv3d.3` The `raw-store` recipe fails when a driven adapter depends on `domain` or on a serializer outside its tests (T2, `just check`; mutant G10).

## `arch-9q4` Port traits shall live in the ports crate

An adapter can only implement a trait it can name, and its permitted dependencies reach
`domain` and `ports` only.

### Acceptance

- `arch-9q4.1` Cargo and the role matrix (T1/T2).

## `arch-v1q` A port shall be async only when its adapter waits on another system
verification: exempt
verification-reason: review only (T4); no gate is built

The store is synchronous: SQLite blocks, and a unit of work is one blocking transaction that a
driving adapter runs off the async executor. A gateway that calls another system is async.
Async on a port whose adapter never waits adds `Send` bounds and executors to every caller and
test for nothing.

```rust
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
```

### Acceptance

- `arch-v1q.1` Review (T4).
