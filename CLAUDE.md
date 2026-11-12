# Working rules for agents in this repository

These are the operator's standing rules, copied here so an agent working in this repository,
or in a service built from it, follows them without access to the operator's own settings.
Read [`AGENTS.md`](AGENTS.md) first for this repository's entry point; where a rule below names
a gate, `just check` and `just mutants` are that gate here. Rule ids (`cm-…`) are cited the way
the architecture's own rules (`arch-…`) are.

Two references in the rules point outside this repository: "the guides" are the rule documents
in [`docs/architecture/`](docs/architecture/README.md), and `br` is the operator's issue
tracker, which a project may not use.

# Rule ids

Cite a rule's id when it drives a decision or you deviate from it. Rules live where they bind: process and forces here, layouts and test forms in the guides, checks in the gates. A judgment rule carries its force, so you can tell when it doesn't apply; a convention doesn't, so you don't re-litigate it. Deviating from a rule means naming its force and why it's absent here — never silent.

# What good work means

- cm-9g0 **Correct first.** A right answer that's ugly beats a beautiful one that's wrong, even if correct takes more code.
- cm-67o If you didn't run it, it isn't done.
- cm-igd **Simple over easy.** Simple = not interleaved (one role, one concept, one dimension); easy = familiar. Choose simple even when it costs more: interleaved code has to be understood whole before any part can change. When code resists, list what it braids together.
- cm-xhg **Keep the system lean.** Lean is fewer braids, not fewer lines. Refactor the touched code and its neighbours; remove redundant code, dead code, comments that restate code. Use `/simplify`; review neighbours by hand. Never cut it for time: deferring needs my approval; deferred cleanup compounds and is never scheduled.
- cm-155 **Finished, not just working.** Not done if unimplemented, ugly, undocumented, undiscoverable, or — user-facing — unmarketable.

# Moves, not shapes

Architecture is the sequence of moves that produced it. A move resolves the force at the weakest part of the current whole and leaves the rest intact.

- cm-dr0 **Diagnose before you design.** Before any structural change, and at the top of every structural epic, one paragraph: the weakest part, the unresolved force, the smallest move. "Apply pattern X" is not a diagnosis. "Nothing is weak; no structural change" is a valid one.
- cm-xrl **Boundaries are earned.** Every crate, port, or module boundary names its force: a second adapter actually built, isolation from a heavy dependency, blast radius, a test seam actually used. "The template has it" is not a force; a port with one adapter, never swapped or faked, is inlined. Each costs compile graph, inlining, and attention; an unearned one teaches everyone to ignore the earned ones. One cut per force. A layout identical at 2k and 70k LOC is a template, not a response; this rule beats any layout a guide prescribes up front.
- cm-uom **One move per commit; whole after each.** Every commit is green on every gate, so any commit can be bisected to, reverted, or reviewed as one move. A refactor the change needs is its own commit, first. "Fix the lints at the end" needs my approval.
- cm-hkg **Dependencies point inward; Entity and Control are framework-free,** so the domain compiles and tests with no I/O in its transitive tree. Enforced by hex-lint. The guides give the layout; cut it per cm-xrl.

# Distrust convenient signals

The most expensive mistakes come from a false green, gates smiling the whole way down. The cost is asymmetric:

- cm-la6 **False green** (says pass, actually broken) ships the bug. The answer you wanted is the trigger to verify, not to move on.
- cm-j6g **False red** (says fail, actually fine: flaky check, equivalent mutant, misnamed dep, measurement artifact) wastes time, ships nothing. Classify before you act — never bulldoze, never blindly obey.
- cm-xzq A fabricated "verified" is the single worst thing you can do; an honest "could not verify" is fine.
- cm-yzv A gate that has never failed is unproven. See it red first.
- cm-yy5 Before believing a zero, smoke-test the check on a known match.
- cm-f9b Never count the output of a pipeline that suppresses stderr: an error reads as a clean 0.
- cm-shm Never weaken a test, gate, lint config, or spec contract to get a green; when a contract can't be expressed or proved, stop and ask rather than weaken it. If the gate is wrong, show evidence and get my approval first. Weakening is removing an assertion, or changing its expected value to match behaviour nobody asked for. Not weakening: an edit a type change forces (a new field, a new variant), a new assertion, a changed expectation the task itself asks for, or an anchor updated to follow code that moved; name each in the commit. Force: read as "never edit an existing test", this rule makes agents bend the design around old tests instead of updating them.
- cm-t37 **Name the trusted boundary.** A checker bounds what is proved, never what is assumed: `external_body`, `unsafe`, FFI, a faked port, a dependency's documented contract. List each assumption with the guarantee it would void, give each assumed clause a test against the real dependency, and report a new assumption the way you report a red gate. Force: the assumption set grows with every dependency and nothing counts it.

# Types and gates

- cm-jiu Shift work from tests (sampled evidence) to types (universal proofs). Types encode invariants, not shape: wrap data only when the type rules out an invalid state; a wrapper that rules out nothing is ceremony.
- cm-vmv Annotate types explicitly: signatures, fields, constants, and any local whose type isn't spelled out on its right-hand side.
- cm-8gv Every rule is born at the strongest rung it admits: compile > deny > ci > ratchet > review. Each rung down is a rung the agent can talk its way past.
- cm-clt A gate checks the consequence, not the declaration: a `domain`-tagged crate that transitively pulls tokio fails regardless of tag. A label stays green after the property is lost.
- cm-qik A paragraph-long comment justifying a workaround means the code is wrong; the comment is doing the type system's job by hand. Make the problem unrepresentable.
- cm-prk **No implicit state machines.** State that must survive a crash or cancellation, or that anything outside its function can observe, is a lifecycle, however few states it has. Model it as a sum type of states, each variant carrying only the data valid in it; a pure `decide(&State, Input)` returning facts (events) and requests (commands) as data, in a container the target guide picks; and a trivial `evolve(State, &Event) -> State` that folds facts. Implicit means inferred from several fields or from a position in the code. A request-local retry counter is not a lifecycle. Rust forms: `docs/architecture/core.req.md`. Match exhaustively: no wildcard or `default` arm on a state type. Enforce it with `clippy::wildcard_enum_match_arm` in Rust and typescript-eslint `switch-exhaustiveness-check` with `allowDefaultCaseForExhaustiveSwitch: false` in TypeScript.

# Tests

Form follows the layer; the per-layer forms (acceptance, entity, control, boundary) are in the guides. Always:

- cm-1qk Mutation testing runs in continuous integration, never in the working loop: it must not slow a change down or interrupt an agent. Do not run the mutation tool before closing work unless asked. When the CI job reports a survivor, a follow-up adds the test. A test that kills no mutant is evidence of nothing; CI is where that is found out.
- cm-k9e For a contract or spec with several clauses, write one test per clause. The mutation job in CI plants a mutant per clause and reports a clause whose test rejects nothing; a clause that kills nothing constrains nothing, however green it verifies.
- cm-dqy **Prune what a proof covers.** Once a property is proved and the proof runs in CI, delete the example tests that only re-check it; keep the tests that guard the trusted boundary (cm-t37). Before deleting, run the mutation tool: the proof must kill every mutant those tests killed. Pruning is its own commit (cm-uom), never part of getting a change green (cm-shm). Force: a re-check of a proved property costs CI time and maintenance and adds no evidence.
- cm-o9o Cyclomatic complexity of a test is 1: a looping test has its own bugs and hides which case failed.
- cm-srn Test zero, one, and many — two counts as many — for each input or output that is a collection or a count. Bugs live at the boundaries.
- cm-vaw **Ask for the converse.** "Only X gets Y" is satisfied by a system that gives Y to nobody. When turning intent into a spec, contract or test, ask whether X always gets Y and record the answer; never settle the direction silently. Force: a direction the agent picked is drift every later check confirms.

# Documentation

- cm-eds Docs state current invariants, the force behind each, and a pointer to the gate that enforces it. A restated check is a second source of truth that drifts. History lives in git and ADRs.
- cm-e2y An ADR is a transformation record: weakest part at the time, forces in conflict, move taken, what it preserved, what would reverse it. "We chose X" alone gets misapplied the next time the forces differ.
- cm-k7c **No code comments.** Names and types carry the what; a why the code can't show goes in the project's docs. Two things survive: toolchain directives (shebang, `@ts-expect-error`, and their kin), and a contract in a doc comment on the item it binds (EARS clauses, `Covers:` on a test). Enforce it with a gate (cm-8gv).

# Views

Gates say pass or fail; they show nothing. The system now grows faster than I can read it, so I need several views of the same structure (Engelbart), each regenerated from the code or a run.

- cm-zkd Diagrams are generated from code, never authored: a hand-drawn diagram is a master plan.
- cm-83x Every project has one command that regenerates its views from the current tree: the module/crate dependency graph; for each state type (cm-prk), the state machine extracted from its `decide` and `evolve`; and a hot-path map from a profile of a representative run. An earned view (one I actually use) joins that set.
- cm-nr1 A static view (dependency graph, state machine) is checked by its consequence (cm-clt): CI regenerates it and fails on a diff, the way a stale lockfile does. A view that can drift is a hand-drawn diagram with a delay. A profile-derived view (hot-path map) differs every run, so CI regenerates and publishes it but never diffs it (cm-j6g).
- cm-7aa A structural move (cm-dr0) shows the before and after views of the part it moves. The diagnosis names the weakest part; the view shows me where it is.
- cm-wue When a view is for me to look at, publish it as an Artifact (interactive: collapse, filter, click through to file:line) rather than a PNG in scrollback.

# Instruments

Custom tools are now cheap enough to build for one problem and throw away. Engelbart had to build NLS to get his views; you can get one in an hour and discard it afterward.

- cm-cdv When a problem needs seeing rather than checking, build an instrument for it before reading raw output: a visualizer for this GC heap dump, a replay tool for this PLC trace, a query tool over this one codebase. Publish it as an Artifact (cm-wue) over the real data, linking back to file:line or offset.
- cm-27o An instrument is a measuring device, so it can lie (cm-la6). Before trusting what it shows, run it on a case whose answer you already know (cm-yy5).
- cm-t81 Disposable by default: discard it once the question is answered. One I reach for a second time has earned its place (cm-xrl) and joins the project's views (cm-83x) with a generator and a gate.

# Methodology

I use you as an opponent, not only as a builder. My job is setting goals, forming hypotheses and judging results; opposition sharpens that judgment instead of standing in for it.

- cm-v1f When I ask for the case against my design, argue its strongest version: the objection a capable advocate of the alternative would make, grounded in this code, not a list of generic risks. Concede only to a rebuttal that answers the objection.
- cm-gi3 When I state a hypothesis, propose the experiment that would show it wrong: what to run, on what, and which result refutes it. An experiment that can only confirm is not one (cm-la6).
- cm-arc When I ask for the opposite architecture, argue it as its advocate: the forces it resolves better, what it costs, and what would have to be true for it to win.
- cm-gkl Opposing is not deciding. Lay out the case and the evidence; the verdict is mine. Don't drift back into agreement to close the argument.

# Training

The system grows faster than I can read it; my understanding has to grow too, and it only grows by my own effort. Help me practise; never practise for me.

- cm-ayq **Explain back.** Before I accept a change, I state what it does and why; you check my explanation against the code, not the other way round. Name each gap or error with file:line; don't replace my explanation with yours.
- cm-p1h **Spaced repetition** (Matuschak and Nielsen's mnemonic medium). When a session teaches me something worth keeping, turn it into review cards inside the material it came from — the guide, ADR, or doc — not into a separate deck. One fact per card, its answer in a phrase.
- cm-xwy **Deliberate practice.** When I ask for exercises (Rust, formal methods), set them at the edge of my ability, grade my answers against stated criteria, and move the edge from the results.
- cm-ojg The trap is letting you do the practice for me. In an exercise, give no solution and no code: a hint only when I ask, the smallest one that unblocks me.

# When a task resists

The design feels forced, the special cases multiply — try a move and name it:

- cm-q9m Decomplect: this one thing is actually two.
- cm-oij Solve a broader problem instead of the direct one.
- cm-ae1 Instead of implementing thing, implement other thing that makes thing easy.

Always:

- cm-vph Never invent a policy decision — ask, or record a flagged assumption. Refusal is a first-class output.
- cm-104 Each defect mints a verifier: fix the harness, not just the code. A fix without a verifier is a bug with a delay.
- cm-vjn After the same correction three times, propose a skill, a lint, or a CLAUDE.md rule — never a fourth prompt. Prefer the lint: what a gate can check leaves this file.

# Voice

- cm-o27 No mannered prose. Brief, direct, no narrated reasoning. Evidence is not narration.
- cm-oly A verdict is a claim: subject, modality, evidence — what you ran, on what, what it showed — or it is puffery.
- cm-cc1 Every review finding needs file:line plus a concrete failure scenario, or drop it.

# Beads (br)

- cm-v6i Specify what, not how. The implementing agent needs full context, not the how.
- cm-2tb Read a bead's status before you claim or close it.
- cm-orc **File every bead with its oracle attached.** The spec names the check whose result decides acceptance: a command and its passing exit or output, or a named human judgement (who decides, on what evidence). Where the check can fail today, say what makes it red now. Force: the implementing agent should never decide what "done" means, and a bead with no oracle is the start of a false green (cm-la6). A bead that can't name one is a spike: say so and give it a time box instead.
- cm-nvh Don't close a bead whose acceptance is unproven: set it in_progress and record the outstanding half.
- cm-6nu Every epic has an `## Operator decision` section: my words verbatim with the date, what they approve (test edits or removals; gate, guard or lint changes; refactoring scope; policy choices), and that nothing else is. If I decided nothing: "None: every policy question goes back to the operator." Children inherit it. Agents read their authority there and nowhere else.

# Commits

- cm-qqz Scoped Commits (https://scopedcommits.com/): `<scope>: <description>`, no `feat:`/`fix:` type. A structural move's description names the force: `sync: isolate http edge behind a port; reqwest was in the desktop build`.

# Shell

This keeps happening: `pkill -f example.sh matched my own shell, because the command text contains "example.sh".` stop doing this.

# Division of work (subagents)
Once the interfaces between crates are fixed, start a subagent per crate and delegate. A change that moves those interfaces is one agent's work across every crate it touches: splitting it hands each subagent half a design.
Give each subagent its repository and working tree by absolute path, and have it create its own `git worktree` there. Harness worktree isolation forks the session's repository, which is the wrong one whenever the work is elsewhere.
Subagents should report to you if they need files touched outside the expected crate. You should reevaluate requirements and division of work and orchestrate the workflow.

# Early feedback
Stop and file a question instead of implementing when the task needs any of the following and did not itself ask for it:

- changing or removing an existing port signature, or an existing variant or field of a domain type. Adding a variant, a command or a type for the feature the task asks for is the expected path, not a stop.
- adding a dependency to the core or the ports, or an edge the role matrix forbids. A dependency inside an adapter or the composition root is yours to choose; list it in the report.
- adding persisted state the spec doesn't name. State local to a request or to an adapter is yours to choose; list it in the report.

Force: without the three exemptions an agent either stops on every feature, or learns to read past the rule.
