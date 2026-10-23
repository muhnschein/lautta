// SPDX-License-Identifier: LGPL-2.1-or-later
// The image viewer on the target (PRV-4, real Silica and Lautta types),
// Jolla Gallery's: it opens a local image full screen on the folder's
// listing with the overlay shown; a tap hides the overlay and another
// brings it back; a double tap zooms in (overlay away, no swiping, no back
// navigation) and a tap zooms out again; the close button goes back; the
// folder goes at the end. Swiping and Delete are tst_viewers_image_swipe's
// (one scenario has 60 s). Taps are real mouse events, so the Silica window
// sits in a Qt Quick window: only items in one get them.
import QtQuick 2.6
import QtQuick.Window 2.2 as QtQuick
import QtTest 1.0
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/viewers"

QtQuick.Window {
    id: window

    property bool scenarioRunning: true
    // What Silica's pages look up in the window they are in, and the page
    // stack that an app's ApplicationWindow, as the root, gives them all.
    property var __silica_applicationwindow_instance: app
    readonly property var pageStack: app.pageStack
    property int step: 0
    property int waited: 0
    // The viewer: null once closed, as the page stack destroys it then.
    property var page
    property var view
    // The viewer is pushed: the steps run from here on.
    property bool started
    // When the last tap went in: a single tap counts once the double-tap time is over.
    property real tappedAt
    property int readyFor
    property bool cleaned

    ViewerTools { id: tools }
    TestEvent { id: events }
    // Makes the folder of the test's images.
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
        // A flickable's contentItem is one of its children too.
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

    // The image shown: the current slot's GalleryImage.
    function image() {
        return view.currentItem ? view.currentItem.item : null
    }

    function tapAt(item, x, y) {
        events.mouseClick(item, x, y, Qt.LeftButton, Qt.NoModifier, -1)
        tappedAt = Date.now()
    }

    // A single tap counts after the double-tap time.
    function tapSettled() {
        return Date.now() - tappedAt > Qt.styleHints.mouseDoubleClickInterval + 150
    }

    function tapImage() {
        var area = named(image(), "tapArea")
        if (expect(area !== null, "the image has a tap area"))
            tapAt(area, area.width / 2, area.height / 2)
    }

    // The state the steps wait on, for a timeout's message.
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
                // Settled on the listing for a moment, as a finger would be.
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
                // The zoom animation ends zoomed in.
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
                // A tap on a zoomed image zooms it out (animated).
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
                // Leave Pictures as it was for the scenarios after this one.
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
        // Larger than the screen: the Gallery zooms up to the longer screen
        // side / 200 (1.6 on the check's 240x320), which a small picture
        // already fits at, so it would not zoom at all.
        tools.installFixture(Qt.resolvedUrl("host/fixtures/large-photo.jpg"), "lautta://user-pictures/ViewerOpen/one.jpg")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), "lautta://user-pictures/ViewerOpen/two.jpg")
        // The page the viewer opens from, to go back to.
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
