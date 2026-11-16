#!/usr/bin/env python3
import os, re, sys

from workspace import Unread, closing, folder, lex, line_of, packages, relative, role, rust_files

CORE = ("domain", "kernel")
TEST_ALLOWS = {"clippy::expect_used", "clippy::panic"}


def comments():
    found = []
    for package in packages():
        for path in rust_files(package):
            for line, text in lex(path)[1]:
                doc = re.match(r"//(/(?!/)|!)", text)
                escape = role(package) in CORE and re.match(r"// fc-allow: \S", text)
                if not doc and not escape:
                    found.append("arch-0d2: %s:%d %s" % (relative(path), line, text.splitlines()[0]))
    return found, ["Names and types carry the meaning; `// fc-allow: <reason>` is read in the core only.", "Fix: remove the comment, or make it a contract in a `///` doc comment on the item it binds."]


def allows():
    found = []
    for package in packages():
        for path in rust_files(package):
            code = lex(path)[0]
            in_tests = os.path.relpath(path, folder(package)).split(os.sep)[0] == "tests"
            inside = 0
            for opened in re.finditer(r"#(!?)\[", code):
                text = code[opened.end() : closing(code, opened.end() - 1, "[]")]
                levels = re.findall(r"\b(allow|expect)\s*\(", text)
                inside += levels.count("allow")
                sanctioned = re.fullmatch(r"\s*allow\s*\(([^()]*)\)\s*", text)
                names = {name.strip() for name in sanctioned.group(1).split(",") if name.strip()} if sanctioned else set()
                if levels and not (in_tests and opened.group(1) and names and names <= TEST_ALLOWS):
                    found.append("arch-tsu: %s:%d #%s[%s]" % (relative(path), line_of(code, opened.start()), opened.group(1), " ".join(text.split())))
            if inside != len(re.findall(r"\ballow\s*\(", code)):
                raise Unread("%s: an `allow(` stands outside every attribute the scan read" % relative(path))
    return found, ["The one allow is `#![allow(%s)]` opening a file under tests/." % ", ".join(sorted(TEST_ALLOWS)), "Fix: remove the attribute and fix what the lint names (a type alias, a smaller function)."]


CHECKS = {"comments": comments, "allows": allows}


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
