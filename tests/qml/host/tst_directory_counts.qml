// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of the item counts of folders in DirectoryModel (no Silica), the
// "N items" under a folder in the grid (SPEC §15.3): asked for per folder
// (`requestCount`), counted off the GUI thread, hidden files only when the
// view shows them, again after the folder changed, capped for a huge
// folder, and none for files, unreadable folders and unknown addresses.
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    readonly property string base: "lautta://user-downloads/Counts/"

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function wait(ms) { tools.pump(ms) }

    function install(name) {
        check(tools.installFixture(Qt.resolvedUrl("fixtures/notes-crlf.txt"), root.base + name), "fixture " + name)
    }

    function row(name) {
        for (var i = 0; i < rows.count; ++i)
            if (rows.itemAt(i).nm === name)
                return rows.itemAt(i)
        return null
    }

    ViewerTools { id: tools }

    property int changes
    // The cells ask on their own; a test that asks by hand switches that off.
    property bool asking: true

    DirectoryModel { id: downloads; uri: "lautta://user-downloads/" }
    DirectoryModel { id: counts }
    DirectoryModel { id: mixed }

    // Asks for the count of its folder as the grid cell does: when it
    // appears and whenever the folder's modified time changes.
    Repeater {
        id: rows
        model: counts
        delegate: Item {
            readonly property string nm: name
            readonly property int items: itemCount
            readonly property bool capped: itemCountCapped
            readonly property real stamp: modified

            onStampChanged: if (root.asking) counts.requestCount(uri)
        }
    }

    function testLocalFolders() {
        wait(300)
        downloads.createFolder("Counts")
        wait(300)
        counts.uri = root.base
        check(counts.folderCounts && downloads.folderCounts, "device folders have counts")
        counts.createFolder("Empty")
        counts.createFolder("Mixed")
        wait(400)
        mixed.uri = root.base + "Mixed/"
        wait(300)
        mixed.createFolder("sub")
        wait(300)
        check(tools.installFixture(Qt.resolvedUrl("fixtures/notes-crlf.txt"), root.base + "Mixed/a.txt"), "file in Mixed")
        check(tools.installFixture(Qt.resolvedUrl("fixtures/notes-crlf.txt"), root.base + "Mixed/.hidden"), "hidden file in Mixed")
        install("plain.txt")
        counts.refresh()
        wait(500)
        check(row("Mixed") !== null && row("Empty") !== null, "the folders are listed")
        counts.requestCount(root.base + "Mixed")
        counts.requestCount(root.base + "Empty")
        wait(300)
        check(row("Mixed").items === 2, "a folder and a file, the hidden file left out: " + row("Mixed").items)
        check(row("Empty").items === 0, "an empty folder has none: " + row("Empty").items)
        check(!row("Mixed").capped, "a small folder is counted whole")
    }

    function testNotFolders() {
        root.asking = false
        install("fresh.txt")
        counts.refresh()
        wait(500)
        var before = root.changes
        counts.requestCount(root.base + "fresh.txt")
        counts.requestCount(root.base + "nothing")
        counts.requestCount("lautta://user-documents/Mixed")
        counts.requestCount("garbage")
        wait(300)
        check(row("fresh.txt") !== null && row("fresh.txt").items === -1, "a file has no count")
        check(counts.count === 4, "nothing else appeared: " + counts.count)
        check(root.changes === before, "nothing was counted for them: " + (root.changes - before))
        root.asking = true
    }

    // The count follows the hidden-files setting of the view.
    function testHiddenFiles() {
        check(counts.setViewPrefs(JSON.stringify({ showHidden: true })), "hidden files shown")
        check(row("Mixed").items === -1, "the old count does not stand: " + row("Mixed").items)
        counts.requestCount(root.base + "Mixed")
        wait(300)
        check(row("Mixed").items === 3, "with the hidden file: " + row("Mixed").items)
        counts.setViewPrefs(JSON.stringify({ showHidden: false }))
        counts.requestCount(root.base + "Mixed")
        wait(300)
        check(row("Mixed").items === 2, "and without: " + row("Mixed").items)
    }

    // Changes inside a folder change its modified time: the cell asks again.
    function testChangedFolder() {
        check(tools.installFixture(Qt.resolvedUrl("fixtures/notes-crlf.txt"), root.base + "Mixed/b.txt"), "another file")
        counts.refresh()
        wait(600)
        check(row("Mixed").items === 3, "the folder was counted again: " + row("Mixed").items)
    }

    // A folder that did not change is not counted again.
    function testAskingAgain() {
        var before = root.changes
        for (var i = 0; i < 3; ++i)
            counts.requestCount(root.base + "Mixed")
        wait(300)
        check(root.changes === before, "asking again changed " + (root.changes - before) + " times")
        check(row("Mixed").items === 3, "the count stands: " + row("Mixed").items)
    }

    function testHugeFolder() {
        var many = root.base + "Many/"
        counts.createFolder("Many")
        wait(400)
        for (var i = 0; i <= 1000; ++i)
            check(tools.installFixture(Qt.resolvedUrl("fixtures/notes-crlf.txt"), many + i), "file " + i)
        counts.refresh()
        wait(600)
        counts.requestCount(root.base + "Many")
        wait(1500)
        check(row("Many").items === 1000 && row("Many").capped,
              "a huge folder gives a lower bound: " + row("Many").items + " " + row("Many").capped)
    }

    function testOtherFolder() {
        counts.uri = root.base + "Mixed/"
        wait(300)
        check(counts.folderCounts && counts.count === 3, "a folder opened later counts too: " + counts.count)
        check(row("Many") === null && row("sub") !== null, "the rows are those of the new folder")
        counts.requestCount(root.base + "Mixed/sub")
        wait(300)
        check(row("sub").items === 0, "and counts its own: " + row("sub").items)
    }

    // A count belongs to the folder it was made for: a folder of the same name
    // and the same modified time elsewhere, as rsync -t leaves them, is counted
    // for itself.
    function testSameStampElsewhere() {
        var when = 1700000000000
        counts.uri = root.base
        wait(300)
        counts.createFolder("One")
        counts.createFolder("Two")
        wait(400)
        mixed.uri = root.base + "One/"
        wait(300)
        mixed.createFolder("Same")
        wait(300)
        check(tools.installFixture(Qt.resolvedUrl("fixtures/notes-crlf.txt"), root.base + "One/Same/a.txt"), "file in One")
        mixed.uri = root.base + "Two/"
        wait(300)
        mixed.createFolder("Same")
        wait(300)
        for (var i = 0; i < 3; ++i)
            check(tools.installFixture(Qt.resolvedUrl("fixtures/notes-crlf.txt"), root.base + "Two/Same/" + i), "file in Two")
        check(tools.setModified(root.base + "One/Same", when) && tools.setModified(root.base + "Two/Same", when),
              "the same modified time")
        counts.uri = root.base + "One/"
        wait(300)
        counts.requestCount(root.base + "One/Same")
        wait(300)
        check(row("Same").items === 1, "the first folder: " + row("Same").items)
        counts.uri = root.base + "Two/"
        wait(300)
        check(row("Same").stamp === when, "with the same modified time: " + row("Same").stamp)
        counts.requestCount(root.base + "Two/Same")
        wait(300)
        check(row("Same").items === 3, "the second folder has its own count: " + row("Same").items)
    }

    // No folder, no counts.
    function testNoFolder() {
        counts.uri = ""
        wait(300)
        check(!counts.folderCounts && counts.count === 0, "nothing to count in no folder: " + counts.folderCounts)
    }

    Component.onCompleted: {
        counts.dataChanged.connect(function () { root.changes += 1 })
        testLocalFolders()
        testNotFolders()
        testHiddenFiles()
        testChangedFolder()
        testAskingAgain()
        testHugeFolder()
        testOtherFolder()
        testSameStampElsewhere()
        testNoFolder()
    }
}
