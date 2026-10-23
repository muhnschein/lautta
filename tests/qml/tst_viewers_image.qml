// SPDX-License-Identifier: LGPL-2.1-or-later
// The image viewer on the target (PRV-4, real Silica and Lautta types),
// Jolla Gallery's: it opens a local image full screen on the folder's
// listing with the overlay shown; a tap hides the overlay and another
// brings it back; a double tap zooms in (overlay away, no swiping, no back
// navigation) and a tap zooms out again; a swipe goes to the folder's next
// image; Delete calls remove() (its remorse runs) and the close button
// goes back. Taps and swipes are real mouse events, so the Silica window
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
    // What Silica's pages look up in the window they are in.
    property var __silica_applicationwindow_instance: app
    property int step: 0
    property int waited: 0
    property var page
    property var view
    // When the last tap went in: a single tap counts once the double-tap time is over.
    property real tappedAt
    property int readyFor

    ViewerTools { id: tools }
    TestEvent { id: events }
    // Makes the folder of the test's images.
    DirectoryModel {
        id: pictures
        uri: "lautta://user-pictures/"
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

    // Finger down in the middle, moved left over half the view, up.
    function swipeLeft() {
        var y = view.height / 2
        var x = view.width * 0.8
        events.mousePress(view, x, y, Qt.LeftButton, Qt.NoModifier, -1)
        for (var i = 1; i <= 10; ++i)
            events.mouseMove(view, x - i * view.width * 0.06, y, 16, Qt.LeftButton)
        events.mouseRelease(view, x - view.width * 0.6, y, Qt.LeftButton, Qt.NoModifier, 16)
    }

    // The state the steps wait on, for a timeout's message.
    function describe() {
        var shown = view ? image() : null
        var photo = shown ? named(shown, "zoomableImage") : null
        return "page " + page.status + ", in folder " + page.inFolder + " (" + page.total + "), on " + page.currentUri
                + ", index " + (view ? view.currentIndex + " of " + view.count : "-")
                + ", photo " + (photo ? photo.status : "none") + ", zoom " + (shown ? shown.zoom + "/" + shown.zoomed : "-")
                + ", overlay " + (view ? view.overlay.active : "-")
    }

    function remorse() {
        var found = findAll(page, function(x) { return x.execute !== undefined && x.cancel !== undefined && x.active !== undefined }, [])
        return found.length > 0 ? found[0] : null
    }

    function opened() {
        view = named(page, "flickableView")
        if (!expect(view !== null, "the viewer has its image view"))
            return
        expect(page.inFolder && page.total === 2, "on the folder's listing: " + page.total)
        expect(page.currentUri === "lautta://user-pictures/ViewerSwipe/one.jpg", "on the opened image: " + page.currentUri)
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
            if (!window.page)
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
                window.swipeLeft()
                break
            case 5:
                if (window.view.currentIndex !== 1 || window.view.moving)
                    return
                window.expect(window.page.currentUri === "lautta://user-pictures/ViewerSwipe/two.jpg",
                              "swiped to the folder's next image: " + window.page.currentUri)
                var remove = window.named(window.view.overlay, "deleteButton")
                if (!window.expect(remove !== null, "the overlay has Delete"))
                    return
                remove.clicked(null)
                break
            case 6:
                var popup = window.remorse()
                if (!window.expect(popup !== null && popup.active, "Delete runs remove(): its remorse shows"))
                    return
                window.expect(!window.view.interactive && !window.view.overlay.enabled,
                              "while the remorse runs: no swipes, no overlay")
                popup.cancel()
                break
            case 7:
                if (window.remorse().active)
                    return
                window.expect(window.page.total === 2, "cancelled: nothing deleted")
                var close = window.named(window.view.overlay, "dismissButton")
                if (!window.expect(close !== null && window.view.overlay.enabled, "the overlay has its close button"))
                    return
                close.clicked(null)
                break
            case 8:
                if (app.pageStack.busy || app.pageStack.currentPage === window.page)
                    return
                window.expect(app.pageStack.depth === 1, "closed: back on the page below: " + app.pageStack.depth)
                window.scenarioRunning = false
                break
            }
            window.step++
            window.waited = 0
        }
    }

    Component.onCompleted: {
        pictures.createFolder("ViewerSwipe")
        tools.pump(500)
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), "lautta://user-pictures/ViewerSwipe/one.jpg")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), "lautta://user-pictures/ViewerSwipe/two.jpg")
        // The page the viewer opens from, to go back to.
        app.pageStack.push(belowComponent, {}, PageStackAction.Immediate)
        tools.pump(300)
        page = app.pageStack.push(pageComponent, {}, PageStackAction.Immediate)
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
        ImageViewer { uri: "lautta://user-pictures/ViewerSwipe/one.jpg"; folderUri: "lautta://user-pictures/ViewerSwipe/" }
    }
}
