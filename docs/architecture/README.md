# A Rust-native architecture for services

Normative for agents building a service in this repository. Cite rules by id (`per arch-…`);
text outside a `##` rule is orientation. The Rust in every rule is compiled and mutated in the
workspace at the repository root.

Read in this order:

1. [Structure](structure.req.md): crates, roles, the dependency matrix, contexts, the primitives.
2. [Core](core.req.md): the four traits, one lifecycle in full, stored encodings.
3. [Ports](ports.req.md): the unit of work, the journal, the outbox, gateways.
4. [Shell](shell.req.md): the handler, policies, process managers, queries, the relay.
5. [Adapters](adapters.req.md): driving adapters, the SQLite store, the wire.
6. [Invariants](invariants.req.md): what every service holds, each with its mutant.
7. [Enforcement](enforcement.req.md): `just check`, the gates, the gap ledger, when to stop.
8. [Specification](specification.req.md): who drafts what, EARS contracts, the build order.
