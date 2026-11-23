# 0028. A unit of work bounds the visits to one stream, and all outcomes by a wider number

Date: 2026-11-23

## Operator decision

"Okay, don't go easy on those problems. Let's fix them all." (operator, 2026-11-23), in answer
to a review that named this as its fifth item. It approves the change to `Unit::apply`, its
tests and mutants. The two numbers are not the operator's; they are flagged (`cm-vph`).
Nothing else is approved. This record amends [0014](0014-a-unit-applies-a-bounded-number-of-outcomes.md).

## Weakest part at the time

The bound of record 0014 counted every outcome of a unit against one number, 32, and said so:
"it also bounds a wide fan-out". A cycle and a fan-out are different things. Seen red: one fact
whose policy told forty other streams failed as `Runaway { outcomes: 32 }`, though nothing
in it repeats and nothing in it is deep.

## Forces in conflict

- What overflows the stack is nesting, and `Unit` cannot see nesting: the recursion is in
  `app`, and `apply` returns before the policy that causes the next `apply` runs.
- A depth the caller reports is a declaration (`cm-clt`); the count in `apply` is not, since
  `apply` is the one way to change a lifecycle (`arch-s6q`).
- A cycle returns to a stream. A chain over fresh ids never does, and still nests.

## Move

`Unit::apply` keeps two counts. Visits to one stream of one lifecycle are bounded by
`Unit::VISITS` (32): a cycle hits it early and the error names the lifecycle that came round
again. All outcomes of the unit are bounded by `Unit::OUTCOMES` (1024): that is what still
stops a chain that never repeats before the stack does, and it is also the widest fan-out.
Both live in `apply`, so neither can be walked around. `StoreError::Runaway` is unchanged;
`outcomes` holds whichever count was reached.

Set aside: a depth handed in by `app` (a declaration); making `Applied`'s facts reachable
only through a scope that counts depth (it would take the facts away from every test that
reads them, for a bound the count already gives).

## What it preserved

The four tests of record 0014, with their expected values: two policies that trigger each
other still end at `outcomes: 32`, and a chain on one stream is stored at 32 and refused at
33. T81 and T82 keep their faults and now anchor on the bound on all outcomes; T98 to T100
plant the same faults on the visits. C13 builds the unit with the field it gained.

## Flagged assumptions

- 1024 outcomes in one unit of work, and so in one transaction. A cascade of 1024 nested
  policies ran in a debug build on a test thread (2 MB of stack) without overflowing; nothing
  measures the margin, and a policy with large frames has less.
- 32 visits to one stream.

## What would reverse it

A unit of work that drives its cascade from a queue, not by recursion: the stack stops being
the hazard, and only the visits need a bound.
