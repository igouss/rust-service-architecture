# 0022. The gates this repository holds are one workspace crate, and no Python or babashka

Date: 2026-11-23

## Operator decision

"propose a move to remove python from this project", then, to the proposal that followed, "go
ahead" (operator, 2026-11-23). The proposal named three decisions. The answer is read as
taking its two recommendations: the gates become a crate of this workspace, not a tool in
another repository, and `guide-sync.bb` goes with the Python. The third was a question with no
recommendation (is planting each gate's mutants by hand against the old and the new reader
enough, or is a mutation run in continuous integration wanted at each step); it was not
answered, so hand-planting is a flagged assumption here, not an approval (`cm-vph`). The
answer approves the new crate, its dependencies, the edits to `justfile`, `planted/`, the
workflow and the gate scripts that the port needs, and nothing else: no gate reads less than
it did, and no mutant's fault or expected text changes.

## Weakest part at the time

The code that enforces the rules was held to none of them. Twelve Python files and one
babashka script (1,093 lines) ran fifteen of the recipes; `clippy`, `cargo test`, `comments`
and `allows` did not read them, and they had no tests of their own, so a gate was exercised
only by its planted mutants, in continuous integration. They read Rust through a
hand-written lexer and a match-arm cutter (`workspace.py`) that has been wrong where a parser
is not (6d04297) and that the code sometimes bent to suit; decision record
[0021](0021-gate-scanners-on-syn.md) laid out that cost.

## Forces in conflict

- A gate must not read less after the move than before (`cm-shm`, `arch-tsu`). Every script
  fails closed by comparing a raw count with what it read; a parser that skips a macro's
  tokens would pass what the lexer refused.
- A mutant names the text a gate prints. The text is part of the gate.
- `arch-ajd` asks for seconds. A parser has to be built before it runs.
- A gate edit has to stay visible to `guarded` (`arch-tsu.2`), which watched top-level
  scripts.

## Move

A member crate, `gates` (role `infra`: it depends on no crate of this workspace, so it
builds in a tree where a mutant has broken `domain`), with one subcommand to a recipe. It
reads tokens with `proc-macro2` and structure with `syn`. Three rules hold for every port:

1. The raw count is taken over the whole token stream, macro bodies included, and compared
   with what the parser read; a difference is exit 2, as before.
2. A line a gate prints is built from the source text under a span, never from re-printed
   tokens, so each mutant's expected text is unchanged.
3. One gate to a commit. Before its Python is deleted, the old and the new are run on the
   tree and on each of that gate's mutants planted by hand, and their output and exit codes
   compared. A mutant whose command was `python3 <script>` names the recipe or the binary
   instead; that edit is forced, and what it plants and how it dies stay.

`guarded` counts every file under `gates/` as a gate from this commit on.

## Ported

Each line is one commit: the gate, and what was compared.

- 1beed7c: the crate, its reader and seven tests of the reader; no recipe changed.
- `comments`: the tree, mutants G5, G14 and G15, and six probes (four slashes, a block doc comment, `fc-allow` outside the core, slashes in a string, a block of two lines, a block that does not end) gave the same lines and exit codes from `lint.py` and from `gates`. Comments do not survive `proc-macro2`, so this gate reads them with `ra-ap-rustc_lexer`, pinned to `=0.186.0`: the crate is published from the compiler with no promise of a stable interface.
- `allows`: the tree, mutants G9, G16, G17 and G18, and seven probes (a call named `allow`, an attribute written across lines, an inner attribute in `src`, an attribute inside a macro, one in a doc comment, a test file with one of the two lints, an outer attribute in a test file): the same lines and exit codes. It reads tokens, so an attribute inside a macro is still seen.

## What it preserved

Every recipe's name, exit codes and printed text. The fail-closed counts. The mutants.
`just check` needs no server and no network once built.

## What it gives up

A build: `syn`, `proc-macro2` and what they pull in join the lock file, in a crate nothing
else depends on. The measured cost is recorded with the last port.

## What would reverse it

`just check` no longer running in seconds on a warm cache, or the gates being wanted by a
second repository: then they are a tool of their own, pinned as decision record
[0017](0017-gate-tools-are-pinned.md) pins the others, and `guarded` loses sight of them.
