// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    readonly property string base: "lautta://user-downloads/Counts/"

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    // Bounded: a test of "nothing happened" waits the whole time.
    function wait(ms) { tools.pump(ms) }

    function until(cond, what) {
        for (var i = 0; i < 400 && !cond(); ++i)
            tools.pump(25)
        check(cond(), "timed out waiting for " + what)
    }

    function listed(model, n) {
        return function () { return !model.loading && model.count === n }
    }

    function counted(name, n) {
        return function () { return row(name) !== null && row(name).items === n }
    }

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
    property bool asking: true

    DirectoryModel { id: downloads; uri: "lautta://user-downloads/" }
    DirectoryModel { id: counts }
    DirectoryModel { id: mixed }

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
        until(function () { return !downloads.loading }, "Downloads listed")
        downloads.createFolder("Counts")
        until(function () { return downloads.indexOf(root.base.slice(0, -1)) >= 0 }, "Counts made")
        counts.uri = root.base
        check(counts.folderCounts && downloads.folderCounts, "device folders have counts")
        until(listed(counts, 0), "Counts listed")
        counts.createFolder("Empty")
        counts.createFolder("Mixed")
        until(listed(counts, 2), "Empty and Mixed made")
        mixed.uri = root.base + "Mixed/"
        until(listed(mixed, 0), "Mixed listed")
        mixed.createFolder("sub")
        until(listed(mixed, 1), "sub made")
        check(tools.installFixture(Qt.resolvedUrl("fixtures/notes-crlf.txt"), root.base + "Mixed/a.txt"), "file in Mixed")
        check(tools.installFixture(Qt.resolvedUrl("fixtures/notes-crlf.txt"), root.base + "Mixed/.hidden"), "hidden file in Mixed")
        install("plain.txt")
        counts.refresh()
        until(listed(counts, 3), "plain.txt listed")
        check(row("Mixed") !== null && row("Empty") !== null, "the folders are listed")
        counts.requestCount(root.base + "Mixed")
        counts.requestCount(root.base + "Empty")
        until(function () { return counted("Mixed", 2)() && counted("Empty", 0)() }, "Mixed and Empty counted")
        check(row("Mixed").items === 2, "a folder and a file, the hidden file left out: " + row("Mixed").items)
        check(row("Empty").items === 0, "an empty folder has none: " + row("Empty").items)
        check(!row("Mixed").capped, "a small folder is counted whole")
    }

    function testNotFolders() {
        root.asking = false
        install("fresh.txt")
        counts.refresh()
        until(listed(counts, 4), "fresh.txt listed")
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

    function testHiddenFiles() {
        check(counts.setViewPrefs(JSON.stringify({ showHidden: true })), "hidden files shown")
        check(row("Mixed").items === -1, "the old count does not stand: " + row("Mixed").items)
        counts.requestCount(root.base + "Mixed")
        until(counted("Mixed", 3), "Mixed counted with hidden files")
        check(row("Mixed").items === 3, "with the hidden file: " + row("Mixed").items)
        counts.setViewPrefs(JSON.stringify({ showHidden: false }))
        counts.requestCount(root.base + "Mixed")
        until(counted("Mixed", 2), "Mixed counted without hidden files")
        check(row("Mixed").items === 2, "and without: " + row("Mixed").items)
    }

    function testChangedFolder() {
        check(tools.installFixture(Qt.resolvedUrl("fixtures/notes-crlf.txt"), root.base + "Mixed/b.txt"), "another file")
        counts.refresh()
        until(counted("Mixed", 3), "Mixed counted again")
        check(row("Mixed").items === 3, "the folder was counted again: " + row("Mixed").items)
    }

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
        until(listed(counts, 5), "Many made")
        for (var i = 0; i <= 1000; ++i)
            check(tools.installFixture(Qt.resolvedUrl("fixtures/notes-crlf.txt"), many + i), "file " + i)
        counts.refresh()
        until(function () { return !counts.loading }, "Many listed again")
        counts.requestCount(root.base + "Many")
        until(counted("Many", 1000), "Many counted")
        check(row("Many").items === 1000 && row("Many").capped,
              "a huge folder gives a lower bound: " + row("Many").items + " " + row("Many").capped)
    }

    function testOtherFolder() {
        counts.uri = root.base + "Mixed/"
        until(listed(counts, 3), "Mixed opened")
        check(counts.folderCounts && counts.count === 3, "a folder opened later counts too: " + counts.count)
        check(row("Many") === null && row("sub") !== null, "the rows are those of the new folder")
        counts.requestCount(root.base + "Mixed/sub")
        until(counted("sub", 0), "sub counted")
        check(row("sub").items === 0, "and counts its own: " + row("sub").items)
    }

    function testSameStampElsewhere() {
        var when = 1700000000000
        counts.uri = root.base
        until(listed(counts, 5), "Counts opened again")
        counts.createFolder("One")
        counts.createFolder("Two")
        until(listed(counts, 7), "One and Two made")
        mixed.uri = root.base + "One/"
        until(listed(mixed, 0), "One listed")
        mixed.createFolder("Same")
        until(listed(mixed, 1), "One/Same made")
        check(tools.installFixture(Qt.resolvedUrl("fixtures/notes-crlf.txt"), root.base + "One/Same/a.txt"), "file in One")
        mixed.uri = root.base + "Two/"
        until(listed(mixed, 0), "Two listed")
        mixed.createFolder("Same")
        until(listed(mixed, 1), "Two/Same made")
        for (var i = 0; i < 3; ++i)
            check(tools.installFixture(Qt.resolvedUrl("fixtures/notes-crlf.txt"), root.base + "Two/Same/" + i), "file in Two")
        check(tools.setModified(root.base + "One/Same", when) && tools.setModified(root.base + "Two/Same", when),
              "the same modified time")
        counts.uri = root.base + "One/"
        until(listed(counts, 1), "One opened")
        counts.requestCount(root.base + "One/Same")
        until(counted("Same", 1), "One/Same counted")
        check(row("Same").items === 1, "the first folder: " + row("Same").items)
        counts.uri = root.base + "Two/"
        until(function () { return listed(counts, 1)() && row("Same") !== null && row("Same").stamp === when }, "Two opened")
        check(row("Same").stamp === when, "with the same modified time: " + row("Same").stamp)
        counts.requestCount(root.base + "Two/Same")
        until(counted("Same", 3), "Two/Same counted")
        check(row("Same").items === 3, "the second folder has its own count: " + row("Same").items)
    }

    function testNoFolder() {
        counts.uri = ""
        until(function () { return counts.count === 0 }, "no folder")
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
