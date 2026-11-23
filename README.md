# rust-service-architecture

A reference architecture for a Rust backend service, written to be read and followed by AI
coding agents. It is a working service (a task list and a payment with its fulfilment, with a
Datastar front end on SQLite), the
rules it obeys, and the checks that prove it obeys them. Start a service from it, or point an
agent at it when an existing project needs to know how to structure a feature.

If you are an agent, read [`AGENTS.md`](AGENTS.md) first. It fits in one read.

## The idea

Business logic is pure functions with no I/O. Everything that touches the world sits around it
in layers, and the compiler plus a few tools stop the layers from leaking into each other. Each
request runs as one database transaction: load a record, decide, save it, append what happened
to a history table, and update everything that depends on it.

## Run it

```sh
just check        # every gate, in seconds
just run          # the server on http://127.0.0.1:3000
```

The page adds and completes tasks, and authorizes and captures payments. A captured payment
starts a fulfilment; the relay, on a thread of its own, hands its requests to a demonstration
warehouse that is always in stock and feeds each reply back, so the page shows the fulfilment
move to shipped. `TASKS_DATABASE`, `TASKS_LISTEN`, `TASKS_DURABILITY` (`full` or `normal`)
and `TASKS_RELAY_ATTEMPTS` (how often a request is tried before it is parked, 10 unless set;
the waits between tries start at a second and double up to five minutes)
override the defaults.

Needs the Rust pinned in `rust-toolchain.toml`, [`just`](https://github.com/casey/just),
[`hex-lint`](https://github.com/igouss/hex-lint), [`effect-audit`](https://github.com/igouss/effect-audit),
and [`req`](https://github.com/igouss/req). The other gates are the `gates` crate of this workspace.

## The layers

Each is a crate; dependencies only point inward.

| Layer | Crate | Contains | May touch |
|---|---|---|---|
| Core | `domain` | state types, commands, events, decisions, derived views, process managers | nothing: no I/O, no clock, no async |
| Ports | `ports` | the traits the outside must implement and the types that cross them | the core |
| Shell | `app` | the handler, each record type's policy, queries, the relay | core and ports |
| Adapters | `adapter-sqlite`, `adapter-memory`, `adapter-postgres`, `adapter-http` | a store, on SQLite, in memory or on Postgres; HTTP and Datastar server-sent events | the outside world |
| Root | `server` | `main()`, which plugs adapters into the shell and runs the relay | everything |

The generated picture is [`docs/views/crates.md`](docs/views/crates.md).

## What a request does

```
HTTP adapter      parse input into a command          (bad input → 400 here)
  └ handler       open one transaction
      ├ load      read the record's state row
      ├ decide    pure: state + command → events, or a named refusal
      ├ evolve    pure: fold the events into the new state
      ├ save      write the state row, append the events, record any refusal
      └ policy    update derived views and process managers, same transaction
HTTP adapter      map the answer to a response; Datastar re-renders from current data
```

## The building blocks

A feature is always a new instance of one of these, never a new kind of thing.

- **Lifecycle**: a record's state as an enum whose variants hold only the data valid in them, plus `evolve`.
- **Command**: an intent to change a record. Its `decide` returns events or a named refusal.
- **Event**: a fact that happened. Appended to history, never edited.
- **Reaction**: one step of a process manager, a lifecycle that reacts to other records' events and issues requests for outside work.
- **Request**: outside work to do (call a service, send an email). Written to an outbox in the same transaction; a relay performs it afterward and hands the reply back to the process manager.
- **Projection**: a derived view folded from events, for views no single state row holds.
- **Policy**: per lifecycle, the one place that names who reads its events.
- **Query**: reads state rows or a view; never runs a decision.
- **Port and adapter**: a trait and its implementation, for I/O.

The worked example is [`domain/src/task.rs`](domain/src/task.rs); the recipe is
[`docs/how-to/add-a-use-case.md`](docs/how-to/add-a-use-case.md).

## What it guarantees, and how you can tell

- **Saved state always matches its events.** The only way to produce a new state is to fold the decided events through `evolve`; any other route is a compile error.
- **A command's whole effect is atomic.** One write call takes the whole outcome, inside one transaction, together with every view and process-manager step it causes.
- **Refusals are recorded**, so you can see with SQL why something was rejected.
- **A request for outside work ends.** Its reply goes back to the process manager once; a request that is rejected, or fails more often than the bound, is parked where SQL can see it and the process manager is told.
- **A creating request can be repeated safely.** Sent with an `Idempotency-Key`, it uses the key as the new record's id; a repeat is refused by name and creates nothing. The page does not send one yet.
- **Time, ids and outside data enter as input**, never read inside the core, so any decision can be replayed in a test.
- **Stored data keeps its encoding.** A test pins the JSON of every stored type.

Each guarantee has a planted mutant that must fail, by a compile error, a named test or a
gate. Mutation testing runs in continuous integration, not while you work: `just check` is the
whole working loop and takes seconds. The rules, with ids like `arch-s6q`, are in [`docs/architecture/`](docs/architecture/);
the reasons are in [`docs/adr/`](docs/adr/).

## Licence

MIT or Apache-2.0, at your option.

## What it is for, and what it is not

It is for a service whose commands run one at a time: SQLite on one node, state held in
memory, or Postgres behind a writer lock. That is what lets one transaction hold everything a
command causes. A service that needs commands to commit concurrently changes the design;
[`docs/adr/0001`](docs/adr/0001-state-rows-with-facts-as-history.md) and
[`0006`](docs/adr/0006-postgres-as-one-writer.md) say how.

## Measured

One laptop, one NVMe disk, 30 timed calls per figure, three runs; `bench/results/` has every
run and its conditions. Commit ids and the `baseline-2026-11-11` tag named in those files and in
`docs/adr/` belong to the development history, which is not published. Treat these as the shape of the costs, not as promises.

- **The durability setting decides write speed.** A command costs about 4 ms with
  `TASKS_DURABILITY=full`, which syncs to disk on every commit, and about 60 µs with `normal`,
  which survives a process crash but can lose the last commits on power loss. The default is
  `full`.
- **History length does not slow a command.** With 10 000 events already on a record, a command
  costs the same as with one.
- **A derived view or a process-manager step in the same transaction is close to free**: not
  visible under `full`, and the whole command stays under 70 µs under `normal`.
- **Listing records by reading their state rows is linear, about 0.37 µs per row**: 0.35 ms for
  1 000 tasks, 4 ms for 10 000, 40 ms for 100 000. Past a few thousand rows, a list needs a
  projection that keeps only what the page shows, or pagination.
- **Running a command off the async executor costs tens of microseconds.**

Not built:

- **A gateway to a real system.** The warehouse in `server` answers from nothing; it shows
  where a gateway plugs in, not how to call one.
- **Compensation.** A fulfilment that is abandoned leaves its payment captured and tells
  nobody; decision record 0027 proposes two ways to close that and takes neither.
- **Schema migrations.** State rows are meant to change shape by migration; there is no
  migration mechanism yet, and the tables are created only if absent.
- **A scheduler adapter, a published wire format for other services, authentication.**

The rules whose check is still a human reviewer are listed in the gap ledger in
[`docs/architecture/enforcement.req.md`](docs/architecture/enforcement.req.md).
