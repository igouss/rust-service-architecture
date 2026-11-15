#!/usr/bin/env python3
import json, os, subprocess, sys

ROOT = os.path.dirname(os.path.abspath(__file__))
BANNED = {"serde", "serde_json"}

meta = json.loads(subprocess.run(["cargo", "metadata", "--format-version", "1", "--no-deps"], cwd=ROOT, capture_output=True, text=True, check=True).stdout)
roles = {p["name"]: (p.get("metadata") or {}).get("hex-arch", {}).get("role") for p in meta["packages"]}
adapters = [p for p in meta["packages"] if roles[p["name"]] == "driven-adapter"]
found = [(p["name"], d["name"]) for p in adapters for d in p["dependencies"] if d["kind"] != "dev" and (d["name"] in BANNED or roles.get(d["name"]) in ("domain", "kernel"))]
for adapter, dependency in found:
    print("raw-store: %s depends on %s" % (adapter, dependency))
if found:
    print("A store adapter implements ports::store over text and never names a domain type or serializes.")
    print("Fix: move the typed code into ports, or make the dependency a dev-dependency if only tests use it.")
SUITE = 'include!("../../ports/tests/conformance/suite.rs");'
for adapter in adapters:
    folder = os.path.dirname(adapter["manifest_path"])
    source = "".join(open(os.path.join(root, name)).read() for root, _dirs, names in os.walk(os.path.join(folder, "src")) for name in names if name.endswith(".rs"))
    test = os.path.join(folder, "tests", "conformance.rs")
    if "impl Store for" in source and not (os.path.exists(test) and SUITE in open(test).read()):
        print("raw-store: %s implements Store and does not include the conformance suite" % adapter["name"])
        print("Fix: add tests/conformance.rs that names the adapter as Subject and ends with %s" % SUITE)
        found.append((adapter["name"], "suite"))
if not adapters:
    print("raw-store: no driven-adapter crate found; the check has nothing to read")
sys.exit(1 if found or not adapters else 0)
