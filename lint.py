#!/usr/bin/env python3
import glob, os, re, sys

from workspace import ROOT, Unread, arms, closing, declared, enum_variants, folder, lex, line_of, packages, relative, role, rust_files, split, top

CORE = ("domain", "kernel")
TEST_ALLOWS = {"clippy::expect_used", "clippy::panic"}
SHELL_FREE = CORE + ("usecase",)


INSTALL = re.compile(r"\bcargo\s+install\b")
GIT = re.compile(r"--git(?![\w-])")
PINNED = re.compile(r"--rev[= ][0-9a-fA-F]{40}(?!\w)")


def pins():
    found, read, every, sources, read_sources = [], 0, 0, 0, 0
    workflows = os.path.join(ROOT, ".github", "workflows")
    paths = sorted(glob.glob(os.path.join(workflows, "*.yml")) + glob.glob(os.path.join(workflows, "*.yaml")))
    if not paths:
        raise Unread("no workflow under %s; the check has nothing to read" % relative(workflows))
    for path in paths:
        text = open(path).read()
        every += len(INSTALL.findall(text))
        sources += len(GIT.findall(text))
        for number, line in enumerate(text.split("\n"), 1):
            installs = list(INSTALL.finditer(line))
            if len(installs) > 1:
                raise Unread("%s:%d: %d `cargo install` on one line; one `--rev` cannot be read as pinning both" % (relative(path), number, len(installs)))
            if not installs:
                continue
            read += 1
            command = line[installs[0].start() :].strip()
            if command.endswith("\\"):
                raise Unread("%s:%d: `%s` goes on past the line; write the install on one line" % (relative(path), number, command))
            read_sources += len(GIT.findall(command))
            source = re.search(r"--git(?:[= ](\S+))?", command)
            if source and not PINNED.search(command):
                found.append("arch-mov6: %s:%d cargo install --git %s names no --rev of 40 hex digits" % (relative(path), number, source.group(1) or "with no url"))
    if read != every:
        raise Unread("%d `cargo install` in the workflows, %d read one to a line" % (every, read))
    if read_sources != sources:
        raise Unread("%d `--git` in the workflows, %d on the line of a `cargo install`; write each install on one line" % (sources, read_sources))
    return found, ["A tool built from a branch is whatever the branch holds the day CI runs, so a green no longer says which gate passed it.", "Fix: add `--rev <commit>`, the 40-hex id `git ls-remote <url> HEAD` prints, once `just check` passes with that build (docs/adr/0017-gate-tools-are-pinned.md)."]


CHECKS = {"pins": pins}


def main():
    try:
        found, fix = CHECKS[sys.argv[1]]()
    except Unread as unread:
        print("%s: %s" % (sys.argv[1], unread))
        sys.exit(2)
    for line in found + (fix if found else []):
        print(line)
    sys.exit(1 if found else 0)


main()
