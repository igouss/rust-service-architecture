# 0026. A claim is a lease

Date: 2026-11-23

## Operator decision

"Okay, don't go easy on those problems. Let's fix them all." (operator, 2026-11-23), in answer
to a review that named this as its sixth item. It approves the fix and the changes it needs to
`Outbox::claim`, `Store::requests`, the three store adapters and the conformance suite.
The length of the lease is not the operator's; it is flagged (`cm-vph`). Nothing else is
approved.

## Weakest part at the time

`Outbox::claim` claimed nothing: it was a `SELECT`. Two relays on one store, which is what
a Postgres store is for, were each handed every open request and each performed it. Record
[0009](0009-a-gateways-reply-re-enters-the-core.md) leaned on the receiver dropping the second
copy, which keeps the effect single and still doubles every call. Seen red: with the lease
tests written and `claim` as it was, the suite did not compile (E0061); the behaviour it
replaces is the test `with_no_lease_a_second_claim_is_handed_the_request_being_performed`,
which pins it for a lease of nothing.

## Forces in conflict

- At least once (`arch-fif`): a claimed request whose relay dies must come back.
- Once at a time: a request someone is performing should not be handed to someone else.
- A lock held across the gateway call would hold a transaction open across another system.
- No migration mechanism; and one store must not answer differently from another.

## Move

`Outbox::claim` and `Store::requests` take `until`. In the transaction that reads the due
requests, each is put off to `until` in the `deferred` table of record
[0025](0025-a-failed-request-waits-before-it-is-tried-again.md), by the larger of the two
moments, so a claim with no lease moves nothing. A failure overwrites the moment: the wait
decides, not the lease. SQLite claims inside one immediate transaction; the memory store under
its write lock; Postgres under the advisory lock that already makes its units of work single
file (`arch-m7du`), now taken in one place, `Postgres::alone`. `app::Retry` gains
`lease`, and `app::relay` claims until `now` plus it.

## What it preserved

Every existing assertion: tests that claim twice pass `Moment::ZERO` as the lease, which is
no lease. Order of claiming. Changed because the signatures did: each `claim` call gains an
argument and each `Retry` a field. The anchors of T30 and T77 follow the memory store's
rewritten claim, T41 and T89 the rewritten Postgres query, and T39 anchors on the one place
the lock is now taken; faults unchanged. T41 survived the first form of that query, which
stated the order of claiming twice; the query states it once and T41 dies. New mutants T93 to
T97.

## Flagged assumptions

- `server` leases for 60 seconds. A pass performs up to 16 requests one after another, so a
  gateway that takes more than a few seconds a call can outlast the lease of the last ones,
  and a second relay will then perform them too. Nothing bounds the gateway call.
- A relay that fails to reach its store after the gateway answered leaves the request leased;
  it is performed again when the lease ends.
- The Postgres claim is safe because it runs under the schema's advisory lock, not because of
  row locks. The concurrency test passed here; without the lock it may also pass most of the
  time, so no mutant is planted for it and review holds that line.

## What would reverse it

One relay per store for good, which makes the lease idle; or gateway calls long enough that
a lease has to be renewed while a request is performed, which is a different mechanism.
