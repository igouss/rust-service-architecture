---
prefix: arch
---

# Invariants

Every service built on this architecture holds these. Each acceptance criterion names the
check that decides it. A mutant named `C…` must fail to compile and one named `T…` must fail a
test; `just mutants` plants every one listed in `planted/` and exits non-zero unless each
dies as stated.

## `arch-30c` The system shall apply exactly one outcome per handled command

When a command is handled, the system shall apply exactly one outcome through `Unit::apply`:
an accepted transition, holding the new state with the facts and requests that produced it, or
a refusal.

Participants: `ports::decide`, `ports::react`, `Unit::apply`, `app::handle`.

### Acceptance

- `arch-30c.1` Applying anything but an `Outcome` is a compile error (E0308; mutant C1).
- `arch-30c.2` Building a `Transition` outside `ports` is a compile error: its fields are private (E0451; mutant C2) and its only constructor is the private fold (E0624; mutant C3).
- `arch-30c.4` An `Outcome` that is computed and never applied is a compile error under `unused_must_use = "deny"` (mutant C5).

## `arch-jwo4` The system shall store, for each lifecycle, the fold of its facts

When an accepted transition is applied, the system shall store as the lifecycle's state the
fold of the transition's facts over the state the decision read, and shall append those facts.
The state row is the stored truth that the next decision loads; the facts are its history.

Participants: `ports::Transition`, `Unit::apply`, the store adapter.

### Acceptance

- `arch-jwo4.1` The stored state is the fold of the facts through `Lifecycle::evolve`: the use-case tests fail when the fold skips `evolve` (mutant T1) and when the state row is not written (mutant T3).
- `arch-jwo4.2` Each fact takes the next version of its stream and its own position in the log: `a_second_fact_of_a_stream_takes_the_next_version` (mutant T21).
- `arch-jwo4.3` A reopened database file loads the same state and version: `a_reopened_file_loads_the_same_state_and_version`.

## `arch-qve` The system shall record every refusal

If `decide` refuses a command, then the system shall record the refusal and change neither the
state nor its version.

Participants: `Command::decide`, `Lifecycle::Refusal`, `Unit::apply`.

### Acceptance

- `arch-qve.2` A refusal is answered, recorded, and changes nothing else: `a_refused_command_is_answered_recorded_and_changes_nothing` (mutant T2).

## `arch-0yc` The system shall write everything a command causes in one transaction

When a command is handled, the system shall write the state, its facts, its refusal, every
read-model row its policies fold, every process-manager step they trigger and every request
those steps issue in one transaction, or none of them.

Participants: `Journal::write`, `Unit::apply`, `Policy::after`, the store adapter.

### Acceptance

- `arch-0yc.1` No write is reachable outside a unit of work: `Journal::write` is the only way to obtain a `Unit` (mutant C13).
- `arch-0yc.2` A unit that fails after applying stores nothing: `a_unit_that_fails_after_applying_stores_nothing` (mutant T9).
- `arch-0yc.3` A process-manager step and its request are stored with the fact that triggered them: `a_capture_starts_the_fulfilment_and_requests_stock_in_the_same_unit` (mutants T11, T19).

## `arch-goru` A unit of work shall apply a bounded number of outcomes

While a unit of work has applied fewer than `Unit::OUTCOMES` outcomes, the system shall apply
the next. If a unit of work has applied `Unit::OUTCOMES` outcomes, then the system shall
refuse the next with `StoreError::Runaway`, naming the lifecycle it was for, and the unit
shall store nothing.

Participants: `Unit::apply`, `Policy::after`, `app::react`.

### Acceptance

- `arch-goru.1` Two policies that trigger each other end as a named error with nothing stored: `two_policies_that_trigger_each_other_end_the_unit_as_a_runaway_and_store_nothing`.
- `arch-goru.2` A cascade of exactly the bound is stored and one outcome more is refused: `a_cascade_of_as_many_outcomes_as_a_unit_allows_is_stored`, `a_cascade_one_outcome_longer_is_a_runaway_and_stores_nothing` (mutants T81 and T82).

## `arch-9k6q` The system shall hand every fact to its lifecycle's policy before the unit ends

When a fact is recorded, the system shall pass it to its lifecycle's policy inside the same
unit of work.

Participants: `ports::Applied`, `ports::Finished`, `app::handle`, `app::react`, `app::resume`, `Policy::after`.

### Acceptance

- `arch-9k6q.1` Applied facts that never reach a policy are a compile error: `Applied` is `must_use` (mutant C6).
- `arch-9k6q.2` A lifecycle without a policy cannot be handled (E0277; mutant C8).
- `arch-9k6q.3` Facts that skip the policy fail the use-case tests (mutant T12).

## `arch-fif` The system shall perform every request at least once

When a request is in the outbox, the system shall perform it at least once, shall finish it
only in the unit of work that applies its gateway's reply, and its receiver shall drop a second
copy by its `RequestId`.

Participants: `Outbox`, `app::relay`, every `Gateway`.

### Acceptance

- `arch-fif.1` A performed request is marked done and not performed again: `the_relay_performs_a_claimed_request_once_and_marks_it_done` (mutants T15, T16).
- `arch-fif.2` A request whose gateway fails stays claimable: `a_request_the_gateway_fails_stays_claimable` (mutant T14), and does not hold back the next one: `a_request_that_keeps_failing_does_not_hold_back_the_next_one` (mutant T26).
- `arch-fif.3` Each real gateway's duplicate-delivery test performs one request twice and observes one effect (CI).

## `arch-ruz9` A gateway's reply shall re-enter the lifecycle that issued the request, once

When a gateway has performed a request, the system shall turn its reply into a reaction,
apply that reaction to the lifecycle and stream that issued the request, and finish the
request, in one unit of work. If the request is already finished, then the system shall apply
nothing.

Participants: `Gateway::Reply`, `app::relay`, `app::resume`, `Unit::finish`, `Writer::finish`.

### Acceptance

- `arch-ruz9.1` A reply advances the process manager with no call from outside the relay: `a_performed_reservation_advances_the_fulfilment_to_shipping_and_requests_the_shipment` (mutants T15 and T65).
- `arch-ruz9.2` A request finished twice applies its reaction once, in two units or in one: `a_request_finished_twice_applies_its_reaction_once`, `a_request_finished_twice_in_one_unit_applies_its_reaction_once` (mutants T59, T60 and T61).
- `arch-ruz9.3` The reaction is applied to the stream that issued the request, and only as its lifecycle: `a_finished_request_applies_its_reaction_to_the_stream_that_issued_it`, `a_request_is_finished_only_by_the_lifecycle_that_issued_it` (mutants T62, T63 and T64).
- `arch-ruz9.4` A unit that fails after finishing a request leaves it claimable: `a_unit_that_fails_after_finishing_a_request_leaves_it_claimable`.
- `arch-ruz9.5` The facts of a finished request that never reach a policy are a compile error: `Finished` is `must_use` (mutant C14).

## `arch-22kr` A request shall end: finished by a reply, or parked with its lifecycle told

If a gateway rejects a request, or has been unavailable for it as often as the relay's bound
allows, then the system shall park the request, record the cause, and apply to the lifecycle
that issued it the reaction built from `Undelivered`, in one unit of work. While a request has
failed less often than the bound and was not rejected, the system shall offer it again, and
not before the wait that its count of failures gives has passed since the failure.

If the unit that applies a reply fails and the store is neither unavailable nor misconfigured,
then the system shall count a failed attempt, and shall go on to the next request. If that
unit has failed as often as the bound allows, or the unit that parks a request fails so, then
the system shall park the request with the error as its cause and apply no reaction.

Participants: `GatewayError`, `domain::Undelivered`, `app::relay`, `app::abandon`, `Unit::park`, `Unit::strand`, `Writer::park`.

### Acceptance

- `arch-22kr.1` An unavailable request is offered again with its failure counted: `a_request_that_fails_once_under_a_bound_of_two_is_offered_again_with_its_failure_counted` (mutants T68 and T76 to T78).
- `arch-22kr.2` A rejected request is not offered again and its lifecycle is told: `a_request_the_gateway_rejects_is_not_offered_again_and_its_fulfilment_is_abandoned` (mutant T66).
- `arch-22kr.3` A request that fails as often as the bound is parked and its lifecycle is told: `a_request_that_fails_as_often_as_the_bound_is_parked_and_its_fulfilment_abandoned`, `under_a_bound_of_one_the_first_failure_parks_the_request` (mutants T67 and T69).
- `arch-22kr.4` A parked request is closed once, like a finished one: `a_request_parked_twice_applies_its_reaction_once`, `a_request_finished_and_parked_in_one_unit_applies_only_the_first_reaction`, `a_unit_that_fails_after_parking_a_request_leaves_it_claimable`.
- `arch-22kr.5` A parked request can be read where the store keeps it, with its cause and its attempts: `a_parked_request_is_read_with_sql_with_its_cause_and_its_attempts` for SQLite and Postgres, `a_parked_request_is_held_as_parked_with_its_cause_and_its_attempts` for memory (mutants T70 to T75).
- `arch-22kr.6` A reply whose unit fails is counted, does not hold back the next request, and at the bound is parked with nothing applied: `a_reply_whose_unit_fails_is_counted_and_offered_again`, `a_request_behind_one_whose_unit_fails_is_performed_in_the_same_pass`, `a_reply_whose_unit_fails_as_often_as_the_bound_parks_the_request_and_tells_nothing` (mutants T83 to T85).
- `arch-22kr.8` A failed request is put off for a wait that doubles with each failure up to a longest wait, and no store hands it out before that moment: `each_further_failure_doubles_the_wait`, `the_wait_is_never_longer_than_the_longest`, `a_failed_request_is_not_performed_again_before_its_wait_is_over`, `five_passes_half_a_second_apart_leave_a_fulfilment_reserving`, `a_request_put_off_is_not_claimed_before_its_moment`, `a_request_put_off_is_claimed_at_its_moment` (mutants T87 to T92).
- `arch-22kr.7` A request whose lifecycle cannot be told is parked all the same, with a cause: `a_rejected_request_whose_lifecycle_cannot_be_told_is_parked_all_the_same`, `a_stranded_request_is_not_claimed_again_and_applies_nothing`, `a_stranded_request_is_held_as_parked_with_its_cause` (mutant T86).

## `arch-oif` A read model shall hold exactly the fold of its lifecycle's facts up to its position

When a fact is projected, the system shall write the row and the read model's position
together, the row shall be the projection's fold of the previous row and that fact, and the
system shall refuse a fact at or below the position.

Participants: `Projection`, `ports::project`, `Unit::put`, `app::project`, `app::rebuild`.

### Acceptance

- `arch-oif.1` Building a `Change` outside `ports` is a compile error (E0451; mutant C4).
- `arch-oif.2` A `Change` that is computed and never put is a compile error (mutant C7).
- `arch-oif.3` A change at or below the position is refused: `a_change_at_or_below_the_read_models_position_is_refused` (mutants T7, T8).
- `arch-oif.4` The fold reads the previous row: `many_added_tasks_are_all_open_and_tallied` (mutant T13).
- `arch-oif.5` A rebuilt read model equals the live one: `a_rebuilt_read_model_equals_the_live_one` (mutants T4, T18).

## `arch-a90r` A recorded fact shall be built only by ports, from a write or a read of the store

The system shall build a `Recorded` fact only when `Unit::apply` appends it or `Unit::facts`
reads it back, so nothing outside `ports` can make one up. A fact carried out of a unit that
then fails was appended and rolled back; it is still one `ports` built.

Participants: `ports::Recorded`, `Unit::apply`, `Unit::facts`.

### Acceptance

- `arch-a90r.1` Building a `Recorded` outside `ports` is a compile error: its fields are private (E0451; mutant C10).

## `arch-hbw` Aggregates shall issue no requests

A command's `decide` shall return facts only; requests come from process managers.

Participants: `domain::Command`, `domain::Reaction`.

### Acceptance

- `arch-hbw.1` Returning a request from `Command::decide` is a compile error (E0308; mutant C9).

## `arch-bim` The evolve function shall be total

`Lifecycle::evolve` shall return a state for every pair of state and event.

Participants: every `Lifecycle` implementation.

### Acceptance

- `arch-bim.1` Every `match` on a state type spells out each pair: `clippy::wildcard_enum_match_arm` is denied, and the `wildcards` recipe reads tuple matches (mutants G6 and G19).

## `arch-7k8` The internal-to-wire event mapping shall be total and versioned

Once a `wire` crate exists, the system shall map every internal event to a versioned wire
event.

Participants: `wire`, the publishing adapter.

### Acceptance

- `arch-7k8.1` The `From<Event>` impl in `wire` matches every variant with no wildcard arm.

## `arch-9ln` The system shall answer queries without calling decide

When a query is received, the system shall answer it from state rows or a read model without
calling `decide`.

Participants: queries in `app`, `Journal::read`, `View`.

### Acceptance

- `arch-9ln.1` A `View` has no method that writes or decides (T1).
