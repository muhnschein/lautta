// SPDX-License-Identifier: LGPL-2.1-or-later Corners and turn of a scanned page: dragged by their
// handles, changed on Done only.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components/scan/ScanMath.js" as ScanMath

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

    function covers(item, p) {
        return p.x >= item.x && p.x <= item.x + item.width && p.y >= item.y && p.y <= item.y + item.height
    }

    // As the dialog places it, for a finger at `p` on the dialog.
    function loupeAt(p) {
        var size = named(win.dialog, "loupe").width
        var at = ScanMath.loupeAt(p.x, p.y, size, Theme.itemSizeLarge, win.dialog.width, win.dialog.height)
        return JSON.stringify([at.x, at.y])
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
            // Hang guard: emulated target needs seconds per picture.
            if (win.waited > 600)
                return win.fail("step " + win.step + " timed out")
            var frame, handle, loupe, start, moved, finger
            switch (win.step) {
            case 0:
                win.scan = maker.createObject(win)
                win.scan.add(win.sheet)
                win.scan.add(win.sheet)
                win.next()
                break
            case 1:
                if (win.scan.busy || win.scan.count !== 2)
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
                // Photo is 480 x 360: frame keeps that shape, as large as its place allows.
                if (!near(frame.width / frame.height, 480 / 360) || frame.rotation !== 0
                        || !near(Math.min(frame.parent.width / frame.width, frame.parent.height / frame.height), 1))
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
                finger = handle.mapToItem(win.dialog, handle.width / 2 + 5, handle.height / 2 + 7)
                if (!near(win.dialog.fingerX, finger.x) || !near(win.dialog.fingerY, finger.y))
                    return win.fail("finger followed on the dialog: " + win.dialog.fingerX + "," + win.dialog.fingerY)
                if (loupeAt(finger) !== JSON.stringify([loupe.x, loupe.y]) || named(loupe, "loupeRing").radius !== loupe.width / 2
                        || !(loupe.width >= Math.min(win.dialog.width, win.dialog.height) / 2 - 1))
                    return win.fail("loupe big, round, placed for the finger: " + loupe.x + "," + loupe.y + " " + loupe.width)
                var held = finger
                // Test screen is too small for room above a handle: finger set where there is.
                win.dialog.fingerX = win.dialog.width / 2 + 10
                win.dialog.fingerY = win.dialog.height - 10
                finger = Qt.point(win.dialog.fingerX, win.dialog.fingerY)
                if (loupe.y + loupe.height > finger.y - Theme.itemSizeLarge + 0.5 || covers(loupe, finger)
                        || !near(loupe.x + loupe.width / 2, finger.x))
                    return win.fail("room above: loupe above the finger, clear of it: " + loupe.x + "," + loupe.y)
                tools.mouse(handle, "move", handle.width / 2 + 5 + dx, handle.height / 2 + 7 + dy)
                // Handle moved with the corner: the finger is where it was, plus the move.
                finger = Qt.point(held.x + dx, held.y + dy)
                if (loupeAt(finger) !== JSON.stringify([loupe.x, loupe.y]))
                    return win.fail("loupe moves with the finger: " + loupe.x + "," + loupe.y)
                moved = start.slice()
                moved[6] += 0.1
                moved[7] += 0.08
                if (!same(win.dialog.corners, moved))
                    return win.fail("corner follows the finger: " + JSON.stringify(win.dialog.corners) + " != " + JSON.stringify(moved))
                var lensPhoto = named(loupe, "loupePhoto")
                if (!near(lensPhoto.width, frame.width * 2) || !near(lensPhoto.x, -moved[6] * lensPhoto.width)
                        || !near(lensPhoto.y, -moved[7] * lensPhoto.height))
                    return win.fail("loupe: held corner twice as large, in its middle: " + lensPhoto.x + "," + lensPhoto.y)
                if (!onCorner(3))
                    return win.fail("handle follows its corner")
                // Far past the opposite corner: no page that way, corner stays.
                tools.mouse(handle, "move", handle.width / 2 + frame.width * 2, handle.height / 2 - frame.height * 2)
                if (!same(win.dialog.corners, moved))
                    return win.fail("a drag that leaves no page is refused")
                tools.mouse(handle, "release", handle.width / 2 + frame.width * 2, handle.height / 2 - frame.height * 2)
                if (win.dialog.grabbed !== -1 || loupe.visible)
                    return win.fail("let go: loupe away")
                if (named(win.dialog, "findEdgesButton") || named(win.dialog, "wholePictureButton"))
                    return win.fail("tools: rotate left and right only")
                named(win.dialog, "rotateRightButton").clicked(null)
                if (win.dialog.turns !== 1 || frame.rotation !== 90 || !near(frame.width / frame.height, 480 / 360))
                    return win.fail("turned right: " + win.dialog.turns)
                // On its side the photo's height runs across.
                if (!near(Math.min(frame.parent.width / frame.height, frame.parent.height / frame.width), 1))
                    return win.fail("turned photo fills its place: " + frame.width + " x " + frame.height)
                named(win.dialog, "rotateLeftButton").clicked(null)
                if (win.dialog.turns !== 0 || frame.rotation !== 0)
                    return win.fail("turned back left: " + win.dialog.turns)
                named(win.dialog, "rotateLeftButton").clicked(null)
                if (win.dialog.turns !== 3 || frame.rotation !== 270)
                    return win.fail("turned left past upright: " + win.dialog.turns)
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
                // Corners as a drag would leave them: the sheet's.
                win.dialog.corners = win.sheetCorners.slice()
                frame = named(win.dialog, "frame")
                // Top left corner held: no room above, loupe beside the finger.
                handle = named(win.dialog, "handle0")
                tools.mouse(handle, "press", handle.width / 2, handle.height / 2)
                loupe = named(win.dialog, "loupe")
                finger = handle.mapToItem(win.dialog, handle.width / 2, handle.height / 2)
                if (finger.y - Theme.itemSizeLarge - loupe.height >= 0)
                    return win.fail("test needs a corner without room above: " + finger.y)
                if (loupeAt(finger) !== JSON.stringify([loupe.x, loupe.y]) || loupe.y < 0 || loupe.y + loupe.height > win.dialog.height)
                    return win.fail("no room above: loupe beside, inside the dialog: " + loupe.x + "," + loupe.y)
                if (covers(loupe, finger) && !near(loupe.x, finger.x) && !near(loupe.x + loupe.width, finger.x))
                    return win.fail("no room above: loupe clear of the finger: " + loupe.x + "," + loupe.y + " " + finger.x)
                tools.mouse(handle, "release", handle.width / 2, handle.height / 2)
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
            case 5:
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
            case 6:
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
                win.dialog.corners = start
                named(win.dialog, "rotateRightButton").clicked(null)
                win.dialog.accept()
                win.next()
                break
            case 7:
                if (pageStack.busy || pageStack.depth !== 1 || win.scan.busy)
                    break
                if (info(0).turns !== 2 || !same(info(0).quad, win.set))
                    return win.fail("Done turns on from the page's turn: " + JSON.stringify(info(0)))
                win.before = info(0)
                win.open()
                win.next()
                break
            case 8:
                if (!win.onTop("corners") || !(named(win.dialog, "frame").width > 0))
                    break
                if (win.dialog.turns !== 2)
                    return win.fail("opens upside down: " + win.dialog.turns)
                // Nothing changed and accepted: no work.
                win.dialog.accept()
                win.next()
                break
            case 9:
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
