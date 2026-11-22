// SPDX-License-Identifier: LGPL-2.1-or-later
// Headless: no finger, no render loop; pulley and header looks unproven here.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"
import "../../qml/components/directory/ContextActions.js" as Actions

ApplicationWindow {
    id: window
    property bool reachedEnd

    readonly property string base: "lautta://user-documents/ArchiveExtract/"
    property string archiveRoot

    ViewerTools { id: tools }

    DirectoryModel { id: documents; uri: "lautta://user-documents/" }
    DirectoryModel { id: here }
    DirectoryModel { id: inside }

    Connections {
        target: Operations
        onJobFinished: if (ok) Operations.openArchive(resultUri, "")
        onArchiveOpened: window.archiveRoot = rootUri
        onFailed: console.error("FAILED: " + kind + " " + message)
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
        return expect(done(), "timed out waiting for " + what)
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

    function named(root, name) {
        var all = findAll(root, function (x) { return x.objectName === name }, [])
        return expect(all.length === 1, "one item called " + name + ": " + all.length) ? all[0] : null
    }

    function listed(model, name, kind) {
        var looks = 0
        model.refresh()
        return waitFor(function () {
            if (++looks % 40 === 0)
                model.refresh()
            return !model.loading && model.nameKind(name) === kind
        }, JSON.stringify(name) + " to be a '" + kind + "' in " + model.uri)
    }

    // Pulley entry `name` of `page`, shown or not; null when absent.
    function pulleyEntry(page, name) {
        var menus = findAll(page, function (x) { return x._isPullDownMenu === true }, [])
        if (!expect(menus.length === 1, "one pulley: " + menus.length))
            return null
        var menu = menus[0]
        menu.active = true
        var kids = menu._contentColumn.children
        var out = null
        for (var i = 0; i < kids.length; ++i) {
            if (kids[i].objectName === name)
                out = { "item": kids[i], "shown": kids[i].visible }
        }
        menu.active = false
        return out
    }

    function viewOf(page) {
        var views = findAll(page, function (x) { return x.runSelectionAction !== undefined }, [])
        return views.length === 1 ? views[0] : null
    }

    function open(uri) {
        var page = pageStack.push(pageComponent, { "uri": uri }, PageStackAction.Immediate)
        waitFor(function () {
            var view = viewOf(page)
            return view && !view.model.loading && view.model.count > 0
        }, "the folder " + uri)
        return page
    }

    function setUp() {
        documents.createFolder("ArchiveExtract")
        listed(documents, "ArchiveExtract", "folder")
        here.uri = base
        here.createFolder("Inner")
        listed(here, "Inner", "folder")
        expect(tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), base + "Inner/a.md"), "fixture a.md")
        Operations.compress(JSON.stringify([base + "Inner"]), base, "pack", "zip", "")
        waitFor(function () { return window.archiveRoot.length > 0 }, "the compressed file")
    }

    function checkPlainFolder() {
        var page = open(base)
        expect(named(page, "folderPageHeader").description === "", "no read-only line outside a compressed file")
        var extract = pulleyEntry(page, "folderMenuExtract")
        expect(extract && !extract.shown, "no Extract in the pulley outside a compressed file")
        var view = viewOf(page)
        view.moreActions({ "uri": base + "pack.zip", "isDir": false, "mimeType": "" })
        waitFor(function () { return pageStack.currentPage === view.moreActionsPage && !pageStack.busy }, "More actions")
        var ids = view.moreActionsPage.actions.map(function (a) { return a.id })
        expect(JSON.stringify(ids) === JSON.stringify(["favourite", "extract", "compress", "checksums", "moveToNewFolder"]),
               "a compressed file offers Extract: " + JSON.stringify(ids))
        pageStack.pop(page, PageStackAction.Immediate)
        view.moreActions({ "uri": base + "Inner", "isDir": true, "mimeType": "" })
        waitFor(function () { return pageStack.currentPage === view.moreActionsPage && !pageStack.busy }, "More actions")
        ids = view.moreActionsPage.actions.map(function (a) { return a.id })
        expect(ids.indexOf("extract") < 0, "a folder offers no Extract: " + JSON.stringify(ids))
        pageStack.pop(page, PageStackAction.Immediate)
    }

    function checkArchive() {
        var page = open(window.archiveRoot)
        var header = named(page, "folderPageHeader")
        expect(header.description === qsTrId("lautta-dir-compressed-readonly"),
               "the read-only line: " + header.description)
        var sub = open(viewOf(page).model.uriAt(0))
        expect(named(sub, "folderPageHeader").description === header.description, "the line shows in its folders too")
        var extract = pulleyEntry(sub, "folderMenuExtract")
        if (!expect(extract && extract.shown, "Extract in the pulley inside a compressed file"))
            return
        expect(extract.item.text === Actions.text("extract"), "the entry says Extract: " + extract.item.text)
        extract.item.clicked()
        listed(here, "Inner 2", "folder")
        inside.uri = base + "Inner 2/"
        listed(inside, "a.md", "file")
        expect(pageStack.currentPage === sub, "Extract opens no page")
    }

    Component.onCompleted: {
        setUp()
        checkPlainFolder()
        checkArchive()
        reachedEnd = true
    }
}
