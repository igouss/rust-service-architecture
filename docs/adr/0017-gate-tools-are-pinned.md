# 0017. The gate tools are pinned, and a pin moves only after `just check` passes with the new build

Date: 2026-11-23

## Operator decision

"Pin the gate tools. .github/workflows/ci.yml installs hex-lint, effect-audit and req from git
with no revision. Pin each to a revision, and record how a pin is raised." (operator,
2026-11-23). The quoted words approve the pins and this record. The `pins` gate, its
requirement `arch-mov6`, mutant G27 and the change to `mutants.py` were asked for by the task
that carried the decision, not by the words quoted; they are flagged here for the operator to
confirm. Nothing else is approved.

## Weakest part at the time

The tools behind three gates. `roles`, `purity` and `ids` run `hex-lint`, `effect-audit` and
`req`, which live in other repositories. `.github/workflows/ci.yml` built each with
`cargo install --locked --git <url>` and no revision, so every run built whatever the default
branch held that day. A tool change could turn a commit red, or green, that nobody changed; a
green said nothing about which build of a gate passed it; and the binaries on a developer's
machine could be any revision at all (on the machine this was written on, `hex-lint` was
installed from a local path, not from git).

## Forces in conflict

- A gate's verdict must be reproducible: the same commit, the same answer, today and at bisect.
- The tools improve. A pin that nobody raises freezes their fixes and new checks out, and a stale
  pin is drift in the other direction.
- A raise can loosen a check as easily as tighten one; the repository that pins cannot see that
  from the revision id alone.

## Move

- Each `cargo install --git` in `.github/workflows/ci.yml` names a full 40-character commit
  with `--rev`: the `HEAD` that `git ls-remote` reported for each tool on 2026-11-23.
  `just check` was run green with binaries built from exactly those revisions, on the
  commit the pins were first made on and again after that commit was placed on top of the
  port and gate changes of decision records 0012 to 0015.
- The `pins` recipe (`python3 lint.py pins`, `arch-mov6`) keeps it: it reads every workflow
  under `.github/workflows` and fails on a `cargo install --git` with no `--rev` of 40 hex
  digits. It reads one install to a line, and exits 2 when a `cargo install` or a `--git` in a
  workflow is not on such a line. Mutant
  G27 removes the `req` pin; `mutants.py` now copies `.github` into each scratch tree, since
  a mutant's file is opened before any gate runs.

## How a pin is raised

One tool per commit, and nothing else in it.

1. The person raising it reads the new revision with
   `git ls-remote https://github.com/igouss/<tool> HEAD`.
2. They read what changed, `git log --oneline <old>..<new>` in a clone of the tool, and name in
   the commit any check that the range removes or loosens.
3. They build that revision apart from their usual binaries,
   `cargo install --locked --root <dir> --git https://github.com/igouss/<tool> --rev <new>`,
   and run `PATH=<dir>/bin:$PATH just check` on the commit that will carry the raise. Red
   means the new build finds something: fix the code in a commit of its own first, or do not
   raise.
4. They replace every `--rev <old>` of that tool under `.github/workflows`, run `just check`
   again (`pins` included), and add a line to the list below. `just guarded` fails a range that
   changes a workflow and touches no decision record; this list is the record each raise
   amends.
5. The commit reads `ci: raise <tool> to <short id>; <why>`. The reviewer checks the line
   added below against the commit's `--rev`, and the range read in step 2.

Raises (date, tool, old, new):

- none yet.

## What it preserved

The same three tools, installed the same way and run by the same recipes. The pinned revisions
are what an unpinned build would have produced on 2026-11-23. Not covered: the actions are pinned by tag, `just` and `cargo-mutants` come from
`taiki-e/install-action` with no version, and the Postgres service is the `postgres:18` tag.
Pinning them is a choice of version, not made here. Babashka, which this list named as installed
by a script piped from its `master` branch, is no longer installed: decision record
[0022](0022-the-gates-are-a-workspace-crate.md) ported the one gate that needed it.

## What would reverse it

The tools published with versions: `cargo install --locked <tool> --version =<x>` would pin
them, and `pins` would check that form instead. The tools moved into this workspace would
take them out of the workflow altogether.
