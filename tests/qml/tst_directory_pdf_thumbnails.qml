// SPDX-License-Identifier: LGPL-2.1-or-later
// PDF thumbnails in a folder on the target (PRV-14): the first page of a PDF
// is its thumbnail, made by the app, and the name stays off a compact grid's
// tile only while a thumbnail covers the tile. A PDF that cannot be drawn
// (encrypted) keeps its icon and gets its name back; with PDF thumbnails off
// no PDF has one. Fails when a readable PDF shows no thumbnail, an
// unreadable one shows no name, or the switch changes nothing.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components/directory"

ApplicationWindow {
    id: window

    readonly property string base: "lautta://user-documents/PdfThumbnails/"

    ViewerTools { id: tools }

    DirectoryModel {
        id: documents
        uri: "lautta://user-documents/"
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

    // The icon of the row `name`: the one whose thumbnail is that file's, or
    // for a row without one, the only icon of its category left over.
    function iconOf(view, category, name) {
        var icons = findAll(view, function (x) { return x.thumbnailFailed !== undefined && x.category === category }, [])
        var own = icons.filter(function (x) { return x.thumbnailSource.indexOf("/" + name) > 0 })
        return own.length === 1 ? own[0] : null
    }

    // The name shown on the tile of `name`, null when the tile has none.
    function nameOn(view, name) {
        var labels = findAll(view, function (x) { return x.text === name && x.truncationMode !== undefined }, [])
        return labels.length > 0 ? labels[0] : null
    }

    // Lets the event loop run until `done()` holds, for at most 5 s.
    function until(done) {
        for (var i = 0; i < 100 && !done(); ++i)
            tools.pump(50)
        return done()
    }

    Component.onCompleted: {
        documents.createFolder("PdfThumbnails")
        tools.pump(500)
        tools.installFixture(Qt.resolvedUrl("host/fixtures/two-pages.pdf"), base + "readable.pdf")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/secret.pdf"), base + "secret.pdf")
        // A compact grid: 4 per row, names on icons only.
        App.setSetting("view_mode", JSON.stringify("grid"))
        App.setSetting("grid_columns", JSON.stringify(4))
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        tools.pump(500)
        var view = pageStack.currentPage.view
        expect(view.gridMode && !view.gridCaptions, "a compact grid")

        var good = iconOf(view, "pdf", "readable.pdf")
        var locked = iconOf(view, "pdf", "secret.pdf")
        if (expect(good && locked, "both PDFs ask for a thumbnail of the app: " + findAll(view, function (x) { return x.thumbnailFailed !== undefined }, []).length)) {
            expect(good.thumbnailSource.indexOf("image://lautta-thumb/" + base) === 0, "from the thumbnail provider: " + good.thumbnailSource)
            expect(until(function () { return good.thumbnailReady }), "the first page of a readable PDF shows")
            expect(nameOn(view, "readable.pdf") === null || !nameOn(view, "readable.pdf").visible,
                   "its tile has no name: the page covers it")
            expect(until(function () { return locked.thumbnailFailed }), "an encrypted PDF has no thumbnail")
            var name = nameOn(view, "secret.pdf")
            expect(name && name.visible, "so its tile shows its name")
        }

        App.setSetting("thumbnails_pdf", JSON.stringify(false))
        tools.pump(300)
        expect(findAll(view, function (x) { return x.thumbnailFailed !== undefined && x.category === "pdf" && x.thumbnailSource.length > 0 }, []).length === 0,
               "without PDF thumbnails no PDF asks for one")
        var plain = nameOn(view, "readable.pdf")
        expect(plain && plain.visible, "and every PDF tile shows its name")

        App.setSetting("thumbnails_pdf", JSON.stringify(true))
        App.setSetting("view_mode", JSON.stringify("list"))
        App.setSetting("grid_columns", JSON.stringify(4))
    }

    Component {
        id: pageComponent

        Page {
            property alias view: folder

            DirectoryView {
                id: folder
                anchors.fill: parent
                uri: window.base
            }
        }
    }
}
