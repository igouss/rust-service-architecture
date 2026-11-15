---
prefix: arch
---

# Ring 3: adapters, the wire and observability

Driving adapters (HTTP with axum and Datastar, schedulers), driven adapters (SQLite, gateways to
other systems) and, once triggered, the `wire` crate. Tokio lives here and in the composition
root, nowhere further in.

## `arch-68v` Driving adapters shall be thin

Deserialize, refine into a command (bad input is answered here with a 400), call the handler,
map the `Answer` or the shell error to a status. Each shell error maps to a status exhaustively,
with no wildcard arm. An `if` about business state in a handler is a ring violation.

### Acceptance

- `arch-68v.1` Review (T4).

## `arch-m3z` Request and response types shall live only in adapters

DTOs, form structs and HTML live in the driving adapter, which maps them to commands and maps
answers back. Stored types are the domain's own, through their serde derive
([core](core.req.md)); an adapter adds no second mapping for them.

### Acceptance

- `arch-m3z.1` Review (T4).

## `arch-0ev` The store adapter shall own the transaction

`Store::transact` opens one `BEGIN IMMEDIATE` transaction, runs the unit of work, and commits
only if it returns `Ok`. The shell never sees a connection. SQLite runs in WAL mode; the
durability setting is the composition root's choice between `Durability::Full`, which syncs to
disk on every commit, and `Durability::Normal`, which survives a process crash but can lose the
last commits on power loss and was measured about a hundred times faster per commit.

### Acceptance

- `arch-0ev.1` A unit that fails stores nothing ([`arch-0yc`](invariants.req.md)).

## `arch-e5eb` A service shall pick one store adapter in the composition root, and every store adapter shall pass the conformance suite

| The service's state | Store adapter |
|---|---|
| must survive the process, on one node | `adapter-sqlite` |
| must survive the process, in a database several processes or other tools share | `adapter-postgres` |
| is seen by other requests but may die with the process | `adapter-memory` |
| none: every request is answered from its input and from other systems | none; see below |

The shell and the driving adapters are written against `Journal` and `Outbox` and do not
change with the store; `main()` names the adapter. A store adapter also offers two functions
the port does not carry, with the same signatures in every adapter: `on_commit`, which the
composition root uses to tell driving adapters that a unit appended a fact, and `rejections`,
which reads the recorded refusals of a stream where there is no SQL to read them with.

`ports/tests/conformance/suite.rs` is the contract of `ports::store`, written once, over
lifecycles of its own. Each store adapter's `tests/conformance.rs` names the adapter as
`Subject` and includes the file, so every case runs against every adapter and a new adapter
is not done until all of them pass. An adapter's own test file holds only what the suite
cannot ask of every store: for SQLite, a reopened file and a corrupt row; for Postgres,
writers on separate connections.

A service with no state has no lifecycle (`arch-f5d`), so it has no store adapter, no
`Policy` and no call to `app::handle`. It keeps `domain` for its refined types and pure
functions, `ports` for its gateways, and its driving adapters, under the same role matrix and
the same gates.

### Acceptance

- `arch-e5eb.1` Every case of the suite passes against each store adapter (`just test`).
- `arch-e5eb.2` The `raw-store` recipe fails when a crate that implements `Store` has no `tests/conformance.rs` that includes the suite (T2, `just check`; mutant G11).
- `arch-e5eb.3` A fault planted in a store adapter fails a case of the suite: mutants T4 to T9, T16 and T26 in the SQLite adapter, mutants T27 to T38 in the memory adapter, mutants T40 to T42 in the Postgres adapter.

## `arch-m7du` The Postgres adapter shall run one unit of work at a time for its schema, and a read shall see one snapshot

`Store::transact` takes a transaction-level advisory lock, keyed by the schema, as its first
statement, so units of work on one schema run one after another however many processes hold
connections. That keeps what SQLite's single writer gives: a fact's position is its place in
commit order, a read model's position guard never refuses a valid fact, and the state a
decision reads is the state it writes over. There is no expected version, no conflict and no
retry (`arch-s6q`). A fact's position is one more than the highest stored, taken under the
lock, so positions have no gaps.

`Store::snapshot` runs in a read-only `REPEATABLE READ` transaction on a connection of its own:
readers never wait for the writer, and a query that reads twice reads one snapshot
(`arch-3q8`).

The store port stays synchronous (`arch-v1q`): the adapter uses the blocking client, and a
driving adapter runs a unit of work off the executor as it does for SQLite.

Without the lock, two units that interleave do one of three things, each observed
(`bench/results/2026-11-15-postgres-writer-lock-spike.txt`): refuse a valid fact as out of
order, move a read model's position backwards, or lose an update to a read-model row. The
last two raise no error.

### Acceptance

- `arch-m7du.1` Writers on separate connections leave the read model equal to the fold of the facts, with positions 1 to N: `many_writers_on_one_schema_keep_the_tally_equal_to_the_facts` (mutant T39).
- `arch-m7du.2` A read does not see a commit made after it began: `a_read_keeps_one_snapshot_while_another_writer_commits` (mutant T43).
- `arch-m7du.3` The `postgres` recipe passes against a server and fails when `STORE_POSTGRES_URL` is unset; the `postgres` job in continuous integration runs it.
- `arch-m7du.4` A mutant that needs the server is reported as not killed when no test passed, so an unreachable server never reads as a kill (mutants T40 to T42).

## `arch-qoy` The wire crate shall exist only once a consumer outside the process exists

Another process, another team or a published bus is the trigger. Minting it is one task that
ships together: the crate, its role in `hex-lint`, and the wire-contract gates. A wire crate
without its gates is worse than none.

### Acceptance

- `arch-qoy.1` The role check aborts on an untagged crate (T2).

## `arch-blp` Wire events shall be a separate, versioned published language

Once `wire` exists: it depends on nothing internal; internal events never serialize onto a
public edge; the internal-to-wire mapping is total and versioned
([`arch-7k8`](invariants.req.md)). Consumed wire enums are tolerant readers:
`#[non_exhaustive]` plus a `#[serde(other)]` fallback variant, the one place `#[non_exhaustive]`
is correct. JSON or protobuf on public edges, where humans and other teams live; `postcard` or
CBOR only on links you fully control.

### Acceptance

- `arch-blp.1` The criteria of [`arch-7k8`](invariants.req.md).
- `arch-blp.2` The `tolerant-reader-audit` run flags a closed Rust `enum` over a growable schema `enum` (CI).

## `arch-0bd` Tracing shall live in rings 2 and 3 only, around the pure island

One span per request and per process-manager step, with the aggregate id, the command type and
the decision outcome as fields. The core stays `tracing`-free. Because `decide` is pure, log
`(state, command) → outcome` around it and replay a captured failure exactly in a test; no log
lines inside the logic. Redact secrets through the types' `Debug` impls and cap the payload
size. Thin CRUD paths are excluded from the replay claim.

### Acceptance

- `arch-0bd.1` The `pure-deps` allowlist keeps `tracing` out of the core (T2).
- `arch-0bd.2` Review elsewhere (T4).
