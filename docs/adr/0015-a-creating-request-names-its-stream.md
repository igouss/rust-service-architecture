# 0015. A creating request names its stream with an idempotency key

Date: 2026-11-23

## Operator decision

"A retried `POST /tasks` creates a second task. Add an idempotency mechanism for commands
arriving over HTTP, designed in a decision record first." (operator, 2026-11-23). No change to
`ports` is approved for this move. It approves the routes' new header, the tests and the
mutants named below, and nothing else.

## Weakest part at the time

The two routes that start a stream. `POST /tasks` and `POST /payments` mint the stream id in
the adapter, so a request that is sent twice (a client retry after a lost answer, a proxy
replay) is two ids and two records. Seen red: two posts carrying one `Idempotency-Key` both
answered 204 and left two tasks. Every other command names an existing stream and is already
safe to repeat: the state it changed refuses the second one (`AlreadyDone`,
`NotAuthorized`).

## Forces in conflict

- The caller must be able to repeat a request without knowing whether the first arrived.
- `arch-7h52`: state whose keys the outside world mints without limit and that may be
  forgotten is not a lifecycle. A table of seen keys is exactly that.
- `arch-s6q`: one way to write. A key recorded beside the command, outside its unit of work,
  can be recorded without the command or the command without it.
- No port change is approved here, and a unit of work cannot read a state row except through
  `apply`.

## Options

1. **The key is the stream id.** The caller sends `Idempotency-Key: <uuid>`; the adapter uses
   it as the id of the stream the request creates. A repeat meets the existing stream and the
   lifecycle refuses it. Gives up: the repeat is answered 409 with the refusal's name, not a
   replay of the first 204; every repeat records a refusal row.
2. **A receipt per key that replays the first answer.** A lifecycle keyed by the key stores
   what was answered; the shell checks it and handles the command in one unit. Gives up
   `arch-7h52` (an unbounded, forgettable lifecycle), adds persisted state the specification
   does not name, and needs the answer of every command to be storable.
3. **A cache of keys in the adapter.** Gives up atomicity: a crash between the commit and the
   cache, or a restart, lets the repeat through.

## Move

Option 1. `created` in `adapter-http` reads the header: no key mints an id as before; a key
that is a UUID becomes the stream id; anything else is answered 400 `MalformedKey` before a
command exists. Both creating routes use it. This is `arch-dohm` applied to the request: the
value that must be unique is the stream, and the refusal is a recorded domain value with a
name. Nothing is stored that was not stored before.

A caller that repeats and is answered 409 `AlreadyExists` under a key it chose knows its
first attempt arrived.

## Not done, flagged for the operator

- **The page sends no key.** Its form posts through Datastar; a key per submission needs the
  action's `headers` option, which could not be exercised here without a browser, and the
  form's markup is pinned by `the_page_posts_the_title_as_a_form`. Until the page sends one, a
  retried submission from the page still creates a second task.
- **A replay of the first answer** (option 2) if 409 on a repeat is not acceptable. It needs
  the operator to allow the stored receipts.
- A request without a key is accepted and is not safe to repeat. Requiring the key on every
  creation is a one-line change and a decision about existing callers.

## What it preserved

Requests without the header behave as before; no existing test changed. Mutant H2 follows the
mint into `created`. New: H10 and H11, applied by hand and seen to die as stated.

## What would reverse it

Option 2, once the operator accepts stored receipts with a retention rule (decision record
0016 proposes one for refusals; receipts would need the same).
