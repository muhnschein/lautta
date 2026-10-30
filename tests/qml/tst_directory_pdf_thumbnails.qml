// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components/directory"

ApplicationWindow {
    id: window

    readonly property string base: "lautta://user-documents/PdfThumbnails/"
    property string archiveRoot

    ViewerTools { id: tools }

    DirectoryModel {
        id: documents
        uri: "lautta://user-documents/"
    }

    Connections {
        target: Operations
        onJobFinished: if (ok) Operations.openArchive(resultUri)
        onArchiveOpened: window.archiveRoot = rootUri
    }

    function expect(ok, what) {
        if (!ok)
            console.error(what)
        return ok
    }

    function findAll(item, pred, out) {
        if (!item || out.indexOf(item) >= 0)
            return out
        if (pred(item))
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out)
        if (item.contentItem && item.contentItem !== item)
            findAll(item.contentItem, pred, out)
        return out
    }

    // No render loop headless: lay out by hand.
    function layOut(view) {
        var flicks = findAll(view, function (x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
        for (var i = 0; i < flicks.length; ++i)
            flicks[i].forceLayout()
    }

    function iconOf(view, category, name) {
        layOut(view)
        var icons = findAll(view, function (x) { return x.thumbnailFailed !== undefined && x.category === category }, [])
        var own = icons.filter(function (x) { return x.thumbnailSource.indexOf("/" + name) > 0 })
        return own.length === 1 ? own[0] : null
    }

    function systemThumbnailOf(icon) {
        return findAll(icon, function (x) { return x.priority !== undefined && x.mimeType !== undefined }, [])[0]
    }

    function until(done) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        return done()
    }

    function push(uri) {
        pageStack.push(pageComponent, { "folderUri": uri }, PageStackAction.Immediate)
        tools.pump(500)
        return pageStack.currentPage.view
    }

    function checkLocal(view) {
        until(function () { return iconOf(view, "pdf", "readable.pdf") && iconOf(view, "pdf", "secret.pdf") })
        var names = ["readable.pdf", "secret.pdf"]
        for (var i = 0; i < names.length; ++i) {
            var icon = iconOf(view, "pdf", names[i])
            if (!expect(icon, names[i] + " asks for a thumbnail"))
                continue
            expect(icon.thumbnailSource.indexOf("file://") === 0, "of the file itself: " + icon.thumbnailSource)
            var system = systemThumbnailOf(icon)
            if (!expect(system, names[i] + " asks the system thumbnailer"))
                continue
            expect(String(system.source) === icon.thumbnailSource && system.mimeType === "application/pdf",
                   "for that file, with the PDF type: " + system.source + " " + system.mimeType)
            expect(system.sourceSize.width > 0 && Math.abs(system.sourceSize.width - icon.width) < 1
                   && system.sourceSize.height === system.sourceSize.width,
                   "at the icon's size: " + system.sourceSize + ", icon " + icon.width)
        }
    }

    function checkArchive() {
        Operations.compress(JSON.stringify([base + "readable.pdf", base + "secret.pdf"]), "lautta://user-documents/",
                            "pdf-thumbnails", "zip")
        if (!expect(until(function () { return window.archiveRoot.length > 0 }), "the archive opens"))
            return
        var view = push(window.archiveRoot)
        until(function () { return iconOf(view, "pdf", "readable.pdf") && iconOf(view, "pdf", "secret.pdf") })
        var good = iconOf(view, "pdf", "readable.pdf")
        var locked = iconOf(view, "pdf", "secret.pdf")
        if (expect(good && locked, "both PDFs of the archive ask for a thumbnail of the app")) {
            expect(good.thumbnailSource.indexOf("image://lautta-thumb/") === 0, "from the thumbnail provider: " + good.thumbnailSource)
            expect(until(function () { return good.thumbnailReady }), "the first page of a readable PDF shows")
            expect(until(function () { return locked.thumbnailFailed }), "an encrypted PDF has no thumbnail")
        }
        pageStack.pop(null, PageStackAction.Immediate)
    }

    Component.onCompleted: {
        documents.createFolder("PdfThumbnails")
        tools.pump(500)
        tools.installFixture(Qt.resolvedUrl("host/fixtures/two-pages.pdf"), base + "readable.pdf")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/secret.pdf"), base + "secret.pdf")
        App.setSetting("view_mode", JSON.stringify("grid"))
        App.setSetting("thumbnails_remote", JSON.stringify(true))
        var view = push(base)
        expect(view.gridMode, "a grid")

        checkLocal(view)
        App.setSetting("view_mode", JSON.stringify("list"))
        expect(until(function () { return !view.gridMode }), "a list")
        checkLocal(view)
        App.setSetting("view_mode", JSON.stringify("grid"))
        expect(until(function () { return view.gridMode }), "the grid again")
        checkArchive()

        App.setSetting("thumbnails_local", JSON.stringify(false))
        tools.pump(300)
        expect(findAll(view, function (x) { return x.thumbnailFailed !== undefined && x.category === "pdf" && x.thumbnailSource.length > 0 }, []).length === 0,
               "without thumbnails no PDF asks for one")

        App.setSetting("thumbnails_local", JSON.stringify(true))
        App.setSetting("view_mode", JSON.stringify("list"))
    }

    Component {
        id: pageComponent

        Page {
            id: page

            property string folderUri
            property alias view: folder

            DirectoryView {
                id: folder

                anchors.fill: parent
                uri: page.folderUri
            }
        }
    }
}
