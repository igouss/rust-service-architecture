#!/usr/bin/env python3
import sys

from workspace import Unread, enum_variants, lifecycles, relative

GENERIC = {"Updated", "Changed", "Modified", "Edited", "Saved", "Set", "Replaced", "Patched"}

try:
    found = [
        "%s: %s::%s" % (relative(path), event, variant)
        for _package, path, state, event, _body, texts in lifecycles()
        for variant in enum_variants(event, texts)
        if variant in GENERIC or (variant.startswith(state) and variant[len(state) :] in GENERIC)
    ]
except Unread as unread:
    print("events: %s; the check would pass it unread" % unread)
    sys.exit(2)
for line in found:
    print("events: %s says that something changed, not what happened" % line)
if found:
    print("An event names one fact in the domain's words, in the past tense, and carries what that fact decided.")
    print("Fix: name what happened (Renamed, Rescheduled, AddressCorrected); if several things can happen, they are several events.")
sys.exit(1 if found else 0)
