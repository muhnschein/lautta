// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of the text thumbnails of DirectoryModel (no Silica, PRV-15):
// text, code and Markdown files have one (`textThumb`), made when a cell
// asks (`requestPreview`), again only after the file changed, dropped when
// the cell goes first (`cancelPreview`); its font and type icon by name;
// none for other files, binary or empty ones, or with thumbnails off.
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    readonly property string base: "lautta://user-downloads/TextThumbs/"

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function waitFor(cond, ms) {
        for (var t = 0; t < ms && !cond(); t += 20)
            tools.pump(20)
    }

    function install(fixture, name) {
        check(tools.installFixture(Qt.resolvedUrl("fixtures/" + fixture), root.base + name), "fixture " + name)
    }

    function row(name) {
        for (var i = 0; i < rows.count; ++i)
            if (rows.itemAt(i).nm === name)
                return rows.itemAt(i)
        return null
    }

    function ask(name) {
        thumbs.requestPreview(root.base + name)
    }

    ViewerTools { id: tools }

    property int changes

    DirectoryModel { id: downloads; uri: "lautta://user-downloads/" }
    DirectoryModel { id: thumbs }

    Repeater {
        id: rows
        model: thumbs
        delegate: Item {
            readonly property string nm: name
            readonly property bool has: textThumb
            readonly property string text: textPreview
            readonly property bool mono: textPreviewMono
            readonly property string type: typeIcon
            readonly property string source: thumbnailSource
        }
    }

    function testWhoHasOne() {
        waitFor(function () { return !downloads.loading }, 2000)
        downloads.createFolder("TextThumbs")
        waitFor(function () { return downloads.indexOf(root.base.slice(0, -1)) >= 0 }, 2000)
        thumbs.uri = root.base
        thumbs.createFolder("Sub")
        install("notes-crlf.txt", "notes.txt")
        install("script.sh", "run.sh")
        install("doc.md", "README.md")
        install("data.bin", "blob.txt")
        install("empty.txt", "empty.txt")
        install("photo.jpg", "a.jpg")
        thumbs.refresh()
        waitFor(function () { return thumbs.count === 7 && !thumbs.loading }, 3000)
        check(thumbs.count === 7, "all listed: " + thumbs.count)
        var want = { "notes.txt": true, "run.sh": true, "README.md": true, "blob.txt": true,
                     "empty.txt": false, "a.jpg": false, "Sub": false }
        for (var n in want)
            check(row(n).has === want[n], n + " textThumb " + row(n).has)
        check(row("notes.txt").source === "", "a text thumbnail is no image")
        check(row("notes.txt").text === "", "none before the cell asks")
        check(row("run.sh").mono && !row("notes.txt").mono && !row("README.md").mono, "code is monospace, prose not")
        check(row("run.sh").type === "types/shell" && row("notes.txt").type === "types/text"
              && row("README.md").type === "types/markdown", "type icons by name")
    }

    // Each request asked for ends in one change of its row, binary data too.
    function testMade() {
        var before = root.changes
        ask("notes.txt")
        ask("run.sh")
        ask("blob.txt")
        ask("a.jpg")
        ask("Sub")
        waitFor(function () { return root.changes - before >= 3 }, 3000)
        check(root.changes - before === 3, "three made: " + (root.changes - before))
        check(row("notes.txt").text === "line one\nline two", "first lines, CRLF tidied: " + JSON.stringify(row("notes.txt").text))
        check(row("run.sh").text.indexOf("#!/bin/sh\n") === 0, "a script's first line: " + row("run.sh").text)
        check(row("blob.txt").text === "", "binary data has none")
        check(row("a.jpg").text === "" && row("Sub").text === "", "nor other files and folders")
    }

    function testAskingAgain() {
        var before = root.changes
        for (var i = 0; i < 3; ++i)
            ask("notes.txt")
        tools.pump(300)
        check(root.changes === before, "asking again changed " + (root.changes - before) + " times")
        check(row("notes.txt").text === "line one\nline two", "the preview stands")
    }

    function testChangedFile() {
        install("short-lines.txt", "notes.txt")
        check(tools.setModified(root.base + "notes.txt", 1700000000000), "new modified time")
        thumbs.refresh()
        waitFor(function () { return row("notes.txt") && row("notes.txt").text === "" }, 3000)
        check(row("notes.txt").text === "", "the old preview does not stand")
        ask("notes.txt")
        waitFor(function () { return row("notes.txt").text !== "" }, 3000)
        check(row("notes.txt").text !== "line one\nline two" && row("notes.txt").text !== "",
              "made again: " + row("notes.txt").text)
    }

    function testCancelled() {
        ask("README.md")
        thumbs.cancelPreview(root.base + "README.md")
        tools.pump(300)
        check(row("README.md").text === "", "a cancelled preview is dropped")
        ask("README.md")
        waitFor(function () { return row("README.md").text !== "" }, 3000)
        check(row("README.md").text !== "", "asked again, it comes")
        var before = root.changes
        thumbs.cancelPreview(root.base + "README.md")
        ask("README.md")
        tools.pump(300)
        check(row("README.md").text !== "" && root.changes === before, "a made one stays when its cell goes")
    }

    function testSwitchedOff() {
        check(thumbs.setViewPrefs(JSON.stringify({ thumbnails: false })), "thumbnails off")
        check(!row("run.sh").has, "no text thumbnail with thumbnails off")
        ask("README.md")
        thumbs.setViewPrefs(JSON.stringify({ thumbnails: true }))
        check(row("run.sh").has, "back on")
    }

    Component.onCompleted: {
        thumbs.dataChanged.connect(function () { root.changes += 1 })
        testWhoHasOne()
        testMade()
        testAskingAgain()
        testChangedFile()
        testCancelled()
        testSwitchedOff()
    }
}
