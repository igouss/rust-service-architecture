---
prefix: arch
---

# Structure: crates, roles and the dependency graph

For long-running services on server hardware: an OS, an allocator, a network stack, a database,
many concurrent requests. The scarce resources are developer comprehension, operational
debuggability and tail latency, in that order; allocation is not one of them. Every rule below
carries the gate that enforces it today, with its tier: T1 the compiler or Cargo, T2 a
workspace-graph check, T3 a source lint, T4 review. A T4 rule is a bug report against the
tooling ([enforcement](enforcement.req.md)).

## `arch-4xd` Each crate shall declare one role and one bounded context

Roles are `hex-lint`'s exact vocabulary: `kernel`, `domain`, `port-and-adapter`, `usecase`,
`driving-adapter`, `driven-adapter`, `infra`, `composition-root`. Crate names in this guide map
to roles: `domain` → `domain`, `ports` → `port-and-adapter`, `app` → `usecase`, `adapter-*` →
`driven-adapter` or `driving-adapter`, `bins/server` → `composition-root`. `"shared"` is the
reserved context for `kernel` and `infra`.

```toml
[package.metadata.hex-arch]
role = "usecase"
context = "orders"
pure-deps = []
```

| Role | Ring | Contents |
|---|---|---|
| `kernel` | 0 | value types shared across contexts (ids, money); context `"shared"` |
| `domain` | 0 | lifecycles, commands, reactions, projections, refined types |
| `port-and-adapter` | 1 | port traits and the types that cross them |
| `usecase` | 2 | the handler, policies, queries, the relay |
| `driving-adapter` | 3 | HTTP and scheduler entry points; DTOs and their mapping |
| `driven-adapter` | 3 | port implementations |
| `infra` | 3 | domain-blind tech glue: pool builders, tracing setup |
| `composition-root` | — | `main()`: builds the adapters and plugs them into generics |

### Acceptance

- `arch-4xd.1` The `hex-lint` run aborts on a missing or unknown role (T2, pre-commit and CI).
- `arch-4xd.2` Review checks that a crate's role matches its shape until `hex-lint` cross-checks it (T4→T2, gap ledger).

## `arch-jxh` Every dependency edge shall conform to the role matrix

Dependencies point inward. Adapters are leaves: no adapter depends on another adapter, and only
the composition root depends on an adapter. Driven adapters never see `usecase`, so editing a
use case rebuilds no driven adapter. `infra` depends on nothing internal, so it can never become
business logic under a tech label. The table is `hex-lint`'s hard-coded matrix; when the two
disagree the tool wins (`hex-lint --capabilities` prints it).

| from \ to | kernel | domain | ports | usecase | driving | driven | infra | root |
|---|---|---|---|---|---|---|---|---|
| **kernel** | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| **domain** | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| **ports** | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ |
| **usecase** | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ |
| **driving** | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ |
| **driven** | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✓ | ✗ |
| **infra** | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | ✗ |
| **root** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | — |

`[dev-dependencies]` are exempt by tool design: anything may dev-depend on fakes, and a
`usecase` crate may dev-depend on a real adapter for an integration test.

### Acceptance

- `arch-jxh.1` Cargo rejects a use of a crate that is not a declared dependency (T1, every build).
- `arch-jxh.2` The `hex-lint` run rejects a declared edge the matrix forbids, with all features on (T2, pre-commit and CI).

## `arch-yr1` Bounded contexts shall not depend on each other directly

An edge is legal only when both crates share a context or the dependency's context is
`"shared"`. Cross-context integration is a `kernel` type or, once a wire crate exists, events on
the wire; never a direct call. Domain types never cross a context boundary. Declare `context` on
every crate from day one: adoption is all or nothing.

### Acceptance

- `arch-yr1.1` The `hex-lint` context axis rejects a cross-context edge (T2, pre-commit and CI).

## `arch-u8nl` A bounded context shall be built from the named primitives only

| Primitive | Rust form | Crate | What it may do |
|---|---|---|---|
| Lifecycle | `impl Lifecycle` on a state `enum` | `domain` | fold facts into state |
| Command | `impl Command` | `domain` | read its lifecycle, return facts or a refusal |
| Reaction | `impl Reaction` | `domain` | one process-manager step: facts and requests |
| Projection | `impl Projection` | `domain` | fold facts into the rows of one read model |
| Policy | `impl Policy for <Lifecycle>` | `app` | name the readers of that lifecycle's facts |
| Query | a function over `Journal::read` | `app` | answer from state rows or a read model |
| Port and adapter | a trait in `ports`, an `impl` in an adapter crate | `ports`, `adapter-*` | I/O |

`app::handle`, `app::react`, `app::project`, `app::rebuild` and `app::relay` are the fixed
machinery that runs the primitives; a service does not add to them. New behaviour is a new
instance of a primitive. Code that is none of them (a service object that holds state, a helper
that writes, a scheduler that does the work) is wrong and is reshaped into one, never
accommodated. An open set of shapes is how a codebase drifts into competing patterns, under a
team or under agents; with a closed set, review is recognising a form.

### Acceptance

- `arch-u8nl.1` Review checks that every public item of `domain` and `app` is one of the primitives or a type one of them names (T4→T3, gap ledger).

## `arch-izj` A workspace shall start with six crates and grow only on a trigger

Start with `domain`, `ports`, `app`, `adapter-sqlite`, `adapter-http` and the composition
root `server`. Add a crate only when its trigger fires:

| Crate | Trigger |
|---|---|
| `kernel` | a second bounded context, or types two contexts genuinely share |
| `infra` | two adapters share tech glue |
| a port conformance suite | a second adapter for the same port |
| `wire` | a consumer outside this process ([adapters](adapters.req.md)) |
| a second four-ring stack | a genuinely separate bounded context; the contract between stacks is the wire schema |

One `domain` crate per bounded context, not per aggregate. No grab-bag crate (`utils`,
`common`, `shared`): shared tech glue goes in `infra`, shared domain types in `kernel`. Split
into separate deployables only for an independent-scaling or independent-deploy need; the crate
graph already gives enforceable boundaries inside one binary.

### Acceptance

- `arch-izj.1` The role matrix makes a grab-bag crate unable to serve both ends (T2).
- `arch-izj.2` Review checks each added crate names its trigger (T4).

## `arch-pcn` The composition root shall be main()

No runtime DI container, no reflection-shaped registry, no macro that wires the graph at
startup. `main()` constructs the adapters and plugs them into generic parameters, so the wiring
stays explicit and greppable.

### Acceptance

- `arch-pcn.1` Review (T4).
