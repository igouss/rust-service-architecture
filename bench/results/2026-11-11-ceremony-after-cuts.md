# Ceremony after the cuts: the task-list use case

Same counting as `2026-11-11-ceremony-baseline.md`: non-blank lines, `enum` and `struct` items,
lines starting with `impl`. Counted on the commit that adds this file. The code was reformatted
with `rustfmt` at width 160 between the two counts, which the baseline was not; treat a
difference of a few lines as layout.

| Part | Baseline | After | Where |
|---|---|---|---|
| Domain: non-blank lines | 124 | 93 | `domain/src/task.rs` above its tests |
| Domain: types | 11 | 7 | |
| Domain: `impl` blocks | 8 | 5 | |
| Adapter codec: lines | 51 | 0 | serde derive in `domain` |
| Adapter codec: `impl` blocks | 7 | 0 | |
| Policy naming the readers | not expressible | 5 | `impl Policy for Task` in `app/src/lib.rs` |
| Queries (open tasks, tally) | not expressible | 15 | `app/src/lib.rs` |
| Stored-encoding assertions | none | 3 tests | `domain/tests/stored_encoding.rs` |
| Files a new use case touches | 5 | 4 | lifecycle file, `domain/src/lib.rs`, `app/src/lib.rs`, the encoding test |

Production code for the same behaviour: 175 lines (124 + 51) before, 98 (93 + 5) after, and the
after figure includes a policy the baseline could not express. Types 11 to 7; `impl` blocks 15
to 6.

Counted before the refined `Title` type was added to `domain/src/task.rs`; that type and its
error add lines the baseline never had, because the baseline accepted any string.

Not the same use case in one respect: the baseline's derived view was "one row per open task";
after the cuts that list is a query over state rows, and the projection is a tally across
tasks, because a projection is now only for a view no single row holds.
