# 0030. The comments gate reads what lies between tokens, and the compiler's lexer is no longer a dependency

Date: 2026-11-23

## Operator decision

"Okay, don't go easy on those problems. Let's fix them all." (operator, 2026-11-23), in answer
to a review whose fourth item was this dependency. It approves replacing the reader of the
`comments` gate and dropping the dependency, on the terms of record
[0022](0022-the-gates-are-a-workspace-crate.md): the gate reads no less, and no mutant's
fault or expected text changes. Nothing else is approved.

## Weakest part at the time

One dependency of the gates. `comments` read source with `ra-ap-rustc_lexer`, the
compiler's lexer republished with no promise of a stable interface, pinned to `=0.186.0`.
A pin of that kind is never raised without rewriting the caller, so the gate would have gone
on lexing with the Rust of the day it was written while the compiler moved.

## Forces in conflict

- A comment does not survive `proc-macro2`, which is why 0022 reached for a lexer.
- A hand-written lexer of Rust is what 0022 removed: strings, raw strings, characters and
  lifetimes are where it goes wrong.
- The gate may not read less (`cm-shm`).

## Move

`proc-macro2` already lexes the file for every other gate and gives each token the bytes it
covers. Whatever lies between two tokens is blank or a comment and nothing else, so the gate
reads the gaps: `//` to the end of the line, `/*` to its matching `*/` with nesting. No
string, character or lifetime can be in a gap, which is the part a hand lexer gets wrong. A
doc comment reaches `proc-macro2` as an attribute whose span is the comment's text; a token
whose text begins `//` or `/*` is left uncovered so that it is read as the comment it is.
Text in a gap that is neither is exit 2. A file `proc-macro2` cannot lex is exit 2 as
before, and when what stopped it is a `/*`, the line is the one the old reader printed.

## What was compared

Before the old reader was deleted both ran on 11,616 texts and gave the same comments, the
same lines and the same errors on every one: the 94 Rust files of the repository; each of
those with one of six comments planted at every seventh line (a line comment, a block, four
slashes, a block doc comment, a nested block, a trailing comment); and 38 probes, the six of
record 0022 among them, with raw strings, characters, lifetimes, byte strings, a carriage
return before the line feed, a comment at the end with no line feed, a string and a block that
do not end, and an unbalanced bracket. The twelve existing tests pass unchanged; six are
added. G5, G14 and G15 were planted through the runner and die with their text.

## What it preserved

Every line the gate prints and both exit codes, wherever compared. `fc-allow` is read in the
core only, as before.

## What it gives up

The gate now trusts `proc-macro2`'s spans, where it trusted the compiler's lexer; a version
of `proc-macro2` that reported a doc comment's span differently would still read the
comment, since both ways of reporting it leave it in a gap. Frontmatter, which the old reader
was told to refuse, is whatever `proc-macro2` makes of it.

## What would reverse it

A comment form `proc-macro2` lexes and the gap reader does not; then the compiler's lexer
comes back, with a record of how its pin is raised.
