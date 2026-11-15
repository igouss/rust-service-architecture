# 0004. Store adapters see text; `ports` owns the typed unit of work and the only way to build a fact

Date: 2026-11-15

## Weakest part at the time

The fact handed to a policy. `Recorded` had public fields because the store adapter had to
build it, so any code in `app` could build one too and pass it to `Policy::after` or
`app::project`. A fact at position 999 that the store never held moves a read model's
position to 999, and the position guard (`arch-oif`) then refuses every real fact until a
rebuild. The adapter's own tests built such facts to exercise the guard.

## Forces in conflict

- Services built on this architecture will run on SQLite, on Postgres, and on no database at
  all. With one adapter, the rules it implemented (number the versions, write the state row
  only when a fact was appended, refuse a change at or below a read model's position, encode
  as JSON) existed once. With three they exist three times, and a conformance suite has to
  catch each copy that drifts.
- Rust has no visibility that admits one other crate. A constructor the adapter can call, the
  shell can call.
- `ports` took no dependency beyond `domain` and `serde`.

## Move

The adapter implements `ports::store`: `Store`, `Writer` and `Reader`, one statement per
method, over JSON text, versions and positions. `Unit` and `View` became structs in `ports`
that hold the writer or reader in a private field; `Journal` and `Outbox` are implemented once,
for every `Store`. `Unit::apply` is now the one place that numbers versions, serializes, and
builds a `Recorded`, whose fields are private. Every `Store` method takes a `Key` whose
constructor is private to `ports`, so the adapter can implement a raw write that the shell
cannot call.

`ports` now depends on `serde_json`. Rows a reader lists reach `ports` as borrowed text
through a callback, not as owned strings.

## Evidence

- All 53 earlier architecture mutants die as they did before the move; the anchors of T2, T3, T7, T19
  and T21 moved from the adapter to `ports` with the code. Five mutants are new: a fact built
  outside `ports` (C10, E0451), the key built or minted outside `ports` (C11, E0603; C12,
  E0624), a unit built around a writer outside `ports` (C13, E0451), an adapter that takes a
  serializer (G10, the `raw-store` recipe).
- `adapter-sqlite` depends on `ports` and `rusqlite`; `domain` is a dev-dependency
  (`docs/views/crates.md`).
- `bench/results/2026-11-15-raw-store-before.txt` and `-after.txt`, one run each on the same
  machine: `handle(AddTask)` under `Durability::Normal` 58.7 µs before and after (p50);
  `open_tasks` over 100 000 tasks 38.9 ms before, 36.6 ms after (min). A first version that
  returned owned strings from `Reader::states` measured 42.5 ms; the callback removed that.
- The adapter's tests that built a `Recorded` now take one from `Unit::apply`; their
  expectations are unchanged.

## What it preserved

One way to change a lifecycle (`arch-s6q`); the unit's read and write in one transaction, with
no expected version and no retry; the synchronous store port (ADR 0002); the shell's code,
apart from a type parameter it no longer names; the decorators that driving-adapter tests
wrap around a `Journal`.

## What it gave up

A reader of `adapter-sqlite` no longer sees what a unit of work does; that is in
`ports/src/lib.rs`. Each raw call goes through `dyn Writer`; the benchmark does not show it.

## What would reverse it

A store whose unit of work cannot be expressed as these statements in one transaction: for
example one that must write the state and its facts in a single call to be atomic. Then the
typed unit moves behind a trait again, and `Recorded` needs another guard.
