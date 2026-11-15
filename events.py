#!/usr/bin/env python3
import glob, os, re, sys

ROOT = os.path.dirname(os.path.abspath(__file__))
GENERIC = {"Updated", "Changed", "Modified", "Edited", "Saved", "Set", "Replaced", "Patched"}

seen = 0
found = []
for path in sorted(glob.glob(os.path.join(ROOT, "*", "src", "**", "*.rs"), recursive=True)):
    source = open(path).read()
    for state, event in re.findall(r"impl Lifecycle for (\w+) \{.*?type Event = (\w+);", source, re.S):
        body = re.search(r"enum %s \{(.*?)\n\}" % event, source, re.S)
        if body is None:
            continue
        seen += 1
        for variant in re.findall(r"^    (\w+)", body.group(1), re.M):
            if variant in GENERIC or (variant.startswith(state) and variant[len(state):] in GENERIC):
                found.append("%s: %s::%s" % (os.path.relpath(path, ROOT), event, variant))
for line in found:
    print("events: %s says that something changed, not what happened" % line)
if found:
    print("An event names one fact in the domain's words, in the past tense, and carries what that fact decided.")
    print("Fix: name what happened (Renamed, Rescheduled, AddressCorrected); if several things can happen, they are several events.")
if seen == 0:
    print("events: no lifecycle's event enum was found; the check has nothing to read")
sys.exit(1 if found or seen == 0 else 0)
