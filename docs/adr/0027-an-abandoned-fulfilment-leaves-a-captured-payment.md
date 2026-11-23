# 0027. An abandoned fulfilment leaves a captured payment, and stock that may be reserved

Date: 2026-11-23

Status: proposed. Nothing is implemented; the choice is the operator's.

## Operator decision

None yet. "Let's fix them all" (operator, 2026-11-23) answered a review that named this
under its first item. Fixing it means new stored states and events of `Payment` or
`Fulfilment` and, in one option, a second request path: persisted state the task did not
name and a domain policy nobody stated (`cm-vph`). So this record lays out the choice and
builds nothing.

## Weakest part

The end of the example's process. `Fulfilment` reaches `Abandoned` when stock is
unavailable, when a request is rejected, or when it stayed undelivered past the last wait of
record [0025](0025-a-failed-request-waits-before-it-is-tried-again.md). `Abandoned` is
terminal and `Policy for Fulfilment` tells nobody. The payment stays `Captured`. When it
was the shipment that failed, the warehouse was told to reserve stock and is never told to
release it. A service copied from this example copies a process manager that gives up
without compensating.

## Forces in conflict

- An example should show the whole shape of a process that can fail, since that is the part
  people get wrong.
- Every state and event added is a stored encoding pinned for good (`arch-btw1`).
- A refund is a call to another system: a request, a gateway, a relay and its own way of
  failing.
- The example is meant to stay small enough to read in one sitting.

## Option A: the payment is told, and shows that a refund is owed

`Policy for Fulfilment` reacts to `Abandoned` on the payment's stream; `Payment` gains an
event `RefundOwed` and a state for it. No gateway. The page shows it, and a person or a
later feature refunds.

Gives up: nothing is refunded by the system; the stock question stays open; two stored
variants are added for a state that is a to-do list.

## Option B: compensation as requests

`Payment` becomes a process manager for its refund: `RefundOwed` issues a `Refund` request
to a payment gateway, relayed like any other, ending in `Refunded` or, parked, in a state
that says the refund itself failed. `Fulfilment` in `Shipping` issues `ReleaseStock` before
it ends.

Gives up: a second gateway and a second relay loop in `server`, roughly doubling the
example's process code; four or more stored variants; and a failure of the compensation that
needs its own end, which is where such designs stop being small.

## Evidence that would decide

Whether services built from this template have copied the fulfilment as it stands. If they
have, B is worth its size once; if the example is read and replaced, A (or a paragraph in
`start-a-service.md` saying the example does not compensate) is enough.

## What would reverse it

Not applicable until one option is taken.
