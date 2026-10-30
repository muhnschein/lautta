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
    property int readyFor
    property bool cleaned
    property bool swiping

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
