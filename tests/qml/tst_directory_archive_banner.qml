// SPDX-License-Identifier: LGPL-2.1-or-later
// Headless: no finger, no render loop; banner looks and tap feel unproven here.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: window

    readonly property string base: "lautta://user-documents/ArchiveBanner/"
    property string archiveRoot
    property string extractedTo

    ViewerTools { id: tools }

    DirectoryModel { id: documents; uri: "lautta://user-documents/" }
    DirectoryModel { id: here }
    DirectoryModel { id: inside }

    Connections {
        target: Operations
        onJobFinished: if (ok) Operations.openArchive(resultUri)
        onArchiveOpened: window.archiveRoot = rootUri
        onExtractFinished: if (ok) window.extractedTo = destUri
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

    function isInside(ancestor, item) {
        for (var p = item; p; p = p.parent) {
            if (p === ancestor)
                return true
        }
        return false
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

    function open(uri) {
        var page = pageStack.push(pageComponent, { "uri": uri }, PageStackAction.Immediate)
        waitFor(function () {
            var views = findAll(page, function (x) { return x.runSelectionAction !== undefined }, [])
            return views.length === 1 && !views[0].model.loading && views[0].model.count > 0
        }, "the folder " + uri)
        return page
    }

    function setUp() {
        documents.createFolder("ArchiveBanner")
        listed(documents, "ArchiveBanner", "folder")
        here.uri = base
        here.createFolder("Inner")
        listed(here, "Inner", "folder")
        expect(tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), base + "Inner/a.md"), "fixture a.md")
        Operations.compress(JSON.stringify([base + "Inner"]), base, "pack", "zip")
        waitFor(function () { return window.archiveRoot.length > 0 }, "the archive")
    }

    function checkPlainFolder() {
        var page = open(base)
        var banner = named(page, "folderArchiveBanner")
        expect(banner && !banner.visible && banner.height === 0, "no banner outside an archive")
    }

    function checkArchive() {
        var page = open(window.archiveRoot)
        var banner = named(page, "folderArchiveBanner")
        if (!expect(banner && banner.visible && banner.height > 0, "the banner shows in an archive"))
            return
        expect(isInside(named(page, "folderHeader"), banner), "the banner sits in the folder header")
        var pathMenu = named(page, "folderPathMenu")
        expect(banner.y >= pathMenu.y + pathMenu.height, "the banner sits under the header: " + banner.y)

        var sub = open(findAll(page, function (x) { return x.runSelectionAction !== undefined }, [])[0].model.uriAt(0))
        var subBanner = named(sub, "folderArchiveBanner")
        expect(subBanner && subBanner.visible, "the banner shows in a folder inside the archive")

        subBanner.clicked(null)
        var dialog
        waitFor(function () {
            dialog = pageStack.currentPage
            return dialog && dialog.archiveUri !== undefined && !pageStack.busy
        }, "the extract dialog")
        expect(dialog.archiveUri === base + "pack.zip", "the dialog extracts the archive: " + dialog.archiveUri)
        waitFor(function () { return dialog.canAccept }, "the archive inspected")
        dialog.accept()
        waitFor(function () { return window.extractedTo.length > 0 }, "the extract")
        expect(window.extractedTo === base + "pack", "extracts next to the archive: " + window.extractedTo)
        inside.uri = base + "pack/Inner/"
        listed(inside, "a.md", "file")
    }

    Component.onCompleted: {
        setUp()
        checkPlainFolder()
        checkArchive()
    }
}
