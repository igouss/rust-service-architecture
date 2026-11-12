# Ceremony baseline: the task-list use case at the port shape of `e960d9f`

Counted on commits `474220e` (domain) and `313d34f` (adapter codec), `git diff --numstat 7cf2604 313d34f`.
`tokei` is not installed on this machine; lines are non-blank lines (`grep -c -v '^[[:space:]]*$'`).
The repository has no code comments, so non-blank lines are code lines. Types are `enum` and `struct`
items; `impl` blocks are lines starting with `impl`. The test rows are separate from the rows above them.

Flagged assumption: a task carries one plain `title: String`. The brief names no payload; a task
with no payload would make every codec body empty and the count smaller.

## Files

| File | Change | Lines added (all) | Non-blank |
|---|---|---|---|
| `domain/src/task.rs` | new | 275 | 227 |
| `domain/src/lib.rs` | `pub mod task;` | 2 | 1 |
| `adapter-sqlite/src/task.rs` | new | 59 | 50 |
| `adapter-sqlite/src/lib.rs` | `mod task;` | 1 | 1 |
| `adapter-sqlite/tests/task.rs` | new | 144 | 125 |
| **5 files (3 new, 2 changed)** | | **481** | **404** |

`ports` and `app` are untouched: `app::handle` and the port traits are generic over the lifecycle.

## By who writes it

| Part | Files | Types | `impl` blocks | Tests | Non-blank lines |
|---|---|---|---|---|---|
| Domain (states, commands, facts, refusals, projection) | 2 | 11 | 8 | | 124 |
| Adapter codec (fact, refusal, read-model row, table name) | 2 | 0 | 7 | | 51 |
| Tests, domain (given/when/then, projection) | same file as domain | 0 | 0 | 19 | 104 |
| Tests, adapter (round trips through the file) | 1 | 0 | 0 | 10 | 125 |
| **Total** | **5** | **11** | **15** | **29** | **404** |

Domain types: `Task`, `TaskEvent`, `AddTask`, `AddTaskOutcome`, `AddTaskRejection`, `CompleteTask`,
`CompleteTaskOutcome`, `CompleteTaskRejection`, `TaskRefusal`, `OpenTask`, `OpenTasks`.

Domain `impl` blocks: `From<AddTaskRejection> for TaskRefusal`, `From<CompleteTaskRejection> for
TaskRefusal`, `From<AddTaskOutcome> for TaskEvent`, `From<CompleteTaskOutcome> for TaskEvent`,
`Command for AddTask`, `Command for CompleteTask`, `Lifecycle for Task`, `Projection for OpenTasks`.

Adapter `impl` blocks: `Fact`, `Encode`, `Decode` for `TaskEvent`; `Encode` for `TaskRefusal`;
`Table` for `OpenTasks`; `Encode`, `Decode` for `OpenTask`.

## Not in the count

- The benchmark's state-row control needs a third mapping, `Snapshot for Task` (17 non-blank lines,
  `bench/src/state_row.rs`). It exists only for the control and no rule asks for it.
- No driving adapter exists in the workspace, so the use case has no HTTP or CLI mapping to count.
- No query handler over `OpenTasks` was written ("list open" is read through `ReadModel::get` in the
  adapter test); the port has no way to list rows, only to get one by key.
