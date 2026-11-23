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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    fn decide(self, reads: &Self::Reads) -> Result<Facts<Self::Reads>, <Self::Reads as Lifecycle>::Refusal>;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Undelivered<R> {
    pub request: R,
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
- `Undelivered<R>` is a request that will not be performed. A process manager's reaction is built from it (`From<Undelivered<Request>>`), as it is from its gateway's reply, so every request it issues has a way to end ([`arch-w0v`](shell.req.md)).
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

## `arch-nf8v` An event shall name one fact in the domain's words and carry what that fact decided

`Completed`, `Captured { amount }`, `Renamed { title }`: past tense, one thing that happened.
Not `TaskUpdated { task }`: an event that carries the whole state says only that something
changed, so every reader has to compare two states to learn what, a projection cannot tell
which of its rows the fact concerns, and the history that facts are kept for
([`arch-jwo4`](invariants.req.md)) holds no more than a log of snapshots would. When several
things can happen to a lifecycle, they are several events.

### Acceptance

- `arch-nf8v.1` The `events` recipe fails when a lifecycle's event is named `Updated`, `Changed`, `Modified`, `Edited`, `Saved`, `Set`, `Replaced` or `Patched`, alone or after the lifecycle's name (T3, `just check`; mutants G12 and G13).
- `arch-nf8v.2` Review checks that an event carries what the fact decided and not the state after it (T4, gap ledger).

## `arch-dohm` A value that must be unique shall be the stream of a lifecycle that holds it

A unique email, a unique slug, one booking for a seat: an invariant over a set. A command
reads one lifecycle and nothing else ([`arch-5mh`](core.req.md)), so `decide` cannot ask
whether another stream already holds the value, and nothing may ask on its behalf: a query
before the command is a check and a write in two units, and the second caller passes the
check before the first one writes.

Make the value a lifecycle. Its stream id is the value in its canonical form, its state says
whether the value is held and by whom, and the command that takes it refuses when it is held.
Units of work run one at a time on every store ([adapters](adapters.req.md)), so two commands
for one value meet in the same state and exactly one is accepted. What the holder needs next
is advanced by that lifecycle's policy, in the same unit ([`arch-cn9`](shell.req.md)). No
unique index and no lookup table is involved, and the refusal is a recorded domain value
([`arch-783`](core.req.md)) with a name.

`Task` is the smallest case: its stream id is the task's identity, and `AddTask` refuses with
`AlreadyExists`.

### Acceptance

- `arch-dohm.1` A command's `decide` receives its own lifecycle and nothing else, so no other stream and no read model can reach a decision (T1; the signature in [`arch-5mh`](core.req.md)).
- `arch-dohm.2` A second claim of a stream is refused: `add_of_an_existing_task_is_refused`.
- `arch-dohm.3` Writers on separate connections that claim one stream leave exactly one accepted: `writers_claiming_one_stream_leave_exactly_one_accepted` (the `postgres` recipe).
- `arch-dohm.4` Review checks that no query result is passed into a command as proof that a value is free (T4, gap ledger).

## `arch-fdo` Illegal states shall be unrepresentable
verification: exempt
verification-reason: review only (T4); no gate is built

Sum-type state, each variant carrying only the data valid in it; parse, don't validate, at the
edge; refined types at construction (`Amount::new` returns `None` for zero; `Title::new` refuses
a blank or over-long title). A stored value is loaded under the type's stored rule, not by its
constructor ([`arch-v9j1`](core.req.md)). Fields stay private where a constructor enforces an
invariant.

### Acceptance

- `arch-fdo.1` Review (T4).

## `arch-v9j1` A refined type shall load every value it once stored, whatever its rule for new input is now

A refined type has two rules. Its constructor holds the rule for new input and is called at
the edge; that rule may be tightened at any time. Its `Deserialize` holds the stored rule: the
loosest rule under which any stored value was accepted. The stored rule is never tightened,
because facts are never rewritten ([`arch-btw1`](core.req.md)) and a value that was valid on
its day must load for as long as it is stored; it is loosened together with the rule for new
input. Loading never calls the constructor. A refinement the representation carries (`Amount`
over a non-zero integer) is its own stored rule.

`Title` shows the form: `Title::new` checks `LIMIT`, `TryFrom<String>` checks `STORED_LIMIT`,
and a constant assertion keeps the first from exceeding the second.

### Acceptance

- `arch-v9j1.1` A title stored at the stored limit loads through the store: `a_title_stored_under_the_rule_of_its_day_loads_whatever_the_rule_for_new_titles_is_now`; with `Title::LIMIT` lowered the same test fails as `StoreError::Corrupt` when loading calls the constructor (decision record 0011).
- `arch-v9j1.2` The rule for new input cannot be looser than the stored rule, and the stored rule cannot be tightened below it: a constant assertion (E0080; mutants C15 and C16).
- `arch-v9j1.3` The stored rule still refuses what was never valid: `a_stored_blank_title_is_refused_on_load`, `a_stored_over_long_title_is_refused_on_load` (mutants T24 and T80).

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
- `arch-j0k.2` The `wildcards` recipe rejects a tuple element in `domain` or `app` that is `_`, `..` or a bare name, which the lint does not see (T3; mutant G19). Review checks a wildcard nested in a variant, and tuple matches elsewhere (T4, gap ledger).

## `arch-1q6` Errors that are matched on shall be concrete enums

No `Box<dyn Error>` and no `anyhow` in `domain`, `ports` or `app`. `StoreError` names its
cause, one variant for each thing a caller can do about it: `Unavailable` (try again),
`Misconfigured` (fix the setting), `Corrupt` with the table (repair what is stored),
`Unencodable` with the table (fix the type), `OutOfOrder` with the read model, `Trimmed` with
the oldest position still held (read from there). `GatewayError` does the same for the relay:
`Unavailable` (try again, up to the bound), `Rejected` (do not).

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
- `arch-btw1.2` A new variant does not compile until that test's match names its JSON (T1; mutant G24), and the `stored` recipe rejects a stored type or variant with no sample there (T3; mutant G25). Review checks a struct stored only as a field (T4, gap ledger).
