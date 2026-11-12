# 0001. State rows are the stored truth; facts are their history; one unit of work per command

Date: 2026-11-11

## Weakest part at the time

The readers of the log. The first commit of this repository stored facts only and let process
managers and read models poll a feed from their own cursors. The review of that state
(`docs/reviews/2026-11-11-log-as-truth.md`) found a read model corrupted by a late projector
(finding 2), redelivered facts applied twice (3), cursors tied to no reader (5) and a
save-before-commit nothing tested (6). No adapter could store a process manager at all (1).

## Forces in conflict

- A log as the only stored truth makes every shape derivable and every change a re-fold.
- Whoever runs the service has to be able to open the database and read the current state.
  Event sourcing is unfamiliar to most teams, and a design nobody on the team can inspect or
  debug is a liability however elegant it is. The architecture should stay pragmatic.
- The store is SQLite on one node with one writer, so one transaction can hold everything a
  command causes; polling exists to survive stores where it cannot.

## Move

State rows are the stored truth. `Unit::apply` loads the row, runs the decision, and writes
the folded state, the facts and the refusal. Each lifecycle's `Policy` runs its projections
and process managers inside the same unit. Facts are kept, always, as history and as the
source a read model is rebuilt from; nothing on the request path loads from them.

The storage rule was chosen for familiarity and for being able to inspect state with plain
SQL, not for speed; the measurements below did not decide it.

## Evidence

Three benchmark runs at tag `baseline-2026-11-11` (`bench/results/`), pre-registered in the
project notes before any run:

- under `synchronous=FULL`, a command on a 100-fact stream cost 1.01, 1.02 and 1.01 times the
  state-row control: speed did not decide this;
- under `synchronous=NORMAL` the fold showed: 1.25 times at 100 facts, about 4.7 at 1 000, about
  38 at 10 000;
- a read-model row written in the commit's transaction cost 1.04, 1.00 and 1.02 times a bare
  commit under FULL (1.70 under NORMAL, about 23 µs);
- FULL against NORMAL is about 4 ms against about 40 µs per commit.

## What it preserved

The pure core; one write that takes the whole outcome; recorded refusals; requests through an
outbox; process managers over nested deciders; a stored state that cannot disagree with its
facts, because the only constructor of a transition is the private fold.

## What it closed

Every review finding except 18, and 8 in the rule but not yet in code. The status table in the
review file names the mutant or the type that covers each.

## What would reverse it

A second writer process or a networked database: then a unit of work can no longer hold every
reader, and readers go back to positions in a feed. A lifecycle whose history must be
re-interpreted often enough that migrating state rows costs more than folding facts.
