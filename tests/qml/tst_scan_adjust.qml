// SPDX-License-Identifier: LGPL-2.1-or-later Corners and turn of a scanned page: dragged by their
// handles, changed on Done only.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

ApplicationWindow {
    id: win

    readonly property string sheet: Qt.resolvedUrl("host/fixtures/sheet.jpg")
    property bool scenarioRunning: true
    property bool reachedEnd
    property int step: 0
    property int waited: 0
    property var dialog
    property var before
    property var set
    // Corners found when the photo was taken.
    property var sheetCorners
    property QtObject scan

    ViewerTools { id: tools }

    Component {
        id: maker

        ScanModel { }
    }

    initialPage: Component {
        Page { }
    }

    function fail(what) {
        console.error("FAILED: " + what)
        scenarioRunning = false
    }

    function findAll(item, pred, out, seen) {
        seen = seen || []
        if (!item || seen.indexOf(item) >= 0)
            return out
        seen.push(item)
        if (pred(item))
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out, seen)
        if (item.contentItem)
            findAll(item.contentItem, pred, out, seen)
        return out
    }

    function named(root, name) {
        var found = findAll(root, function(x) { return x.objectName === name }, [])
        return found.length === 1 ? found[0] : null
    }

    function onTop(property) {
        var page = pageStack.currentPage
        return page && page[property] !== undefined && page.status === PageStatus.Active && !pageStack.busy ? page : null
    }

    function info(index) {
        return JSON.parse(scan.pageJson(index))
    }

    function near(a, b) {
        return Math.abs(a - b) < 0.002
    }

    function same(a, b) {
        for (var i = 0; i < 8; ++i) {
            if (!near(a[i], b[i]))
                return false
        }
        return a.length === 8 && b.length === 8
    }

    function whole(corners) {
        return same(corners, [0, 0, 1, 0, 1, 1, 0, 1])
    }

    // Handle sits with its middle on its corner of the photo.
    function onCorner(index) {
        var frame = named(win.dialog, "frame")
        var handle = named(win.dialog, "handle" + index)
        return near((handle.x + handle.width / 2) / frame.width, win.dialog.corners[2 * index])
                && near((handle.y + handle.height / 2) / frame.height, win.dialog.corners[2 * index + 1])
    }

    function open() {
        win.dialog = pageStack.push(Qt.resolvedUrl("../../qml/dialogs/ScanAdjustDialog.qml"), { "scan": win.scan, "index": 0 })
    }

    function next() {
        step++
        waited = 0
    }

    Timer {
        interval: 100
        repeat: true
        running: win.scenarioRunning
        onTriggered: {
            win.waited++
            if (win.waited > 150)
                return win.fail("step " + win.step + " timed out")
            var frame, handle, loupe, start, moved
            switch (win.step) {
            case 0:
                win.scan = maker.createObject(win)
                win.scan.add(win.sheet)
                win.next()
                break
            case 1:
                if (win.scan.busy || win.scan.count !== 1)
                    break
                win.before = info(0)
                win.sheetCorners = win.before.quad
                if (!win.before.found || whole(win.sheetCorners))
                    return win.fail("sheet's edges found")
                win.open()
                win.next()
                break
            case 2:
                frame = named(win.dialog, "frame")
                if (!win.onTop("corners") || !(frame.width > 0))
                    break
                if (!same(win.dialog.corners, win.before.quad) || win.dialog.turns !== 0 || !win.dialog.canAccept)
                    return win.fail("starts from the page's corners and turn")
                // Photo is 480 x 360: frame keeps that shape.
                if (!near(frame.width / frame.height, 480 / 360) || frame.rotation !== 0)
                    return win.fail("photo shown whole, upright: " + frame.width + " x " + frame.height)
                for (var i = 0; i < 4; ++i) {
                    if (!onCorner(i))
                        return win.fail("handle " + i + " on its corner")
                }
                loupe = named(win.dialog, "loupe")
                if (loupe.visible || win.dialog.grabbed !== -1)
                    return win.fail("no loupe until a corner is held")
                // Test screen is tiny: handles overlap, the last one lies on top.
                handle = named(win.dialog, "handle3")
                start = win.dialog.corners.slice()
                var dx = frame.width * 0.1, dy = frame.height * 0.08
                // Taken off its middle: the corner must not jump under the finger.
                tools.mouse(handle, "press", handle.width / 2 + 5, handle.height / 2 + 7)
                if (win.dialog.grabbed !== 3 || !loupe.visible || !same(win.dialog.corners, start))
                    return win.fail("held corner: loupe shows, corner stays")
                tools.mouse(handle, "move", handle.width / 2 + 5 + dx, handle.height / 2 + 7 + dy)
                moved = start.slice()
                moved[6] += 0.1
                moved[7] += 0.08
                if (!same(win.dialog.corners, moved))
                    return win.fail("corner follows the finger: " + JSON.stringify(win.dialog.corners) + " != " + JSON.stringify(moved))
                if (!onCorner(3))
                    return win.fail("handle follows its corner")
                // Far past the opposite corner: no page that way, corner stays.
                tools.mouse(handle, "move", handle.width / 2 + frame.width * 2, handle.height / 2 - frame.height * 2)
                if (!same(win.dialog.corners, moved))
                    return win.fail("a drag that leaves no page is refused")
                tools.mouse(handle, "release", handle.width / 2 + frame.width * 2, handle.height / 2 - frame.height * 2)
                if (win.dialog.grabbed !== -1 || loupe.visible)
                    return win.fail("let go: loupe away")
                named(win.dialog, "rotateRightButton").clicked(null)
                if (win.dialog.turns !== 1 || frame.rotation !== 90 || !near(frame.width / frame.height, 480 / 360))
                    return win.fail("turned right: " + win.dialog.turns)
                named(win.dialog, "rotateLeftButton").clicked(null)
                named(win.dialog, "rotateLeftButton").clicked(null)
                if (win.dialog.turns !== 3 || frame.rotation !== 270)
                    return win.fail("turned left twice: " + win.dialog.turns)
                named(win.dialog, "wholePictureButton").clicked(null)
                if (!whole(win.dialog.corners))
                    return win.fail("whole picture")
                pageStack.pop()
                win.dialog = null
                win.next()
                break
            case 3:
                if (pageStack.busy || pageStack.depth !== 1)
                    break
                if (win.scan.busy || JSON.stringify(info(0)) !== JSON.stringify(win.before))
                    return win.fail("left without Done: page as it was")
                win.scan.setCorners(0, JSON.stringify([0, 0, 1, 0, 1, 1, 0, 1]))
                win.next()
                break
            case 4:
                if (win.scan.busy)
                    break
                if (!win.dialog) {
                    win.before = info(0)
                    if (!whole(win.before.quad))
                        return win.fail("page uses the whole picture")
                    win.open()
                }
                if (!win.onTop("corners") || !(named(win.dialog, "frame").width > 0))
                    break
                if (!whole(win.dialog.corners))
                    return win.fail("opens on the whole picture")
                named(win.dialog, "findEdgesButton").clicked(null)
                if (!win.scan.busy)
                    return win.fail("edge search runs")
                win.next()
                break
            case 5:
                if (win.scan.busy)
                    break
                if (!same(win.dialog.corners, win.sheetCorners))
                    return win.fail("edges found again: " + JSON.stringify(win.dialog.corners))
                if (JSON.stringify(info(0)) !== JSON.stringify(win.before))
                    return win.fail("page itself untouched by the search")
                frame = named(win.dialog, "frame")
                handle = named(win.dialog, "handle3")
                tools.mouse(handle, "press", handle.width / 2, handle.height / 2)
                tools.mouse(handle, "move", handle.width / 2 + frame.width * 0.05, handle.height / 2 - frame.height * 0.1)
                tools.mouse(handle, "release", handle.width / 2 + frame.width * 0.05, handle.height / 2 - frame.height * 0.1)
                win.set = win.sheetCorners.slice()
                win.set[6] += 0.05
                win.set[7] -= 0.1
                if (!same(win.dialog.corners, win.set))
                    return win.fail("bottom left corner moved: " + JSON.stringify(win.dialog.corners))
                named(win.dialog, "rotateRightButton").clicked(null)
                win.dialog.accept()
                win.next()
                break
            case 6:
                if (pageStack.busy || pageStack.depth !== 1 || win.scan.busy)
                    break
                if (!same(info(0).quad, win.set) || info(0).turns !== 1)
                    return win.fail("Done sets corners and turn: " + JSON.stringify(info(0)))
                if (info(0).view === win.before.view)
                    return win.fail("page drawn anew")
                win.before = info(0)
                win.open()
                win.next()
                break
            case 7:
                if (!win.onTop("corners") || !(named(win.dialog, "frame").width > 0))
                    break
                if (!same(win.dialog.corners, win.set) || win.dialog.turns !== 1 || named(win.dialog, "frame").rotation !== 90)
                    return win.fail("opens with what was set")
                // Turned photo: a drag to the left on screen moves the corner down the photo.
                frame = named(win.dialog, "frame")
                handle = named(win.dialog, "handle3")
                start = win.dialog.corners.slice()
                var from = handle.mapToItem(win.contentItem, handle.width / 2, handle.height / 2)
                tools.mouse(handle, "press", handle.width / 2, handle.height / 2)
                var to = handle.mapFromItem(win.contentItem, from.x - frame.height * 0.05, from.y)
                tools.mouse(handle, "move", to.x, to.y)
                tools.mouse(handle, "release", to.x, to.y)
                moved = start.slice()
                moved[7] += 0.05
                if (!same(win.dialog.corners, moved))
                    return win.fail("drag on a turned photo: " + JSON.stringify(win.dialog.corners) + " != " + JSON.stringify(moved))
                // Nothing changed and accepted: no work.
                win.dialog.corners = start
                win.dialog.accept()
                win.next()
                break
            case 8:
                if (pageStack.busy || pageStack.depth !== 1)
                    break
                if (win.scan.busy || JSON.stringify(info(0)) !== JSON.stringify(win.before))
                    return win.fail("Done without a change changes nothing")
                win.reachedEnd = true
                win.scenarioRunning = false
                break
            }
        }
    }
}
