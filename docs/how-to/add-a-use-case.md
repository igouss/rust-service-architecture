# Add a use case

A use case is a command on a lifecycle, plus whatever reads the facts it produces. Copy the
form of `domain/src/task.rs`; it is the worked example for every step. Run `just check` after
each step; it takes seconds.

## 1. Write the contract first, in EARS

One clause per behaviour, in the doc comment of the command, before any body:

- "While the task is `Open`, when `CompleteTask` is decided, `decide` shall emit `Completed`."
- "If the task is `Absent`, then `decide` shall refuse with `Absent`."
- "If the task is `Done`, then `decide` shall refuse with `AlreadyDone`."

Each clause becomes one test, named after it. That is the test budget: a test per clause, not
a search for more cases. Move on when `just check` is green.

## 2. Core: `domain/src/<lifecycle>.rs`

A new command on an existing lifecycle:

1. Add the command `struct` and `impl Command` for it. `decide` returns `Ok(vec![facts])` or
   `Err(refusal)`.
2. If it produces a new fact, add the variant to the lifecycle's event `enum`; the compiler
   then lists every `match` that must handle it (`evolve`, each projection, the policy).
3. If it can refuse for a new reason, add the variant to the lifecycle's refusal `enum`.
4. Add one test per EARS clause in the file's `tests` module: `given(&[facts])`, call
   `decide`, compare.
5. Add the new variants to `domain/tests/stored_encoding.rs`, as new assertions.

A new event or refusal variant is not contained in `domain`: the compiler names every
exhaustive `match` on it, in `app` (the policy) and in the driving adapter (the refusal's name
and status). Fix those in the same commit, so the commit builds.

A new lifecycle is the same in a new file: state `enum`, event `enum`, refusal `enum` (or
`Never`), `impl Lifecycle` with a `NAME` no other lifecycle uses, then `pub mod` it in
`domain/src/lib.rs`.

Time, ids and random values are fields of the command; the adapter supplies them
([`arch-fh9`](../architecture/core.req.md)).

## 3. Shell: `app/src/<area>.rs`

`app/src/lib.rs` is the fixed machinery; do not add to it. Policies and queries go in a module
per area, as `app/src/tasks.rs` and `app/src/payments.rs` do.

1. A new lifecycle needs `impl Policy for <Lifecycle>`; without it `app::handle` does not
   compile for its commands. An empty policy matches every event and returns `Ok(())`.
2. To keep a derived view, write an `impl Projection` in the lifecycle's file and call
   `project::<U, TheView>(unit, fact)` from the policy. `apply` sees only the row and the
   fact, so a fact must carry whatever its views need to fold it; decide that before the
   fact's encoding is first stored.
3. To start or advance a process manager, call `react(unit, &its_id, TheProgress)` from the
   policy arm of the fact that causes it.
4. A query is a function over `journal.read(|view| …)`, reading state rows or a view's rows.
5. Add use-case tests in `app/tests/use_cases.rs` through `handle` against
   `Sqlite::in_memory()`: the answer, the stored state, each view, the outbox, the rejections.

Nothing in `ports` or `adapter-sqlite` changes for a new use case.

## 4. Driving adapter: `adapter-http/src/lib.rs`

1. Add the route in `router`. Its handler parses the path and body into the command, refusing
   malformed input with 400 before a command exists, then calls `app::handle` through
   `blocking`, which runs it off the async executor.
2. Map the `Answer` exhaustively: accepted is 204, a refusal is 409 with its name. Record the
   route, the command and the outcome on the request span; never log user text.
3. The page is not rendered by the route. `GET /events` sends the whole board as a
   `datastar-patch-elements` frame at once and again after every commit, so a new thing to show
   goes in the query the frame reads and in the function that renders it. Escape user text.
4. Tests go in `adapter-http/tests/routes/`, one file per area, one behaviour per test:
   each status, the frame's markup, and that a commit produces a new frame.

## 5. Before you call it done

- `just check` is green.
- `just views` if you added a lifecycle or changed `evolve`; commit the regenerated files.
- A lint you are tempted to silence is fixed instead: `just check` rejects an `allow`
  attribute outside `tests/`.

## When to stop and ask

Stop and report, with the rule id, instead of working around it when the change needs any of:
a new method on a port, a new crate, a dependency in `domain` or `ports` (one inside an adapter,
the composition root or a test is yours to choose and to name in the commit), a second way to
write (`arch-s6q`), a
comment, an `allow` attribute, a change to the lints or to a `pure-deps` list, removing or
renaming an existing variant of a stored type (`arch-btw1`), or weakening a test
(`arch-tsu` says exactly what counts as weakening and what does not; `arch-0bz`).

Adding a variant, a command or a lifecycle for the feature you were asked for is the expected
path, not a reason to stop.
