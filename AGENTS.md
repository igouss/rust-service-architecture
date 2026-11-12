# For agents working in this repository

This repository is a reference architecture for a Rust service: a pure core, one unit of work
per command, SQLite as the store. You are either extending it or building a service from it.

## The three things to know

1. **Run `just check` before and after every change.** It takes seconds and runs every gate:
   formatting, lints, tests, the role matrix, core purity, the comment scan, docs in step with
   code, ids, views. Fix the code, never the gate; `docs/architecture/enforcement.req.md`
   (`arch-tsu`) says which edits to a test or a mutant are legitimate.
2. **A feature is a new instance of a primitive, never a new kind of thing.** Lifecycle,
   Command, Reaction, Projection, Policy, Query, Port and adapter. The worked example is
   `domain/src/task.rs` with its policy and queries in `app/src/tasks.rs`.
3. **When a rule blocks you, stop and report the rule id.** Do not add an `allow`, a comment, a
   second write path, a dependency or a crate to get past it.

## Where to read

| You need to | Read |
|---|---|
| start a new service from this repository | `docs/how-to/start-a-service.md` |
| add a feature | `docs/how-to/add-a-use-case.md` |
| understand a rule a gate named | `docs/architecture/` (ids look like `arch-s6q`; `grep -rn 'arch-s6q' docs/architecture`) |
| see the shape before editing | `docs/views/crates.md`, `docs/views/state-machines.md` |
| know why it is this way | `docs/adr/` |
| know the standing rules for how to work (verification, tests, commits) | `CLAUDE.md` |

## Conventions the gates enforce

- No comments. Names and types carry the meaning; a contract goes in a doc comment as EARS.
- Explicit types on signatures, fields, constants, and locals whose type is not on the right-hand side.
- No wildcard arm on an enum; no `unwrap` or `expect` outside tests; no `unsafe`.
- The core (`domain`) does no I/O, reads no clock, runs no async, and depends on `serde` only.
- Tests have no loops or branches; cover zero, one and many.
- Commits: `<scope>: <what>; <why>`. One move per commit, `just check` green at each.
- Keep every source and document file under 20 KB, so it can be read whole; split by area.

## Mutation testing is not your job

`just check` is the whole working loop. Mutation testing (`just mutants`, `cargo mutants`) runs
in continuous integration, after the fact, and never blocks a change. Do not run it while
working, and do not wait for it. When the mutation job reports a survivor or a moved anchor, a
follow-up task fixes it; [`docs/architecture/enforcement.req.md`](docs/architecture/enforcement.req.md)
(`arch-h6j`) says how.
