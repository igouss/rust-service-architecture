#!/usr/bin/env python3
import re, sys

from workspace import Unread, closing, lex, packages, relative, rust_files

IMPL = re.compile(r"\bimpl\s+(Lifecycle|Projection)\s+for\s+(\w+)\s*\{")
NAME = re.compile(r'\bconst\s+NAME\s*:\s*&\s*\'static\s+str\s*=\s*"([^"\\]*)"\s*;')
KIND = {"Lifecycle": "lifecycles", "Projection": "projections"}


def named():
    found, declared = [], 0
    for package in packages():
        for path in rust_files(package, "src"):
            code, source = lex(path)[0], open(path).read()
            declared += len(re.findall(r"\b(?:Lifecycle|Projection)\s+for\b", code))
            for match in IMPL.finditer(code):
                names = NAME.findall(source[match.end() : closing(code, match.end() - 1)])
                if len(names) != 1:
                    raise Unread("%s: impl %s for %s holds %d NAME written as a string literal; the scan needs exactly one" % (relative(path), match.group(1), match.group(2), len(names)))
                found.append((match.group(1), names[0], "%s (%s)" % (match.group(2), relative(path))))
    if declared != len(found) or not found:
        raise Unread("%d `Lifecycle for` and `Projection for` in the workspace's src, %d read as `impl … for Type { const NAME … }`" % (declared, len(found)))
    return found


def main():
    try:
        found = named()
    except Unread as unread:
        print("names: %s; the check would pass it unread" % unread)
        sys.exit(2)
    shared = 0
    for kind, name in sorted({(kind, name) for kind, name, _holder in found}):
        holders = [holder for other, text, holder in found if (other, text) == (kind, name)]
        if len(holders) > 1:
            shared += 1
            print('arch-1bmf: two %s are named "%s": %s' % (KIND[kind], name, " and ".join(holders)))
    if shared:
        print("A NAME names the stored rows; two types with one name read and overwrite each other's rows.")
        print("Fix: give each its own NAME. A NAME that has been stored is never reused for another type.")
    sys.exit(1 if shared else 0)


main()
