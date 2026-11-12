---
prefix: arch
---

# Ring 3: adapters, the wire and observability

Driving adapters (HTTP with axum and Datastar, schedulers), driven adapters (SQLite, gateways to
other systems) and, once triggered, the `wire` crate. Tokio lives here and in the composition
root, nowhere further in.

## `arch-68v` Driving adapters shall be thin

Deserialize, refine into a command (bad input is answered here with a 400), call the handler,
map the `Answer` or the shell error to a status. Each shell error maps to a status exhaustively,
with no wildcard arm. An `if` about business state in a handler is a ring violation.

### Acceptance

- `arch-68v.1` Review (T4).

## `arch-m3z` Request and response types shall live only in adapters

DTOs, form structs and HTML live in the driving adapter, which maps them to commands and maps
answers back. Stored types are the domain's own, through their serde derive
([core](core.req.md)); an adapter adds no second mapping for them.

### Acceptance

- `arch-m3z.1` Review (T4).

## `arch-0ev` The store adapter shall own the transaction

`Journal::write` opens one `BEGIN IMMEDIATE` transaction, runs the unit of work, and commits
only if it returns `Ok`. The shell never sees a connection. SQLite runs in WAL mode; the
durability setting is the composition root's choice between `Durability::Full`, which syncs to
disk on every commit, and `Durability::Normal`, which survives a process crash but can lose the
last commits on power loss and was measured about a hundred times faster per commit.

### Acceptance

- `arch-0ev.1` A unit that fails stores nothing ([`arch-0yc`](invariants.req.md)).

## `arch-qoy` The wire crate shall exist only once a consumer outside the process exists

Another process, another team or a published bus is the trigger. Minting it is one task that
ships together: the crate, its role in `hex-lint`, and the wire-contract gates. A wire crate
without its gates is worse than none.

### Acceptance

- `arch-qoy.1` The role check aborts on an untagged crate (T2).

## `arch-blp` Wire events shall be a separate, versioned published language

Once `wire` exists: it depends on nothing internal; internal events never serialize onto a
public edge; the internal-to-wire mapping is total and versioned
([`arch-7k8`](invariants.req.md)). Consumed wire enums are tolerant readers:
`#[non_exhaustive]` plus a `#[serde(other)]` fallback variant, the one place `#[non_exhaustive]`
is correct. JSON or protobuf on public edges, where humans and other teams live; `postcard` or
CBOR only on links you fully control.

### Acceptance

- `arch-blp.1` The criteria of [`arch-7k8`](invariants.req.md).
- `arch-blp.2` The `tolerant-reader-audit` run flags a closed Rust `enum` over a growable schema `enum` (CI).

## `arch-0bd` Tracing shall live in rings 2 and 3 only, around the pure island

One span per request and per process-manager step, with the aggregate id, the command type and
the decision outcome as fields. The core stays `tracing`-free. Because `decide` is pure, log
`(state, command) → outcome` around it and replay a captured failure exactly in a test; no log
lines inside the logic. Redact secrets through the types' `Debug` impls and cap the payload
size. Thin CRUD paths are excluded from the replay claim.

### Acceptance

- `arch-0bd.1` The `pure-deps` allowlist keeps `tracing` out of the core (T2).
- `arch-0bd.2` Review elsewhere (T4).
