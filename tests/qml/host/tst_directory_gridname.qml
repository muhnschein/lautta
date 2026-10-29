// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of the name cut of a grid cell at 3 per row (GridName.js, SPEC
// §15.3): a name that fits stays whole, a long one takes two lines with the
// beginning on the first and the end, the extension kept, on the second, and
// no cut splits a character. The width is measured with a fixed pitch of 10
// per character, 150 for a line of 15.
import QtQuick 2.6
import "../../../qml/components/directory/GridName.js" as GridName

Item {
    id: root

    readonly property int lineWidth: 150

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function same(a, b, what) {
        check(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    // A pair counts for one character, as a glyph does.
    function measure(text) {
        return text.replace(/[\uD800-\uDBFF][\uDC00-\uDFFF]/g, "x").length * 10
    }

    function lines(name, width) {
        return GridName.lines(name, width === undefined ? root.lineWidth : width, root.measure)
    }

    function ends(text, tail) {
        return text.length >= tail.length && text.lastIndexOf(tail) === text.length - tail.length
    }

    function starts(text, head) {
        return text.indexOf(head) === 0
    }

    function fits(out, what) {
        check(out.length >= 1 && out.length <= 2, what + ": one or two lines, not " + out.length)
        for (var i = 0; i < out.length; ++i)
            check(measure(out[i]) <= root.lineWidth, what + ": line " + i + " fits: " + out[i])
    }

    function testShortNames() {
        same(lines("Exam notes.txt"), ["Exam notes.txt"], "a short name is whole")
        same(lines("123456789012345"), ["123456789012345"], "a name of exactly one line is whole")
        same(lines(""), [""], "no name, no line")
        same(lines("a"), ["a"], "one letter")
    }

    function testTwoLinesWhole() {
        same(lines("first-line-ab second-line-x"), ["first-line-ab", "second-line-x"],
             "a name that fits two lines is broken at its space")
        same(lines("photos-from-the-excursion.zip"), ["photos-from-the", "-excursion.zip"],
             "and shown whole where a break in a word is needed")
        check(lines("photos-from-the-excursion.zip").join("").indexOf(GridName.ELLIPSIS) < 0, "with no ellipsis")
        same(lines("Holiday photos from Italy.zip"), ["Holiday photos", "from Italy.zip"],
             "words are not cut when the rest fits")
    }

    // The board: the cut is in the middle of the second line and the end stays.
    function testLongNameKeepsItsExtension() {
        var name = "Minutes of the department meeting 2026-10-14.pdf"
        var out = lines(name, 200)
        same(out.length, 2, "two lines")
        same(out[0], "Minutes of the", "the first line breaks at a space")
        check(starts(out[1], "departme"), "the second line starts with its beginning: " + out[1])
        check(out[1].indexOf("…") > 0, "and has an ellipsis in the middle: " + out[1])
        check(ends(out[1], "10-14.pdf"), "and ends with the end of the name: " + out[1])
        for (var i = 0; i < out.length; ++i)
            check(measure(out[i]) <= 200, "line " + i + " fits: " + out[i])
        out = lines("Thesis_draft_chapter3_final_v2.odt")
        fits(out, "an underscore name")
        same(out[0], "Thesis_draft_", "the first line breaks after an underscore")
        check(out[1].indexOf("…") > 0 && ends(out[1], "_v2.odt"), "and keeps the end: " + out[1])
        out = lines("Lecture 05 - Distributed systems and algorithms.pdf")
        fits(out, "a name with spaces")
        check(ends(out[1], "ms.pdf") || ends(out[1], "s.pdf"), "the extension stays: " + out[1])
    }

    function testNoExtension() {
        var out = lines("AVeryLongDirectoryNameWithoutAnyDots")
        fits(out, "no extension")
        same(out[0], "AVeryLongDirect", "the first line is full")
        check(out[1].indexOf("…") > 0 && ends(out[1], "AnyDots"), "the end of the name stays: " + out[1])
        out = lines(".hidden-configuration-directory-name-and-more")
        fits(out, "a hidden name is not all extension")
        check(starts(out[0], ".hidden-"), "its dot starts the name: " + out[0])
    }

    function testVeryLongSingleWord() {
        var name = ""
        for (var i = 0; i < 25; ++i)
            name += "word" + i
        name += ".txt"
        var out = lines(name)
        fits(out, "a long word")
        same(out.length, 2, "two lines")
        check(starts(out[0], "word0word1"), "the beginning: " + out[0])
        check(out[1].indexOf("…") > 0 && ends(out[1], "d24.txt"), "the end: " + out[1])
        var big = new Array(300).join("x") + ".png"
        out = lines(big)
        fits(out, "a name of 300 characters")
        check(ends(out[1], ".png"), "keeps .png: " + out[1])
        out = lines(new Array(40).join("y") + ".tar.gz")
        check(ends(out[1], ".tar.gz") || ends(out[1], ".gz"), "keeps the end of a double extension: " + out[1])
    }

    function testLongExtensionIsNotKeptWhole() {
        var out = lines("a-name-with-a.very-very-very-long-extension-after-the-dot")
        fits(out, "a long extension")
        check(out[1].indexOf("…") > 0, "still elided: " + out[1])
    }

    function repeated(text, times) {
        var out = ""
        for (var i = 0; i < times; ++i)
            out += text
        return out
    }

    // A line starting with the second half of a character, or ending with the
    // first half of one, or an ellipsis cutting one in two.
    function torn(text) {
        return /(^|\u2026)[\uDC00-\uDFFF\u0300-\u036F\u200D\uFE0F]|[\uD800-\uDBFF]($|\u2026)/.test(text)
    }

    // A cut never parts a pair of surrogates, a letter from its accent or a
    // sequence from its joiner, at any width and any offset of the cut.
    function testNoSplitCharacters() {
        var bases = [
            repeated("\uD83D\uDE00", 40) + ".png",
            repeated("e\u0301", 40) + ".txt",
            repeated("\uD83D\uDC68\u200D\uD83D\uDC69\u200D\uD83D\uDC67", 20) + ".jpg",
            repeated("\u2764\uFE0F", 30) + ".gif",
            repeated("\uD800\uDC00", 40) + ".bin",
            repeated("\uDBFF\uDFFF", 40) + ".bin",
            repeated("a\u0300", 40) + ".txt",
            repeated("a\u036F", 40) + ".txt"
        ]
        var cuts = 0
        for (var b = 0; b < bases.length; ++b) {
            for (var pad = 0; pad < 4; ++pad) {
                var name = "abc".slice(0, pad) + bases[b]
                for (var width = 0; width <= 300; width += 10) {
                    var out = lines(name, width)
                    for (var i = 0; i < out.length; ++i) {
                        ++cuts
                        check(!torn(out[i]), "a character is whole: " + JSON.stringify(out) + " at " + width)
                    }
                }
            }
        }
        check(cuts > 0, "cuts were made")
        var out = lines(bases[0])
        fits(out, "emoji")
        check(ends(out[1], ".png"), "keeps the extension: " + out[1])
        out = lines(bases[1])
        fits(out, "accents")
        check(ends(out[1], ".txt"), "keeps the extension: " + out[1])
        same(lines("e\u0301xyzabcdefghijklm", 5), ["e\u0301", "\u2026"], "a first line takes its letter with the accent")
        out = lines("alpha-beta-\u0301gamma-delta-epsilon.txt")
        check(!torn(out[0]) && !torn(out[1]), "no break parts a mark from the dash before it: " + JSON.stringify(out))
    }

    // The cuts pinned to the character, so that a break, a share or a bound
    // moved by one shows.
    function testPinnedCuts() {
        same(lines("alpha-beta-gamma-delta-epsilon.txt"), ["alpha-beta-", "gamma-d\u2026lon.txt"],
             "a break after a dash")
        same(lines("alpha_beta_gamma_delta_epsilon.txt"), ["alpha_beta_", "gamma_d\u2026lon.txt"],
             "a break after an underscore")
        same(lines("abcdefgh ijklmnopqrstuvwxyz0123456789.txt"), ["abcdefgh ijklmn", "opqrstu\u2026789.txt"],
             "a space far back is not a break")
        same(lines("abcdefghi jklmnopqrstuvwxyz0123456789.txt", 140), ["abcdefghi jklm", "nopqrs\u2026789.txt"],
             "the far back is two thirds of the line, rounded up")
        same(lines("abc defghijklmnopqr"), ["abc", "defghijklmnopqr"],
             "an early break is taken when the second line fits whole, to the character")
        same(lines("abcdefghijk lmnopqrstuvwxyz.tx"), ["abcdefghijk lmn", "opqrstuvwxyz.tx"],
             "a rest that fills a line to the character needs no break")
        same(lines("abcdefghijklmnopqrstuvwxyz.txt"), ["abcdefghijklmno", "pqrstuvwxyz.txt"],
             "a second line that fills the width is not elided")
        same(lines("AVeryLongDirectoryNameWithoutAnyDots"), ["AVeryLongDirect", "oryName\u2026AnyDots"],
             "the elided line is as wide as the cell")
        same(lines("AVeryLongDirectoryNameWithoutAnyDots", 160), ["AVeryLongDirecto", "ryNameW\u2026tAnyDots"],
             "the end takes the larger share")
        same(lines("Ubuntu-24.04-desktop-amd64-final-edition.torrent"), ["Ubuntu-24.04-", "des\u2026ion.torrent"],
             "a long extension stays with some of the name")
        same(lines("Ubuntu-24.04-desktop-amd64-final-edition.markdown"), ["Ubuntu-24.04-", "desktop\u2026arkdown"],
             "a longer one is cut like the rest")
        same(lines("abcdefghijklmnopq", 5), ["a", "\u2026"], "less than a character wide")
    }

    function testBlanks() {
        same(lines(repeated(" ", 20) + "end.txt"), ["end.txt"], "leading spaces are no line")
        same(lines("word" + repeated(" ", 20)), ["word"], "trailing spaces are no line")
        same(lines(repeated(" ", 20)), [""], "only spaces are one empty line")
    }

    // Right to left names are cut in logical order, which Qt shows right.
    function testRightToLeft() {
        var out = lines("שלום עולם מסמך חשוב מאוד לקריאה.pdf")
        fits(out, "Hebrew")
        check(ends(out[out.length - 1], ".pdf"), "keeps the extension: " + JSON.stringify(out))
    }

    function testOddInput() {
        fits(lines("abcdefghijklmnopqrstuvwxyz", 0), "no width")
        fits(lines("abcdefghijklmnopqrstuvwxyz", 5), "less than a character")
        check(lines("   ").length >= 1, "only spaces")
        fits(lines("trailing spaces and more words                         "), "trailing spaces")
        fits(lines("word                     another-very-long-word-here"), "a run of spaces")
        same(lines("abcdefghijklmnopq", 10).length, 2, "narrow width")
    }

    Component.onCompleted: {
        testShortNames()
        testTwoLinesWhole()
        testLongNameKeepsItsExtension()
        testNoExtension()
        testVeryLongSingleWord()
        testLongExtensionIsNotKeptWhole()
        testNoSplitCharacters()
        testPinnedCuts()
        testBlanks()
        testRightToLeft()
        testOddInput()
    }
}
