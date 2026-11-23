# 0012. A stream id carries its lifecycle in its type

Date: 2026-11-23

## Operator decision

"The owner approves, for this task only, the changes to `ports` named in moves 1, 2 and 5."
(operator, 2026-11-23). Move 5, first item: "`StreamId` is one type for every lifecycle, so
an id for one lifecycle can be handed to a command on another. Make that a compile error. Show
it with a planted mutant that must fail to build." It approves the port, test and mutant
changes named below, and nothing else.

## Weakest part at the time

`StreamId(pub String)` was one type for every lifecycle. Seen before the change: in the
payment policy, `react(unit, fact.stream(), Progress::PaymentCaptured)`, which hands the
payment's own id to the fulfilment's reaction with no correlation, compiled. The store would
have opened a fulfilment under whatever text arrived; nothing but a reviewer stood between a
task's id and a payment's command.

## Forces in conflict

- An id is text in the store and in a URL; a lifecycle is a type. The check has to live where
  both are known.
- A store adapter names no domain type (`arch-iv3d`), so it cannot hold a typed id.
- Process managers must name one lifecycle's stream after another's; that has to stay
  possible and become visible.

## Move

- `StreamId<S>` holds the text and a phantom `S`; `StreamId::new` and `as_str` are its
  surface. Its field is private, so an id is made by naming its lifecycle, in an annotation or
  a signature.
- `Recorded<S>` replaces `Recorded<E>`: a fact knows its lifecycle, and `Policy::after`
  receives `&Recorded<Self>`.
- `Unit::apply`, `View::state`, `View::states`, `View::refusals`, `app::handle`,
  `app::react` and `app::advance` take or return the id of the lifecycle they act on.
- The raw store, `StoredFact`, `StoredRequest`, `Claimed` and `Gateway::perform` carry the
  stream as text.
- A correlation is a function from one lifecycle's id to another's (`fulfilment_of`).

Considered and set aside: one newtype per lifecycle (a declaration per lifecycle that the
compiler cannot tie to `Lifecycle::NAME`); a runtime check of the lifecycle's name in
`apply` (an error at run time for what the type can refuse).

## What it preserved

Every stored row and every assertion. Tests changed only where the type forced it: `id`
helpers became generic, a claimed request's stream is compared as text, `Recorded<TaskEvent>`
is written `Recorded<Task>`. Mutant anchors C8, C10, T2, T10, T12, T65, T73, T75 and H2 follow
the code; each was applied by hand again and died as before. New: C17 and C18 (E0271).

## What would reverse it

A lifecycle chosen at run time (a generic administration tool that loads any stream by name)
needs an untyped id again; it would get one of its own rather than loosen this one.
