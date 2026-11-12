---
prefix: arch
---

# Specification: skeletons, contracts and invariants

The operator drafts the structure; one agent per crate writes the bodies. Structure set by a
human sets the test budget: tests on components grow linearly with the program, end-to-end tests
on an agent-picked structure grow exponentially
(Golatkar et al.). A hand-written
skeleton can be wrong; it is revised, never worked around.

## `arch-dn0` The operator shall draft the skeleton and the implementer only the bodies

| Ring | Operator drafts | Implementer writes | Verified by |
|---|---|---|---|
| 0 `domain` | state, event and refusal enums, command structs, refined types, the `decide`, `react`, `evolve` and projection signatures, EARS contracts | the bodies | given/when/then tests |
| 1 `ports` | the traits and the types that cross them | nothing | the compile-time mutants |
| 2 `app` | which projections and process managers read each lifecycle's facts | the policies and queries | use-case tests against the real store |
| 3 adapters | nothing beyond the port trait | everything | the adapter's own tests |

Put the effort into Rings 0 and 1. The form to copy is `domain/src/task.rs`: one file per
lifecycle, holding its state, events, refusals, commands, projections and tests.

### Acceptance

- `arch-dn0.1` Review checks the skeleton is drafted before bodies are written (T4).

## `arch-a2v` Skeleton files shall be read-only to implementing agents

An agent writes only inside its own crate's bodies and tests. A change to the skeleton goes
through the operator. An agent that thinks the skeleton is wrong stops and proposes the change;
it never works around it. When a test fails because the spec was wrong, fix the skeleton first,
then regenerate the affected bodies.

### Acceptance

- `arch-a2v.1` Review (T4).

## `arch-0d2` Specification shall be added in tiers, stopping when the tests pass reliably

1. Signatures: types, traits, `fn` signatures with `todo!()` bodies.
2. Local contracts: EARS `# Requires` / `# Ensures` in doc comments.
3. Global invariants: a `.req.md` document.

A contract is written in EARS, one clause per behaviour, and each clause becomes one test
named after it:

- Event-driven: "While the task is `Open`, when `CompleteTask` is decided, `decide` shall emit `Completed`."
- Unwanted behaviour: "If the task is `Done`, then `decide` shall refuse with `AlreadyDone`."
- State-driven: "While the fulfilment is `Reserving`, when stock is unavailable, `react` shall emit `Abandoned` and request nothing."

A doc comment carries a contract on the item it binds, and nothing else. Every other comment
is banned. The mutation job in continuous integration checks,
after the fact, that each clause's test would catch a change to it.

### Acceptance

- `arch-0d2.1` The comment scan of `just check` rejects any `//` comment that is not a doc comment or a toolchain directive (T3).
- `arch-0d2.2` The mutation job reports no survivor in the file ([`arch-h6j`](enforcement.req.md)).

## `arch-0id` Invariants shall live in a req document with minted ids

One `##` requirement per invariant, in EARS, with `### Acceptance` criteria that each name the
check deciding them (cm-orc). `req mint` assigns the ids; never number by hand. A criterion names its
test; the test does not cite back. Keep the list short. The architecture's own invariants
are [invariants.req.md](invariants.req.md); a service adds its own beside them and runs `req mint`
after each edit.

### Acceptance

- `arch-0id.1` The `req check --out requirements.toon` run exits 0 (CI).

## `arch-saq` A service shall be built inward-out

1. The operator drafts the lifecycle's enums, command structs and signatures with their EARS
   contracts, and names its readers.
2. The domain agent writes the bodies until the given/when/then tests pass.
3. The app agent writes the policy and the queries, tested against the real store.
4. The adapter agent adds the driving-adapter mapping last.
5. The operator reviews the composition root.

Command types, like everything a decision names, live in `domain`.

### Acceptance

- `arch-saq.1` Review (T4).
