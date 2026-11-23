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

A policy may call `react`, whose facts go to a policy, which may call `react` again. Two
policies that trigger each other without a state that ends it would never return, so a unit of
work applies at most `Unit::OUTCOMES` outcomes and fails with `StoreError::Runaway` on the
next, storing nothing ([`arch-goru`](invariants.req.md)).

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
the unit would be lost, or done twice, by a crash.

The gateway answers with a typed `Reply`. The relay turns it into the process manager's
`Reaction` (`From<Reply>`, a pure function beside the lifecycle) and applies it through
`app::resume`, in a unit of its own that also finishes the request. So a request is performed
at least once, its receiver drops a second copy by the `RequestId`, and its reply re-enters
the process manager once: a second relay, or one that crashed between the gateway and the
unit, finds the request finished and applies nothing ([`arch-fif`](invariants.req.md),
[`arch-ruz9`](invariants.req.md)). A
result that arrives later than the call (a webhook) comes back through `app::advance`, called
by the route that receives it.

A gateway fails in one of two ways. `Unavailable` is worth another attempt: the request is
counted and claimed after those that have failed less, so one dead receiver does not hold back
the rest. It is also put off: `Retry::wait` gives the first wait after the first failure and
twice the wait before after each further one, up to a longest wait, and the request is not
claimed before that moment. The bound on attempts is therefore a bound on time; counted in
passes alone, five attempts half a second apart gave up on a receiver that was away for three
seconds. The relay reads no clock: the composition root reads it once a pass and hands the
moment in, as `now` ([`arch-jbno`](shell.req.md)). `Rejected` is not. A rejected request, and one that has been unavailable as often as
the relay's bound allows, is parked: `app::abandon` closes it, records the cause, and applies
the reaction built from `Undelivered`, in one unit, so the process manager gives up instead of
waiting ([`arch-22kr`](invariants.req.md)). The bound is the composition root's setting.

The unit that closes a request can itself fail: the reply's reaction runs into a corrupt row or
a runaway policy. The gateway has already been called, and a pass that returned the error would
call it again on every pass and never reach the requests behind it. So a closing unit that
fails for a reason that is not the store being down counts as a failed attempt, and at the
bound the request is parked through `Unit::strand` with the error as its cause and no
reaction, because the reaction is what failed. A store that is `Unavailable` or
`Misconfigured` ends the pass: nothing can be recorded.

`relay` waits on the gateway and makes blocking store calls around it, so it runs on a thread
of its own with its own runtime, never on the executor that serves requests
([`arch-v1q`](ports.req.md)). The composition root starts that thread, passes over the outbox
after each pause, and stops it on the signal that stops the driving adapters; `server` does so
with a demonstration gateway that answers from nothing. A gateway that calls a real system is a
driven adapter in a crate of its own.

```rust
pub async fn relay<X, K, G>(store: &K, gateway: &G, limit: usize, retry: &Retry, now: Moment) -> Result<usize, StoreError>
where
    X: Reaction + From<G::Reply> + From<Undelivered<X::Request>>,
    X::Reads: Policy,
    K: Journal + Outbox,
    G: Gateway<X::Request>,
{
    let claimed: Vec<Claimed<X::Request>> = store.claim::<X>(limit, now)?;
    let mut performed: usize = 0;
    for Claimed {
        id,
        stream,
        request,
        attempts: failures,
    } in claimed
    {
        let failed: u64 = failures.saturating_add(1);
        let again: bool = failed < u64::from(retry.attempts.get());
        let outcome: Result<G::Reply, GatewayError> = gateway.perform(id, &stream, &request).await;
        let (closed, counted): (Result<usize, StoreError>, bool) = match outcome {
            Ok(reply) => (store.write(|unit: &mut Unit<'_>| resume(unit, id, X::from(reply))).map(|()| 1), again),
            Err(GatewayError::Unavailable { .. }) if again => (store.failed(id, retry.again(now, failed)).map(|()| 0), false),
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
            ) if counted => store.failed(id, retry.again(now, failed))?,
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
verification: exempt
verification-reason: review only (T4); no gate is built

Bound inputs at the edge: body size limits, pagination, a limit on every `claim` and `facts`
call. Unbounded growth driven by a client's request is a denial-of-service vector.

### Acceptance

- `arch-kpb.1` Review (T4).
