# 0016. A retention rule for recorded refusals

Date: 2026-11-23

Status: proposed. Nothing is implemented; deletion waits for the operator's approval of this
record.

## Operator decision

"Every refusal is stored forever on an endpoint with no authentication. Propose a retention
rule in a decision record. Do not implement deletion without the owner's approval of that
record." (operator, 2026-11-23).

## Weakest part

`Unit::apply` records every refusal (`arch-qve`), and on SQLite and Postgres nothing ever
removes one (`rejections` in `adapter-sqlite/src/lib.rs` and
`adapter-postgres/src/statements.rs`). The routes take no credentials, so anyone who can reach
the port chooses how many rows are written: every `POST /tasks/{id}/complete` on an unknown id
is one `Absent` row, and since decision record 0015 every repeat of a keyed creation is one
more. The memory store already bounds this with `Kept::refusals`
(`arch-7h52`); the stores on disk do not. `arch-7h52` names the driving adapter's limit
(`arch-kpb`) as the bound, and no such limit is built.

## Forces in conflict

- A refusal is kept so that support can say why a request was rejected. That question is
  asked about recent requests.
- A refusal is not a fact: no state and no read model is rebuilt from it, so removing one
  changes no decision.
- Rows written at a stranger's request must be bounded by something the stranger does not
  control.
- A second write path, or a background job that deletes, is a new kind of thing in this
  architecture (`arch-s6q`, `arch-u8nl`).

## Proposed rule

1. **Per stream, the last N.** A store keeps at most `N` refusals for one lifecycle and
   stream; recording the next removes the oldest of that stream, in the same statement group
   and the same unit of work that records it. No job, no clock, no second write path. This
   bounds what one stream can cost.
2. **Overall, the last M.** A store keeps at most `M` refusals in all; recording one past `M`
   removes the oldest overall, in the same unit. This bounds what many streams can cost, which
   matters here because a caller mints stream ids freely.
3. `N` and `M` are settings the composition root reads, as `Kept` is for the memory store,
   and the three stores share one meaning for them. Proposed defaults: `N = 20`,
   `M = 100 000`.
4. No rule by age. An age needs a clock in the store or a scheduler, and neither exists; the
   count already bounds the rows. If a legal period applies to refusals, it is added when the
   scheduler adapter is.

The conformance suite gains the cases (the last `N` of a stream, the oldest overall dropped,
a refusal of another stream untouched) and each store a mutant per bound.

## What each part gives up

- Rule 1 alone leaves the total unbounded. Rule 2 alone lets one busy stream push out every
  other stream's history.
- Both together cost one more statement on the refusal path, which is not the path a healthy
  client is on.
- Any trimming gives up the full audit trail of rejected requests. A service that must keep it
  sets the bounds to the largest value and bounds the callers instead.

## Not part of this rule, and still needed

A limit on refused requests per client at the driving adapter (`arch-kpb`), and
authentication. Retention bounds the rows; it does not stop the requests.

## What the operator is asked to decide

Whether refusals may be deleted at all; if so, `N` and `M`; and whether the port may gain the
setting (a constructor argument on each store, as `Kept` is, needs no new port method).
