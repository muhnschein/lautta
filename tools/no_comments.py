#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-2.1-or-later
import ast
import io
import re
import subprocess
import sys
import tokenize

EXCLUDED_PREFIXES = (
    "third_party/",
    "qml/components/viewers/gallery/",
    "tests/bridge-contract/",
    ".github/codeql/rust-queries/AccessInvalidPointer.ql",
    "tests/qml/host/fixtures/",
    "crates/lautta-core/tests/fixtures/",
    "doc/",
    "design/",
    "store/",
    "translations/",
)

SPDX = re.compile(r"^(//|#) SPDX-License-Identifier: [\w.+-]+( OR [\w.+-]+)*$")
NOSONAR = "// NOSONAR"
LUPDATE = re.compile(r"^//% ")
SAFETY_DOC = re.compile(r'(?m)^[ \t]*/// # Safety\n(?:[ \t]*///[^\n]*\n)+(?:[ \t]*#\[[^\n]*\]\n)*[ \t]*pub unsafe (?:extern "C" )?fn ')

C_LIKE = {".rs": "rust", ".qml": "js", ".js": "js"}
HASH_FULL_LINE = {".spec", ".desktop", ".properties", ".gitignore"}


def language(path):
    name = path.rsplit("/", 1)[-1]
    if name == "Makefile":
        return "make"
    if name == ".gitignore":
        return "line"
    ext = "." + name.rsplit(".", 1)[-1] if "." in name else ""
    if ext in C_LIKE:
        return C_LIKE[ext]
    if ext in (".sh",):
        return "sh"
    if ext == ".py":
        return "py"
    if ext in (".toml", ".yml", ".yaml", ".qls"):
        return "hash"
    if ext in HASH_FULL_LINE:
        return "line"
    return None


def ident(c):
    return c.isalnum() or c == "_"


def c_like_comments(text, rust):
    out = []
    i, n = 0, len(text)
    prev = ""
    while i < n:
        c = text[i]
        if text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            out.append((i, j))
            i = j
            continue
        if text.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if rust and text.startswith("/*", j):
                    depth += 1
                    j += 2
                elif text.startswith("*/", j):
                    depth -= 1
                    j += 2
                else:
                    j += 1
            out.append((i, j))
            i = j
            continue
        if rust and c in "rbc" and (i == 0 or not ident(text[i - 1])):
            m = re.compile(r'(?:br|cr|r)(#*)"').match(text, i)
            if m:
                close = '"' + m.group(1)
                j = text.find(close, m.end())
                i = n if j < 0 else j + len(close)
                prev = '"'
                continue
        if c == '"' or (not rust and c in "'`"):
            j = i + 1
            while j < n and text[j] != c:
                j += 2 if text[j] == "\\" else 1
            i = j + 1
            prev = c
            continue
        if rust and c == "'":
            m = re.compile(r"'(\\(u\{[0-9a-fA-F]+\}|x[0-9a-fA-F]{2}|.)|[^\\'\n])'").match(text, i)
            i = m.end() if m else i + 1
            prev = "'"
            continue
        if not rust and c == "/" and regex_allowed(prev):
            j = i + 1
            klass = False
            while j < n and text[j] != "\n":
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == "[":
                    klass = True
                elif text[j] == "]":
                    klass = False
                elif text[j] == "/" and not klass:
                    break
                j += 1
            i = j + 1
            prev = "/"
            continue
        if not c.isspace():
            if ident(c):
                j = i
                while j < n and ident(text[j]):
                    j += 1
                prev = text[i:j]
                i = j
                continue
            prev = c
        i += 1
    return out


def regex_allowed(prev):
    return prev == "" or prev in "(,=:[!&|?{};+-*%<>~^" or prev in ("return", "typeof", "case", "in")


def shell_comments(text):
    out = []
    lines = text.split("\n")
    offset = 0
    heredoc = None
    quote = None
    for line in lines:
        if heredoc is not None:
            if line.lstrip("\t") == heredoc:
                heredoc = None
            offset += len(line) + 1
            continue
        i, n = 0, len(line)
        while i < n:
            c = line[i]
            if quote:
                if c == "\\" and quote == '"':
                    i += 2
                    continue
                if c == quote:
                    quote = None
                i += 1
                continue
            if c == "\\":
                i += 2
                continue
            if c in "'\"":
                quote = c
            elif c == "#" and (i == 0 or line[i - 1] in " \t;(|&"):
                out.append((offset + i, offset + n))
                break
            elif line.startswith("<<", i):
                m = re.compile(r"<<-?\s*['\"]?(\w+)['\"]?").match(line, i)
                if m:
                    heredoc = m.group(1)
                    i = m.end()
                    continue
            i += 1
        offset += len(line) + 1
    return out


def hash_comments(text, make=False):
    out = []
    offset = 0
    for line in text.split("\n"):
        quote = None
        i, n = 0, len(line)
        while i < n:
            c = line[i]
            if quote:
                if c == "\\" and quote == '"':
                    i += 2
                    continue
                if c == quote:
                    quote = None
            elif c in "'\"" and not make:
                quote = c
            elif c == "\\" and make:
                i += 2
                continue
            elif c == "#" and (i == 0 or line[i - 1] in " \t"):
                out.append((offset + i, offset + n))
                break
            i += 1
        offset += len(line) + 1
    return out


def line_comments(text):
    out = []
    offset = 0
    for line in text.split("\n"):
        stripped = line.lstrip()
        if stripped.startswith("#"):
            start = offset + len(line) - len(stripped)
            out.append((start, offset + len(line)))
        offset += len(line) + 1
    return out


def python_comments(text):
    out = []
    starts = [0]
    for line in text.split("\n"):
        starts.append(starts[-1] + len(line) + 1)
    for tok in tokenize.generate_tokens(io.StringIO(text).readline):
        if tok.type == tokenize.COMMENT:
            (r1, c1), (r2, c2) = tok.start, tok.end
            out.append((starts[r1 - 1] + c1, starts[r2 - 1] + c2))
    for node in ast.walk(ast.parse(text)):
        if isinstance(node, (ast.Module, ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            body = node.body
            if body and isinstance(body[0], ast.Expr) and isinstance(body[0].value, ast.Constant):
                if isinstance(body[0].value.value, str):
                    d = body[0]
                    out.append((starts[d.lineno - 1] + d.col_offset, starts[d.end_lineno - 1] + d.end_col_offset))
    return sorted(out)


def comments(path, text):
    lang = language(path)
    if lang in ("rust", "js"):
        return c_like_comments(text, lang == "rust")
    if lang == "sh":
        return shell_comments(text)
    if lang == "py":
        return python_comments(text)
    if lang == "hash":
        return hash_comments(text)
    if lang == "make":
        return hash_comments(text, make=True)
    if lang == "line":
        return line_comments(text)
    return []


def allowed(path, text, start, end):
    body = text[start:end]
    first_line = text.count("\n", 0, start) == 0
    second_line = text.count("\n", 0, start) == 1 and text.startswith("#!")
    if first_line and body.startswith("#!") and start == 0:
        return True
    if (first_line or second_line) and SPDX.match(body):
        return True
    if body == NOSONAR:
        return True
    lang = language(path)
    if lang == "js" and LUPDATE.match(body):
        return True
    if lang == "rust" and in_safety_doc(text, start):
        return True
    return False


def in_safety_doc(text, start):
    for m in SAFETY_DOC.finditer(text):
        if m.start() <= start < m.end():
            return True
    return False


def offences(path, text):
    return [(s, e) for s, e in comments(path, text) if not allowed(path, text, s, e)]


def keep_minimal(path, text, start, end):
    body = text[start:end]
    if NOSONAR in body and body.startswith("// NOSONAR"):
        return text[start : start + len(NOSONAR)]
    m = re.match(r"^(// SPDX-License-Identifier: [\w.+-]+)\s", body)
    if m and text.count("\n", 0, start) == 0:
        return m.group(1)
    return None


def strip(path, text):
    pieces, pos, touched = [], 0, set()
    for s, e in comments(path, text):
        if allowed(path, text, s, e):
            continue
        keep = keep_minimal(path, text, s, e) or ""
        pieces.append(text[pos:s])
        pieces.append(keep + "\n" * (text.count("\n", s, e) - keep.count("\n")))
        touched.update(range(text.count("\n", 0, s), text.count("\n", 0, e) + 1))
        pos = e
    pieces.append(text[pos:])
    result, gaps = [], set()
    for idx, line in enumerate("".join(pieces).split("\n")):
        if idx in touched:
            if line.strip() == "":
                gaps.add(len(result))
                continue
            line = line.rstrip()
        result.append(line)
    return "\n".join(tidy_blank(result, gaps))


def tidy_blank(lines, gaps):
    out = []
    for idx, line in enumerate(lines):
        if line.strip() == "":
            at_gap = idx in gaps or (idx + 1) in gaps
            if at_gap:
                prev = out[-1] if out else None
                nxt = next((l for l in lines[idx + 1 :] if True), None)
                if prev is None or prev.strip() == "":
                    continue
                if prev.rstrip().endswith(("{", "[", "(")):
                    continue
                if nxt is not None and nxt.strip().startswith(("}", "]", ")")):
                    continue
        out.append(line)
    while len(out) > 1 and out[-1] == "" and out[-2] == "":
        out.pop()
    return out


def tracked():
    files = subprocess.run(["git", "ls-files", "-z"], capture_output=True, check=True).stdout
    for path in files.decode().split("\0"):
        if path and not path.startswith(EXCLUDED_PREFIXES) and language(path):
            yield path


def line_of(text, offset):
    return text.count("\n", 0, offset) + 1


def main(argv):
    fix = argv[:1] == ["--fix"]
    paths = argv[1:] if fix else argv
    paths = paths or list(tracked())
    bad = 0
    for path in paths:
        with open(path, encoding="utf-8") as f:
            text = f.read()
        if fix:
            new = strip(path, text)
            if new != text:
                with open(path, "w", encoding="utf-8") as f:
                    f.write(new)
            continue
        for s, e in offences(path, text):
            bad += 1
            print(f"{path}:{line_of(text, s)}: {text[s:e].splitlines()[0][:80]}")
    if bad:
        print(f"no-comments: {bad} comment(s); Lautta has no code comments (AGENTS.md).", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
