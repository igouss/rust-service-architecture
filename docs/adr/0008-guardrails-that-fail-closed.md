# 0008. Guardrails fail closed, and the port's errors and ids say only what is true

Date: 2026-11-15

## Operator decision

"address these issues. I approve all the changes. This is your project. You have a full
authority to modify and change it." (operator, 2026-11-15). It approves the gate, lint, test,
mutant and port changes named below, and nothing else.

## Weakest part at the time

The gates, not the rules. The architecture exists to steer an agent mechanically, and several
gates reported green after the property was lost: a new crate was scanned by no source gate,
two greps hid their own errors, the views and the event-name check skipped what they could not
parse, and `just check` passed with no Postgres case run. Five rules an agent breaks while
building a feature were checked by review only. In the port, `StoreError::Unavailable` covered
five conditions, ids were wider than any store's column, and the notify-after-commit rule was
written once per adapter.

## Forces in conflict

- A gate that can pass falsely is worse than no gate: both the operator and the agent stop
  looking where it reports green.
- A text scan is cheap and fails open; a type or a compiler lint fails closed and costs a
  signature. The mutants anchor on source text, so every signature that moves takes anchors
  with it.
- A guard must not decide design. Each check added here rejects a form (a comment, a wildcard,
  a trait outside the kernel, an unpinned stored type), never a choice of lifecycle, policy or
  adapter.

## Move

- Every text gate takes its crates from `cargo metadata` and fails when what it parsed differs
  from what it counted. A gate edit without a decision record fails `just guarded`.
- `Command::decide` refuses with the lifecycle's `Refusal`; the `Rejection` type is gone.
- `Reader::refusals` and `Store::committed` are on the port; `Journal::write` announces a
  commit once, after the unit has returned.
- `Version`, `Position` and `RequestId` hold at most `i64::MAX`. Counting saturates there;
  2^63 appends are out of reach, and the alternative was a fallible `after` at every site.
- `StoreError` has one variant for each thing a caller can do: `Unavailable`, `Misconfigured`,
  `Corrupt`, `Unencodable`, `OutOfOrder`, `Trimmed`.
- A SQLite read runs in a read transaction, so the session type carries its transaction for
  reads and writes alike. Measured on the benchmark's queries: `task_tally` 3.2 to 4.0 µs at
  10 tasks, the linear reads within run-to-run noise.

## What it preserved

The conformance suite's cases and every planted fault. Anchors followed the code; refusal
assertions became typed with the same expectations; no assertion was removed.

## What would reverse it

A caller that must tell two conditions apart inside one `StoreError` variant splits it again.
A store whose ids are not integers (a UUID log position) turns the three id types back into
opaque values and returns the range check to the adapter.
