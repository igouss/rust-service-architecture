---
prefix: arch
---

# Ring 3: adapters, the wire and observability

Driving adapters (HTTP with axum and Datastar, schedulers), driven adapters (SQLite, gateways to
other systems) and, once triggered, the `wire` crate. Tokio lives here and in the composition
root, nowhere further in.

## `arch-68v` Driving adapters shall be thin
verification: exempt
verification-reason: review only (T4); no gate is built

Deserialize, refine into a command (bad input is answered here with a 400), call the handler,
map the `Answer` or the shell error to a status. Each shell error maps to a status exhaustively,
with no wildcard arm. An `if` about business state in a handler is a ring violation.

### Acceptance

- `arch-68v.1` Review (T4).

## `arch-59ap` A request that creates a lifecycle shall be safe to repeat under a key the caller chooses

A request that starts a new stream carries, in the `Idempotency-Key` header, the id the caller
chose for it; the driving adapter uses that key as the stream id. A repeat of the request
meets the stream its first arrival created, and the lifecycle refuses it by name
(`AlreadyExists`, `AlreadyAuthorized`): the caller is answered 409 and nothing is created
twice. This is [`arch-dohm`](core.req.md) applied to the request itself: the value that must
be unique is the stream. The adapter stores no key and compares none
([`arch-7h52`](adapters.req.md)). A request without a key gets a minted id and is not safe to
repeat. Commands on an existing stream need no key: the state they change refuses the second
one.

### Acceptance

- `arch-59ap.1` A creation repeated under its key creates nothing and is answered with the refusal: `a_task_posted_again_under_its_key_adds_no_second_task`, `a_payment_authorized_again_under_its_key_authorizes_no_second_payment` (mutant H10).
- `arch-59ap.2` A key that is not a UUID is answered 400 before a command exists: `a_task_posted_under_a_key_that_is_not_a_uuid_answers_400_and_adds_nothing` (mutant H11).

## `arch-m3z` Request and response types shall live only in adapters
verification: exempt
verification-reason: review only (T4); no gate is built

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
change with the store; `main()` names the adapter. A store adapter's `on_commit` takes the
listener the composition root uses to tell driving adapters that a unit appended a fact;
`Journal::write` calls it once, after the unit has committed.

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

- `arch-e5eb.1` Every case of the suite passes against the SQLite and memory adapters (`just test`) and against the Postgres adapter (`just postgres`, which needs a server and fails unless it counts every case as passed; mutant G26).
- `arch-e5eb.2` The `raw-store` recipe fails when a crate that implements `Store` has no `tests/conformance.rs` that includes the suite (T2, `just check`; mutant G11).
- `arch-e5eb.3` A fault planted in a store adapter fails a case of the suite: mutants T4 to T9, T14, T16, T26, T51, T59, T62, T70, T73 and T76 in the SQLite adapter, mutants T27 to T38, T60, T63, T71, T74 and T77 in the memory adapter, mutants T40 to T42, T52, T61, T64, T72, T75 and T78 in the Postgres adapter, mutants T17, T53 and T65 in the port.

## `arch-7h52` The memory store shall hold a bounded history, and what a service may forget shall not be a lifecycle

| What a store holds | SQLite and Postgres | Memory |
|---|---|---|
| state rows, read-model rows | all | all |
| facts | all | the last `Kept::facts` |
| recorded refusals | all | the last `Kept::refusals` |
| requests | all, marked done; a parked one also in `parked`, with its cause | until done, then none; the last `Kept::parked` parked ones, in `Memory::parked` |

On disk, history is what a read model is rebuilt from and what explains a state, so it is
kept whole and archiving it is an operation, not a code path. In memory a rebuild serves
nothing: a new read-model shape is new code, new code is a restart, and a restart empties the
store. So the memory store keeps a tail for diagnosis, and the composition root says how long
a tail in `Memory::new(Kept { .. })`. Positions keep counting past what was trimmed, and a read
of facts that reaches into the trimmed part is refused rather than answered short, so a
rebuild over a trimmed history fails where it would have produced a wrong read model.
`Memory::held` reports how much the store holds, for a service to publish.

State rows and read-model rows are not trimmed by any store. The domain bounds them: a
lifecycle is something the business counts (a task, a payment, a booking). What may be
forgotten, and whose keys the outside world can mint without limit (a session, a rate
limiter's window, a cache entry), is not a lifecycle (`arch-f5d`): it lives inside the
adapter that owns it, with that adapter's own expiry, and no decision reads it without it
first entering as a command (`arch-jbno`).

A parked request is a request nobody will perform, kept until someone has looked at it. On
disk it is never trimmed. In memory nobody can be counted on to look before the process has
run for weeks, so the store keeps the last `Kept::parked` and counts the ones it dropped in
`Held::forgotten`: a number above zero says requests were parked and are no longer there to
read.

Recorded refusals on disk grow with refused requests. Limiting how many a client may send is
the driving adapter's bound (`arch-kpb`), not the store's.

### Acceptance

- `arch-7h52.1` The memory store drops facts beyond the kept number and keeps counting positions: `facts_beyond_the_kept_number_are_dropped_and_positions_keep_counting` (mutants T47 and T50).
- `arch-7h52.2` A read that reaches into trimmed facts is refused: `a_read_of_facts_that_were_trimmed_is_refused` (mutant T48).
- `arch-7h52.3` The memory store drops the oldest refusals beyond the kept number: `refusals_beyond_the_kept_number_drop_the_oldest` (mutant T49).
- `arch-7h52.4` A request marked done is no longer held and is not claimed again: `a_done_request_is_no_longer_held` (mutant T32).
- `arch-7h52.6` The memory store drops the oldest parked requests beyond the kept number and counts them: `with_as_many_parked_requests_as_are_kept_none_is_dropped`, `parked_requests_beyond_the_kept_number_drop_the_oldest_and_are_counted` (mutant T101).
- `arch-7h52.5` Review checks that state with unbounded keys that may be forgotten is not modelled as a lifecycle (T4, gap ledger).

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

Connections come from a pool. A unit of work or a read checks one out for its duration, so a
read never waits behind a unit in the same process, and the pool's size bounds what the
service holds open on the server. Each connection names itself `store_<schema>` to the server.
A connection the server closed is dropped from the pool, and a unit or read that finds one
before it has run anything starts again on another.

The composition root chooses the durability, as it does for SQLite (`arch-0ev`):
`Durability::Full` waits for the disk on every commit; `Durability::Normal` does not, and a
server crash can lose the last commits but never leaves the database inconsistent. A disk
sync is most of what a durable command costs: on the machine these were measured on, one sync
took 4.2 ms and a durable command 5.6 ms, against 1.2 ms under `Normal`
(`docs/adr/0006-postgres-as-one-writer.md`).

The store port stays synchronous (`arch-v1q`), and the store can be opened, called and dropped
from any thread, as the SQLite store can. The blocking client runs a runtime of its own and
panics when called from a thread that is driving async tasks, so each pooled connection lives
on a thread of its own inside the adapter and a call hands it one statement and waits.

Without the lock, two units that interleave do one of three things, each observed
(`bench/results/2026-11-15-postgres-writer-lock-spike.txt`): refuse a valid fact as out of
order, move a read model's position backwards, or lose an update to a read-model row. The
last two raise no error.

### Acceptance

- `arch-m7du.1` Writers on separate connections leave the read model equal to the fold of the facts, with positions 1 to N: `many_writers_on_one_schema_keep_the_tally_equal_to_the_facts` (mutant T39).
- `arch-m7du.2` A read does not see a commit made after it began: `a_read_keeps_one_snapshot_while_another_writer_commits` (mutant T43).
- `arch-m7du.5` The store is opened, used through `spawn_blocking`, used as the relay's outbox and dropped inside a runtime: `the_store_may_be_opened_and_used_inside_a_runtime`, `a_unit_of_work_runs_through_spawn_blocking_on_a_multi_thread_runtime`, `the_relay_runs_on_a_runtime_of_its_own_with_the_store_as_its_outbox`, `the_store_may_be_dropped_inside_a_runtime`.
- `arch-m7du.6` A read runs while a unit is open in the same process, and a store whose connections the server closed answers the next command: `a_store_reads_on_one_connection_while_a_unit_is_open_on_another`, `a_store_whose_connections_the_server_closed_recovers` (mutant T44).
- `arch-m7du.7` A failed unit leaves no open transaction on the connection it returns to the pool (mutant T45).
- `arch-m7du.8` A store opened as `Normal` commits without waiting for the disk, and one opened as `Full` waits: `a_store_opened_as_normal_does_not_wait_for_the_disk`, `a_store_opened_as_full_commits_synchronously` (mutant T46).
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
