# 0029. A gate reads that a refined type's loader does not call its constructor

Date: 2026-11-23

## Operator decision

"Okay, don't go easy on those problems. Let's fix them all." (operator, 2026-11-23), in answer
to a review whose eighth item was the stored rule of record
[0011](0011-a-stored-value-loads-under-its-stored-rule.md). It approves a gate, its recipe in
`just check`, its tests and its mutants. Nothing else is approved.

The review's item also said `Amount` loads under no rule. That was wrong: `Amount` wraps a
non-zero integer, so its representation is its stored rule (`arch-v9j1` says so). Nothing
changes for `Amount`.

## Weakest part at the time

Record 0011 separated the rule for new titles from the rule stored titles load under, and
named what it left open: while the two limits are equal, a `TryFrom` that goes back to
calling `Title::new` passes every test and every mutant. The separation the record is for
could be undone by a one-line edit that nothing would see until a rule was tightened and
stored tasks stopped loading. Seen red: with `(false, false) => Title::new(&text)` planted,
`just check` was green before this gate and `just refined` is red after it.

## Forces in conflict

- No behaviour differs at equal limits, so no test can tell (`cm-la6`).
- Making the limits differ to have a test would invent a policy.
- A gate over source sees the call, and can be walked around by a helper function.

## Move

`gates refined`, in `just check`: in every member's `src`, a struct or enum marked
`#[serde(try_from = …)]` must have an `impl TryFrom<…> for` it in the member, and that
impl's body must not name `Type::new` or `Self::new`. It fails closed twice: every
`try_from =` written must be read as a mark on a struct or an enum, and every
`TryFrom<…> for` written must be read as an impl for a plain type; a difference is exit 2.
It reads tokens, so a call inside a closure or a macro is seen.

## What it preserved

Every other gate and the code it reads; `domain` is unchanged. Mutants G31 (the loader calls
`Title::new`) and G32 (the loader is written for another type), each planted through the
runner and seen to die with its text.

## What it gives up

- It is a source lint (T3), not a proof: a helper that `new` and `try_from` both call
  merges the rules again and passes. The gap ledger keeps that row.
- A refined type that derives `Deserialize` with no mark is not read at all; mutant T24
  plants that on `Title` and tests kill it, which is per type, not general.
- One more recipe in `just check`.

## What would reverse it

The two rules given different types, so that the compiler holds them apart; then the lint has
nothing left to read.
