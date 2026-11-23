# 0011. A stored value loads under its type's stored rule, not under the rule for new input

Date: 2026-11-23

## Operator decision

"Separate parsing new input from loading stored values ... make a stored value load regardless
of later rule changes, while new input is still validated at the boundary. State the general
rule for refined types in `docs/architecture/core.req.md`. Do not change the pinned JSON of
any stored type ... `domain/tests/stored_encoding.rs` is unchanged except for additions."
(operator, 2026-11-23). It approves the mutants added below, and nothing else.

## Weakest part at the time

`Title` derived `Deserialize` through `TryFrom<String>`, which called `Title::new`. Loading a
state row or a fact therefore re-ran today's validation on a value accepted under the rule of
the day it was stored. Seen red: with `Title::LIMIT` lowered from 200 to 100, a state row
holding a 200-character title loaded as
`Err(Corrupt { table: "states", cause: "a title is at most 100 characters ..." })`. Tightening
a rule for new input would have made every task with a longer title unreadable, and its
history with it, since facts are never rewritten.

## Forces in conflict

- A rule for new input has to be free to tighten.
- A stored value has to load for as long as it is stored (`arch-btw1`).
- Two existing tests in `stored_encoding.rs` require a blank or over-long stored title to be
  refused on load, and that file may only gain tests. So loading cannot simply skip
  validation (`#[serde(transparent)]`, which mutant T24 plants as a fault).

## Move

A refined type carries two rules. `Title::new` holds the rule for new input (`LIMIT`).
`TryFrom<String>`, which `Deserialize` uses, holds the stored rule (`STORED_LIMIT`): the
loosest rule under which a stored title was ever accepted. The two are separate code, so an
edit to one does not move the other, and
`const _: () = assert!(Title::LIMIT <= Title::STORED_LIMIT)` makes loosening the input rule
past the stored rule, or tightening the stored rule below the input rule, a compile error.

Considered and set aside: `#[serde(transparent)]` (contradicts the two pinned tests and
accepts a corrupted row); a version tag in the encoding (changes the pinned JSON); a second
type for stored titles (every state and event would name it, and projections would convert).

## What it preserved

The pinned JSON of every stored type, and `domain/tests/stored_encoding.rs` byte for byte.
T22 to T24 are unchanged. New: C15 and C16 (the constant assertion, E0080) and T80 (a stored
blank title loads). Each was applied by hand and seen to die as stated; with `LIMIT` lowered
to 100 the hazard test passes.

## What would reverse it

A rule that must be enforced on stored values too (a legal requirement to purge something) is
a migration of the rows, not a tightened `Deserialize`.
