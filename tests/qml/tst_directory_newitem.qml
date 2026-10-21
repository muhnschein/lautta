// SPDX-License-Identifier: LGPL-2.1-or-later
// Scenario on the target (real Silica): the folder pulley's New folder opens
// NewItemDialog titled New folder, with the name field and no type to pick,
// and accepting it makes a folder. Fails when the dialog offers a type, takes
// the bin's name at the root or makes something other than a folder.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: win

    readonly property string folder: "lautta://user-documents/"
    property bool scenarioRunning: true
    property int step: 0
    property int waited: 0
    property var dialog

    DirectoryModel {
        id: docs
        uri: win.folder
    }

    initialPage: Component {
        DirectoryPage {
            uri: win.folder
        }
    }

    function fail(what) {
        console.error("FAILED: " + what)
        scenarioRunning = false
    }

    // A flickable's content item is one of its children too: each item once.
    function findAll(item, pred, out, seen) {
        seen = seen || []
        if (!item || seen.indexOf(item) >= 0)
            return out
        seen.push(item)
        if (pred(item))
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out, seen)
        if (item.contentItem)
            findAll(item.contentItem, pred, out, seen)
        return out
    }

    function next() {
        step++
        waited = 0
    }

    Timer {
        interval: 100
        repeat: true
        running: win.scenarioRunning
        onTriggered: {
            win.waited++
            if (win.waited > 100)
                return win.fail("step " + win.step + " timed out")
            switch (win.step) {
            case 0:
                var views = findAll(win.contentItem, function(x) { return x.newFolder !== undefined }, [])
                if (views.length === 0)
                    break
                if (views[0].newItem !== undefined)
                    return win.fail("the folder view still makes items other than folders")
                views[0].newFolder()
                win.next()
                break
            case 1:
                win.dialog = pageStack.currentPage
                if (!win.dialog || win.dialog.itemName === undefined || win.waited < 5)
                    break
                var header = findAll(win.dialog, function(x) { return x.defaultAcceptText !== undefined }, [])
                var title = header.length === 1 ? header[0].title : ""
                if (title !== "New folder" && title !== "lautta-dir-new-folder-title")
                    return win.fail("titled New folder: " + title)
                var combos = findAll(win.dialog, function(x) { return x.currentIndex !== undefined && x.menu !== undefined }, [])
                if (combos.length > 0)
                    return win.fail("a type to pick")
                if (win.dialog.isFolder !== undefined || win.dialog.foldersOnly !== undefined)
                    return win.fail("folder or file is still a choice")
                var fields = findAll(win.dialog, function(x) { return x.errorHighlight !== undefined && x.visible }, [])
                if (fields.length !== 1)
                    return win.fail("one name field: " + fields.length)
                // The bin's name, at the location's root (OPS-8).
                fields[0].text = ".lautta-trash"
                if (win.dialog.canAccept || !fields[0].errorHighlight)
                    return win.fail("the bin's name is refused at the root")
                fields[0].text = "Plans"
                win.next()
                break
            case 2:
                if (!win.dialog.canAccept)
                    break
                win.dialog.accept()
                win.next()
                break
            case 3:
                if (win.waited % 5 === 0)
                    docs.refresh()
                if (docs.nameKind("Plans") === "folder")
                    win.scenarioRunning = false
                else if (docs.nameKind("Plans") === "file")
                    win.fail("made a file")
                break
            }
        }
    }
}
