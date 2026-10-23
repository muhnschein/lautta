// SPDX-License-Identifier: LGPL-2.1-or-later
// Scenario on the target (real Silica): a file deleted from Documents is in
// the Trash page (OPS-8), titled Trash, with the note that deletes on
// volumes and servers are permanent; Restore from its menu puts the file
// back in Documents.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

ApplicationWindow {
    id: win

    readonly property string folder: "lautta://user-documents/"
    readonly property string file: folder + "trash-page.md"
    property bool scenarioRunning: true
    property int step: 0
    property int waited: 0
    property bool removed
    property var page
    property var menu

    ViewerTools { id: tools }

    DirectoryModel {
        id: docs
        uri: win.folder
    }

    Connections {
        target: Operations
        onRemoved: win.removed = true
        onFailed: win.fail("the delete failed: " + kind)
    }

    initialPage: Component {
        Page { }
    }

    function fail(what) {
        App.setSetting("recently_deleted", JSON.stringify(true))
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

    function either(text, english, id) {
        return text === english || text === id
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
                if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), win.file))
                    return win.fail("fixture")
                if (Operations.deletesPermanently(JSON.stringify([win.file])))
                    return win.fail("Documents deletes to the Trash")
                Operations.remove(JSON.stringify([win.file]))
                win.next()
                break
            case 1:
                if (!win.removed)
                    break
                win.page = pageStack.push(Qt.resolvedUrl("../../qml/pages/TrashPage.qml"))
                win.next()
                break
            case 2:
                var views = findAll(win.page, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
                if (views.length !== 1 || !views[0].model.loaded || views[0].model.busy)
                    break
                views[0].forceLayout()
                var header = findAll(win.page, function(x) { return x.title !== undefined && x.description !== undefined && x._titleItem !== undefined }, [])
                if (header.length !== 1 || !either(header[0].title, "Trash", "lautta-trash-title"))
                    return win.fail("titled Trash: " + (header.length ? header[0].title : "no header"))
                var note = findAll(win.page, function(x) {
                    return x.wrapMode !== undefined && x.visible && either(x.text,
                        "Only files deleted from this device's folders appear here. Deletes on volumes and servers are permanent.",
                        "lautta-trash-footer")
                }, [])
                if (note.length !== 1)
                    return win.fail("the note says volumes and servers")
                var names = findAll(win.page, function(x) { return x.text === "trash-page.md" && x.truncationMode !== undefined }, [])
                if (names.length !== 1)
                    break
                var row = names[0]
                while (row && row.openMenu === undefined)
                    row = row.parent
                if (!row)
                    return win.fail("the item's row")
                win.menu = row.openMenu()
                win.next()
                break
            case 3:
                if (!win.menu._expanded)
                    break
                var items = findAll(win.menu, function(x) {
                    return x.hasOwnProperty("__silica_menuitem") && either(x.text, "Restore", "lautta-trash-restore")
                }, [])
                if (items.length !== 1)
                    return win.fail("one Restore")
                win.menu._activatedMenuItem(items[0])
                win.next()
                break
            case 4:
                if (win.waited % 5 === 0)
                    docs.refresh()
                if (docs.nameKind("trash-page.md") === "file")
                    win.next()
                break
            case 5:
                // Leave Documents as it was for the scenarios after this one.
                App.setSetting("recently_deleted", JSON.stringify(false))
                win.removed = false
                Operations.remove(JSON.stringify([win.file]))
                win.next()
                break
            case 6:
                if (!win.removed)
                    break
                App.setSetting("recently_deleted", JSON.stringify(true))
                win.next()
                break
            case 7:
                if (win.waited % 5 === 0)
                    docs.refresh()
                if (docs.nameKind("trash-page.md") === "")
                    win.scenarioRunning = false
                break
            }
        }
    }
}
