// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"
import "../../qml/components/directory/ContextActions.js" as Actions

ApplicationWindow {
    id: win

    readonly property string folder: "lautta://user-documents/"
    readonly property string kept: folder + "delete-keep.md"
    readonly property string trashed: folder + "delete-trash.md"
    property bool scenarioRunning: true
    property int step: 0
    property int waited: 0
    property int trashBefore: -1
    property int trashLoads: 0
    property int loadsAsked: -1
    property var row
    property var menu
    property var view
    property var dialog

    ViewerTools { id: tools }

    DirectoryModel {
        id: docs
        uri: win.folder
    }

    TrashModel {
        id: trash
        onChanged: win.trashLoads++
    }

    initialPage: Component {
        DirectoryPage {
            uri: win.folder
        }
    }

    function finish(what) {
        App.setSetting("recently_deleted", JSON.stringify(true))
        App.setSetting("remorse_seconds", JSON.stringify(5))
        if (what)
            console.error("FAILED: " + what)
        scenarioRunning = false
    }

    function fail(what) {
        finish(what)
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

    function rowOf(uri) {
        // No render loop headless: lay out by hand.
        var views = findAll(win.contentItem, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
        for (var i = 0; i < views.length; ++i)
            views[i].forceLayout()
        var rows = findAll(win.contentItem, function(x) {
            return x.openMenu !== undefined && x.info !== undefined && x.info.uri === uri
        }, [])
        return rows.length > 0 ? rows[0] : null
    }

    function tapDelete(uri) {
        if (!win.menu) {
            win.row = rowOf(uri)
            if (win.row)
                win.menu = win.row.openMenu()
            return false
        }
        if (!win.menu._expanded)
            return false
        var items = findAll(win.menu, function(x) {
            return x.hasOwnProperty("__silica_menuitem") && x.visible && x.text === Actions.text("delete")
        }, [])
        if (items.length !== 1)
            return false
        win.menu._activatedMenuItem(items[0])
        win.menu = null
        return true
    }

    function openDialog() {
        var page = pageStack.currentPage
        return page && page.uris !== undefined && page.status === PageStatus.Active ? page : null
    }

    function has(uri) {
        return docs.nameKind(App.nameOf(uri)) === "file"
    }

    function reloadTrash() {
        loadsAsked = trashLoads
        trash.reload()
    }

    function trashReloaded() {
        return trash.loaded && !trash.busy && trashLoads > loadsAsked
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
                if (docs.loading || win.waited < 3)
                    break
                if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), win.kept)
                        || !tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), win.trashed))
                    return win.fail("fixtures")
                App.setSetting("recently_deleted", JSON.stringify(false))
                App.setSetting("remorse_seconds", JSON.stringify(3))
                win.reloadTrash()
                win.next()
                break
            case 1:
                if (!win.trashReloaded() || !win.rowOf(win.kept))
                    break
                win.trashBefore = trash.count
                win.next()
                break
            case 2:
                if (win.tapDelete(win.kept))
                    win.next()
                break
            case 3:
                win.dialog = win.openDialog()
                if (!win.dialog)
                    break
                if (win.dialog.uris.length !== 1 || win.dialog.uris[0] !== win.kept)
                    return win.fail("the dialog names the item: " + JSON.stringify(win.dialog.uris))
                if (!win.dialog.canAccept)
                    return win.fail("the dialog can be accepted")
                win.dialog.reject()
                win.next()
                break
            case 4:
                if (win.waited < 40)
                    break
                if (win.openDialog())
                    return win.fail("the dialog went away")
                docs.refresh()
                win.next()
                break
            case 5:
                if (docs.loading || win.waited < 3)
                    break
                if (!win.has(win.kept))
                    return win.fail("a cancelled delete deleted the file")
                var views = findAll(win.contentItem, function(x) { return x.runSelectionAction !== undefined }, [])
                if (views.length !== 1)
                    return win.fail("one folder view: " + views.length)
                win.view = views[0]
                win.view.model.select(win.kept)
                win.view.runSelectionAction("delete")
                win.next()
                break
            case 6:
                win.dialog = win.openDialog()
                if (!win.dialog)
                    break
                if (win.dialog.uris.length !== 1 || win.dialog.uris[0] !== win.kept)
                    return win.fail("the selection's dialog names the item: " + JSON.stringify(win.dialog.uris))
                win.dialog.accept()
                win.next()
                break
            case 7:
                if (win.waited % 5 === 0)
                    docs.refresh()
                if (win.has(win.kept))
                    break
                if (win.waited > 25)
                    return win.fail("deleted only after a delay")
                win.reloadTrash()
                win.next()
                break
            case 8:
                if (!win.trashReloaded())
                    break
                if (trash.count !== win.trashBefore)
                    return win.fail("a permanent delete went to the Trash")
                if (win.view.selecting)
                    return win.fail("the delete ends selection mode")
                App.setSetting("recently_deleted", JSON.stringify(true))
                win.next()
                break
            case 9:
                if (win.tapDelete(win.trashed))
                    win.next()
                break
            case 10:
                if (win.openDialog())
                    return win.fail("a delete to the Trash asks nothing")
                if (win.waited < 10)
                    break
                if (!win.has(win.trashed))
                    return win.fail("the remorse timer comes first")
                win.next()
                break
            case 11:
                if (win.waited % 5 === 0)
                    docs.refresh()
                if (win.has(win.trashed))
                    break
                win.reloadTrash()
                win.next()
                break
            case 12:
                if (!win.trashReloaded())
                    break
                if (trash.count === win.trashBefore + 1)
                    return win.finish("")
                if (win.waited > 50)
                    return win.fail("the file is in the Trash: " + trash.count + " after " + win.trashBefore)
                win.reloadTrash()
                break
            }
        }
    }
}
