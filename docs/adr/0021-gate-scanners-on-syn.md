# 0021. The source gates read Rust with a hand-written lexer; a `syn` tool could parse it

Date: 2026-11-23
Status: proposed

## Weakest part

The reader under the source gates. `comments`, `allows`, `wildcards`, `traits` and `stored`
(`lint.py`), `events.py`, `views.py` and `raw-store.py` read Rust through `workspace.py`: a lexer that blanks
strings, raw strings, characters and comments (`workspace.py:45-114`) and pattern readers
that cut match arms by bracket depth (`:121-154`). Each fails closed with `Unread` when the
counts disagree (`lint.py:51`), and some forms are refused to suit the reader: "put the value
of the arm before it in braces" (`workspace.py:150`). It has been wrong in ways a parser is
not: 6d04297 fixed an arm cut at a generic comma that hid the next arm's pattern, and a
`true`/`false` element flagged as a catch-all.

## Option A: keep the Python scanners

Gives up: a reader that knows Rust's grammar. Each new form is either taught to the reader
or refused with `Unread`, and the code bends to the scanner. Keeps: no build step; each
`lint.py` check, `events.py` and `views.py --check` took 0.02 to 0.14 s here; a gate edit is a Python edit, which `guarded.py` already
watches; nothing joins the compile graph.

## Option B: one Rust tool built on `syn` that the recipes call

Gives up: a build and a dependency on `syn` and what it pulls in, either in this workspace,
with a role and a context for `hex-lint`, or in another repository that `ci.yml` installs and
ADR 0017 pins. `syn` parses tokens, in which ordinary comments do not survive, so the
`comments` gate would still need a lexer. Neither form sees code a macro expands to. The
mutants run the recipes or scripts by name (`planted/gates.py`), so they survive if those
names stay.

## Evidence that would decide

- How often `Unread` or a scanner fix appears in history (`git log -- workspace.py lint.py`),
  against how often a code form was changed to suit the reader.
- `just check` time with the tool built in a warm cache, against today: `arch-ajd` asks for
  seconds.
- A port of one gate (`wildcards`) run against G19 and the cases 6d04297 fixed: if `syn`
  reads them with no special case, the grammar is earning its build.
