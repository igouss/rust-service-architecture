# Native mutation of the four crates, 2026-11-11

`cargo-mutants 27.1.0` on the tree of commit `1ee214c` (`fc3ac10` plus the one test this record
names). Each crate was run alone, before the `bench` crate joined the workspace. `adapter-http`
was measured earlier at 0 missed and was not re-run.

| Crate | Command | Mutants | Caught | Missed | Unviable | Timeout |
|---|---|---|---|---|---|---|
| `domain` | `cargo mutants -p domain --no-shuffle` | 32 | 18 | 0 | 14 | 0 |
| `ports` | `cargo mutants -p ports --test-workspace=true --no-shuffle` | 19 | 3 | 0 | 16 | 0 |
| `app` | `cargo mutants -p app --test-workspace=true --no-shuffle` | 29 | 15 | 1 (equivalent) | 13 | 0 |
| `adapter-sqlite` | `cargo mutants -p adapter-sqlite --test-workspace=true --no-shuffle` | 57 | 24 | 0 | 33 | 0 |

`ports` has no tests of its own and `adapter-sqlite` is exercised mostly by `app/tests`, so the
three crates outside the core were run against the whole workspace's tests.

## First pass, on `fc3ac10`, before the added test

`domain` 18 caught, 0 missed, 14 unviable. `ports` 3 caught, 0 missed, 16 unviable. `app` 15
caught, 1 missed, 13 unviable. `adapter-sqlite` 23 caught, 1 missed, 33 unviable. No timeout.

## The missed mutant that is equivalent

`app/src/lib.rs:95:9: replace <impl Policy for Fulfilment>::after -> Result<(), StoreError> with Ok(())`

The body is an exhaustive match on `fact.event` in which every arm is `Ok(())`, and its unit
parameter is unused (`_unit`). The original is already the constant function the mutant puts in
its place: same value for every input, no effect. No test can tell them apart. It stops being
equivalent the day a fulfilment fact gets a reader.

## The missed mutant that was a gap

`adapter-sqlite/src/lib.rs:359:9: replace <impl View for SqliteView<'_>>::rows -> Result<Vec<(String, P::Row)>, StoreError> with Ok(vec![])`

The only caller of `View::rows` in the workspace is a test that asserts an empty list after
`clear`. Killed by `a_put_row_is_listed_under_its_key` in `adapter-sqlite/tests/store.rs`, which
puts one tally row and reads it back through `rows`; the second pass reports the mutant caught.
`TaskTally` has one key, so "many" rows cannot be written with the projections this workspace
holds.

## What the counts do not say

Unviable means the compiler rejected the mutant, so nothing was tested. In `ports` that is every
mutant of `Transition::fold`, `Transition::state`, `Transition::into_parts`, `decide`, `react`,
`Change::into_parts` and `project`: their return types have no `Default`, and cargo-mutants
generated no other mutant inside them. The three caught `ports` mutants are all of `key`. Native
mutation therefore says nothing about the other `ports` functions; `mutants.py` plants its own
(T1 for the fold).
