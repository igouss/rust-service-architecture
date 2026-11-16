#!/usr/bin/env python3
import glob, os, re, shutil, subprocess, sys, tempfile

from planted.gates import GATES
from planted.http import HTTP
from planted.shell import SHELL
from planted.stores import STORES
from workspace import ROOT, folders

CRATES = folders()
FILES = ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "clippy.toml", "rustfmt.toml", "justfile"] + [os.path.basename(script) for script in glob.glob(os.path.join(ROOT, "*.py")) + glob.glob(os.path.join(ROOT, "*.bb"))]

MUTANTS = SHELL + STORES + HTTP + GATES

EXAMPLE = {"C5", "C8", "C9", "C10", "T10", "T11", "T20", "T22", "T23", "T24", "G6", "H1", "H2", "H3", "H4", "H5", "H6", "H7", "H8", "H9"}


def uncompiled(output):
    return "DID NOT COMPILE: " + " ".join(sorted(set(re.findall(r"error\[E\d+\]", output))))


def failed_tests(output):
    return sorted(set(re.findall(r"^---- (\S+) stdout ----", output, re.M)))


def run(path, old, new, expect):
    scratch = tempfile.mkdtemp(prefix="mutant-")
    try:
        for crate in CRATES:
            shutil.copytree(os.path.join(ROOT, crate), os.path.join(scratch, crate), copy_function=shutil.copy)
        for file in FILES:
            shutil.copy(os.path.join(ROOT, file), scratch)
        shutil.copytree(os.path.join(ROOT, "planted"), os.path.join(scratch, "planted"), ignore=shutil.ignore_patterns("__pycache__"))
        target = os.path.join(scratch, path)
        source = open(target).read()
        if source.count(old) != 1:
            return False, "ANCHOR FOUND %d TIMES" % source.count(old)
        open(target, "w").write(source.replace(old, new))
        env = dict(os.environ, CARGO_TARGET_DIR=os.path.join(ROOT, "target", "mutants"))
        if isinstance(expect, tuple) and expect[0] == "server":
            if not os.environ.get(expect[2]):
                return False, "NEEDS %s: this mutant is killed by tests that run against a server" % expect[2]
            done = subprocess.run(expect[1], cwd=scratch, env=env, capture_output=True, text=True)
            output = done.stdout + done.stderr
            failed = failed_tests(output)
            passed = sum(int(count) for count in re.findall(r"test result: \w+\. (\d+) passed", output))
            if "could not compile" in output:
                return False, uncompiled(output)
            if passed == 0:
                return False, "NO TEST PASSED: the server is not reachable, so a failure proves nothing"
            return done.returncode != 0 and bool(failed), "killed by " + ", ".join(failed) if failed else "SURVIVED"
        if isinstance(expect, tuple):
            shutil.copytree(os.path.join(ROOT, "docs"), os.path.join(scratch, "docs"))
            done = subprocess.run(expect[1], cwd=scratch, env=env, capture_output=True, text=True)
            output = done.stdout + done.stderr
            hit = done.returncode != 0 and expect[2] in output
            return hit, "exit %d, %s %s" % (done.returncode, "names" if expect[2] in output else "DOES NOT NAME", expect[2])
        command = ["cargo", "test", "-q", "--no-fail-fast"] if expect == "test" else ["cargo", "check", "-q", "--all-targets"]
        done = subprocess.run(command, cwd=scratch, env=env, capture_output=True, text=True)
        output = done.stdout + done.stderr
        if done.returncode == 0:
            return False, "SURVIVED"
        if expect == "test":
            if "could not compile" in output:
                return False, uncompiled(output)
            failed = failed_tests(output)
            return bool(failed), "killed by " + ", ".join(failed)
        codes = sorted(set(re.findall(r"error\[(E\d+)\]", output)))
        unused = "unused" in output and "must be used" in output
        hit = unused if expect == "unused" else expect in codes
        return hit, ("unused_must_use" if unused else "") + " ".join(codes)
    finally:
        shutil.rmtree(scratch, ignore_errors=True)


def main():
    wanted = sys.argv[1:]
    if wanted == ["--list"]:
        for name, path, _old, _new, _expect in MUTANTS:
            print("%-9s %-28s %s" % ("example" if name.split()[0] in EXAMPLE else "machinery", path, name))
        return
    red = 0
    for name, path, old, new, expect in MUTANTS:
        if wanted and name.split()[0] not in wanted:
            continue
        ok, detail = run(path, old, new, expect)
        red += 0 if ok else 1
        print("%-4s %s [%s] %s" % ("ok" if ok else "FAIL", name, expect if isinstance(expect, str) else " ".join(expect[1][:2]), detail), flush=True)
    print("%d mutant(s) not killed as expected" % red)
    sys.exit(1 if red else 0)


main()
