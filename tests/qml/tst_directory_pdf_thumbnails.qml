// SPDX-License-Identifier: LGPL-2.1-or-later
// PDF thumbnails in a folder on the target (PRV-14): the first page of a PDF
// is its thumbnail, made by the app. A PDF that cannot be drawn (encrypted)
// keeps its icon; with Show thumbnails off no PDF has one (the name under the
// tile stays either way, tst_directory_grid_captions.qml). Fails when a
// readable PDF shows no thumbnail, an unreadable one is not marked as failed,
// or the switch changes nothing.
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

    // Lets the event loop run until `done()` holds, for at most 10 s.
    function until(done) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        return done()
    }

    Component.onCompleted: {
        documents.createFolder("PdfThumbnails")
        tools.pump(500)
        tools.installFixture(Qt.resolvedUrl("host/fixtures/two-pages.pdf"), base + "readable.pdf")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/secret.pdf"), base + "secret.pdf")
        App.setSetting("view_mode", JSON.stringify("grid"))
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        tools.pump(500)
        var view = pageStack.currentPage.view
        expect(view.gridMode, "a grid")

        until(function () { return iconOf(view, "pdf", "readable.pdf") && iconOf(view, "pdf", "secret.pdf") })
        var good = iconOf(view, "pdf", "readable.pdf")
        var locked = iconOf(view, "pdf", "secret.pdf")
        if (expect(good && locked, "both PDFs ask for a thumbnail of the app: " + findAll(view, function (x) { return x.thumbnailFailed !== undefined }, []).length)) {
            expect(good.thumbnailSource.indexOf("image://lautta-thumb/" + base) === 0, "from the thumbnail provider: " + good.thumbnailSource)
            expect(until(function () { return good.thumbnailReady }), "the first page of a readable PDF shows")
            expect(until(function () { return locked.thumbnailFailed }), "an encrypted PDF has no thumbnail")
        }

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
            property alias view: folder

            DirectoryView {
                id: folder
                anchors.fill: parent
                uri: window.base
            }
        }
    }
}
