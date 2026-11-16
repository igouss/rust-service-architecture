#!/usr/bin/env python3
import os, subprocess, sys, tomllib

from workspace import ROOT

TABLES = (("workspace", "lints"), ("lints",), ("package", "metadata", "hex-arch"))
TOP = ("justfile", "clippy.toml", "rustfmt.toml")
UNDER = ("planted/", ".github/workflows/")


def git(*args):
    done = subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True)
    if done.returncode:
        print("guarded: git %s: %s" % (" ".join(args), done.stderr.strip()))
        sys.exit(2)
    return done.stdout


def tables(commit, path):
    held = subprocess.run(["git", "cat-file", "-e", "%s:%s" % (commit, path)], cwd=ROOT, capture_output=True).returncode == 0
    manifest = tomllib.loads(git("show", "%s:%s" % (commit, path))) if held else {}
    found = []
    for keys in TABLES:
        value = manifest
        for key in keys:
            value = value.get(key, {}) if isinstance(value, dict) else {}
        found.append(value)
    return found


def gate(base, path):
    top = "/" not in path
    if os.path.basename(path) == "Cargo.toml":
        return tables(base, path) != tables("HEAD", path)
    return (top and (path in TOP or path.endswith((".py", ".bb")))) or path.startswith(UNDER) or os.path.basename(path) == "clippy.toml"


def main():
    if len(sys.argv) != 2:
        print("guarded: name the base, as in `just guarded origin/main`")
        sys.exit(2)
    base = git("merge-base", sys.argv[1], "HEAD").strip()
    changed = git("diff", "--name-only", "--no-renames", base, "HEAD").split("\n")
    gates = [path for path in changed if path and gate(base, path)]
    records = git("diff", "--name-only", "--no-renames", "--diff-filter=AM", base, "HEAD", "--", "docs/adr/").split()
    if not gates or records:
        print("guarded: %d file(s) changed since %s, %d of them a gate, %d decision record(s)" % (len([path for path in changed if path]), base[:7], len(gates), len(records)))
        return
    for path in gates:
        print("arch-tsu: %s changes a gate and the range %s..HEAD holds no decision record" % (path, base[:7]))
    print("A gate is weakened where nobody looks unless the change is its own decision.")
    print("Fix: add or amend a file under docs/adr/ in this range: what the gate held, what it holds now, and what would reverse it.")
    sys.exit(1)


main()
