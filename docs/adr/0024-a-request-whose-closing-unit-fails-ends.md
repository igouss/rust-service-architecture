# 0024. A request whose closing unit fails also ends

Date: 2026-11-23

## Operator decision

"Okay, don't go easy on those problems. Let's fix them all." (operator, 2026-11-23), in answer
to a review that named this defect first. It approves the fix, `Unit::strand` in `ports`,
the tests and the mutants below. One thing it does not settle is flagged under "What it
gives up" (`cm-vph`). Nothing else is approved.

## Weakest part at the time

The relay's handling of its own store. Record [0010](0010-outbox-failures-end.md) made a
failing gateway end; a failing unit did not. `app::relay` returned with `?` when the unit
that applies a reply failed, before any attempt was counted. Seen red: a reply whose reaction
ran into `StoreError::Runaway` was performed on four passes out of four, each pass returned
the error, and the request stayed claimable with `attempts: 0`. In `server` that is a call
to the gateway twice a second for ever, and since requests are claimed in order of attempts
and id, nothing behind it in the batch was reached.

## Forces in conflict

- A request must end (`arch-22kr`), and ending means telling its lifecycle.
- The lifecycle is told by applying a reaction, and applying the reaction is what fails.
- A store that is down is not a property of the request: counting attempts against it would
  park every open request during an outage of the store.
- The reply is not stored, so trying the unit again means calling the gateway again
  (`arch-fif` already allows it: the receiver drops a second copy).

## Move

`relay` sorts the error of a closing unit by what can be done about it, as `StoreError`'s
variants are meant to be read. `Unavailable` and `Misconfigured` end the pass. Any other
error counts as a failed attempt for a reply below the bound, and the pass goes on. At the
bound, or when the failing unit was already the one that parks, the request is parked by
`Unit::strand`: `Writer::park` with the error as the cause, and no reaction. The match names
every variant, so a new `StoreError` does not compile until it is put on one side.

No raw-store method changed; `strand` is `Writer::park` without `apply`.

## What it preserved

Every existing test, and every mutant's fault. The anchors of T15 and T66 follow the relay's
new lines (the commit after the move: the move itself left both anchors found nowhere); each
was planted through the runner again and dies by the tests it died by. A reply whose unit succeeds, an unavailable gateway and a
rejected request behave as before.

## What it gives up

"Parked with its lifecycle told" no longer holds for every parked request: a stranded one
ends with its lifecycle still waiting, and only the `parked` row says so. That narrows
`arch-22kr`, and the narrowing is an assumption of this record, not the operator's words.
The alternative was a request that never ends. Nothing alerts on a stranded request beyond
the row; a service that needs to know reads `parked`.

## What would reverse it

A reply stored before it is applied, so that a failed unit can be retried without the
gateway; or a lifecycle-level "could not be told" that does not go through the failing
reaction.
