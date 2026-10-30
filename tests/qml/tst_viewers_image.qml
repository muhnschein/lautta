// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import QtQuick.Window 2.2 as QtQuick
import QtTest 1.0
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/viewers"

QtQuick.Window {
    id: window

    property bool scenarioRunning: true
    property var __silica_applicationwindow_instance: app
    readonly property var pageStack: app.pageStack
    property int step: 0
    property int waited: 0
    property var page
    property var view
    property bool started
    property real tappedAt
    property int readyFor
    property bool cleaned

    ViewerTools { id: tools }
    TestEvent { id: events }
    DirectoryModel {
        id: pictures
        uri: "lautta://user-pictures/"
    }
    Connections {
        target: Operations
        onRemoved: window.cleaned = true
    }

    function fail(what) {
        console.error("FAILED: " + what)
        scenarioRunning = false
    }

    function expect(ok, what) {
        if (!ok)
            fail(what)
        return ok
    }

    function findAll(item, pred, out) {
        if (!item)
            return out
        if (pred(item) && out.indexOf(item) < 0)
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out)
        if (item.contentItem && item.contentItem !== item)
            findAll(item.contentItem, pred, out)
        return out
    }

    function named(root, name) {
        var found = findAll(root, function(x) { return x.objectName === name && x.visible }, [])
        return found.length > 0 ? found[0] : null
    }

    function image() {
        return view.currentItem ? view.currentItem.item : null
    }

    function tapAt(item, x, y) {
        events.mouseClick(item, x, y, Qt.LeftButton, Qt.NoModifier, -1)
        tappedAt = Date.now()
    }

    function tapSettled() {
        return Date.now() - tappedAt > Qt.styleHints.mouseDoubleClickInterval + 150
    }

    function tapImage() {
        var area = named(image(), "tapArea")
        if (expect(area !== null, "the image has a tap area"))
            tapAt(area, area.width / 2, area.height / 2)
    }

    function describe() {
        if (!page)
            return "viewer closed, " + app.pageStack.depth + " pages"
        var shown = view ? image() : null
        var photo = shown ? named(shown, "zoomableImage") : null
        return "page " + page.status + ", in folder " + page.inFolder + " (" + page.total + "), on " + page.currentUri
                + ", index " + (view ? view.currentIndex + " of " + view.count : "-")
                + ", photo " + (photo ? photo.status : "none") + ", zoom " + (shown ? shown.zoom + "/" + shown.zoomed : "-")
                + ", overlay " + (view ? view.overlay.active : "-")
    }

    function opened() {
        view = named(page, "flickableView")
        if (!expect(view !== null, "the viewer has its image view"))
            return
        expect(page.inFolder && page.total === 2, "on the folder's listing: " + page.total)
        expect(page.currentUri === "lautta://user-pictures/ViewerOpen/one.jpg", "on the opened image: " + page.currentUri)
        expect(view.currentIndex === 0, "the opened image first in the folder's order: " + view.currentIndex)
        expect(view.overlay.active && view.overlay.opacity > 0, "the overlay shows at first")
        var more = named(view.overlay, "moreButton")
        if (expect(more !== null, "the overlay has More")) {
            expect(String(more.icon.source).slice(-15) === "/icons/more.svg", "More shows the app's icon: " + more.icon.source)
            expect(more.icon.status === Image.Ready, "the More icon loaded: " + more.icon.status)
            expect(more.icon.width === Theme.iconSizeMedium && more.icon.height === Theme.iconSizeMedium,
                   "the More icon has the theme icons' size: " + more.icon.width + "x" + more.icon.height)
        }
        var photo = named(image(), "zoomableImage")
        expect(photo !== null && photo.status === Image.Ready, "the image loaded")
        expect(!image().zoomed && view.interactive && page.backNavigation, "fitted: swipes and back navigation")
        tapImage()
    }

    Timer {
        interval: 100
        repeat: true
        running: window.scenarioRunning
        onTriggered: {
            if (!window.started)
                return
            if (++window.waited > 100)
                return window.fail("step " + window.step + " timed out: " + window.describe())
            switch (window.step) {
            case 0:
                window.view = window.named(window.page, "flickableView")
                if (app.pageStack.busy || !window.page.inFolder || !window.view
                        || !window.image() || window.named(window.image(), "zoomableImage").status !== Image.Ready) {
                    window.readyFor = 0
                    return
                }
                if (++window.readyFor < 5)
                    return
                window.opened()
                break
            case 1:
                if (!window.tapSettled())
                    return
                window.expect(!window.view.overlay.active, "a tap hides the overlay")
                window.tapImage()
                break
            case 2:
                if (!window.tapSettled())
                    return
                window.expect(window.view.overlay.active, "another tap brings it back")
                var area = window.named(window.image(), "tapArea")
                events.mouseDoubleClickSequence(area, area.width / 2, area.height / 2, Qt.LeftButton, Qt.NoModifier, -1)
                break
            case 3:
                if (!window.image().zoomed || window.image().enabled === false)
                    return
                var shown = window.image()
                window.expect(shown.zoom > shown.fittedZoom * 2, "a double tap zooms in: " + shown.zoom + " of " + shown.fittedZoom)
                window.expect(shown.contentWidth > window.view.width, "zoomed content is wider than the view")
                window.expect(!window.view.overlay.active, "zooming puts the overlay away")
                window.expect(!window.view.interactive && !window.page.backNavigation,
                              "zoomed: drags pan, no swiping, no back navigation")
                window.tapImage()
                break
            case 4:
                if (!window.tapSettled() || window.image().zoomed || window.image().enabled === false)
                    return
                var back = window.image()
                window.expect(Math.abs(back.zoom - back.fittedZoom) < 0.001 && back.contentX === 0,
                              "a tap zooms out: " + back.zoom + " at " + back.contentX)
                window.expect(window.view.overlay.active, "zoomed out: the overlay is back")
                window.expect(window.view.interactive && window.page.backNavigation, "swipes and back navigation again")
                var close = window.named(window.view.overlay, "dismissButton")
                if (!window.expect(close !== null && window.view.overlay.enabled, "the overlay has its close button"))
                    return
                close.clicked(null)
                break
            case 5:
                if (app.pageStack.busy || app.pageStack.currentPage === window.page)
                    return
                window.expect(app.pageStack.depth === 1, "closed: back on the page below: " + app.pageStack.depth)
                window.cleaned = false
                Operations.remove(JSON.stringify(["lautta://user-pictures/ViewerOpen"]))
                break
            case 6:
                if (!window.cleaned)
                    return
                window.scenarioRunning = false
                break
            }
            window.step++
            window.waited = 0
        }
    }

    Component.onCompleted: {
        pictures.createFolder("ViewerOpen")
        tools.pump(500)
        tools.installFixture(Qt.resolvedUrl("host/fixtures/large-photo.jpg"), "lautta://user-pictures/ViewerOpen/one.jpg")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), "lautta://user-pictures/ViewerOpen/two.jpg")
        app.pageStack.push(belowComponent, {}, PageStackAction.Immediate)
        tools.pump(300)
        page = app.pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        started = true
    }

    width: 540
    height: 960

    ApplicationWindow {
        id: app
    }

    Component {
        id: belowComponent
        Page { }
    }

    Component {
        id: pageComponent
        ImageViewer { uri: "lautta://user-pictures/ViewerOpen/one.jpg"; folderUri: "lautta://user-pictures/ViewerOpen/" }
    }
}
