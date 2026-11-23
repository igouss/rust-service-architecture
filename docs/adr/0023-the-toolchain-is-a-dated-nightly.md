# 0023. The toolchain is a dated nightly

Date: 2026-11-23

Status: reversed by [0031](0031-the-toolchain-is-the-newest-stable.md)

## Operator decision

"let's use latest rust (nightly, syn 3)" (operator, 2026-11-23). It approves moving
`rust-toolchain.toml` and the workflow from 1.95 to nightly, and the edits the new compiler
forces. The words say nightly; they do not say whether the channel floats or names a date.
A date is used here and is a flagged assumption (`cm-vph`), for the reason below. `syn` 3 is
recorded in [0022](0022-the-gates-are-a-workspace-crate.md). Nothing else is approved.

## Weakest part at the time

The compiler, `clippy` and `rustfmt` are gates, and they were seven releases old: 1.95 of
2026-04-14, with stable at 1.100. The pin was exact, so nothing drifted, and nothing arrived
either: a lint added since April had never read this code.

## Forces in conflict

- The newest lints and the newest formatter are the strongest gate available today.
- A gate's verdict must be reproducible: the same commit, the same answer, today and at
  bisect. `channel = "nightly"` is whatever was published the day a machine last updated, so
  a commit could turn red, or reformat, with no change to it. That is the force decision
  record [0017](0017-gate-tools-are-pinned.md) settled for the gate tools.
- A nightly can break, and can change what unstable code means. This repository uses no
  unstable feature; it takes the nightly's compiler, lints and formatter only.

## Move

`rust-toolchain.toml` names `nightly-2026-11-23`, which is
`rustc 1.102.0-nightly (137d1d494 2026-11-22)`, the newest nightly on the day. The three
jobs of `.github/workflows/ci.yml` install the same date. What the newer compiler asked for:

- `adapter-http/tests/routes/support.rs`: `fetch_update` is deprecated and renamed
  `try_update`; the call is renamed, same arguments, same result. A test helper, no assertion
  touched.
- Nothing else. `cargo fmt --check` shows no diff, `clippy` finds nothing more, every test
  passes, and `hex-lint`, `effect-audit` and `req` read the tree as before.

Not changed, and reported: cargo now warns that `repository` in `[workspace.package]` is
inherited by no member. It is a warning, not a failure; whether to drop the field or have
the members inherit it is the operator's.

## How the date is raised

One commit, nothing else in it: change the date in `rust-toolchain.toml` and the three
`toolchain:` lines of the workflow, run `just check` and `just postgres`, fix what the new
compiler names in a commit of its own first if it names anything, and add a line below.
`guarded` fails a range that changes the workflow and touches no decision record; this list
is the record a raise amends.

Raises (date, old, new):

- none yet.

## What it preserved

Every gate and every recipe. An exact pin: one toolchain for every machine and for
continuous integration. No `#![feature]` anywhere.

## Not gated

Nothing fails when `rust-toolchain.toml` and the workflow name different dates, or when the
channel is written without a date. Review reads both.

## What would reverse it

A nightly that miscompiles or that no later date fixes; a need to build on a machine that
only carries stable; or the first `#![feature]`, which would tie the code to the channel and
deserves its own record. Then the pin goes back to the newest stable, by the same procedure.
