---
prefix: arch
---

# Invariants

Every service built on this architecture holds these. Each acceptance criterion names the
check that decides it. A mutant named `C…` must fail to compile and one named `T…` must fail a
test; `python3 mutants.py` plants every one and exits non-zero unless each dies as stated.

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

Participants: `Command::Rejection`, `Lifecycle::Refusal`, `Unit::apply`.

### Acceptance

- `arch-qve.2` A refusal is answered, recorded, and changes nothing else: `a_refused_command_is_answered_recorded_and_changes_nothing` (mutant T2).

## `arch-0yc` The system shall write everything a command causes in one transaction

When a command is handled, the system shall write the state, its facts, its refusal, every
read-model row its policies fold, every process-manager step they trigger and every request
those steps issue in one transaction, or none of them.

Participants: `Journal::write`, `Unit::apply`, `Policy::after`, the store adapter.

### Acceptance

- `arch-0yc.1` No write is reachable outside a unit of work: `Journal::write` is the only way to obtain a `Unit`.
- `arch-0yc.2` A unit that fails after applying stores nothing: `a_unit_that_fails_after_applying_stores_nothing` (mutant T9).
- `arch-0yc.3` A process-manager step and its request are stored with the fact that triggered them: `a_capture_starts_the_fulfilment_and_requests_stock_in_the_same_unit` (mutants T11, T19).

## `arch-9k6q` The system shall hand every fact to its lifecycle's policy before the unit ends

When a fact is recorded, the system shall pass it to its lifecycle's policy inside the same
unit of work.

Participants: `ports::Applied`, `app::handle`, `app::react`, `Policy::after`.

### Acceptance

- `arch-9k6q.1` Applied facts that never reach a policy are a compile error: `Applied` is `must_use` (mutant C6).
- `arch-9k6q.2` A lifecycle without a policy cannot be handled (E0277; mutant C8).
- `arch-9k6q.3` Facts that skip the policy fail the use-case tests (mutant T12).

## `arch-fif` The system shall perform every request at least once

When a request is in the outbox, the system shall perform it at least once, shall mark it done
only after its gateway succeeds, and its receiver shall drop a second copy by its `RequestId`.

Participants: `Outbox`, `app::relay`, every `Gateway`.

### Acceptance

- `arch-fif.1` A performed request is marked done and not performed again: `the_relay_performs_a_claimed_request_once_and_marks_it_done` (mutants T15, T16).
- `arch-fif.2` A request whose gateway fails stays claimable: `a_request_the_gateway_fails_stays_claimable` (mutant T14), and does not hold back the next one: `a_request_that_keeps_failing_does_not_hold_back_the_next_one` (mutant T26).
- `arch-fif.3` Each real gateway's duplicate-delivery test performs one request twice and observes one effect (CI).

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

## `arch-hbw` Aggregates shall issue no requests

A command's `decide` shall return facts only; requests come from process managers.

Participants: `domain::Command`, `domain::Reaction`.

### Acceptance

- `arch-hbw.1` Returning a request from `Command::decide` is a compile error (E0308; mutant C9).

## `arch-bim` The evolve function shall be total

`Lifecycle::evolve` shall return a state for every pair of state and event.

Participants: every `Lifecycle` implementation.

### Acceptance

- `arch-bim.1` Every `match` on a state type spells out each pair: `clippy::wildcard_enum_match_arm` is denied, plus review of tuple matches.

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
