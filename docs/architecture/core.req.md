---
prefix: arch
---

# Ring 0: the core

The `domain` crate (and `kernel`, once two contexts share types): lifecycles, commands,
reactions and projections. The core is pure and synchronous; correctness lives here, and it is
cheap to test because nothing in it waits or touches the world.

## `arch-zhm` The core shall do no I/O and run no async code

No `async fn`, no `.await`. Banned calls: `SystemTime::now`, `Instant::now`, `.elapsed()`,
`Utc::now`, `Uuid::new_v4`, `thread_rng`, `std::env::*`, `std::fs::*`, `std::net::*`,
`std::process::*`, `println!` and its family. No `tracing` in the core. The only dependency the
core vouches for is `serde`, for the derive that fixes each stored type's encoding
([`arch-btw1`](core.req.md)).

### Acceptance

- `arch-zhm.1` The `effect-audit` run with `pure-deps = ["serde"]` flags every other dependency of `domain` (T2, `just check`).
- `arch-zhm.2` The `effect-audit` source scan flags a banned call by its presence (T3, `just check`); `// fc-allow: <reason>` is the only escape.
- `arch-zhm.3` The `effect-audit --strict` run flags `async` in the core (T3, `just check`).

## `arch-fh9` Time, ids and randomness shall enter the core as command fields

The driving adapter reads them once per external request and puts them in the command, so a
decision can be replayed in a test from its inputs alone.

### Acceptance

- `arch-fh9.1` The banned-call scan of [`arch-zhm`](core.req.md) (T3).

## `arch-5mh` A decision shall be a pure function from state and input to facts

Four traits are the whole vocabulary of the core.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Never {}

pub trait Lifecycle: Sized + Serialize + DeserializeOwned {
    const NAME: &'static str;
    type Event: Serialize + DeserializeOwned;
    type Refusal: Serialize;
    fn initial() -> Self;
    fn evolve(self, event: &Self::Event) -> Self;
}

pub type Facts<S> = Vec<<S as Lifecycle>::Event>;

pub trait Command {
    type Reads: Lifecycle;
    type Rejection: Into<<Self::Reads as Lifecycle>::Refusal>;
    fn decide(self, reads: &Self::Reads) -> Result<Facts<Self::Reads>, Self::Rejection>;
}

#[derive(Debug, PartialEq, Eq)]
pub struct Step<E, R> {
    pub events: Vec<E>,
    pub requests: Vec<R>,
}

impl<E, R> Step<E, R> {
    pub fn nothing() -> Self {
        Step {
            events: Vec::new(),
            requests: Vec::new(),
        }
    }
}

pub trait Reaction {
    type Reads: Lifecycle;
    type Request: Serialize + DeserializeOwned;
    fn react(self, reads: &Self::Reads) -> Step<<Self::Reads as Lifecycle>::Event, Self::Request>;
}

pub trait Projection {
    const NAME: &'static str;
    type Reads: Lifecycle;
    type Row: Serialize + DeserializeOwned;
    fn key(stream: &str, event: &<Self::Reads as Lifecycle>::Event) -> Option<String>;
    fn apply(row: Option<Self::Row>, event: &<Self::Reads as Lifecycle>::Event) -> Option<Self::Row>;
}
```

- A `Command` reads one lifecycle and returns the facts it decides, or a refusal. It has nowhere to put a request, so an aggregate cannot issue one ([`arch-hbw`](invariants.req.md)).
- A `Reaction` is one step of a process manager: it returns facts and requests, and cannot refuse. Its lifecycle's `Refusal` is `Never`.
- Something that already happened outside (a webhook, a clock tick) is a `Command` on the lifecycle that records it; when nothing can refuse it, that lifecycle's `Refusal` is `Never`.
- `Lifecycle::NAME` and `Projection::NAME` name the stored rows. A projection's name carries its version.
- A test calls `decide` or `react` directly and compares the result; it needs no store and no runtime.

### Acceptance

- `arch-5mh.1` A command that returns anything but its lifecycle's facts is a compile error (E0308; mutant C9).

## `arch-783` Every refusal shall be a recorded domain value

`Err` carries a variant of the lifecycle's `Refusal`, one per reason in the ubiquitous language.
The store records it ([`arch-qve`](invariants.req.md)); support reads it with SQL. A refusal is
not an event, so `evolve` has no arm for it. I/O failures cannot enter a refusal, and malformed
input never reaches `decide`: it is answered at the edge.

### Acceptance

- `arch-783.1` The criteria of [`arch-qve`](invariants.req.md).

## `arch-h1j` Evolve shall be a total method on the state that consumes it

`Lifecycle::evolve(self, &event) -> Self`, by move. `initial()` is the state before the first
fact. An event that does not apply returns the state unchanged.

### Acceptance

- `arch-h1j.1` The criteria of [`arch-bim`](invariants.req.md).

## `arch-fdo` Illegal states shall be unrepresentable

Sum-type state, each variant carrying only the data valid in it; parse, don't validate, at the
edge; refined types at construction (`Amount::new` returns `None` for zero; `Title::new` refuses
a blank or over-long title, at construction and again when a stored one is loaded). Fields stay
private where a constructor enforces an invariant.

### Acceptance

- `arch-fdo.1` Review (T4).

## `arch-f5d` State that survives a crash or is observable shall be an explicit lifecycle

State is a lifecycle if it must survive a crash, restart or cancellation, or if another request
can observe it: a named `enum` with `decide` and `evolve`, however few states it has.

| Where it hides | Looks like | Form |
|---|---|---|
| `bool` beside data | `exists: bool, value: u64` | `enum` with the data in the variant |
| `Option` valid in one phase | `shipped_at: Option<Timestamp>` | move the field into that variant |
| Shared flag | `Arc<AtomicBool>`, `Arc<Mutex<Status>>` | a lifecycle in the store |
| Position in an `async fn` | two durable writes in one future | a process manager ([shell](shell.req.md)) |

Not a lifecycle: a retry counter that dies with its request, an iterator's position, a config
flag with no transitions.

### Shape

`domain/src/task.rs` is one lifecycle in full: its state, events, refusals, a refined type,
two commands, a derived view, the EARS contract on each
function and a test per clause. Copy its form.

### Acceptance

- `arch-f5d.1` The `effect-audit` run flags a `static` or interior mutability in a domain crate (T2).
- `arch-f5d.2` Review holds the rows of the table (T4).

## `arch-j0k` Every match inside the workspace shall be exhaustive

No wildcard arm on a state, event or refusal type. Spell out every pair in a tuple match.
`#[non_exhaustive]` is forbidden on internal enums.

### Acceptance

- `arch-j0k.1` The workspace denies `clippy::wildcard_enum_match_arm` (T3, every `just check`).
- `arch-j0k.2` Review checks tuple matches, which the lint does not see (T4).

## `arch-1q6` Errors that are matched on shall be concrete enums

No `Box<dyn Error>` and no `anyhow` in `domain`, `ports` or `app`. `StoreError` names its
cause: `Unavailable`, `Corrupt` with the table, `OutOfOrder` with the read model.

### Acceptance

- `arch-1q6.1` The `pure-deps` allowlist keeps `anyhow` out of the core (T2).

## `arch-btw1` Every stored type shall keep its stored encoding

States, events, refusals, requests and read-model rows are stored as JSON through their serde
derive. Once a value of a type has been stored, its encoding only grows: a new variant or a new
field with a default. A variant is never renamed, removed or given a new meaning; a changed
meaning is a new variant. A state's shape may change with a migration of its rows; a fact's
may not, because facts are never rewritten.

### Acceptance

- `arch-btw1.1` The test `domain/tests/stored_encoding.rs` pins the JSON of every variant of every stored type and fails when one changes (mutant T20).
- `arch-btw1.2` Review checks that a new stored type is added to that test (T4→T3, gap ledger).
