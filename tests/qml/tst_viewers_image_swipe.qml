// SPDX-License-Identifier: LGPL-2.1-or-later
// The image viewer's swipes and Delete on the target (PRV-4, real Silica
// and Lautta types): a swipe goes to the folder's next image; Delete calls
// remove(), whose remorse holds the swipes and the overlay while it runs;
// cancelled, nothing is deleted; the close button goes back; the folder
// goes at the end. Swipes are real mouse events, so the Silica window sits
// in a Qt Quick window: only items in one get them.
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
    property int readyFor
    property bool cleaned
    // A swipe's moves wait between them, and the step timer must not run a
    // step again meanwhile.
    property bool swiping

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

    // Finger down right of the middle, moved left over half the view, up.
    function swipeLeft() {
        var y = view.height / 2
        var x = view.width * 0.8
        swiping = true
        events.mousePress(view, x, y, Qt.LeftButton, Qt.NoModifier, -1)
        for (var i = 1; i <= 10; ++i)
            events.mouseMove(view, x - i * view.width * 0.06, y, 16, Qt.LeftButton)
        events.mouseRelease(view, x - view.width * 0.6, y, Qt.LeftButton, Qt.NoModifier, 16)
        swiping = false
    }

    // The state the steps wait on, for a timeout's message.
    function describe() {
        if (!page)
            return "viewer closed, " + app.pageStack.depth + " pages"
        return "page " + page.status + ", in folder " + page.inFolder + " (" + page.total + "), on " + page.currentUri
                + ", index " + (view ? view.currentIndex + " of " + view.count + (view.moving ? ", moving" : "") : "-")
    }

    function remorse() {
        var found = findAll(page, function(x) { return x.execute !== undefined && x.cancel !== undefined && x.active !== undefined }, [])
        return found.length > 0 ? found[0] : null
    }

    Timer {
        interval: 100
        repeat: true
        running: window.scenarioRunning
        onTriggered: {
            if (!window.started || window.swiping)
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
                window.expect(window.page.total === 2 && window.view.currentIndex === 0
                              && window.page.currentUri === "lautta://user-pictures/ViewerSwipe/one.jpg",
                              "opened on the folder's first image: " + window.describe())
                window.expect(window.view.interactive, "fitted with a neighbour: swipes")
                window.swipeLeft()
                break
            case 1:
                if (window.view.currentIndex !== 1 || window.view.moving)
                    return
                window.expect(window.page.currentUri === "lautta://user-pictures/ViewerSwipe/two.jpg",
                              "swiped to the folder's next image: " + window.page.currentUri)
                var remove = window.named(window.view.overlay, "deleteButton")
                if (!window.expect(remove !== null, "the overlay has Delete"))
                    return
                remove.clicked(null)
                break
            case 2:
                var popup = window.remorse()
                if (!window.expect(popup !== null && popup.active, "Delete runs remove(): its remorse shows"))
                    return
                window.expect(!window.view.interactive && !window.view.overlay.enabled,
                              "while the remorse runs: no swipes, no overlay")
                popup.cancel()
                break
            case 3:
                if (window.remorse().active)
                    return
                window.expect(window.page.total === 2, "cancelled: nothing deleted")
                var close = window.named(window.view.overlay, "dismissButton")
                if (!window.expect(close !== null && window.view.overlay.enabled, "the overlay has its close button"))
                    return
                close.clicked(null)
                break
            case 4:
                if (app.pageStack.busy || app.pageStack.currentPage === window.page)
                    return
                window.expect(app.pageStack.depth === 1, "closed: back on the page below: " + app.pageStack.depth)
                // Leave Pictures as it was for the scenarios after this one.
                window.cleaned = false
                Operations.remove(JSON.stringify(["lautta://user-pictures/ViewerSwipe"]))
                break
            case 5:
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
        pictures.createFolder("ViewerSwipe")
        tools.pump(500)
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), "lautta://user-pictures/ViewerSwipe/one.jpg")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), "lautta://user-pictures/ViewerSwipe/two.jpg")
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
        ImageViewer { uri: "lautta://user-pictures/ViewerSwipe/one.jpg"; folderUri: "lautta://user-pictures/ViewerSwipe/" }
    }
}
