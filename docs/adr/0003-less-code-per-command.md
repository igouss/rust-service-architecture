# 0003. Less code per command

Date: 2026-11-11

## Weakest part at the time

The cost of one use case. At tag `baseline-2026-11-11` the task-list use case (add, complete,
one derived view) took 404 non-blank lines in 5 files, 11 types and 15 `impl` blocks
(`bench/results/2026-11-11-ceremony-baseline.md`). An agent facing that cost for a small
feature shortcuts the form.

## Move, and what each cut gives up

| Cut | Protection given up | What covers it now |
|---|---|---|
| the generic decision container (`Decision`, `Request`) | none | `Command::decide` returns facts, so a request has nowhere to go (mutant C9); a transition is still built only in `ports` (C2, C3) |
| a per-command `Emits` enum and its `From` impl | a command emitting a fact outside its declared subset no longer fails to compile | one test per EARS clause; the mutation job in continuous integration |
| a per-command `Rejection` enum wrapped in a `Refusal` enum | the refusal no longer names which command refused | one `Refusal` enum per lifecycle whose variants name the reason |
| the `Observation` trait | "cannot refuse" was a trait | a command on a lifecycle whose `Refusal` is `Never` |
| hand-written row mapping in the adapter | the adapter owned the stored encoding | serde derive in `domain`, pinned by `domain/tests/stored_encoding.rs` (mutant T20) and `arch-btw1` |
| an expected version passed through the port | none on one writer | the decision's read and its write share the transaction |
| the in-memory fake and the port conformance suite | a second implementation to compare against | tests run against `Sqlite::in_memory()`; a suite returns with a second adapter |
| the `docs-check` crate of illustrative skeletons | none | every Rust block in the rule documents is code in the workspace (mutant G7) |

## What it preserved

Sum-type states, exhaustive matches, recorded refusals, pure decisions, the private fold.

## What would reverse a row

A defect of the kind that row's protection would have stopped, found in a service built on
this architecture.
