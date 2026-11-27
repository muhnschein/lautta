// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import "../../../qml/components/FileIcons.js" as FileIcons

Item {
    property bool reachedEnd
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

    function testKinds() {
        var categories = ["folder", "image", "video", "audio", "archive", "package", "database",
                          "text", "code", "markdown", "pdf", "document", "spreadsheet", "presentation",
                          "mail", "calendar", "contact", "notebook", "key", "executable", "font", "disk", "cad",
                          "file", ""]
        same(categories.map(function (c) { return FileIcons.kind(c, false) }),
             ["folder", "image", "video", "audio", "archive", "package", "database",
              "text", "code", "markdown", "pdf", "document", "spreadsheet", "presentation",
              "mail", "calendar", "contact", "notebook", "key", "executable", "font", "disk", "cad", "file",
              "file"], "the glyph of each category, unknown ones the plain page")
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
             ["file-pdf-28", "file-folder-32", "file-archive-28"], "the glyph of an item at a size")
    }

    Image {
        id: drawn
    }

    function testSource() {
        var want = String(Qt.resolvedUrl("../../../qml/icons/server.svg")).replace(/^file:\/\//, "")
        same(FileIcons.source("server", 1.5), "image://lautta-icon" + want + "?stroke=3",
             "an app icon comes from the icon provider, outlines 2 units at the pixel ratio")
        same(FileIcons.source("file-folder-28", 1.75).slice(-11), "?stroke=3.5", "pixel ratio 1.75: 3.5 px")
        same(FileIcons.source("image://theme/icon-m-favorite", 1.5), "image://theme/icon-m-favorite", "a theme icon stays")
        same(FileIcons.source("", 1.5), "", "no icon stays none")
        drawn.sourceSize = Qt.size(168, 168)
        drawn.source = FileIcons.source("file-folder-28", 1.5)
        check(drawn.status === Image.Ready && drawn.implicitWidth === 168,
              "the provider draws the glyph at its size: status " + drawn.status + ", " + drawn.implicitWidth + " px")
    }

    Component.onCompleted: {
        testSource()
        testSets()
        testSnap()
        testKinds()
        testTokens()
        reachedEnd = true
    }
}
