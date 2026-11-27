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
    property bool reachedEnd
    property var __silica_applicationwindow_instance: app
    readonly property var pageStack: app.pageStack
    readonly property string folder: "lautta://user-pictures/ViewerVideo/"
    property int step: 0
    property int waited: 0
    property var page
    property var view
    property bool started
    property real tappedAt
    property bool cleaned
    property bool swiping
    property var videoSlot

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

    function shown() {
        return view.currentItem ? view.currentItem.item : null
    }

    function slotOf(uri) {
        var found = findAll(view, function(x) { return x.itemUri === uri }, [])
        return found.length > 0 ? found[0] : null
    }

    function tapAt(item, x, y) {
        events.mouseClick(item, x, y, Qt.LeftButton, Qt.NoModifier, -1)
        tappedAt = Date.now()
    }

    function tapSettled() {
        return Date.now() - tappedAt > Qt.styleHints.mouseDoubleClickInterval + 150
    }

    function tapVideo() {
        var area = named(shown(), "videoTapArea")
        if (expect(area !== null, "the video has a tap area"))
            tapAt(area, area.width / 2, area.height / 4)
    }

    function swipe(towardsNext) {
        var y = view.height / 2
        var x = view.width * (towardsNext ? 0.8 : 0.2)
        var dir = towardsNext ? -1 : 1
        swiping = true
        events.mousePress(view, x, y, Qt.LeftButton, Qt.NoModifier, -1)
        for (var i = 1; i <= 10; ++i)
            events.mouseMove(view, x + dir * i * view.width * 0.06, y, 16, Qt.LeftButton)
        events.mouseRelease(view, x + dir * view.width * 0.6, y, Qt.LeftButton, Qt.NoModifier, 16)
        swiping = false
    }

    function describe() {
        if (!page)
            return "viewer closed, " + app.pageStack.depth + " pages"
        var slot = view ? view.currentItem : null
        return "page " + page.status + ", in folder " + page.inFolder + " (" + page.total + "), on " + page.currentUri
                + ", index " + (view ? view.currentIndex + " of " + view.count + (view.moving ? ", moving" : "") : "-")
                + ", video " + (slot ? slot.video : "-") + ", player " + (slot && slot.player ? slot.player.errorKind : "none")
                + ", overlay " + (view ? view.overlay.active : "-")
    }

    function openedOnVideo() {
        expect(page.inFolder && page.total === 3, "photos and the video in one sequence: " + page.total)
        expect(view.currentIndex === 1 && page.currentUri === folder + "b.mp4",
               "on the opened video, where the folder has it: " + describe())
        expect(view.currentItem.video && named(shown(), "zoomableImage") === null, "the video's slot holds no image")
        expect(view.currentItem.player === null, "nothing plays before play is tapped")
        expect(named(shown(), "playButton") !== null, "the poster shows its play button")
        expect(view.interactive && page.backNavigation, "swipes and back navigation, as for a photo")
        expect(named(view.overlay, "videoControls") !== null && named(view.overlay, "positionSlider") !== null,
               "the overlay shows the position slider")
        expect(named(view.overlay, "detailsButton") === null, "no camera details for a video")
        expect(named(view.overlay, "shareButton") !== null && named(view.overlay, "deleteButton") !== null,
               "the photo's actions are there")
        var controls = named(view.overlay, "videoControls")
        var top = controls.mapToItem(page, 0, 0).y
        var toolbarTop = view.overlay.toolbar.mapToItem(page, 0, 0).y
        expect(controls.height > 0 && top + controls.height <= toolbarTop,
               "the slider sits above the toolbar: " + top + "+" + controls.height + " " + toolbarTop)
        expect(page.height - view.overlay.toolbarSpace <= top,
               "the back hint sits above the slider: " + view.overlay.toolbarSpace + " " + top)
    }

    Timer {
        interval: 100
        repeat: true
        running: window.scenarioRunning
        onTriggered: {
            if (!window.started || window.swiping)
                return
            if (++window.waited > 150)
                return window.fail("step " + window.step + " timed out: " + window.describe())
            switch (window.step) {
            case 0:
                window.view = window.named(window.page, "flickableView")
                if (app.pageStack.busy || !window.page.inFolder || !window.view || !window.view.currentItem
                        || !window.shown() || window.view.currentIndex !== 1)
                    return
                window.openedOnVideo()
                window.tapVideo()
                break
            case 1:
                if (!window.tapSettled())
                    return
                window.expect(!window.view.overlay.active, "a tap hides the overlay")
                window.tapVideo()
                break
            case 2:
                if (!window.tapSettled())
                    return
                window.expect(window.view.overlay.active, "another tap brings it back")
                window.named(window.shown(), "playButton").clicked(null)
                break
            case 3:
                var player = window.view.currentItem.player
                var next = window.slotOf(window.folder + "c.jpg")
                if (!player || player.errorKind === "" || !next || !next.loadAllowed)
                    return
                window.expect(window.slotOf(window.folder + "a.jpg").loadAllowed, "neighbours load once the poster has")
                window.expect(player.statusText === "lautta-err-not-playable" || player.statusText === "Can't play b.mp4.",
                              "the file is named as unplayable: '" + player.statusText + "'")
                window.expect(!player.playing && !window.view.currentItem.playing, "nothing plays")
                window.expect(window.named(window.shown(), "videoError") !== null, "the error shows on the video")
                window.expect(window.view.interactive, "a stopped video keeps the swipes")
                window.videoSlot = window.view.currentItem
                window.swipe(true)
                break
            case 4:
                if (window.view.currentIndex !== 2 || window.view.moving)
                    return
                window.expect(window.page.currentUri === window.folder + "c.jpg", "swiped to the next photo: " + window.describe())
                window.expect(window.named(window.shown(), "zoomableImage") !== null, "the photo shows as a photo")
                window.expect(!window.videoSlot || !window.videoSlot.player,
                              "the video left the screen: its player is gone")
                window.expect(window.named(window.view.overlay, "videoControls") === null
                              && window.named(window.view.overlay, "detailsButton") !== null,
                              "a photo's overlay: details, no slider")
                window.swipe(false)
                break
            case 5:
                if (window.view.currentIndex !== 1 || window.view.moving || !window.shown())
                    return
                window.expect(window.view.currentItem.video && window.view.currentItem.player === null,
                              "back on the video: poster again, not playing")
                window.expect(window.named(window.shown(), "playButton") !== null, "the play button is back")
                window.page = app.pageStack.replace(hiddenComponent, {}, PageStackAction.Immediate)
                window.view = null
                break
            case 6:
                window.view = window.named(window.page, "flickableView")
                if (app.pageStack.busy || !window.view || !window.view.currentItem || !window.shown()
                        || window.view.count !== 1 || window.page.total !== 1)
                    return
                window.expect(!window.page.inFolder, "a hidden video opens on its own: " + window.describe())
                window.expect(window.view.currentItem.video && window.named(window.shown(), "playButton") !== null,
                              "opened alone, it is still a video")
                var close = window.named(window.view.overlay, "dismissButton")
                if (!window.expect(close !== null && window.view.overlay.enabled, "the overlay has its close button"))
                    return
                close.clicked(null)
                break
            case 7:
                if (app.pageStack.busy || app.pageStack.currentPage === window.page)
                    return
                window.expect(app.pageStack.depth === 1, "closed: back on the page below: " + app.pageStack.depth)
                window.cleaned = false
                Operations.remove(JSON.stringify(["lautta://user-pictures/ViewerVideo"]))
                break
            case 8:
                if (!window.cleaned)
                    return
                window.reachedEnd = true
                window.scenarioRunning = false
                break
            }
            window.step++
            window.waited = 0
        }
    }

    Component.onCompleted: {
        pictures.createFolder("ViewerVideo")
        for (var i = 0; i < 200 && pictures.indexOf(pictures.uri + "ViewerVideo") < 0; ++i)
            tools.pump(50)
        if (pictures.indexOf(pictures.uri + "ViewerVideo") < 0)
            console.error("FAILED: folder ViewerVideo not made")
        if (!(tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), folder + "a.jpg")
              && tools.installFixture(Qt.resolvedUrl("host/fixtures/data.bin"), folder + "b.mp4")
              && tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), folder + "c.jpg")
              && tools.installFixture(Qt.resolvedUrl("host/fixtures/data.bin"), folder + ".d.mp4")))
            console.error("FAILED: fixtures not installed")
        app.pageStack.push(belowComponent, {}, PageStackAction.Immediate)
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
        ImageViewer { uri: window.folder + "b.mp4"; folderUri: window.folder; video: true }
    }

    Component {
        id: hiddenComponent
        ImageViewer { uri: window.folder + ".d.mp4"; folderUri: window.folder; video: true }
    }
}
