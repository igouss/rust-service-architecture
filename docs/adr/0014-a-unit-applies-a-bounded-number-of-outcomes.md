# 0014. A unit of work applies a bounded number of outcomes; a runaway cascade is a named error

Date: 2026-11-23

## Operator decision

"Bounded policy cascades. `Policy::after` can call `react`, which settles and calls `after`
again. Find out what happens when two lifecycles' policies trigger each other, then make that
either unrepresentable or a named error. Report which you chose and why." (operator,
2026-11-23), under the approval of changes to `ports` for move 5. It approves the port, test
and mutant changes named below, and nothing else.

## What happened before

Found by running it (`app/tests/cascade.rs`, two lifecycles whose policies react to each
other): the calls nest until the thread runs out of stack.
`thread ... has overflowed its stack; fatal runtime error: stack overflow, aborting`, exit
status 134. Rust aborts the whole process on a stack overflow, so in `server` one such
command would take down every request in flight. Nothing is stored, only because the
transaction dies with the process.

## Forces in conflict

- Unrepresentable is the stronger rung. It needs the graph of "whose policy reacts on whom"
  to be acyclic in the types: a rank per lifecycle, checked where `react` is called. That
  forbids more than runaways. A payment that starts a fulfilment, whose completion settles the
  payment, is a cycle in that graph and ends after two steps because each step is guarded by
  state.
- Whether a cascade ends is a property of states and data, not of types.
- A stack overflow cannot be caught; a bound checked before each outcome can.

## Move: a named error

`Unit::apply` counts the outcomes a unit of work has applied and refuses the next one past
`Unit::OUTCOMES` (32) with `StoreError::Runaway { lifecycle, outcomes }`. `apply` is the one
way to change a lifecycle (`arch-s6q`), so the bound cannot be walked around, and it needs
nothing from the shell. The unit fails as any other: nothing is stored, the driving adapter
answers 500 and logs the lifecycle.

Why count outcomes and not nesting depth: depth is what overflows the stack, and every nested
call applies an outcome, so the count bounds the depth; it also bounds a wide fan-out, and it
lives in one place.

## Assumption flagged for the operator

The bound is 32. The longest cascade in the example applies two outcomes. A service whose
unit of work legitimately applies more raises the constant in its own decision record.

## What it preserved

Every existing unit of work applies far fewer outcomes; no test changed. Mutant C13 builds the
unit with the field it gained and still dies on the private fields (E0451). New: T81 and T82,
applied by hand and seen to die by the two boundary tests. `app` takes `serde` as a
dev-dependency for the test's own lifecycles.

## What would reverse it

A rank on `Policy` that makes the reaction graph acyclic becomes the better answer if the
operator decides that no flow may return to a lifecycle within one unit of work.
