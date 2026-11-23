# 0013. Unique stored names are checked by a gate that runs before the tests

Date: 2026-11-23

## Operator decision

"Unique names. Two lifecycles or two projections with the same `NAME` would share rows. Make
a duplicate fail before any test runs; prefer a compile-time mechanism, and fall back to a
gate only if you can show none works." (operator, 2026-11-23). It approves the gate, the
`justfile` recipe and the mutants named below, and nothing else.

## Weakest part at the time

`Lifecycle::NAME` and `Projection::NAME` are free strings that key every stored row. Seen
before the change: with `Fulfilment::NAME` set to `"payment"`, `cargo check --all-targets`
exits 0. Only a test that happens to use one stream id for both lifecycles would notice.

## Forces in conflict

- The strongest rung a rule admits (`cm-8gv`): a duplicate should not compile.
- A gate checks the consequence, not a declaration (`cm-clt`): a list of names that someone
  keeps by hand is a declaration.
- The fail-closed gates already in place must not be loosened to admit a mechanism
  (`arch-tsu`).

## What was tried at the compile rung

1. A registry macro in `app`: one `names! { lifecycles: …; projections: … }` that asserts the
   names distinct in a constant and implements a marker trait that `Policy` requires. It was
   built: it compiles and the tests pass. The `wildcards` recipe then fails closed:
   `wildcards: app/src/lib.rs: 8 \`=>\` and 7 match arms read`, because a `macro_rules` arm is
   an arrow it cannot read as a match arm. The macro cannot move out of `domain` and `app`:
   the marker's implementations name the core's types, so the orphan rule (E0117) keeps them
   in one of those two crates. Admitting macro arms is a change to a gate, which this task
   does not approve.
2. The same registry without a macro: a constant list and one hand-written marker
   implementation per type, pointing at its entry. Reasoned, not built: a copied lifecycle
   that keeps both the `NAME` and the entry of its original passes, which is the very case
   the rule is for.
3. A type-level list (membership by type identity, names compared by recursive associated
   constants). Reasoned, not built: it closes the hole and costs about six traits in the
   shell and an inferred type parameter on `handle`, `react` and `relay`; the boundary would
   cost more attention than the property (`cm-xrl`).
4. Constant `TypeId` comparison, constant generic strings and linker symbols are not
   available on stable Rust without `unsafe`, which the workspace forbids.

So no compile-time mechanism is both complete and admissible today. The first is one gate
decision away.

## Move

`names.py` reads every `impl Lifecycle for` and `impl Projection for` in the workspace's
`src`, takes the `NAME` literal of each, and fails on two lifecycles or two projections with
one name. It fails closed when an `impl` has no `NAME` it can read as one string literal or
when it reads fewer `impl`s than the source declares. `just check` runs it after `clippy` and
before `test`. Mutants G28 to G30, each applied by hand and seen red.

## What it preserved

No type, port or stored name changed. Every other gate is as it was.

## What would reverse it

The operator allowing a `macro_rules` arm in the `wildcards` scan (it would have to tell a
macro arm from a match arm and still fail closed) makes attempt 1 admissible; the gate then
becomes the check that the registry is complete, or goes.
