#!/usr/bin/env python3
import glob, os, re, subprocess, sys

from workspace import ROOT, folder, lex, packages, role, rust_files

BANNED = {"serde", "serde_json"}
SUITE = 'include!("../../ports/tests/conformance/suite.rs");'


def ran(adapter, feature):
    suite = os.path.normpath(os.path.join(folder(adapter), "tests", os.path.dirname(SUITE.split('"')[1])))
    cases = sum(len(re.findall(r"#\[test\]", lex(path)[0])) for path in glob.glob(os.path.join(suite, "*.rs")))
    done = subprocess.run(["cargo", "test", "-q", "-p", adapter["name"], "--features", feature, "--test", "conformance"], cwd=ROOT, capture_output=True, text=True)
    output = done.stdout + done.stderr
    passed, failed = (sum(int(count) for count in re.findall(pattern, output)) for pattern in (r"test result: \w+\. (\d+) passed", r"test result: [^\n]*; (\d+) failed"))
    print("raw-store: %s ran %d of the %d conformance cases and passed %d" % (adapter["name"], passed + failed, cases, passed))
    if done.returncode or passed != cases or not cases:
        print(output)
        print("Fix: every case of the suite runs and passes against this store; a case that is filtered, ignored or behind an unset feature has proved nothing.")
        sys.exit(1)


members = packages()
roles = {p["name"]: role(p) for p in members}
adapters = [p for p in members if roles[p["name"]] == "driven-adapter"]
found = [(p["name"], d["name"]) for p in adapters for d in p["dependencies"] if d["kind"] != "dev" and (d["name"] in BANNED or roles.get(d["name"]) in ("domain", "kernel"))]
for adapter, dependency in found:
    print("raw-store: %s depends on %s" % (adapter, dependency))
if found:
    print("A store adapter implements ports::store over text and never names a domain type or serializes.")
    print("Fix: move the typed code into ports, or make the dependency a dev-dependency if only tests use it.")
if sys.argv[1:2] == ["--ran"]:
    ran([p for p in adapters if p["name"] == sys.argv[2]][0], sys.argv[3])
    sys.exit(0)
for adapter in adapters:
    source = "".join(open(path).read() for path in rust_files(adapter, "src"))
    test = os.path.join(folder(adapter), "tests", "conformance.rs")
    if "impl Store for" in source and not (os.path.exists(test) and SUITE in open(test).read()):
        print("raw-store: %s implements Store and does not include the conformance suite" % adapter["name"])
        print("Fix: add tests/conformance.rs that names the adapter as Subject and ends with %s" % SUITE)
        found.append((adapter["name"], "suite"))
if not adapters:
    print("raw-store: no driven-adapter crate found; the check has nothing to read")
sys.exit(1 if found or not adapters else 0)
