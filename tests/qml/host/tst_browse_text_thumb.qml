// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root
    property bool reachedEnd

    readonly property string base: "lautta://user-downloads/RecentTextThumbs/"
    property int changes

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function waitFor(cond, what) {
        for (var t = 0; t < 30000 && !cond(); t += 20)
            tools.pump(20)
        check(cond(), what)
    }

    function install(fixture, name) {
        check(tools.installFixture(Qt.resolvedUrl("fixtures/" + fixture), root.base + name), "fixture " + name)
    }

    ViewerTools { id: tools }
    DirectoryModel { id: downloads; uri: "lautta://user-downloads/" }

    TextThumb {
        id: thumb

        onTextChanged: root.changes++
    }

    function testMade() {
        waitFor(function () { return !downloads.loading }, "Downloads listed")
        downloads.createFolder("RecentTextThumbs")
        waitFor(function () { return downloads.indexOf(root.base.slice(0, -1)) >= 0 }, "folder made")
        install("notes-crlf.txt", "notes.txt")
        install("script.sh", "run.sh")
        install("data.bin", "blob.txt")
        check(thumb.text === "", "nothing before a file is named")
        thumb.uri = root.base + "notes.txt"
        waitFor(function () { return thumb.text !== "" }, "first lines come")
        check(thumb.text === "line one\nline two", "first lines, CRLF tidied: " + JSON.stringify(thumb.text))
    }

    function testOtherFile() {
        thumb.uri = root.base + "run.sh"
        check(thumb.text === "", "another file: the old lines go at once")
        waitFor(function () { return thumb.text !== "" }, "the new file's lines come")
        check(thumb.text.indexOf("#!/bin/sh\n") === 0, "a script's first line: " + thumb.text)
    }

    function testSwitchedFast() {
        thumb.uri = root.base + "notes.txt"
        thumb.uri = root.base + "run.sh"
        thumb.uri = root.base + "notes.txt"
        waitFor(function () { return thumb.text !== "" }, "the last file's lines come")
        var before = root.changes
        tools.pump(300)
        check(thumb.text === "line one\nline two" && root.changes === before,
              "only the last file asked shows: " + JSON.stringify(thumb.text))
    }

    function testNone() {
        thumb.uri = root.base + "blob.txt"
        tools.pump(300)
        check(thumb.text === "", "binary data has none")
        thumb.uri = ""
        check(thumb.text === "", "no file, no lines")
    }

    Component.onCompleted: {
        testMade()
        testOtherFile()
        testSwitchedFast()
        testNone()
        reachedEnd = true
    }
}
