# 0010. A failing request ends: retried up to a bound, then parked, and its lifecycle is told

Date: 2026-11-23

## Operator decision

"The owner approves, for this task only, the changes to `ports` named in moves 1, 2 and 5."
(operator, 2026-11-23). Move 2 is: "Distinguish a failure worth retrying from one that is
not ... Add a bounded number of attempts, after which a request is parked and is visible with
SQL. Choose the bound as configuration read in `server`. A parked or permanently failed
request must reach its lifecycle as a `Reaction` ... Apply the change to all three store
adapters and extend the conformance suite." It approves the port, schema, test and mutant
changes named below, and nothing else.

## Weakest part at the time

The failure path of the relay. Every `GatewayError` incremented `attempts` and left the
request open. Seen red before the change: fifty relays against a gateway that always fails
left the request claimable (`left: Ok(1), right: Ok(0)`). A request to a receiver that will
never accept it was offered for ever, and the process manager that issued it waited for ever.

## Forces in conflict

- At least once (`arch-fif`) says keep trying; a request that cannot succeed says stop.
- Stopping silently strands the lifecycle: `Fulfilment` in `Reserving` has no way out but a
  reply.
- No migration mechanism exists; tables are created only if absent. A new column on `outbox`
  would break every existing database file.
- `done` beside a second flag would be a state inferred from two fields (`cm-prk`).

## Move

- `GatewayError` is an enum of what the relay can do: `Unavailable` (try again) and
  `Rejected` (do not), the way `StoreError` separates its variants.
- `Claimed` and `StoredRequest` carry `attempts`, the failures recorded so far. `app::relay`
  takes the number of attempts a request gets; an unavailable request below it is marked
  failed, and one at it, or a rejected one, is parked.
- Parking is `Writer::park` under `Unit::park`: it closes the request exactly as `finish`
  does, counts the last failure, records the cause in a `parked` table of its own (created if
  absent, so existing databases keep working), and applies a reaction in the same unit.
  `SELECT * FROM parked JOIN outbox USING (id)` lists them; the memory store answers
  `Memory::parked()`.
- The reaction is built from `domain::Undelivered<Request>`, a new type of the core's
  vocabulary, by `From`. `Progress` gains `ReservationUndelivered` and `ShipmentUndelivered`;
  both emit `Abandoned`, and `evolve` now takes `Shipping` to `Abandoned` on that fact.

The bound is a parameter of `relay`. `server` reads it from its configuration in the commit
that starts the relay there; a setting with no reader would be dead code in this one.

Considered and set aside: a `parked` column (needs a migration); deleting a parked request
(nothing left to look at); a back-off schedule (time enters as a command, `arch-jbno`, and no
scheduler exists yet; ordering by attempts already keeps one dead receiver from starving the
rest).

## What it preserved

A request still open is still performed at least once. Closing stays one outcome in one unit
(`arch-s6q`, `arch-ruz9`): `finish` and `park` share one statement in each store. No stored
type's JSON changed; `Progress` is not stored. Existing tests changed only where the type
forced it: `relay` gained a parameter, `Claimed` a field, `GatewayError` its variants.

Mutants: T15, T32, T60 and C14 follow code that moved; new T66 to T78. Each was applied by
hand and seen to die as stated.

## Assumptions flagged for the operator

- The memory store keeps every parked request until the process ends. Trimming them like
  facts would drop the one record that says a request was given up.
- An undelivered shipment abandons the fulfilment and does not release the stock it reserved.
  A compensating request is a product decision, not made here.

## What would reverse it

A scheduler adapter makes a back-off possible and may replace the bare count. A migration
mechanism would allow `parked` to become a column of `outbox`.
