# 0007. The memory store keeps a bounded tail of history

Date: 2026-11-15

## Weakest part at the time

The memory store's growth. It kept every fact, every refusal and every request, done or not,
for the life of the process, because it was written to pass the same suite as the stores that
keep them on disk. A service with no database runs for weeks.

## Forces in conflict

- One conformance suite for every store: a store that answers differently is a second port.
- History has two uses, rebuilding a read model and explaining a state. In a store that a
  restart empties, the first never happens: a new read-model shape arrives with a restart.
- Whatever is trimmed must not turn into a wrong answer. A rebuild over a short history
  produces a read model that looks right.
- State rows also grow, with the number of streams. Dropping the row of a lifecycle that has
  ended was considered and set aside: the stream's version would start again at zero in
  memory and not on disk, which the suite exists to prevent.

## Move

`Memory::new` takes `Kept { facts, refusals }` from the composition root. A commit drops facts
and refusals beyond those numbers, oldest first; positions keep counting; a read of facts that
reaches into the trimmed part is refused. A request is removed when it is marked done.
`Memory::held` reports the counts. State rows and read-model rows are kept: the rule
(`docs/architecture/adapters.req.md`) is that a lifecycle is something the business counts,
and state that may be forgotten lives in the adapter that owns it.

## What it preserved

The suite: all 38 cases pass unchanged, because none reads further back than the store keeps.

## What would reverse it

A service on the memory store whose lifecycles do end in large numbers and are counted by the
business while they live. Then ending a lifecycle becomes a thing every store does, with the
version question answered for all of them at once.
