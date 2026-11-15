# 0006. Postgres runs one unit of work at a time, behind an advisory lock, on pooled connections that each have a thread

Date: 2026-11-15

## Weakest part at the time

The claim that a database reached over the network reverses ADR 0001. Some services built on
this architecture will run on Postgres, and ADR 0001 names "a second writer process or a
networked database" as what sends readers back to positions in a feed. Nothing had measured
what Postgres actually breaks.

## Forces in conflict

- Postgres lets transactions interleave. A spike ran the unit of work as plain SQL in two
  sessions (`bench/results/2026-11-15-postgres-writer-lock-spike.txt`): a valid fact was
  refused as out of order; a read model's position moved backwards; a read-model row lost an
  update. The last two raised no error.
- The usual answer is optimistic: an expected version on every write, a conflict error, a
  retry. That needs a unit of work that can run twice, so `Command::decide(self)` and
  `Journal::write`'s `FnOnce` would both change, and it does not order read models: two
  transactions that touch different streams still race on the same read-model row and on its
  position.
- A service picks Postgres to share a database, to use its tooling, or to run more than one
  process; not, usually, because one writer is too slow.

## Move

`adapter-postgres` takes `pg_advisory_xact_lock`, keyed by the schema, as the first statement
of every unit of work. Units on one schema run one after another across every process, so
every invariant holds exactly as on SQLite and the port did not change: no expected version,
no conflict, no retry. Reads run on their own connections in read-only `REPEATABLE READ`
transactions and never wait for the writer. Positions are assigned under the lock as one more
than the highest stored. The client is the blocking one; ADR 0002 stands.

Connections come from an `r2d2` pool: a unit or a read checks one out, so reads do not queue
behind a unit, a connection the server closed is replaced, and the pool's size is the
service's footprint on the server. A first version kept one writer connection and a fixed
ring of readers behind mutexes; it could not replace a dead connection and made a reader wait
for a busy one while others were idle.

Each pooled connection lives on a thread of its own, and a call hands it one statement and
waits for the answer. The blocking client is the async client with a private runtime, and it
panics when called from a thread that is driving tasks: with the client called directly,
`app::relay` panicked under `Runtime::block_on`, and dropping the store inside a runtime
aborted the process. Passing the conformance suite had shown neither, because the suite runs
on plain threads.

Considered and set aside: an expected version with retry (above); a lock per lifecycle, which
lets commands on different lifecycles interleave but deadlocks when policies cross lifecycles
and still serialises the common case.

## Evidence

- The spike, with the lock: the three interleavings and a same-stream race all end with the
  tally equal to the facts. Over 8 s runs, facts, tally and position agree to the last row.
- Its cost, same spike: under `synchronous_commit=on`, 184 units a second with the lock against
  208 without (8 clients); under `off`, 1 007 against about 2 950. The unlocked runs ended with
  a tally of 201 for 1 675 facts and 5 508 for 23 589.
- The adapter passes the 38 cases of the conformance suite. Four writers on separate
  connections leave the tally equal to the facts with positions 1 to 100 (mutant T39 removes
  the lock and the test fails). A read does not see a commit made after it began (mutant T43).
- `app::handle(AddTask)` through the adapter, 500 commands on one connection over TCP
  loopback, test profile: 5.6 ms p50 with `synchronous_commit=on`, 1.2 ms with `off`. The
  SQLite adapter on the same machine: 3.9 ms under `Durability::Full`, 59 µs under `Normal`.
  A durable commit costs about the same on both; without one, Postgres pays for a round trip
  and a thread hand-off per statement, about ten of each per command.
- With the server's connections closed under it, the store answered the next command in each of
  40 runs. Before a fatal server error counted as a dead connection, it failed one run in
  three.
- One disk sync on that machine: 4.2 ms (`pg_test_fsync`, 236 a second). A durable command
  is one sync plus about a millisecond, on Postgres and on SQLite alike, so the adapter takes
  the same `Durability` choice SQLite has. Sharing one sync between waiting commands is the
  move after that, when a service needs more durable commands a second than one sync each.
- Not measured: the adapter under concurrent load, or any disk but this one.

## What it preserved

ADR 0001 whole: state rows as truth, one transaction per command, policies in the unit.
The port, the shell and the core, unchanged by this move.

## What would reverse it

A measured need for more committed commands a second than one writer gives on the service's
own hardware. The next move then is to shorten the unit (send its statements in one round
trip) before giving up the single writer; only after that, per-lifecycle ordering with read
models fed from positions, which is the design ADR 0001 left.
