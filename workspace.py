#!/usr/bin/env python3
import json, os, re, subprocess

ROOT = os.path.dirname(os.path.abspath(__file__))
RAW = re.compile(r'(?:b|c)?r(#*)"')
LIFECYCLE = re.compile(r"\bimpl\s+Lifecycle\s+for\s+(\w+)\s*\{")


class Unread(Exception):
    pass


def packages():
    done = subprocess.run(["cargo", "metadata", "--format-version", "1", "--no-deps"], cwd=ROOT, capture_output=True, text=True, check=True)
    return sorted(json.loads(done.stdout)["packages"], key=lambda p: p["name"])


def role(package):
    return (package.get("metadata") or {}).get("hex-arch", {}).get("role")


def folder(package):
    return os.path.dirname(package["manifest_path"])


def folders():
    return [os.path.relpath(folder(p), ROOT) for p in packages()]


def rust_files(package, *under):
    found = []
    for top, dirs, names in os.walk(os.path.join(folder(package), *under)):
        dirs[:] = sorted(d for d in dirs if d != "target")
        found += [os.path.join(top, name) for name in sorted(names) if name.endswith(".rs")]
    missing = [t["src_path"] for t in package["targets"] if not under and t["src_path"] not in found]
    if missing or (not under and not found):
        raise Unread("%s: cargo builds %s and the scan did not walk it" % (package["name"], ", ".join(missing) or "nothing"))
    return found


def relative(path):
    return os.path.relpath(path, ROOT)


def lex(path):
    source = open(path).read()
    code = list(source)
    comments = []
    i, n, line = 0, len(source), 1

    def blank(start, end):
        for k in range(start, end):
            if code[k] != "\n":
                code[k] = " "

    def unread(what, at):
        raise Unread("%s:%d: %s does not end; the scan cannot read this file" % (relative(path), at, what))

    while i < n:
        c = source[i]
        if source.startswith("//", i):
            end = source.find("\n", i)
            end = n if end < 0 else end
            comments.append((line, source[i:end]))
            blank(i, end)
            i = end
        elif source.startswith("/*", i):
            depth, j = 1, i + 2
            while depth and j < n:
                if source.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif source.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            if depth:
                unread("a block comment", line)
            comments.append((line, source[i:j]))
            blank(i, j)
            line += source.count("\n", i, j)
            i = j
        elif c in "bcr" and (i == 0 or not (source[i - 1].isalnum() or source[i - 1] == "_")) and RAW.match(source, i):
            opened = RAW.match(source, i)
            end = source.find('"' + opened.group(1), opened.end())
            if end < 0:
                unread("a raw string", line)
            blank(opened.end(), end)
            line += source.count("\n", i, end)
            i = end + 1 + len(opened.group(1))
        elif c == '"':
            j = i + 1
            while j < n and source[j] != '"':
                j += 2 if source[j] == "\\" else 1
            if j >= n:
                unread("a string", line)
            blank(i + 1, j)
            line += source.count("\n", i, j)
            i = j + 1
        elif c == "'":
            if source.startswith("\\", i + 1):
                end = source.find("'", i + 3)
                if end < 0:
                    unread("a character", line)
                blank(i + 1, end)
                i = end + 1
            elif source.startswith("'", i + 2):
                blank(i + 1, i + 2)
                i += 3
            else:
                i += 1
        else:
            line += c == "\n"
            i += 1
    return "".join(code), comments


def line_of(code, offset):
    return code.count("\n", 0, offset) + 1


def closing(code, start, pair="{}"):
    depth = 0
    for k in range(start, len(code)):
        depth += (code[k] == pair[0]) - (code[k] == pair[1])
        if depth == 0:
            return k
    raise Unread("line %d: %s is never closed" % (line_of(code, start), pair[0]))


def top(code, token, start, end):
    depth = 0
    for k in range(start, end):
        if depth == 0 and code.startswith(token, k):
            return k
        depth += (code[k] in "([{") - (code[k] in ")]}")
    return end


def arms(code, opened):
    found = []
    end, k = closing(code, opened), opened + 1
    while code[k:end].strip():
        arrow = top(code, "=>", k, end)
        start = arrow + 2 + len(code[arrow + 2 : end]) - len(code[arrow + 2 : end].lstrip())
        block = code[start] == "{"
        after = closing(code, start) + 1 if block else start
        if not block or code[after:end].lstrip()[:1] in (".", "?"):
            after = top(code, ",", after, end)
        if top(code, ",", k, arrow) != arrow:
            raise Unread("line %d: the scan cut a match arm at a comma; put the value of the arm before it in braces" % line_of(code, k))
        found.append((re.split(r"\bif\b", code[k:arrow])[0], code[start:after].strip()))
        k = after + len(code[after:end]) - len(code[after:end].lstrip())
        k += code[k] == ","
    return found


def sources():
    return [(package, path, lex(path)[0]) for package in packages() for path in rust_files(package, "src")]


def declared(name, texts):
    heads = [m for text in texts for m in re.finditer(r"\b(enum|struct)\s+%s\b\s*([{(;])" % name, text)]
    if len(heads) != 1:
        raise Unread("%s is declared %d times as an enum or a struct in the crate's src; the scan needs exactly one" % (name, len(heads)))
    head = heads[0]
    opened = head.end() - 1
    return head.group(1), "" if head.group(2) == ";" else head.string[opened + 1 : closing(head.string, opened, "{}" if head.group(2) == "{" else "()")]


def enum_variants(name, texts):
    kind, body = declared(name, texts)
    if kind != "enum":
        raise Unread("%s is a struct where the scan needs an enum" % name)
    return [re.sub(r"#\[[^\]]*\]", "", part).split()[0].split("(")[0].split("{")[0] for part in split(body, ",") if part.strip()]


def split(text, separator):
    parts, depth, start, k = [], 0, 0, 0
    while k < len(text):
        depth += (text[k] in "([{") - (text[k] in ")]}")
        if depth == 0 and text.startswith(separator, k):
            parts.append(text[start:k])
            start = k + len(separator)
            k = start
        else:
            k += 1
    return parts + [text[start:]]


def lifecycles():
    found, declared = [], 0
    for package, path, code in sources():
        texts = None
        declared += len(re.findall(r"\bLifecycle\s+for\b", code))
        for match in LIFECYCLE.finditer(code):
            body = code[match.end() : closing(code, match.end() - 1)]
            event = re.search(r"\btype\s+Event\s*=\s*(\w+)\s*;", body)
            if event is None:
                continue
            texts = texts or [lex(other)[0] for other in rust_files(package, "src")]
            found.append((package, path, match.group(1), event.group(1), body, texts))
    if declared != len(found) or not found:
        raise Unread("%d `Lifecycle for` in the workspace's src, %d read as `impl Lifecycle for State { type Event = Name; … }`" % (declared, len(found)))
    return found
