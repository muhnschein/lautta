// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of the app's own file-type glyphs (FileIcons.js, UI-2): the set
// each pixel size gets, so strokes land on whole pixels (28-unit set at
// multiples of 28, 32-unit set at multiples of 32, the 80 px set at 80); the
// grid's snap to multiples of 28; the video badge's side, the largest sharp
// one up to its target; the glyph of each category; glyph tokens sized,
// anything else left as it is. Whether the glyphs look sharp only a
// phone shows.
import QtQuick 2.6
import "../../../qml/components/FileIcons.js" as FileIcons

Item {
    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function same(a, b, what) {
        check(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    function sets(sizes) {
        return sizes.map(function (px) { return FileIcons.setFor(px) })
    }

    function testSets() {
        // A list row's glyph is Theme.iconSizeMedium: scale 1.0, 1.25, 1.5, 1.75, 2.0.
        same(sets([64, 80, 96, 112, 128]), [32, 80, 32, 28, 32], "list sizes")
        same(sets([28, 56, 84, 112, 140, 168, 196, 224, 252]), [28, 28, 28, 28, 28, 28, 28, 28, 28],
             "multiples of 28: the grid")
        same(sets([32, 160, 192]), [32, 32, 32], "multiples of 32 that are not of 28")
        same(sets([240, 400]), [80, 80], "multiples of 80 that are not of 28 or 32")
        same(sets([0, 24, 30, 36, 42, 48, 100]), [28, 28, 28, 28, 28, 28, 28], "no set fits: the 28-unit one")
        same(sets([63.6, 79.5, 111.7]), [32, 80, 28], "a fractional size is rounded first")
    }

    function testSnap() {
        same([84, 97.9, 98, 170, 228, 340].map(function (px) { return FileIcons.snap(px) }),
             [84, 84, 112, 168, 224, 336], "the nearest multiple of 28")
        same([0, 10, 41].map(function (px) { return FileIcons.snap(px) }), [28, 28, 28], "at least 28")
    }

    function testBadge() {
        var badge = function (px) { return FileIcons.sharpAtMost(px) }
        // Three eighths of a list row's thumbnail, scale 1.0 to 2.0.
        same([24, 30, 36, 42, 48].map(badge), [28, 28, 32, 32, 32], "list badges")
        // Theme.iconSizeSmall on a grid tile, scale 1.0 to 2.0.
        same([32, 40, 48, 56, 64].map(badge), [32, 32, 32, 56, 64], "grid badges")
        same([0, 27.9, 63.5, 84, 100].map(badge), [28, 28, 56, 84, 96], "the largest multiple of 28 or 32, at least 28")
        same([28, 32, 56, 64].map(function (px) { return FileIcons.setFor(px) }), [28, 32, 28, 32],
             "each badge size has a sharp set")
    }

    function testKinds() {
        var categories = ["folder", "image", "video", "audio", "archive", "package", "database",
                          "text", "code", "markdown", "pdf", "document", "spreadsheet", "presentation", "file", ""]
        same(categories.map(function (c) { return FileIcons.kind(c, false) }),
             ["folder", "image", "video", "audio", "archive", "package", "database",
              "document", "document", "document", "document", "document", "document", "document", "document",
              "document"], "the glyph of each category: text and office files share the document glyph")
        same(FileIcons.kind("image", true), "folder", "a folder is a folder, whatever its name says")
    }

    function testTokens() {
        same([FileIcons.sized("file-folder", 64), FileIcons.sized("file-document", 80),
              FileIcons.sized("file-image", 168)],
             ["file-folder-32", "file-document-80", "file-image-28"], "a glyph token gets the set of its size")
        same(FileIcons.sized("image://theme/icon-m-favorite", 64), "image://theme/icon-m-favorite",
             "a theme icon stays")
        same(FileIcons.sized("dir-copy", 64), "dir-copy", "another app icon stays")
        same([FileIcons.glyph("pdf", false, 112), FileIcons.glyph("video", true, 96),
              FileIcons.glyph("archive", false, 84)],
             ["file-document-28", "file-folder-32", "file-archive-28"], "the glyph of an item at a size")
    }

    Component.onCompleted: {
        testSets()
        testSnap()
        testBadge()
        testKinds()
        testTokens()
    }
}
