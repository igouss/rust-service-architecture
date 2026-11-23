# 0019. An event that does not apply is ignored by `evolve` without a word

Date: 2026-11-23
Status: proposed

## Weakest part

`arch-h1j` (`docs/architecture/core.req.md:117`): "An event that does not apply returns the
state unchanged." Each lifecycle states it (`domain/src/fulfilment.rs:77`,
`domain/src/payment.rs:95`, `domain/src/task.rs:130`) and tests pin it
(`domain/src/fulfilment.rs:272-274`, `domain/src/payment.rs:194-196`).

Outside tests, `evolve` folds only the events `decide` or `react` just returned
(`ports/src/lib.rs:85-88`, called at `:109` and `:116`). An identity arm is reached there only
when they emit an event their own state cannot take: a defect. `Unit::apply` then appends the
fact and saves the unchanged state (`ports/src/lib.rs:233-245`), so the history records what
the state does not show. Read by hand, every pair `react` and `decide` emit from has a real
arm (`domain/src/fulfilment.rs:109-126`; `domain/src/payment.rs:57`, `:75-78`;
`domain/src/task.rs:96`, `:112`). Not verified by a run.

## Option A: keep `evolve` total

Gives up: that defect is silent unless a test asserts the state after the command. Keeps:
`evolve(self, &Event) -> Self`, an infallible fold, no new error in the unit.

## Option B: `evolve` says when an event does not apply, and the unit does not commit it

For example `-> Result<Self, Inapplicable>`, with `Transition::fold` and `Unit::apply`
failing the command. Gives up: totality and the `Lifecycle` signature (a domain-type change
for the operator); every path through `Unit` gains an error a caller can only report; the
identity arms of 0018 become error arms, so the enumeration stays.

## Evidence that would decide

- A mutant that makes `react` emit `Completed` from `Reserving`: if no test fails under
  Option A, the silence is real.
- Instrumented identity arms under `cargo test` and the benchmark: a hit from `decide` or
  `react` refutes the reading above.
- Whether facts will ever be replayed. An event recorded under an older rule may then
  legitimately not apply, and Option B needs a second mode for replay.
