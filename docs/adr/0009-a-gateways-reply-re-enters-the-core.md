# 0009. A gateway's reply re-enters the core through the relay, once

Date: 2026-11-23

## Operator decision

"The owner approves, for this task only, the changes to `ports` named in moves 1, 2 and 5.
Every other item on the 'stop and ask' list in `docs/how-to/add-a-use-case.md` still applies:
no new crate, no `allow`, no comment, no weakened test, no second write path, no new dependency
in `domain` or `ports`." (operator, 2026-11-23). Move 1 is: "Change the port so a performed
request yields a typed outcome that the shell turns into a `Reaction` on the requesting
lifecycle, applied in a new unit of work." It approves the port, test and mutant changes named
below, and nothing else.

## Weakest part at the time

The way back from a gateway. `Gateway::perform` returned `Result<(), GatewayError>`, so the
relay learned that a request had been performed and nothing about what the other system
answered. `Fulfilment` waits for `StockReserved` or `StockUnavailable`; the only code that
could produce either was a test calling `app::advance`. The outbox could send and could not
receive.

## Forces in conflict

- The core is synchronous and pure, and the gateway waits on another system, so the reply
  cannot be handed to `react` where the request was issued.
- The relay is at least once: it can crash between the gateway and the store, and two relays
  can claim one request. A reply applied by a second write beside "mark done" would be applied
  twice, or marked done and never applied.
- `arch-s6q`: a unit of work has one way to change a lifecycle. Marking a request done was a
  write outside any unit (`Outbox::done`), which is the "call both" sentence the port exists to
  remove.

## Move

- `Gateway<R>` has an associated `Reply`, and `perform` returns it. A reply is a type of the
  core; `impl From<Reply>` for the process manager's `Reaction` is the pure function that says
  what each answer means (`FulfilmentReply` and `Progress` in `domain/src/fulfilment.rs`).
- `Writer::finish(lifecycle, request)` closes an open request of that lifecycle and returns
  the stream that issued it, or nothing when it is not open. `Unit::finish` wraps it around
  `Unit::apply`, so closing a request and applying the reaction to its reply are one outcome
  in one transaction, on the stream the stored row names and not one a caller passes.
  `Finished` is `must_use`, as `Applied` is.
- `app::relay` takes the store as journal and outbox, and after each performed request runs
  `app::resume` in a unit of its own.
- `Outbox::done` and `Store::mark_done` are removed: nothing called them, and left in place
  they were a way to close a request with no reaction.

Considered and set aside: the gateway returning the `Reaction` itself (a gateway could then
answer `PaymentCaptured`, which no gateway can know); an expected-state check in `react` as
the only guard (it holds for `Fulfilment`, whose steps are guarded by state, and fails for any
reaction that counts).

## What it preserved

The core's traits are unchanged. `app::advance` stays for results that arrive later than the
call. Every existing assertion stays: `a_done_request_is_not_claimed_again` and
`a_done_request_is_no_longer_held` now finish their request through `Unit::finish` instead of
`Outbox::done`, and the test gateways answer `StockUnavailable`, which issues no further
request, so the relay tests count what they counted.

Mutants: T15 and T32 follow the code that moved. T14 planted "the relay marks a failed request
done", which the relay can no longer express (finishing needs a reply); the same fault is now
planted where it can still occur, in the store's `mark_failed`, and dies by the same test. New:
C14, T59 to T65. Each new and moved mutant was applied by hand and seen to die as stated;
`mutants.py` itself runs in continuous integration (`arch-h6j`).

## What would reverse it

A gateway whose answer never matters to any process manager would make `Reply` ceremony; none
exists here. A store that cannot return the row it updated would split `Writer::finish` into a
read and a write inside the unit, which changes the adapter and not the port.
