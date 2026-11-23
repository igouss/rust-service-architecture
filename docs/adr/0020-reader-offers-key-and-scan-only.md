# 0020. The `Reader` port offers lookup by key and a full scan, and nothing between

Date: 2026-11-23
Status: proposed

## Weakest part

A query over many records. `Reader` (`ports/src/store.rs:29-35`) has `state` and `row` by key
(`:30`, `:32`) and `states` and `rows`, which visit every record of a lifecycle or read model
(`:31`, `:33`; SQLite: `... WHERE lifecycle = ?1 ORDER BY stream` at
`adapter-sqlite/src/lib.rs:167`). `open_tasks` decodes every task and drops those not open
(`app/src/tasks.rs:12-21`). `payments` decodes every payment, then looks up one fulfilment
per payment (`app/src/payments.rs:35-44`). Neither can ask for a page. The README measures
the scan at about 0.37 µs per row, 40 ms at 100 000, and says a list past a few thousand rows
needs a projection or pagination (`README.md:120-122`).

## Option A: keep the port; a query that must be selective reads a projection

Each such query gets a read model holding only what it shows, kept in the same unit
(`app/src/tasks.rs:6-10` does this for the tally) and read by key or scanned whole.

Gives up: a read model per query shape, its rebuild, and its stored-encoding pins; `rows` is
still a full scan, so a projection bounds the size but not the page. An ad hoc query still
reads everything.

## Option B: widen `Reader` with a bounded read, such as rows after a key, at most a limit

Gives up: a port signature change (for the operator), carried by all three store adapters and
the conformance suite. The adapters implement the port over JSON text (ADR 0004, Move), so the bound
can be on the key and not on a field inside the row; a filter on content still needs a projection whose key is
that field. Each new read form is one more clause every store must honour.

## Evidence that would decide

- The expected record counts per lifecycle and per read model in a service built from this. Under
  a few thousand, the README's figure says a scan is cheap.
- `payments` at 10 000 and 100 000 payments; it adds a key lookup per row, and
  `bench/src/queries.rs` times only `open_tasks`, `task_tally` and `task_board`.
- Whether a user-facing list needs pages. If it does, Option A alone does not provide them.
