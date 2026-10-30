// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"
import "../../qml/components/directory/ContextActions.js" as Actions

ApplicationWindow {
    id: window

    readonly property string base: "lautta://user-documents/MoveNew/"
    property string archiveRoot
    property var folderPage

    ViewerTools { id: tools }

    DirectoryModel { id: documents; uri: "lautta://user-documents/" }
    DirectoryModel { id: here }
    DirectoryModel { id: inside }

    Connections {
        target: Operations
        onJobFinished: if (ok) Operations.openArchive(resultUri)
        onArchiveOpened: window.archiveRoot = rootUri
    }

    Component {
        id: pageComponent

        DirectoryPage { }
    }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        return expect(done(), "timed out waiting for " + (typeof what === "function" ? what() : what))
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

    function names(model) {
        var out = []
        for (var i = 0; i < model.count; ++i)
            out.push(App.nameOf(model.uriAt(i)))
        return JSON.stringify(out)
    }

    function listed(model, name, kind) {
        var looks = 0
        model.refresh()
        return waitFor(function () {
            if (++looks % 40 === 0)
                model.refresh()
            return !model.loading && model.nameKind(name) === kind
        }, function () {
            return JSON.stringify(name) + " to be a '" + kind + "' in " + model.uri + ", it lists " + names(model)
        })
    }

    function folderView() {
        var views = findAll(pageStack.currentPage, function (x) { return x.runSelectionAction !== undefined }, [])
        return views.length === 1 ? views[0] : null
    }

    // No render loop headless: lay out by hand.
    function layOut() {
        var views = findAll(window.contentItem, function (x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
        for (var i = 0; i < views.length; ++i)
            views[i].forceLayout()
    }

    function rowOf(uri) {
        layOut()
        var rows = findAll(window.contentItem, function (x) {
            return x.openMenu !== undefined && x.info !== undefined && x.info.uri === uri
        }, [])
        return rows.length > 0 ? rows[0] : null
    }

    function moreIds(view) {
        waitFor(function () { return view.moreActionsPage && pageStack.currentPage === view.moreActionsPage && !pageStack.busy },
                "the More actions page")
        return view.moreActionsPage.actions.map(function (a) { return a.id })
    }

    function tapMore(view, id) {
        findAll(view.moreActionsPage, function (x) { return x.positionViewAtEnd !== undefined }, []).forEach(function (v) {
            v.positionViewAtEnd()
        })
        layOut()
        var rows = findAll(view.moreActionsPage, function (x) {
            return x.objectName === "moreRow" && findAll(x, function (y) {
                return y.truncationMode !== undefined && y.text === Actions.text(id)
            }, []).length > 0
        }, [])
        if (expect(rows.length === 1, "one row for " + id + ": " + rows.length))
            rows[0].clicked(null)
    }

    function contextMore(uri) {
        var menu
        waitFor(function () {
            var row = rowOf(uri)
            menu = row ? row.openMenu() : null
            return menu
        }, "the context menu")
        waitFor(function () { return menu._expanded }, "the menu opened")
        var items = findAll(menu, function (x) {
            return x.hasOwnProperty("__silica_menuitem") && x.visible && x.text === Actions.text("more")
        }, [])
        if (expect(items.length === 1, "one More actions entry: " + items.length))
            menu._activatedMenuItem(items[0])
    }

    function nameAndAccept(name) {
        var dialog
        waitFor(function () {
            dialog = pageStack.currentPage
            return dialog && dialog.itemName !== undefined && !pageStack.busy
        }, "the name dialog")
        var header = findAll(dialog, function (x) { return x.defaultAcceptText !== undefined }, [])
        expect(header.length === 1 && (header[0].title === "lautta-dir-new-folder-title" || header[0].title === "New folder"),
               "it is the dialog of New folder: " + (header.length ? header[0].title : "no header"))
        expect(dialog.parentUri === base, "which makes the folder here: " + dialog.parentUri)
        var field = findAll(dialog, function (x) { return x.errorHighlight !== undefined && x.visible }, [])
        if (!expect(field.length === 1, "one name field: " + field.length))
            return
        field[0].text = name
        if (waitFor(function () { return dialog.canAccept }, "a name to accept"))
            dialog.accept()
        waitFor(function () { return pageStack.currentPage === window.folderPage && !pageStack.busy }, "the folder again")
    }

    function setUp() {
        documents.createFolder("MoveNew")
        listed(documents, "MoveNew", "folder")
        var names = ["a.md", "b.md", "c.md", "d.md"]
        for (var i = 0; i < names.length; ++i)
            expect(tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), base + names[i]), "fixture " + names[i])
        here.uri = base
        waitFor(function () { return !here.loading && here.count === 4 }, "the folder")
        folderPage = pageStack.push(pageComponent, { "uri": base }, PageStackAction.Immediate)
        waitFor(function () {
            var view = folderView()
            return view && !view.model.loading && view.model.writable
        }, "the folder open")
    }

    function checkSelection() {
        var view = folderView()
        view.startSelection()
        view.model.select(base + "a.md")
        view.model.select(base + "b.md")
        view.moreSelection()
        var ids = moreIds(view)
        expect(ids.indexOf("moveToNewFolder") === ids.length - 1 && ids.indexOf("compress") >= 0,
               "the page offers Move to new folder after Compress: " + JSON.stringify(ids))
        tapMore(view, "moveToNewFolder")
        nameAndAccept("Pair")
        expect(!view.selecting, "selection ends")
        listed(here, "Pair", "folder")
        inside.uri = base + "Pair/"
        listed(inside, "a.md", "file")
        listed(inside, "b.md", "file")
        listed(here, "a.md", "")
        listed(here, "b.md", "")
        expect(here.nameKind("c.md") === "file" && here.nameKind("d.md") === "file", "the rest stays")
    }

    function checkItem() {
        var view = folderView()
        contextMore(base + "c.md")
        var ids = moreIds(view)
        expect(ids.indexOf("moveToNewFolder") === ids.length - 1 && ids.indexOf("favourite") === 0,
               "the page offers Move to new folder after Compress: " + JSON.stringify(ids))
        tapMore(view, "moveToNewFolder")
        nameAndAccept("Solo")
        listed(here, "Solo", "folder")
        inside.uri = base + "Solo/"
        listed(inside, "c.md", "file")
        listed(here, "c.md", "")
        expect(here.nameKind("d.md") === "file", "the rest stays")
    }

    function checkFailure() {
        var view = folderView()
        view.toMove = [base + "d.md"]
        view.model.createFolder("Pair")
        waitFor(function () { return view.toMove.length === 0 }, "the failure")
        listed(here, "d.md", "file")
        inside.uri = base + "Pair/"
        listed(inside, "d.md", "")
    }

    function checkArchive() {
        Operations.compress(JSON.stringify([base + "Pair"]), base, "pairs", "zip")
        waitFor(function () { return window.archiveRoot.length > 0 }, "the archive")
        var archivePage = pageStack.push(pageComponent, { "uri": window.archiveRoot }, PageStackAction.Immediate)
        var view
        waitFor(function () {
            view = folderView()
            return view && view.uri === window.archiveRoot && !view.model.loading && view.model.count > 0
        }, "the archive open")
        expect(!view.model.writable, "an archive is read-only")
        view.startSelection()
        view.model.selectAll()
        view.moreSelection()
        var ids = moreIds(view)
        expect(ids.length > 0 && ids.indexOf("moveToNewFolder") < 0, "no row for a selection in an archive: " + JSON.stringify(ids))
        pageStack.pop(archivePage, PageStackAction.Immediate)
        view.endSelection()
        view.moreActions({ "uri": view.model.uriAt(0), "isDir": true })
        ids = moreIds(view)
        expect(ids.length > 0 && ids.indexOf("moveToNewFolder") < 0, "no row for an item in an archive: " + JSON.stringify(ids))
        pageStack.pop(folderPage, PageStackAction.Immediate)
    }

    Component.onCompleted: {
        setUp()
        checkSelection()
        checkItem()
        checkFailure()
        checkArchive()
    }
}
