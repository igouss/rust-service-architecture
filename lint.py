#!/usr/bin/env python3
import os, re, sys

from workspace import Unread, closing, folder, lex, line_of, packages, relative, role, rust_files, split

CORE = ("domain", "kernel")
TEST_ALLOWS = {"clippy::expect_used", "clippy::panic"}
SHELL_FREE = CORE + ("usecase",)


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


def top(code, token, start, end):
    depth = 0
    for k in range(start, end):
        if depth == 0 and code.startswith(token, k):
            return k
        depth += (code[k] in "([{") - (code[k] in ")]}")
    return end


def arms(code):
    found = []
    for keyword in re.finditer(r"\bmatch\b", code):
        opened = top(code, "{", keyword.end(), len(code))
        if opened == len(code):
            raise Unread("line %d: a match with no body" % line_of(code, keyword.start()))
        end, k = closing(code, opened), opened + 1
        while code[k:end].strip():
            arrow = top(code, "=>", k, end)
            found.append(re.split(r"\bif\b", code[k:arrow])[0])
            k = arrow + 2 + len(code[arrow + 2 : end]) - len(code[arrow + 2 : end].lstrip())
            block = code[k] == "{"
            k = closing(code, k) + 1 if block else k
            if not block or code[k:end].lstrip()[:1] in (".", "?"):
                k = top(code, ",", k, end)
            k += len(code[k:end]) - len(code[k:end].lstrip())
            k += code[k] == ","
    if len(found) != code.count("=>"):
        raise Unread("%d `=>` and %d match arms read" % (code.count("=>"), len(found)))
    return found


def loose(pattern, element):
    for alternative in split(pattern, "|"):
        text = re.sub(r"^\s*&?\s*(?:\w+\s*@)?", "", alternative).strip()
        if text.startswith("(") and closing(text, 0, "()") == len(text) - 1:
            for inner in split(text[1:-1], ","):
                yield from loose(inner, True)
        elif element and re.fullmatch(r"\.\.|(?:(?:ref|mut)\s+)*[a-z_]\w*", text):
            yield " ".join(alternative.split())


def wildcards():
    found, read = [], 0
    for package in packages():
        if role(package) not in SHELL_FREE:
            continue
        for path in rust_files(package, "src"):
            try:
                patterns = arms(lex(path)[0])
            except Unread as unread:
                raise Unread("%s: %s" % (relative(path), unread))
            read += len(patterns)
            found += ["arch-j0k: %s `%s`" % (relative(path), " ".join(alternative.split())) for pattern in patterns for alternative in split(pattern, "|") if any(loose(alternative, False))]
    if not read:
        raise Unread("no match arm in a crate of role %s; the check has nothing to read" % ", ".join(SHELL_FREE))
    return found, ["A tuple element that is `_`, `..` or a bare name takes every case the arms above left, and a new variant lands there unseen.", "Fix: spell out every pair, naming each variant the element can be (`A | B`)."]


CHECKS = {"comments": comments, "allows": allows, "wildcards": wildcards}


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
