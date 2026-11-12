# Review of the log-as-truth rewrite, 2026-11-11

An independent reviewer read the nine rule documents and the workspace as they stood in this
repository's first state. Tests, clippy, the
guide-sync check and all 22 mutants (9 compile errors, 13 test failures) reproduced. Paths are
relative to `docs/architecture/` and the workspace root. Each finding is open until a commit
names it as closed. The table after it gives the status after the redesign recorded in
`docs/adr/0001-state-rows-with-facts-as-history.md`.

| # | Severity | Finding | Where |
|---|---|---|---|
| 1 | high | No driven adapter can store a process manager: its `Lifecycle` and `Reaction` live in `app`, which driven adapters may not see, so the adapter can't name the event and request types it must serialize. | `structure.req.md` primitives table and role matrix; `adapters.req.md` `arch-m3z` |
| 2 | high | A late or duplicate projector corrupts a read model while its position looks right: `ReadModel::put` has no previous-position guard, and `Position` and `Recorded` are publicly constructible. Probe: a counting projection over 2 facts ended at position 2, row 4. No `ReadModel` or `Cursor` conformance suite exists. | `ports/src/lib.rs` `ReadModel`, `project` |
| 3 | high | Redelivery is absorbed only by a process manager that never revisits a state. One that counts, or returns to `Idle`, applies a redelivered fact again, and the duplicate request gets a new id because the version advanced. | `log.req.md` `arch-5qcw`; `invariants.req.md` `arch-nww.4`; `app/src/lib.rs` `pump` |
| 4 | medium | A step that requests without emitting a fact leaves the version unchanged, so two such steps share a request id and an append-constraint adapter never sees their conflict. A mutant skipping the version check on a zero-fact commit survived all 22 tests. | `ports/src/lib.rs` `react`; `invariants.req.md` `arch-fif.1` |
| 5 | medium | `Cursor` is tied to no reader or feed and `pump` takes one event type, but a process manager reads facts of several lifecycles: a shared cursor skips facts, separate cursors lose cross-feed order. | `ports/src/lib.rs` `Cursor`; `log.req.md` |
| 6 | medium | `arch-nww.3` claims the cursor is saved after the step commits; a mutant that saves it first survived all tests. | `invariants.req.md`; `app/src/lib.rs` `pump` |
| 7 | medium | `arch-jbno.1` cites the core's banned-call scan, but `Reaction::decide` lives in `app`, which the scan does not cover. | `shell.req.md` |
| 8 | medium | `arch-jbno` lets a scheduler submit a `Reaction` input directly, bypassing the feed, so a doubled tick is not "a fact delivered twice". No `app` handler or `impl Observation` exists; `ports::observe` is uncalled and unmutated. | `shell.req.md`; `ports/src/lib.rs` |
| 9 | medium | Three rules disagree on where command types live (`ports` or `app`; `ports`; `domain`). | `specification.req.md` `arch-saq`; `ports.req.md` `arch-9q4`; `structure.req.md` |
| 10 | medium | The relay's port and the request id exist only in prose: no trait, and `Transition::into_parts` hands the adapter requests with no id. | `shell.req.md` `arch-w0v`; `invariants.req.md` `arch-fif.1` |
| 11 | low | The closed primitive set omits handler, pump, projector and relay, which other rules require; "a background job with its own table" describes the relay. | `structure.req.md` `arch-u8nl` |
| 12 | low | `arch-t81.2` names conformance cases that don't do the killing: the zero-facts case passes under both mutants it is credited with. | `invariants.req.md` |
| 13 | low | `arch-nww.1`, `arch-nww.2` and `arch-qve.2` name checks that no mutant, or only half a mutant, exercises. | `invariants.req.md` |
| 14 | low | `run-mutants.sh` cannot go red: a missing anchor or a surviving mutant still exits 0, and `mutate.sh` shows only the first two failures. | `mutate.sh`, `run-mutants.sh` |
| 15 | low | "event sink" survives as an example of an I/O port, inviting a publish after `commit`. | `ports.req.md` `arch-v1q` |
| 16 | low | The query primitive (read model only) is narrower than the query rule (read model or `Store::load`). | `structure.req.md`; `shell.req.md` `arch-3q8` |
| 17 | low | Two rows of the lifecycle table prescribe "one owning task holds the enum", which `arch-i2t` bans for aggregates. | `core.req.md` `arch-f5d`; `shell.req.md` |
| 18 | low | No test carries the `Covers:` citation that `arch-0d2` and `arch-0id` require. | `specification.req.md` |

## Status after the redesign

| # | Status | By |
|---|---|---|
| 1 | closed | process managers live in `domain`; the store adapter stores `Fulfilment` (`a_capture_starts_the_fulfilment_and_requests_stock_in_the_same_unit`) |
| 2 | closed | `Unit::put` refuses a change at or below the read model's position (mutants T7, T8) |
| 3 | closed | a fact is handed to its policy once, inside the unit that recorded it; there is no redelivery (`arch-9k6q`) |
| 4 | closed | a request's id is its outbox row id; no version is involved |
| 5 | closed | no cursor: readers run in the unit of work |
| 6 | closed | no cursor to save |
| 7 | closed | reactions live in `domain`, which the purity gate scans (mutants G2, G4) |
| 8 | closed in the rule, open in code | `arch-jbno` now has a scheduler submit a command; the repository has no scheduler adapter yet |
| 9 | closed | command types live in `domain` (`arch-saq`) |
| 10 | closed | `Outbox` and `Gateway` are traits in `ports`, with `app::relay` and its tests (mutants T14, T15, T16) |
| 11 | closed | `arch-u8nl` names the fixed machinery beside the primitives |
| 12 | closed | each criterion names the test the mutant runner reported as the killer |
| 13 | closed | same; criteria with no mutant were removed |
| 14 | closed | `mutants.py` exits non-zero on a survivor, a missing anchor, a mutant that does not compile or a wrong error code; it reports every killing test |
| 15 | closed | the example is gone from `arch-v1q` |
| 16 | closed | the query primitive reads state rows or a read model |
| 17 | closed | the two rows were removed from `arch-f5d` |
| 18 | closed | the requirement was dropped: a criterion names its test, and `just check` fails when the name does not exist |

Not checked by the reviewer: `hex-lint`, `effect-audit`, the no-comments gate, `cargo-deny`,
`cargo-regime-check` (the workspace has no config for them); real concurrency (the fake runs
under one mutex and a single-poll executor).
