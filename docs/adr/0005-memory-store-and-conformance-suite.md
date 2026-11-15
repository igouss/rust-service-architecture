# 0005. An in-memory store adapter, and one conformance suite every store adapter runs

Date: 2026-11-15

## Weakest part at the time

The store for a service with no database. Such a service still has state that other requests
see (sessions, a job board, a cache that decisions read), and the only store was SQLite, in a
file or in its own in-memory mode. ADR 0003 removed the in-memory fake and the conformance
suite because there was one adapter; its row says a suite returns with a second.

## Forces in conflict

- A second adapter can drift from the first in ways no type shows: a unit that fails but
  keeps a write, a unit that cannot read what it just wrote, a cleared read model that keeps
  rows.
- A suite written against the example lifecycles has to be ported by every service that
  replaces them.
- Test code that panics may not live outside a `tests/` directory (`arch-tsu`), and a crate
  for it is one more boundary.

## Move

`adapter-memory` implements `ports::store` over ordered maps behind a read-write lock. A unit
of work writes into a pending set that its own reads consult and that is applied only when the
unit returns `Ok`, so a failed or panicking unit stores nothing.

`ports/tests/conformance/suite.rs` holds the cases, over lifecycles defined in the file. Each
adapter's `tests/conformance.rs` names the adapter and includes it. The cases that existed in
`adapter-sqlite/tests/store.rs` moved there with their assertions, rewritten over the suite's
lifecycles; that file keeps the reopened file and the corrupt row.

## Evidence

- 38 cases pass against SQLite and against memory.
- Twelve faults planted in the memory adapter each fail a case (mutants T27 to T38); the
  SQLite mutants T4 to T9, T16, T17, T21, T25 and T26 are now killed by cases of the suite.
- The `raw-store` recipe fails when a store adapter does not include the suite (mutant G11).

## What it preserved

The six-crate start: a service keeps one store adapter and deletes the other.

## What would reverse it

A store whose contract cannot be asked through `Journal` and `Outbox`; then the suite needs a
second entry point, and that is the moment to look at whether the port is still one port.
