#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-2.1-or-later
import contextlib
import io
import pathlib
import re
import tempfile
import unittest
from unittest import mock

import cover_lanes as cl


def lanes_of(text):
    return re.findall(r"\n<g( opacity=\"[0-9.]+\")?>(.*?)</g>(?=\n)", text)


class Pattern(unittest.TestCase):
    def test_every_fourth_lane_is_lit(self):
        self.assertEqual([n for n in range(cl.LANES) if cl.lit(n)], [1, 5, 9])

    def test_lautta_glyph_rides_lit_lanes_a_step_further_each(self):
        rows = [[c for c in range(cl.COLUMNS) if cl.glyph_at(n, c) == "lautta"] for n in range(cl.LANES)]
        self.assertEqual(rows, [[], [0, 4, 8], [], [], [], [1, 5, 9], [], [], [], [2, 6, 10], [], [], []])

    def test_other_glyphs_cycle_through_own_icons(self):
        names = {cl.glyph_at(n, c) for n in range(cl.LANES) for c in range(cl.COLUMNS)}
        self.assertEqual(names, set(cl.GLYPHS) | {"lautta"})
        for name in names:
            self.assertTrue((cl.ICONS / f"{name}.svg").exists(), name)


class Drawing(unittest.TestCase):
    def setUp(self):
        self.files = {p.name: t for p, t in cl.files().items()}

    def test_lit_file_holds_lit_lanes_only_at_full_strength(self):
        lanes = lanes_of(self.files["lanes-lit.svg"])
        self.assertEqual(len(lanes), 3)
        self.assertTrue(all(op == "" for op, _ in lanes))
        self.assertEqual([body.count('d="M12.5 11H40') for _, body in lanes], [3, 3, 3])

    def test_faint_file_holds_the_rest_faded(self):
        lanes = lanes_of(self.files["lanes-faint.svg"])
        self.assertEqual([op for op, _ in lanes],
                         [' opacity="0.55"', ' opacity="0.55"', ' opacity="0.42"'] * 2 + [' opacity="0.55"', ' opacity="0.55"', ' opacity="0.42"'] + [' opacity="0.55"'])

    def test_glyphs_sized_placed_and_stroked_alike(self):
        body = lanes_of(self.files["lanes-lit.svg"])[0][1]
        first = re.findall(r'<g transform="translate\(([-0-9.]+) ([-0-9.]+)\) scale\(([0-9.]+)\)[^>]*stroke-width="([0-9.]+)"', body)
        self.assertEqual(len(first), cl.COLUMNS)
        self.assertEqual([float(x) for x, _, _, _ in first], [4 + c * cl.STEP for c in range(cl.COLUMNS)])
        self.assertTrue(all(float(y) == cl.PITCH + 8 for _, y, _, _ in first))
        self.assertTrue(all(abs(float(s) * float(w) - cl.STROKE) < 1e-3 for _, _, s, w in first))

    def test_field_tilted_about_the_cover_centre(self):
        for text in self.files.values():
            self.assertIn('viewBox="0 0 234 374"', text)
            self.assertIn('<g transform="rotate(-18 117 187) translate(-113 -93)">', text)
            self.assertTrue(text.startswith("<!-- SPDX-License-Identifier: LGPL-2.1-or-later AND Apache-2.0 -->"))


class Command(unittest.TestCase):
    def test_writes_then_checks_clean_and_finds_stale(self):
        with tempfile.TemporaryDirectory() as tmp, mock.patch.object(cl, "OUT", pathlib.Path(tmp)), \
                mock.patch.object(cl, "ROOT", pathlib.Path(tmp)), contextlib.redirect_stdout(io.StringIO()) as said:
            self.assertEqual(cl.main(["--check"]), 1)
            self.assertIn("lanes-lit.svg is stale", said.getvalue())
            self.assertEqual(cl.main([]), 0)
            self.assertEqual(cl.main(["--check"]), 0)
            (pathlib.Path(tmp) / "lanes-lit.svg").write_text("old")
            self.assertEqual(cl.main(["--check"]), 1)

    def test_committed_files_are_current(self):
        self.assertEqual(cl.main(["--check"]), 0)


if __name__ == "__main__":
    unittest.main()
