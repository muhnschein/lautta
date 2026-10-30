// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: win

    readonly property string base: "lautta://user-documents/RenameRemorse/"
    readonly property string before: base + "before.md"
    readonly property string after: base + "after.md"
    readonly property string sub: base + "Sub"
    readonly property string kept: base + "Kept"
    property bool scenarioRunning: true
    property int step: 0
    property int waited: 0
    property int made: 0
    property var view
    property var remorse
    property int fullTimeout
    property var remorseBefore

    ViewerTools { id: tools }

    DirectoryModel {
        id: documents

        uri: "lautta://user-documents/"
        onCreated: win.made++
        onCreateFailed: win.made++
    }

    DirectoryModel {
        id: folder

        uri: win.made > 0 ? win.base : ""
        onCreated: win.made++
        onCreateFailed: win.made++
    }

    Component {
        id: pageComponent

        DirectoryPage {
            uri: win.base
        }
    }

    function fail(what) {
        console.error("FAILED: " + what)
        finish()
    }

    function finish() {
        App.setSetting("remorse_seconds", JSON.stringify(remorseBefore))
        scenarioRunning = false
    }

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

    // No render loop headless: lay out by hand.
    function layOut() {
        var views = findAll(win.contentItem, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
        for (var i = 0; i < views.length; ++i)
            views[i].forceLayout()
    }

    function rowsIn(shown) {
        var out = []
        var kids = shown ? shown.contentItem.children : []
        for (var i = 0; i < kids.length; ++i) {
            var cells = kids[i].regroup !== undefined ? kids[i].children : [kids[i]]
            for (var j = 0; j < cells.length; ++j)
                out.push(cells[j])
        }
        return out
    }

    function rowOf(uri) {
        var name = App.nameOf(uri)
        var rows = rowsIn(shownView())
        for (var i = 0; i < rows.length; ++i) {
            var x = rows[i]
            if (x.info !== undefined ? x.info.uri === uri : x.itemName === name)
                return x
        }
        return null
    }

    function activeRemorse(row) {
        var kids = row.data
        for (var i = 0; i < kids.length; ++i) {
            if (kids[i]._countdown !== undefined && kids[i].state === "active")
                return kids[i]
        }
        return null
    }

    function shownView() {
        layOut()
        var views = findAll(win.view, function(x) { return x.cacheBuffer !== undefined && x.visible }, [])
        return views.length === 1 ? views[0] : null
    }

    function onFolder() {
        return !pageStack.busy && pageStack.currentPage && pageStack.currentPage.uri === win.base
               && pageStack.currentPage.status === PageStatus.Active
    }

    function startRename(uri) {
        var row = rowOf(uri)
        if (!row || !onFolder())
            return false
        win.view.rename(row.info)
        return true
    }

    function acceptName(name) {
        var dialog = pageStack.currentPage
        if (!dialog || dialog.newName === undefined || pageStack.busy)
            return false
        var field = findAll(dialog, function(x) { return x.errorHighlight !== undefined && x.visible }, [])
        if (field.length !== 1)
            return false
        field[0].text = name
        if (!dialog.canAccept)
            return false
        dialog.accept()
        return true
    }

    function remorseOn(uri, ids) {
        var row = rowOf(uri)
        var r = row ? activeRemorse(row) : null
        if (!r)
            return null
        if (ids.indexOf(r.text) < 0) {
            fail("the remorse tells " + JSON.stringify(ids) + ", not " + r.text)
            return null
        }
        if (r.parent !== row) {
            fail("the remorse is in the renamed row")
            return null
        }
        return r
    }

    function anyRemorse() {
        var rows = rowsIn(shownView())
        for (var i = 0; i < rows.length; ++i) {
            if (rows[i].data && activeRemorse(rows[i]))
                return true
        }
        return false
    }

    function setViewMode(mode) {
        win.view.model.setViewPrefs(JSON.stringify({ "viewMode": mode }))
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
                win.remorseBefore = App.setting("remorse_seconds")
                App.setSetting("remorse_seconds", JSON.stringify(10))
                documents.createFolder("RenameRemorse")
                win.next()
                break
            case 1:
                if (win.made < 1)
                    break
                if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), win.before))
                    return win.fail("fixture")
                folder.createFolder("Sub")
                win.next()
                break
            case 2:
                if (win.made < 2)
                    break
                pageStack.push(pageComponent, {}, PageStackAction.Immediate)
                win.next()
                break
            case 3:
                var views = findAll(win.contentItem, function(x) { return x.renameDone !== undefined && x.runAction !== undefined }, [])
                if (views.length !== 1)
                    break
                win.view = views[0]
                if (win.startRename(win.before))
                    win.next()
                break
            case 4:
                if (win.acceptName("after.md"))
                    win.next()
                break
            case 5:
                if (!win.onFolder())
                    break
                win.remorse = win.remorseOn(win.after, ["File renamed", "lautta-dir-renamed-file"])
                if (!win.remorse)
                    break
                if (win.rowOf(win.before))
                    return win.fail("the old name is gone")
                win.fullTimeout = win.remorse._timeout
                win.setViewMode("grid")
                win.next()
                break
            case 6:
                if (!win.view.gridMode)
                    break
                var cellRemorse = win.remorseOn(win.after, ["File renamed", "lautta-dir-renamed-file"])
                if (!cellRemorse)
                    break
                win.setViewMode("list")
                win.next()
                break
            case 7:
                if (win.view.gridMode)
                    break
                var listRemorse = win.remorseOn(win.after, ["File renamed", "lautta-dir-renamed-file"])
                if (!listRemorse)
                    break
                if (listRemorse._timeout >= win.fullTimeout)
                    return win.fail("a new row shows the rest of the remorse: " + listRemorse._timeout + " of " + win.fullTimeout)
                listRemorse.cancel()
                win.next()
                break
            case 8:
                if (!win.rowOf(win.before) || win.rowOf(win.after))
                    break
                if (win.view.renamed !== null)
                    return win.fail("the undone rename is forgotten")
                if (win.waited < 15)
                    break
                if (win.anyRemorse())
                    return win.fail("renaming back shows no remorse")
                App.setSetting("remorse_seconds", JSON.stringify(3))
                if (win.startRename(win.sub))
                    win.next()
                break
            case 9:
                if (win.acceptName("Kept"))
                    win.next()
                break
            case 10:
                if (!win.onFolder())
                    break
                if (win.remorseOn(win.kept, ["Folder renamed", "lautta-dir-renamed-folder"]))
                    win.next()
                break
            case 11:
                if (win.anyRemorse() || win.view.renamed !== null)
                    break
                if (!win.rowOf(win.kept) || win.rowOf(win.sub))
                    return win.fail("the folder keeps its new name")
                win.next()
                break
            case 12:
                if (win.waited < 10)
                    break
                if (win.anyRemorse() || !win.rowOf(win.kept))
                    return win.fail("the remorse comes back after running out")
                win.finish()
                break
            }
        }
    }
}
