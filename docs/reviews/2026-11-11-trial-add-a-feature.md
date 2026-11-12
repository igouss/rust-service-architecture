# Trial: a fresh agent adds a feature from the repository's instructions alone, 2026-11-11

An agent with no knowledge of this repository was pointed at `AGENTS.md` and asked to make a
done task reopenable, through the core, the shell and HTTP, and to report every point where
the instructions left it guessing. It worked on a throwaway branch from `44f1a80`; the feature
was not merged. It finished with `just check` and `just mutants` green and no surviving native
mutant of its own, in four commits.

## What it tripped on, and what changed

| # | Finding | Change |
|---|---|---|
| 1 | The recipe never said the rule documents embed code and must change in the same commit; the `docs` gate's red gave no rule and no fix. | The documents no longer embed example code that every feature edits (the task lifecycle, the policies, the queries); they point to the files. `guide-sync.bb` now prints the rule and the fix. |
| 2 | "Never edit an existing test" forbade edits a type change forces, and bent the design twice: a second query beside `task_board`, and a list omitted when empty to keep a pinned frame. | `arch-tsu` now says what weakening a test is and what it is not; the recipe's stop list cites it. |
| 3 | `mutants.py` anchors must occur once per file, which constrained how code could be written, and no document said so. | `arch-tsu` and the recipe say an anchor may follow moved code and that `ANCHOR FOUND n TIMES` means it must. |
| 4 | The recipe presented the core step as self-contained; a new refusal variant breaks an exhaustive match in the adapter. | The recipe says so and tells you to fix those in the same commit. |
| 5 | The recipe's driving-adapter step was two sentences. | It now covers the route, the status mapping, the span, how the page is fed, and where tests go. |
| 6 | The recipe's "done" list omitted `just mutants`, which `AGENTS.md` required. | Added. |
| 7 | `AGENTS.md` said "a red names the rule"; two of the reds it hit did not. | The claim is gone; `fmt` and `docs` now name their fix. |
| 8 | A commit hook with a file-size budget existed in history and in no document. | `AGENTS.md` states the convention: every file under 20 KB. |
| 9 | Adding a variant looked like "changing a domain type", which the working rules in `CLAUDE.md` then said to stop for. | The recipe says adding for the requested feature is the expected path; removing or renaming a stored variant is the stop. |

## Not changed

- No gate checks prose: a sentence like "its two commands" can go stale silently.
- Nothing stops an agent from weakening a test; `arch-tsu` is still review-only, and the gap
  ledger says so.
- The page was driven with curl in the trial, not in a browser.
