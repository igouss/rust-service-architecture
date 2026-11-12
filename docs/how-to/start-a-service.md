# Start a service from this repository

The repository is two things in one tree: fixed machinery you keep, and an example you
replace. Keep the example compiling until your first lifecycle works end to end; it is the
form you are copying.

## What you keep, and what you replace

| Path | What to do |
|---|---|
| `ports/src/`, `adapter-sqlite/src/` | keep, byte for byte |
| `app/src/lib.rs`, `domain/src/lib.rs` | keep; only their `pub mod` lines change |
| `clippy.toml`, `rustfmt.toml`, the root `Cargo.toml`, `views.py`, `guide-sync.bb`, `CLAUDE.md`, `.req/`, `requirements.toon`, `LICENSE-*`, `.gitignore` | keep |
| `domain/src/task.rs`, `payment.rs`, `fulfilment.rs`; `app/src/tasks.rs`, `payments.rs` | replace with your lifecycles, policies and queries |
| `app/tests/use_cases.rs`, `adapter-sqlite/tests/store.rs`, `domain/tests/stored_encoding.rs` | port: keep every assertion's intent, swap the example types for yours |
| `adapter-http/src/lib.rs`, `adapter-http/tests/routes/` | keep `blocking`, `Changes`, the frame encoding and the request span (rename its `task` field); replace the routes, the page and the renderers |
| `server/`, `justfile` | keep; rename the `TASKS_*` settings and the default database file |
| `mutants.py` | keep the machinery mutants; re-plant the example ones (below) and update `CRATES` and `EXAMPLE` |
| `docs/architecture/`, `docs/how-to/add-a-use-case.md`, `AGENTS.md` | keep the rules; re-point every pointer at an example file and every cited example test to yours (`just check` names a citation that no longer exists) |
| `docs/adr/` | keep: they say why the architecture is this way; number your own after them |
| `bench/` | port it to your commands and queries, or delete it together with its workspace member line, its entry in `mutants.py`'s `CRATES`, and its `justfile` recipe |
| `README.md`, `docs/reviews/`, `docs/how-to/start-a-service.md` | replace the first, delete the other two |

## Steps

1. Copy the tree into a new repository. In every `Cargo.toml`, change `context = "example"` to
   your bounded context's name; `hex-lint` requires every crate to carry one.
2. Add your first lifecycle beside the examples, following
   [`add-a-use-case.md`](add-a-use-case.md), until `just check` is green with both present.
3. Delete the example files in the right-hand column, their `pub mod` lines and their routes.
   The compiler names what still refers to them.
4. Leave `mutants.py` for the mutation job in continuous integration. Its first run after the
   examples are gone reports every mutant whose anchor left with them; the section below says
   what to do with each. This does not block building the service.
5. Choose the durability in `server`: `full` syncs to disk on every commit; `normal` survives a
   process crash, can lose the last commits on power loss, and commits about a hundred times
   faster. The default is `full`.
6. If a process manager of yours issues requests, write a `Gateway` for each receiver and run
   `app::relay` on a thread of its own ([`arch-w0v`](../architecture/shell.req.md)); route each
   result back through `app::advance`.

## The mutants after the examples are gone

A follow-up task, started from the mutation job's report, not part of starting the service.

`python3 mutants.py --list` marks every mutant `machinery` or `example`.

- A **machinery** mutant is planted in code you kept. It was killed by the example's tests, so
  it must now be killed by yours. Do not delete one that survives: it is telling you that your
  tests do not exercise that guarantee (a refusal being recorded, a failed unit storing
  nothing, a policy being run). Add the use-case test that kills it. The outbox and relay
  mutants (T14, T15, T16, T19, T26) and the failed-unit mutant (T9) need a `Reaction`; if your
  service has no process manager yet, keep them alive with a small lifecycle and reaction
  defined inside the test file, not by turning one of your aggregates into a process manager.
  A test may take `serde` as a dev-dependency for that.
- An **example** mutant is planted in code you deleted, so its anchor is gone. Re-plant the
  same fault in your own code: a lifecycle with no policy (C8), a command returning a request
  (C9), a policy skipping its projection or its process manager (T10, T11), a stored type
  changing its encoding (T20), a refined type accepting what it must refuse (T22 to T24), a
  wildcard arm (G6), and the driving adapter's faults (H1 to H9). Changing a mutant's anchor
  and replacement to plant the same fault in new code is a legitimate edit
  ([`arch-tsu`](../architecture/enforcement.req.md)); dropping the fault is not. If your
  service has nothing the fault applies to (no process manager, so no T11), remove that mutant
  and say so in the commit. Keep the `EXAMPLE` set in `mutants.py` in step with what you
  re-planted and added.

## Not provided

Schema migrations, authentication, a scheduler adapter, a running relay and a published wire
format are not built. The README's "Not built" list is the current state.
