# 0002. The core and the store port are synchronous; gateways are asynchronous

Date: 2026-11-11

## Weakest part at the time

Every port was `async`. On SQLite each method body was synchronous, and the baseline adapter
paid for the signature anyway: `Send` bounds repeated on every associated type, a `Mutex`
around a connection that is not `Sync`, three hand-written executors in tests and benches, and
a conformance suite nobody could run without one.

## Forces in conflict

- An async runtime is where the HTTP adapter and calls to other systems live.
- A unit of work holds an open SQLite transaction. Held across an `.await` it needs `Send`
  connections, async closures and a silent rollback whenever a future is dropped.

## Move

`domain`, `ports` and `app` are synchronous, except `Gateway::perform` and `app::relay`, which
wait on another system. A driving adapter runs a unit of work off the executor. A port is async
only when its adapter waits on another system (`arch-v1q`).

Making the core and every port asynchronous was considered and set aside: it would buy nothing
on a blocking store and would cost every caller and test.

## What it preserved

Tokio stays in the adapters and the composition root; tests of the core and the shell need no
runtime.

## What would reverse it

The store moving to a database reached over the network. The change is confined to `ports`,
the store adapter and the generic functions in `app`; `domain` does not move.
