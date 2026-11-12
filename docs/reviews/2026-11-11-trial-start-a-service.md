# Trial: a fresh agent starts a service from this repository, 2026-11-11

An agent with no knowledge of this repository was pointed at `AGENTS.md` and asked to build a
new service from it, a reading list of bookmarks, with the example domains gone. It finished in
ten commits with `just check` and `just mutants` green (53 mutants, the 33 machinery ones
unchanged), no surviving native mutant, and the server answering over HTTP. It was not merged;
it is a separate repository.

## What it tripped on, and what changed

| # | Finding | Change |
|---|---|---|
| 1 | The outbox, relay and failed-unit mutants need a `Reaction`, and the guide did not say what a service with no process manager does; the agent made an aggregate double as one. | The guide says to keep them alive with a lifecycle and reaction local to the test file. |
| 2 | Files the guide listed as "keep" had to change: the store's tests import the examples, two `lib.rs` files change their `pub mod` lines, the rule documents cite example tests, the server's settings are named after tasks. | The keep-and-replace table is now per path and says exactly what changes in each. |
| 3 | `bench/`, `docs/adr/`, `docs/reviews/` and this guide itself had no stated fate. | The table gives one for each. |
| 4 | Two machinery mutants (G2, G4) were anchored on an example's `pub mod` line. | They are anchored on machinery. |
| 5 | Clippy suggested an `allow` attribute, which the rules forbid and nothing caught. | `just check` rejects an `allow` outside `tests/` (mutant G9). |
| 6 | "Any new dependency" in the recipe's stop list disagreed with the working rules. | The recipe now says a dependency in `domain` or `ports`. |
| 7 | A projection folds only the row and the fact, so a fact has to carry what its views need; the agent found this after choosing an encoding. | The recipe says so at the step where a projection is added. |
| 8 | `cargo mutants` leaves `mutants.out/` in the tree. | Ignored by git. |

## Not changed

- The machinery's own tests still live in files that use the example types, so starting a
  service means porting those tests, not keeping them. A fixture lifecycle owned by the
  machinery's tests would remove that step.
- The path "a surviving machinery mutant tells you what to test" was not exercised: the agent
  ported the tests before deleting the examples, so none survived.
- `adapter-http/src/lib.rs` still mixes the reusable pieces with the task routes.
- The page was driven with curl, not in a browser.
