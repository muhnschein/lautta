#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-2.1-or-later
"""English plural forms of the engineering English catalogue (UI-7).

lupdate lists a `%n` message with empty `<numerusform>`s. lrelease then makes
the `//%` text the answer for every n, "1 items". `english-plurals.py fill
FILE.ts` gives each such message its two English forms: the singular and the
plural, which is the source text itself. It runs after lupdate, in
`make translations` and in `make translations-check`.

The catalogue gets `language="en"` when it has none: lrelease writes no plural
rules without it.

A message is filled when its forms are not two non-empty ones with the source
text last: a new message, or one whose source changed. A singular someone
corrected by hand stays until the source changes.

The singular comes from the source by rule: the word after `%n` loses its
plural ending ("%n items" -> "%n item"), and the verbs after it that agree
with the plural follow ("are" -> "is"). A word after `%n` that is no noun in
the plural stays when it is a participle or one of a few others ("%n paused",
"%n more"). Any other word is refused, and so is "(s)": the message is left as
it is, the run fails, and both forms are written by hand into the `.ts`. Read
the printed pairs: the rule knows nothing of other verbs.
"""
import re
import sys

MESSAGE = re.compile(r'    <message id="(?P<id>[^"]*)"(?P<numerus> numerus="yes")?>\n.*?\n    </message>\n', re.S)
SOURCE = re.compile(r"<source>(.*?)</source>", re.S)
FORM = re.compile(r"<numerusform>(.*?)</numerusform>", re.S)
AFTER_N = re.compile(r"%n (\w+)")
HEADER = re.compile(r"<TS ([^>]*)>")
# Plural endings and what the singular has instead.
ENDINGS = (("ies", "y"), ("sses", "ss"), ("xes", "x"))
# Plural endings with two singulars ("matches", "caches"): left to a hand.
AMBIGUOUS = ("ches", "shes")
# Words after `%n` that are no plural noun and need no change.
STAYS = ("ed", "ing")
STAYS_WORDS = ("more", "at")
# Verbs that agree with the counted noun: plural form, singular form. The
# apostrophe is XML-escaped as lupdate writes it.
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
    head = re.sub(r"        <oldsource>.*?</oldsource>\n", "", head, flags=re.S)
    lines = "".join(f"            <numerusform>{form}</numerusform>\n" for form in forms)
    return f"{head}        <translation>\n{lines}        </translation>\n    </message>\n"


def fill_message(match, made):
    """The text of one message with its forms made when they are not there;
    the forms made go to `made` as (id, forms). Raises ValueError on a message
    the rule must not guess."""
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
    """The catalogue text with every English plural form in place that could be
    made, the messages whose forms were made, as (id, forms), and the messages
    left alone because the rule must not guess, as texts."""
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
    if len(argv) != 3 or argv[1] != "fill":
        print("usage: english-plurals.py fill FILE.ts", file=sys.stderr)
        return 2
    with open(argv[2], encoding="utf-8") as handle:
        text = handle.read()
    try:
        out, made, refused = fill(text)
    except ValueError as error:
        print(f"english-plurals: {error}", file=sys.stderr)
        return 1
    for name, (single, plural) in made:
        print(f"english-plurals: {name}: {single} | {plural}")
    if out != text:
        with open(argv[2], "w", encoding="utf-8") as handle:
            handle.write(out)
    for reason in refused:
        print(f"english-plurals: {reason}", file=sys.stderr)
    return 1 if refused else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
