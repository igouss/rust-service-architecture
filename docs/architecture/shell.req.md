---
prefix: arch
---

# Ring 2: the shell

The `app` crate, role `usecase`: the handler, each lifecycle's policy, queries, the relay.

## `arch-wp0` A command shall be handled as one unit of work

```
driving adapter   parse and refine into a command        (bad input answered here, 400)
  └ app::handle   journal.write: one transaction
      ├ unit.apply    load the state row, decide, fold, write state + facts + refusal
      └ Policy::after every reader of each new fact, in the same transaction
driving adapter   map the Answer to a response            (refused → 409 or 422)
```

```rust
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
```

`handle` has no `match` on the decision: the outcome goes to `apply` whole, so no branch can
forget a refusal or a fact.

### Acceptance

- `arch-wp0.1` The criteria of [`arch-30c`](invariants.req.md).

## `arch-cn9` Each lifecycle shall name every reader of its facts in one policy

`impl Policy for <Lifecycle>` is the one place that says what happens after that lifecycle's
facts: which projections fold them and which process manager they feed. A lifecycle with no
policy does not compile into a handler. A process manager is a lifecycle whose input is a
`Reaction`; `app::react` applies the step and runs the process manager's own policy, all inside
the unit that recorded the fact. Nothing is delivered later, so nothing is delivered twice.

Cross-aggregate flows are process managers, never one command calling another and never a
decider nested in a decider. A position in an `async fn` is not persisted state: a future
holding two durable writes is an implicit state machine; replace it with a process manager.

The example policies are in `app/src/tasks.rs` (`Task` folds a projection) and
`app/src/payments.rs` (`Payment` starts a process manager, `Fulfilment` has no readers).
`app/src/lib.rs` holds only the fixed machinery.

### Acceptance

- `arch-cn9.1` A lifecycle with no policy is a compile error where it is handled (E0277; mutant C8).
- `arch-cn9.2` A policy that skips its projection or its process manager fails the use-case tests (mutants T10, T11, T12).

## `arch-w0v` Requests shall reach the outbox in the unit of work and be performed by the relay

A process manager's requests are written by `Unit::apply` in the same transaction as its
facts, and performed afterward by `app::relay` through a `Gateway`. A request performed inside
the unit would be lost, or done twice, by a crash. The relay marks a request done only after
the gateway succeeds, so a request is performed at least once and its receiver drops a second
copy by the `RequestId` ([`arch-fif`](invariants.req.md)). A request whose gateway fails is
counted and claimed after those that have failed less, so one dead receiver does not hold back
the rest. The result of a request comes back as input to the process manager through
`app::advance`, called by whatever receives it: a webhook route, or the gateway's caller.

`relay` waits on the gateway and makes blocking store calls around it, so it runs on a thread
of its own with its own runtime, never on the executor that serves requests
([`arch-v1q`](ports.req.md)).

```rust
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
```

### Acceptance

- `arch-w0v.1` The criteria of [`arch-0yc`](invariants.req.md) and [`arch-fif`](invariants.req.md).

## `arch-3q8` Queries shall read state rows or read models and never bypass the role matrix

The read path is driving adapter → `app` query → `Journal::read` → the store adapter. A query
reads state rows (`View::state`, `View::states`) or a read model's rows (`View::row`,
`View::rows`); it never calls `decide` and never reaches for SQL itself. Because readers run in
the unit of work, a query after a command sees that command's effect. A page that shows two
things reads them in one `Journal::read`, as `task_board` does, so they are one snapshot.

The example queries (`open_tasks`, `task_tally`, `task_board`) follow the policy in
`app/src/tasks.rs`.

### Acceptance

- `arch-3q8.1` The role matrix (T2).
- `arch-3q8.2` The criteria of [`arch-9ln`](invariants.req.md).

## `arch-06ys` A read model shall be a projection: a pure, total, keyed fold of facts

A projection is for a view no single state row holds. `key` names the row a fact changes, or
`None` when the fact does not concern the view; `apply` folds the fact into that row, and
`None` deletes it. Both are total. One fact changes at most one row of a projection.

### Acceptance

- `arch-06ys.1` The purity gates of [`arch-zhm`](core.req.md) cover a projection as they cover `decide` (T2/T3).
- `arch-06ys.2` Each projection's given-row-and-fact tests pass, and `cargo mutants` on its file leaves no survivor (CI).

## `arch-wmwy` A read model shall change shape by being rebuilt, never by being migrated

A read model's `NAME` carries a version. A new shape is a new projection with a new name,
filled by `app::rebuild` from the facts; queries switch to it, and the old rows are deleted. No
code path writes a read-model row except `Unit::put`.

```rust
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
```

### Acceptance

- `arch-wmwy.1` The criteria of [`arch-oif`](invariants.req.md).

## `arch-jbno` Time and external data shall enter as commands before anything decides on them

A scheduler is a driving adapter: when its time comes it submits a command carrying the time
it read, and does no work. Data fetched from another system is recorded by a command before
any decision reads it. What was never recorded cannot be replayed or explained.

### Acceptance

- `arch-jbno.1` The banned-call scan keeps clocks and I/O out of `domain`, where every decision lives ([`arch-zhm`](core.req.md), T3).
- `arch-jbno.2` Review checks that a scheduler only submits, and that fetched data is recorded before use (T4).

## `arch-kpb` Every external input shall be bounded

Bound inputs at the edge: body size limits, pagination, a limit on every `claim` and `facts`
call. Unbounded growth driven by a client's request is a denial-of-service vector.

### Acceptance

- `arch-kpb.1` Review (T4).
