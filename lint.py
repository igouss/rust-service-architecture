#!/usr/bin/env python3
import glob, os, re, sys

from workspace import ROOT, Unread, arms, closing, declared, enum_variants, folder, lex, line_of, packages, relative, role, rust_files, split, top

CORE = ("domain", "kernel")
TEST_ALLOWS = {"clippy::expect_used", "clippy::panic"}
SHELL_FREE = CORE + ("usecase",)


def patterns(code):
    found = []
    for keyword in re.finditer(r"\bmatch\b", code):
        opened = top(code, "{", keyword.end(), len(code))
        if opened == len(code):
            raise Unread("line %d: a match with no body" % line_of(code, keyword.start()))
        found += [pattern for pattern, _then in arms(code, opened)]
    if len(found) != code.count("=>"):
        raise Unread("%d `=>` and %d match arms read" % (code.count("=>"), len(found)))
    return found


def loose(pattern, element):
    for alternative in split(pattern, "|"):
        text = re.sub(r"^\s*&?\s*(?:\w+\s*@)?", "", alternative).strip()
        if text.startswith("(") and closing(text, 0, "()") == len(text) - 1:
            for inner in split(text[1:-1], ","):
                yield from loose(inner, True)
        elif element and text not in ("true", "false") and re.fullmatch(r"\.\.|(?:(?:ref|mut)\s+)*[a-z_]\w*", text):
            yield " ".join(alternative.split())


def wildcards():
    found, read = [], 0
    for package in packages():
        if role(package) not in SHELL_FREE:
            continue
        for path in rust_files(package, "src"):
            try:
                read_arms = patterns(lex(path)[0])
            except Unread as unread:
                raise Unread("%s: %s" % (relative(path), unread))
            read += len(read_arms)
            found += ["arch-j0k: %s `%s`" % (relative(path), " ".join(alternative.split())) for pattern in read_arms for alternative in split(pattern, "|") if any(loose(alternative, False))]
    if not read:
        raise Unread("no match arm in a crate of role %s; the check has nothing to read" % ", ".join(SHELL_FREE))
    return found, ["A tuple element that is `_`, `..` or a bare name takes every case the arms above left, and a new variant lands there unseen.", "Fix: spell out every pair, naming each variant the element can be (`A | B`)."]


def traits():
    found = []
    for package in packages():
        roots = {target["src_path"] for target in package["targets"] if "lib" in target["kind"]}
        home = set(rust_files(package, "src")) if role(package) == "port-and-adapter" else roots if role(package) in ("domain", "usecase") else set()
        kept = 0
        for path in rust_files(package):
            code = lex(path)[0]
            declared = list(re.finditer(r"\btrait\s+(\w+)", code))
            if len(declared) != len(re.findall(r"\btrait\b", code)):
                raise Unread("%s: a `trait` the scan could not name" % relative(path))
            kept += len(declared) if path in home else 0
            found += ["arch-u8nl: %s:%d trait %s" % (relative(path), line_of(code, match.start()), match.group(1)) for match in declared if path not in home]
        if home and not kept:
            raise Unread("%s declares no trait where its primitives live; the check has nothing to read" % package["name"])
    return found, ["A trait is a new kind of thing; the kinds are the traits of domain/src/lib.rs, app/src/lib.rs and ports/src.", "Fix: make the feature an instance of one of them: a lifecycle, a command, a reaction, a projection, a policy, a query, a port."]


STORED = {"Lifecycle": ("Event", "Refusal"), "Reaction": ("Request",), "Projection": ("Row",)}


def stored_types(package, texts):
    named = []
    for text in texts:
        for kind, slots in STORED.items():
            for head in re.finditer(r"\bimpl\s+%s\s+for\s+(\w+)\s*\{" % kind, text):
                body = text[head.end() : closing(text, head.end() - 1)]
                slot = [re.search(r"\btype\s+%s\s*=\s*(\w+)\s*;" % name, body) for name in slots]
                if None in slot:
                    raise Unread("%s: `impl %s for %s` does not name its %s as one type" % (package["name"], kind, head.group(1), " and ".join(slots)))
                named += [head.group(1)] * (kind == "Lifecycle") + [found.group(1) for found in slot]
        if sum(len(re.findall(r"\b%s\s+for\b" % kind, text)) for kind in STORED) != sum(1 for _ in re.finditer(r"\bimpl\s+(%s)\s+for\s+\w+\s*\{" % "|".join(STORED), text)):
            raise Unread("%s: an impl of %s the scan could not read" % (package["name"], ", ".join(STORED)))
    direct = set(named)
    reached, queue = set(), sorted(direct)
    while queue:
        name = queue.pop()
        reached.add(name)
        queue += [inner for inner in set(re.findall(r"\b[A-Z]\w*\b", declared(name, texts)[1])) - reached if any(re.search(r"\b(enum|struct)\s+%s\b" % inner, text) for text in texts)]
    return direct, reached


def stored():
    found, pins = [], 0
    for package in packages():
        texts = [lex(path)[0] for path in rust_files(package, "src")]
        direct, reached = stored_types(package, texts)
        test = os.path.join(folder(package), "tests", "stored_encoding.rs")
        code = lex(test)[0] if os.path.exists(test) else ""
        for name in sorted(reached):
            enum = declared(name, texts)[0] == "enum"
            wanted = ["%s::%s" % (name, variant) for variant in enum_variants(name, texts)] if enum else [name] if name in direct else []
            pins += len(wanted)
            found += ["arch-btw1: %s does not pin %s" % (relative(test), sample) for sample in wanted if not re.search(r"\bpinned\(\s*&\s*%s\b" % sample, code)]
    if not pins:
        raise Unread("no stored type found; the check has nothing to read")
    return found, ["A stored type's encoding only grows, so each variant's JSON is pinned where a change to it fails.", "Fix: add `pinned(&Type::Variant, form)` to the test, and the variant's JSON to `form`'s match."]


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


CHECKS = {"wildcards": wildcards, "traits": traits, "stored": stored, "pins": pins}


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
