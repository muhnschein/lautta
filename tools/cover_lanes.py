#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-2.1-or-later
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
ICONS = ROOT / "qml/icons"
OUT = ROOT / "qml/cover"

WIDTH, HEIGHT = 234, 374
FIELD_W, FIELD_H = 460, 560
TILT = -18
LANES, COLUMNS = 13, 11
STEP, PITCH, GLYPH = 46, 44, 26
STROKE = 1.5
GLYPHS = ["file-database-28", "file-video-28", "file-document-28", "types/text", "types/markdown", "link",
          "types/json", "file-folder-28", "server", "home", "file-package-28", "lock", "types/shell", "dir-copy",
          "file-image-28", "file-audio-28", "file-archive-28", "types/rust"]
FAINT = {0: 0.55, 2: 0.55, 3: 0.42}


def lit(lane):
    return lane % 4 == 1


def glyph_at(lane, column):
    if lit(lane) and column % 4 == (lane // 4) % 4:
        return "lautta"
    return GLYPHS[(lane * COLUMNS + column) % len(GLYPHS)]


def parse(name):
    text = re.sub(r"<!--.*?-->", "", (ICONS / f"{name}.svg").read_text(), flags=re.S)
    head, body = re.search(r"<svg([^>]*)>(.*)</svg>", text, re.S).groups()
    box = [float(v) for v in re.search(r'viewBox="([^"]*)"', head).group(1).split()]
    keep = {k: v for k, v in re.findall(r'\b(fill|stroke|stroke-linecap|stroke-linejoin)="([^"]*)"', head)}
    return box, keep, re.sub(r"\s*\n\s*", "", body.strip())


def glyph(name, x, y):
    box, keep, body = parse(name)
    scale = GLYPH / box[2]
    attrs = "".join(f' {k}="{v}"' for k, v in sorted(keep.items()))
    width = round(STROKE / scale, 4)
    return (f'<g transform="translate({x:g} {y:g}) scale({round(scale, 6):g}) translate({-box[0] + 0:g} {-box[1] + 0:g})"'
            f'{attrs} stroke-width="{width:g}">{body}</g>')


def lanes(keep):
    out = []
    for lane in range(LANES):
        if not keep(lane):
            continue
        y = lane * PITCH + 8
        x0 = (lane % 2) * 24 - 20
        glyphs = "".join(glyph(glyph_at(lane, c), x0 + c * STEP, y) for c in range(COLUMNS))
        opacity = "" if lit(lane) else f' opacity="{FAINT[lane % 4]:g}"'
        out.append(f"<g{opacity}>{glyphs}</g>")
    return out


def svg(keep):
    left, top = (WIDTH - FIELD_W) / 2, (HEIGHT - FIELD_H) / 2
    rotate = f"rotate({TILT} {WIDTH / 2:g} {HEIGHT / 2:g})"
    body = "\n".join(lanes(keep))
    return ("<!-- SPDX-License-Identifier: LGPL-2.1-or-later AND Apache-2.0 -->\n"
            "<!-- Made by tools/cover_lanes.py from qml/icons; type glyphs: JetBrains s.r.o. and contributors, "
            "Apache 2.0 (qml/icons/types/README.md). -->\n"
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}">\n'
            f'<g transform="{rotate} translate({left:g} {top:g})">\n{body}\n</g>\n</svg>\n')


def files():
    return {OUT / "lanes-lit.svg": svg(lit), OUT / "lanes-faint.svg": svg(lambda lane: not lit(lane))}


def main(argv):
    stale = [p for p, text in files().items() if not p.exists() or p.read_text() != text]
    if "--check" in argv:
        for p in stale:
            print(f"{p.relative_to(ROOT)} is stale: run python3 tools/cover_lanes.py")
        return 1 if stale else 0
    for p, text in files().items():
        p.write_text(text)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
