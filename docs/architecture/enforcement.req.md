---
prefix: arch
---

# Enforcement, tests and agent rules

Tiers, strongest first: T1 the compiler and Cargo (violating code cannot be expressed); T2 a
check on the crate graph (`hex-lint`, `effect-audit` manifest checks); T3 a source lint
(workspace lints, `clippy.toml`, the `effect-audit` scan, the comment scan); T4 review. A rule's
tier states what enforces it today. A claimed tier with no running gate is a defect in these
documents.

## `arch-ajd` Each gate shall run in one command, in seconds, and say how to fix what it flags

`just check` runs every gate below and must be green before a commit. Each recipe's doc line
names the rule it enforces.

| Recipe | Gate | Covers |
|---|---|---|
| `fmt` | `cargo fmt --check` | one layout, so a diff shows only the change |
| `clippy` | workspace lints, warnings denied | wildcard arms, `unwrap`, dropped outcomes, `unsafe` |
| `test` | `cargo test` | behaviour, through the real SQLite store |
| `roles` | `hex-lint` | the role matrix and the context axis |
| `purity` | `effect-audit --require-domain --strict` | no I/O, clock, async or unvouched dependency in the core |
| `comments` | a source scan | no comment except a doc comment or `fc-allow` |
| `allows` | a source scan | no `allow` attribute outside a `tests/` directory |
| `docs` | `guide-sync.bb` | every Rust block in these documents is code in the workspace |
| `ids` | `req check` | the requirement ids and their projection are current |

Mutation testing is not in `just check` and not in the working loop. The `mutation` job in
`.github/workflows/ci.yml` runs `just mutants` and `cargo mutants` after a push to `main` and
weekly; it never blocks a change ([`arch-h6j`](enforcement.req.md)).

### Acceptance

- `arch-ajd.1` Each gate goes red on a planted violation: mutants G1 to G9.

## `arch-4za` The workspace lints shall be set in the root manifest

`unsafe_code = "forbid"`, `unused_must_use = "deny"`, and clippy's `wildcard_enum_match_arm`,
`unwrap_used` and `expect_used` denied, with tests exempt: `clippy.toml` exempts test functions,
and an integration test file opens with `#![allow(clippy::expect_used, clippy::panic)]` for its
helpers. That attribute, in a file under `tests/`, is the one `allow` this repository accepts.

### Acceptance

- `arch-4za.1` Every build applies the lints (T3; mutant G6).

## `arch-tsu` Escapes shall go through the ledgers only

The sanctioned escapes are `hex-lint`'s exceptions file (per edge, with ticket and reason; a
stale entry fails) and `effect-audit`'s `// fc-allow: <reason>`. Never weaken a gate anywhere
else (an allow attribute, a lint level, a role relabel, a `pure-deps` entry, a weakened test or
mutant) to make a task pass; a gate change is its own task with a decision record.

A test is weakened when an assertion it made is removed or its expected value is changed to
match new behaviour nobody asked for. A test is not weakened by an edit a type change forces (a
new field in a struct literal, a new variant in a match), by a new assertion, or by a changed
expectation that the task itself asks for; name such edits in the commit. A mutant in
`mutants.py` is not weakened by updating its anchor text to follow code that moved; what it
plants and how it must die stay as they were. Each anchor must occur exactly once in its file,
so `just mutants` reports `ANCHOR FOUND n TIMES` when an edit moved or duplicated one.

### Acceptance

- `arch-tsu.1` The `hex-lint` run fails on a stale exception (T2).
- `arch-tsu.2` Review of any diff that touches `Cargo.toml` lints, `clippy.toml`, `justfile`, `mutants.py` or a `pure-deps` list (T4).

## `arch-ukt` Noisy measurements shall be ratchets, never hard gates

Build time, binary size and the benchmark's timings are compared with a committed baseline and
reported; they do not fail `just check`.

### Acceptance

- `arch-ukt.1` The benchmark writes its results beside the previous run's (`bench/results/`).

## `arch-yod` The gap ledger shall list every rule whose gate is not built

| Gap | Rule | Today |
|---|---|---|
| a lint that every public item of `domain` and `app` is a named primitive | [`arch-u8nl`](structure.req.md) | review |
| a check that a crate's role matches its shape | [`arch-4xd`](structure.req.md) | review |
| a check that every stored type appears in the stored-encoding test | [`arch-btw1`](core.req.md) | review |
| banned calls and purity in `app`, where policies and queries live | [`arch-zhm`](core.req.md) | `domain` only |
| tuple matches with a wildcard element | [`arch-j0k`](core.req.md) | review |
| a guard on edits that weaken a gate | [`arch-tsu`](enforcement.req.md) | review |

### Acceptance

- `arch-yod.1` Review checks that a rule added with a review-only criterion adds a row here (T4).

## `arch-844` Each layer shall be tested in its own form

- Core: given facts, when a command or a reaction, then facts or a refusal; one test per EARS clause, calling `decide` or `react` directly. A projection is tested as a fold: given a row and a fact, then a row.
- Stored encodings: one assertion per variant in `domain/tests/stored_encoding.rs`.
- Shell: use cases through `app::handle` against `Sqlite::in_memory()`, asserting on states, read models, the outbox and the rejections.
- Store adapter: its own tests for what only a file shows (a reopened file, a corrupt row).

A test's cyclomatic complexity is 1: no loop and no branch. Test zero, one and many.

### Acceptance

- `arch-844.1` The `test` recipe passes.

## `arch-h6j` Mutation testing shall run in continuous integration and never in the working loop

A prototype has to move; a test that kills no mutant still has to be found. So `just mutants`
(every architecture mutant) and `cargo mutants` (the core) run in the `mutation` job, after a
push to `main` and weekly, and their result is a report, not a gate on the change that
triggered it. A survivor, a moved anchor (`ANCHOR FOUND n TIMES`) or a mutant that no longer
compiles becomes a follow-up task: add the test, or update the anchor to the code as it now
reads. An equivalent mutant is named as such in that task.

### Acceptance

- `arch-h6j.1` The `mutation` job runs `python3 mutants.py` and `cargo mutants -p domain` and reports their results.
- `arch-h6j.2` No recipe that `just check` runs plants a mutant.

## `arch-7iz` Every rule shall map to a test, a mutant or a gate

A rule with no check is flagged in the gap ledger, not trusted. The mapping is checked
mechanically or by a human, never self-attested by the agent that did the work.

### Acceptance

- `arch-7iz.1` Review (T4).

## `arch-0bz` An agent shall stop and report when a rule blocks a task

Before adding a dependency, check the role matrix (`hex-lint explain <role>`). Before creating
a crate, give it a role and a context in the same commit. When a rule blocks the change, report
the rule's id and the alternatives; never violate it silently and never abandon the task
silently. Cite rules by id in commits and reports.

### Acceptance

- `arch-0bz.1` The `roles` recipe (T2); review of task reports (T4).
