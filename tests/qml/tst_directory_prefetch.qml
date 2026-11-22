// SPDX-License-Identifier: LGPL-2.1-or-later
// Finger down on an image, list and grid: its decode for the viewer starts. Not for other files,
// not for folders named like images, not while selecting. Hold, scroll or finger let go off the
// item: dropped.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components/directory"
import "../../qml/components/viewers/ViewerImage.js" as ViewerImage

ApplicationWindow {
    id: window

    property bool reachedEnd
    property var log: []
    property int fresh: 0
    readonly property string folder: "lautta://user-pictures/DirPrefetch/"
    property var view: pageStack.currentPage ? pageStack.currentPage.view : null
    property var prefetch: view ? named(view, "viewerPrefetch") : null

    ViewerTools { id: tools }
    DirectoryModel {
        id: pictures
        uri: "lautta://user-pictures/"
    }
    DirectoryModel { id: inner }

    // Status as the source changes: not Ready when dropped before the decode is done.
    Connections {
        target: window.prefetch
        onSourceChanged: window.log.push({ "source": String(window.prefetch.source), "status": window.prefetch.status })
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

    function named(item, name) {
        if (!item)
            return null
        if (item.objectName === name)
            return item
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i) {
            var found = named(kids[i], name)
            if (found)
                return found
        }
        return null
    }

    function delegates(f) {
        var out = []
        var kids = f.contentItem.children
        for (var i = 0; i < kids.length; ++i) {
            var cells = kids[i].regroup !== undefined ? kids[i].children : [kids[i]]
            for (var j = 0; j < cells.length; ++j)
                out.push(cells[j])
        }
        return out
    }

    function cell(name) {
        var f = view.flickable
        f.forceLayout()
        var index = view.model.indexOf(folder + name)
        var kids = delegates(f)
        for (var i = 0; i < kids.length; ++i) {
            var k = kids[i]
            var p = k.mapToItem(f.contentItem, k.width / 2, k.height / 2)
            if (k.width > 0 && k.visible && f.itemIndexAt(p.x, p.y) === index)
                return k
        }
        return null
    }

    function press(name) {
        view.showIndex(view.model.indexOf(folder + name))
        var c = cell(name)
        if (!expect(c !== null, name + " on screen"))
            return null
        log = []
        tools.mouse(c, "press", c.width / 2, c.height / 2)
        return c
    }

    function releaseOff(c) {
        tools.mouse(c, "release", c.width / 2, c.height * 4)
    }

    // Each check its own photo: one decoded before comes from the cache, let go at once.
    function photo() {
        return "p" + (fresh++) + ".jpg"
    }

    function check(mode) {
        var c = press("notes.md")
        releaseOff(c)
        expect(log.length === 0, mode + ": a text file starts no decode: " + JSON.stringify(log))
        c = press("album.jpg")
        releaseOff(c)
        expect(log.length === 0, mode + ": a folder named like an image starts no decode: " + JSON.stringify(log))

        var name = photo()
        c = press(name)
        expect(log.length === 1 && log[0].source === ViewerImage.source(folder + name),
               mode + ": finger down on an image starts its decode: " + JSON.stringify(log))
        releaseOff(c)
        expect(log.length === 2 && log[1].source === "" && log[1].status !== Image.Ready,
               mode + ": let go off the item drops it: " + JSON.stringify(log))

        name = photo()
        c = press(name)
        for (var i = 1; i <= 15; ++i)
            tools.mouse(c, "move", c.width / 2, c.height / 2 - 10 * i)
        expect(log.length === 2 && log[1].source === "" && log[1].status !== Image.Ready,
               mode + ": scroll drops it: " + JSON.stringify(log))
        tools.mouse(c, "release", c.width / 2, c.height / 2 - 150)
        view.flickable.cancelFlick()
        view.flickable.positionViewAtBeginning()
        expect(pageStack.depth === 1, mode + ": no viewer opened")

        // Grid's hold needs a mouse event: a real one comes after the decode is done.
        if (mode === "list") {
            c = press(photo())
            c.pressAndHold(null)
            expect(log.length === 2 && log[1].source === "" && log[1].status !== Image.Ready,
                   mode + ": hold drops it: " + JSON.stringify(log))
            releaseOff(c)
            if (c.menuOpen)
                c.closeMenu()
        }

        view.startSelection()
        expect(view.selecting, mode + ": selecting")
        c = press(photo())
        releaseOff(c)
        expect(log.length === 0, mode + ": no decode while selecting: " + JSON.stringify(log))
        view.endSelection()
    }

    Component.onCompleted: {
        pictures.createFolder("DirPrefetch")
        waitFor(function () { return pictures.indexOf(folder.slice(0, -1)) >= 0 }, "the folder")
        inner.uri = folder
        inner.createFolder("album.jpg")
        waitFor(function () { return inner.indexOf(folder + "album.jpg") >= 0 }, "the image-named folder")
        var installed = tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), folder + "notes.md")
        for (var i = 0; i < 40; ++i)
            installed = tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), folder + "p" + i + ".jpg") && installed
        if (!expect(installed, "fixtures installed"))
            return
        for (var m = 0; m < 2; ++m) {
            var mode = m === 0 ? "list" : "grid"
            if (m === 0)
                pageStack.push(pageComponent, {}, PageStackAction.Immediate)
            else
                pageStack.replace(pageComponent, {}, PageStackAction.Immediate)
            waitFor(function () { return !view.model.loading && view.model.count === 42 }, "the folder")
            view.model.setViewPrefs(JSON.stringify({ "viewMode": mode }))
            waitFor(function () { return cell("notes.md") !== null }, "the " + mode)
            if (!expect(prefetch !== null && view.gridMode === (mode === "grid"), mode + ": the view has a prefetch"))
                return
            check(mode)
        }
        reachedEnd = true
    }

    Component {
        id: pageComponent

        Page {
            property alias view: folder

            DirectoryView {
                id: folder
                anchors.fill: parent
                uri: window.folder
            }
        }
    }
}
