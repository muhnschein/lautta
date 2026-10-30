#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-2.1-or-later
"""English plural forms for `%n` messages of the engineering English catalogue.

`english-plurals.py fill < in.ts > out.ts` gives each message with empty
`<numerusform>`s two forms: singular (by rule) and plural (the source text). Filter, no
file names. Adds `language="en"` when missing: lrelease writes no plural rules without it.

Filled when the forms are not two non-empty ones with the source text last. A hand-corrected
singular stays until the source changes.

Rule: word after `%n` loses its plural ending, agreeing verbs follow ("are" -> "is");
participles and a few others stay ("%n paused", "%n more"). Any other word or "(s)" is
refused: message left, rest filled, run fails, write both forms by hand. Read the pairs
printed to stderr: the rule knows no other verbs.
"""
import re
import sys

MESSAGE = re.compile(r' {4}<message id="(?P<id>[^"]*)"(?P<numerus> numerus="yes")?>\n.*?\n {4}</message>\n', re.S)
SOURCE = re.compile(r"<source>(.*?)</source>", re.S)
FORM = re.compile(r"<numerusform>(.*?)</numerusform>", re.S)
AFTER_N = re.compile(r"%n (\w+)")
HEADER = re.compile(r"<TS ([^>]*)>")
ENDINGS = (("ies", "y"), ("sses", "ss"), ("xes", "x"))
# Plural endings with two singulars ("matches", "caches"): left to a hand.
AMBIGUOUS = ("ches", "shes")
STAYS = ("ed", "ing")
STAYS_WORDS = ("more", "at")
# Plural form, singular form. Apostrophe XML-escaped as lupdate writes it.
VERBS = (
    ("are", "is"),
    ("were", "was"),
    ("have", "has"),
    ("aren&apos;t", "isn&apos;t"),
    ("weren&apos;t", "wasn&apos;t"),
    ("haven&apos;t", "hasn&apos;t"),
    ("don&apos;t", "doesn&apos;t"),
)


def one(noun):
    """The singular of a noun in the plural; ValueError when `noun` is no such noun."""
    for ending, instead in ENDINGS:
        if noun.endswith(ending):
            return noun[: -len(ending)] + instead
    if noun.endswith("s") and not noun.endswith(("ss", "us", "is") + AMBIGUOUS):
        return noun[:-1]
    if noun.endswith(STAYS) or noun in STAYS_WORDS:
        return noun
    raise ValueError(f'cannot tell the singular of "{noun}": write it by hand')


def singular(source):
    """`source` for n = 1."""
    found = AFTER_N.search(source)
    if not found:
        return source
    head = source[: found.start(1)] + one(found.group(1))
    tail = source[found.end(1) :]
    for plural, single in VERBS:
        tail = re.sub(rf"\b{plural}\b", single, tail)
    return head + tail


def valid(source, forms):
    """Two non-empty forms, the plural one the source text."""
    return len(forms) == 2 and all(forms) and forms[1] == source


def filled(block, forms):
    """`block` (one message) with `forms` as its finished translation."""
    head = block[: block.index("        <translation")]
    head = re.sub(r" {8}<oldsource>.*?</oldsource>\n", "", head, flags=re.S)
    lines = "".join(f"            <numerusform>{form}</numerusform>\n" for form in forms)
    return f"{head}        <translation>\n{lines}        </translation>\n    </message>\n"


def fill_message(match, made):
    """Forms made go to `made` as (id, forms). ValueError on a message the rule must not guess."""
    block, name = match.group(0), match.group("id")
    source = SOURCE.search(block).group(1)
    if not match.group("numerus"):
        if "%n" in source:
            raise ValueError(f"{name}: has %n but qsTrId gets no count")
        return block
    if "(s)" in source or "(es)" in source:
        raise ValueError(f"{name}: write a plain plural, not (s)")
    if valid(source, FORM.findall(block)):
        return block
    try:
        forms = [singular(source), source]
    except ValueError as error:
        raise ValueError(f"{name}: {error}") from error
    made.append((name, forms))
    return filled(block, forms)


def english(text):
    """`text` with `language="en"` in its header; ValueError for another language."""
    header = HEADER.search(text)
    if header is None:
        raise ValueError("not a TS catalogue")
    if "language=" not in header.group(1):
        return text[: header.end(1)] + ' language="en"' + text[header.end(1) :]
    if 'language="en"' not in header.group(1):
        raise ValueError('the engineering English catalogue must be language="en"')
    return text


def fill(text):
    """(text, made as (id, forms), refused as texts)."""
    text = english(text)
    made, refused = [], []

    def one_message(match):
        try:
            return fill_message(match, made)
        except ValueError as error:
            refused.append(str(error))
            return match.group(0)

    return MESSAGE.sub(one_message, text), made, refused


def main(argv):
    if argv[1:] != ["fill"]:
        print("usage: english-plurals.py fill < in.ts > out.ts", file=sys.stderr)
        return 2
    # UTF-8 whatever the locale says.
    for stream in (sys.stdin, sys.stdout, sys.stderr):
        stream.reconfigure(encoding="utf-8")
    try:
        out, made, refused = fill(sys.stdin.read())
    except ValueError as error:
        print(f"english-plurals: {error}", file=sys.stderr)
        return 1
    sys.stdout.write(out)
    for name, (single, plural) in made:
        print(f"english-plurals: {name}: {single} | {plural}", file=sys.stderr)
    for reason in refused:
        print(f"english-plurals: {reason}", file=sys.stderr)
    return 1 if refused else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
