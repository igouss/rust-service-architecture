# 0018. `evolve` spells out every pair, and each transition is written twice

Date: 2026-11-23
Status: proposed

## Weakest part

The cost of adding a variant to a lifecycle. `evolve` is one `match (self, event)` over every
pair (`arch-bim`, `arch-j0k`): clippy denies a wildcard arm, `lint.py:65` rejects a `_`,
`..` or bare name in a tuple, and `views.py:46-48` refuses a lifecycle whose arms miss a pair.
Most pairs hand the state back: `Fulfilment` has 20 pairs and 5 transitions
(`domain/src/fulfilment.rs:80-84`, identity arms `:85-91`), `Payment` 12 and 3
(`domain/src/payment.rs:98-104`), `Task` 6 and 2 (`domain/src/task.rs:133-137`).

Each transition is also written twice. `react` matches `(Reserving, StockReserved)` and emits
`ShipmentRequested` (`domain/src/fulfilment.rs:113-116`); `evolve` takes
`(Reserving, ShipmentRequested)` to `Shipping` (`:81`). `Transition::fold`
(`ports/src/lib.rs:85-88`) is the only caller of `evolve` outside tests; facts are not
replayed (ADR 0001, lines 24-27).

## Option A: keep the full match, and each transition in both places

Gives up: authoring cost that grows as states times events, and two statements of one
transition that only tests hold together. Keeps: a new variant does not compile until each of
its pairs is decided; one function draws the view; and "a stored state that cannot disagree
with its facts, because the only constructor of a transition is the private fold" (ADR 0001,
lines 48-49).

## Option B: `decide` and `react` return the next state with the facts; `evolve` goes

Gives up: the fold, so a fact and a state chosen side by side can disagree with nothing to
check them, and no state can be rebuilt from its facts. `views.py` would read `decide` and
`react` instead. `Lifecycle`, `Step`, `arch-h1j` and `arch-bim` change: a domain-type change
for the operator.

## Evidence that would decide

- Lines touched per new variant: 5917106 added two `Progress` variants, one `evolve` arm,
  and both variants to `react`'s do-nothing arms (`domain/src/fulfilment.rs:127-141`).
- The number and size of lifecycles in a service built from this.
- Whether any lifecycle will be re-folded from its facts. If so, Option B removes the function
  that would do it.
