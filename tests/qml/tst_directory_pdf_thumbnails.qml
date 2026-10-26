// SPDX-License-Identifier: LGPL-2.1-or-later
// PDF thumbnails on the target (PRV-1, PRV-14). In a folder of this device a
// PDF asks the system thumbnailer for its first page at the icon's size,
// with the PDF type, without which it draws no PDF, in the grid and the list. In an archive (it stands
// in for a server: neither is on this device) the app draws the first page;
// the name stays off a compact grid's tile only while a thumbnail covers the
// tile, and an encrypted PDF keeps its icon and gets its name back. With
// Show thumbnails off no PDF has one. Fails when a local PDF asks without its
// type, a readable archive PDF shows no thumbnail, an unreadable one shows no
// name, or the switch changes nothing. Here the system thumbnailer draws
// nothing (no scene graph: a `Thumbnail` stays `Null`, a JPEG's too): only a
// phone shows a local PDF's page, or its icon when the page cannot be drawn.
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

    // Items are reached both as children and as a content item: each once.
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

    // Headless, no render loop polishes the view: lays out the delegates.
    function layOut(view) {
        var flicks = findAll(view, function (x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
        for (var i = 0; i < flicks.length; ++i)
            flicks[i].forceLayout()
    }

    // The icon of the row `name`: the one whose thumbnail is that file's, or
    // for a row without one, the only icon of its category left over.
    function iconOf(view, category, name) {
        layOut(view)
        var icons = findAll(view, function (x) { return x.thumbnailFailed !== undefined && x.category === category }, [])
        var own = icons.filter(function (x) { return x.thumbnailSource.indexOf("/" + name) > 0 })
        return own.length === 1 ? own[0] : null
    }

    // The system thumbnail inside `icon`, undefined while it has none.
    function systemThumbnailOf(icon) {
        return findAll(icon, function (x) { return x.priority !== undefined && x.mimeType !== undefined }, [])[0]
    }

    // The name shown on the tile of `name`, null when the tile has none.
    function nameOn(view, name) {
        layOut(view)
        var labels = findAll(view, function (x) { return x.text === name && x.truncationMode !== undefined }, [])
        return labels.length > 0 ? labels[0] : null
    }

    // Lets the event loop run until `done()` holds, for at most 10 s.
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

    // Each local PDF asks the system thumbnailer for its own file, as a PDF,
    // at the icon's size.
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

    // In an archive the app draws the first page; an encrypted PDF fails.
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
            expect(nameOn(view, "readable.pdf") === null || !nameOn(view, "readable.pdf").visible,
                   "its tile has no name: the page covers it")
            expect(until(function () { return locked.thumbnailFailed }), "an encrypted PDF has no thumbnail")
            var name = nameOn(view, "secret.pdf")
            expect(name && name.visible, "so its tile shows its name")
        }
        pageStack.pop(null, PageStackAction.Immediate)
    }

    Component.onCompleted: {
        documents.createFolder("PdfThumbnails")
        tools.pump(500)
        tools.installFixture(Qt.resolvedUrl("host/fixtures/two-pages.pdf"), base + "readable.pdf")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/secret.pdf"), base + "secret.pdf")
        // A compact grid: 4 per row, names on icons only.
        App.setSetting("view_mode", JSON.stringify("grid"))
        App.setSetting("grid_columns", JSON.stringify(4))
        App.setSetting("thumbnails_remote", JSON.stringify(true))
        var view = push(base)
        expect(view.gridMode && !view.gridCaptions, "a compact grid")

        checkLocal(view)
        // The list's rows ask the same way.
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
        var plain = nameOn(view, "readable.pdf")
        expect(plain && plain.visible, "and every PDF tile shows its name")

        App.setSetting("thumbnails_local", JSON.stringify(true))
        App.setSetting("view_mode", JSON.stringify("list"))
        App.setSetting("grid_columns", JSON.stringify(4))
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
