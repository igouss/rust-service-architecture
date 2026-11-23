# 0025. A failed request waits before it is tried again, and the wait doubles

Date: 2026-11-23

## Operator decision

"Okay, don't go easy on those problems. Let's fix them all." (operator, 2026-11-23), in answer
to a review whose first item under "decisions to review" was this one. It approves the fix
and the changes it needs to `Outbox`, `Store`, the three store adapters, the conformance
suite and `app::relay`. The numbers below are not the operator's; they are flagged
(`cm-vph`). Nothing else is approved.

## Weakest part at the time

The bound of record [0010](0010-outbox-failures-end.md) counted attempts and nothing else.
`server` passed over the outbox every 500 ms and allowed five attempts, so a receiver that
was away for 2.5 seconds had every request in flight parked and every fulfilment folded to
`Abandoned`, which is terminal. Record 0010 set a back-off aside because "time enters as a
command (`arch-jbno`) and no scheduler exists yet". That reason does not hold against this:
the relay is not a decision, and the moment can be handed to it.

Seen red: the tests in `app/tests/retries.rs` did not compile (E0432, E0061), there being no
way to tell the relay what time it is; the arithmetic above is the behaviour they replace.

## Forces in conflict

- A request that cannot succeed must end; a receiver that is briefly away must not end it.
- Only time tells the two apart, and `app` reads no clock (`arch-jbno`, `arch-zhm`).
- No migration mechanism exists: a new column on `outbox` breaks existing database files
  (the force that gave `parked` its own table).
- A store must not answer differently from another (`arch-e5eb`).

## Move

- `ports::Moment` is milliseconds since the Unix epoch, an ordinal like `Position`.
- `Outbox::claim` and `Store::requests` take `now` and hand out no request put off to a
  later moment. `Outbox::failed` and `Store::mark_failed` take the moment to put the
  request off to.
- SQLite and Postgres keep that moment in a table of its own, `deferred (id, not_before)`,
  created if absent; a request with no row is due. The memory store keeps it on the request.
- `app::Retry` holds the attempts, the first wait and the longest wait; `Retry::wait` is
  the first wait after one failure and doubles after each further one, up to the longest.
  `app::relay` takes `&Retry` and `now`.
- `server` reads the system clock once a pass and hands it in.

## What it preserved

Order of claiming (fewest failures first, then oldest), parking, stranding, and every
existing assertion. Changed because the signatures did: every `claim`, `failed` and `relay`
call in the tests gains `Moment::ZERO` or a `Retry` with no wait, which keeps each test's
"at once" meaning. One expectation changes on purpose: `server` gives a request ten
attempts by default where it gave five (`the_relay_gives_a_request_ten_attempts_by_default`).
The anchors of T16, T26, T41, T42, T67, T68, T69 and T83 follow the lines that moved; their
faults are the same. New mutants T87 to T92. T84's planted line called `failed` with one
argument and stopped compiling here; a full run of the mutants found it, and it now passes the
moment as the code does.

## Flagged assumptions

- First wait one second, longest five minutes, ten attempts: a receiver may be away about
  eight and a half minutes (1 + 2 + … + 256 seconds) before its requests are parked. These are
  constants and a default in `server`; only the attempts are a setting.
- `Misconfigured` and `Unavailable` stores still end a pass without counting (record 0024).
- A `deferred` row outlives its request; nothing deletes it, as nothing deletes a done
  request.
- A fulfilment that is abandoned after the last wait still leaves a captured payment with
  nothing refunding it. That is domain policy and is proposed, not built, in record 0027.

## What would reverse it

A scheduler that submits retries as commands, which would let the wait be a recorded fact;
or receivers that are never briefly away.
