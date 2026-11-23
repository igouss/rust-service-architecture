# 0031. The toolchain is the newest stable release, pinned exactly

Date: 2026-11-23

## Operator decision

"let's use latest rust (nightly, syn 3)" (operator, 2026-11-23) asked for nightly, and record
[0023](0023-the-toolchain-is-a-dated-nightly.md) took it. A review the same day named that as
a decision to look at again, and the operator answered "Okay, don't go easy on those
problems. Let's fix them all." This record reads that answer as taking the review's
recommendation, stable. It is the last commit of the work so that it can be dropped alone if
the reading is wrong. `syn` 3 stays. Nothing else is approved.

## Weakest part at the time

The toolchain pin. Record 0023 moved from 1.95 to a nightly dated the day it was written, for
newer lints and a newer formatter. What the move found was one deprecated call and no lint
finding. What it cost is carried by every service built from this template: a compiler from a
branch that makes no promise, and 0023's own condition for reversal, "a need to build on a
machine that only carries stable", is the ordinary case for a template.

## Forces in conflict

- The newest lints are the strongest gate; they arrive on nightly first and on stable six to
  twelve weeks later.
- This repository uses no unstable feature, so nightly buys only that head start.
- A gate's verdict must be reproducible: an exact pin either way.

## Move

`rust-toolchain.toml` and the three jobs of the workflow name `1.100.0`
(0ef1bc386, 2026-11-09), the newest stable. `just check` and `just postgres-local` pass on
it with no change to any source file: `try_update`, which 0023 moved to, is stable in 1.100,
and `cargo fmt --check` shows no difference between the two formatters on this tree.

## How the pin is raised

As record 0023 set out, with a version in place of a date: one commit, the version in
`rust-toolchain.toml` and the three `toolchain:` lines, `just check` and `just postgres`
green, anything the new compiler names fixed in a commit of its own first, and a line below.

Raises (date, old, new):

- none yet.

## What it preserved

Every gate and recipe, an exact pin, no `#![feature]`, and the code as nightly left it.

## Not gated

Nothing fails when `rust-toolchain.toml` and the workflow name different versions. Review
reads both.

## What would reverse it

A lint that exists only on nightly and that this code needs to be read by; or the operator
saying that nightly was meant.
