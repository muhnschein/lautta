// SPDX-License-Identifier: LGPL-2.1-or-later Camera page of a scan, photos handed in as the camera
// would: page flies into the stack, page count, retake, going on to the pages and to saving.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

ApplicationWindow {
    id: win

    readonly property string sheet: Qt.resolvedUrl("host/fixtures/sheet.jpg")
    readonly property string plain: Qt.resolvedUrl("host/fixtures/photo.jpg")
    property bool scenarioRunning: true
    property bool reachedEnd
    property int step: 0
    property int waited: 0
    property var camera
    property var retaker
    property var pageView
    property var ended: []
    // Null once the scan is destroyed.
    property QtObject scan

    Component {
        id: maker

        ScanModel { }
    }

    Item {
        id: owner

        function endScan(scan, saved) {
            win.ended = win.ended.concat([saved])
            scan.destroy()
        }
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
            switch (win.step) {
            case 0:
                win.scan = maker.createObject(win)
                win.camera = pageStack.push(Qt.resolvedUrl("../../qml/pages/ScanPage.qml"),
                                            { "parentUri": "lautta://user-documents/", "owner": owner, "scan": win.scan })
                win.next()
                break
            case 1:
                if (!win.onTop("photoTaken"))
                    break
                if (win.camera.allowedOrientations !== Orientation.Portrait)
                    return win.fail("portrait only")
                if (win.camera.pages !== 0 || named(win.camera, "flyer").visible)
                    return win.fail("live and empty at first")
                if (named(win.camera, "still") || named(win.camera, "adjustButton") || named(win.camera, "retakeButton"))
                    return win.fail("no still after a shot: adjust and retake are in the pages")
                win.camera.photoTaken(win.sheet)
                if (!win.camera.working)
                    return win.fail("busy while the photo is worked on")
                win.next()
                break
            case 2:
                var flyer = named(win.camera, "flyer")
                if (win.camera.working || win.camera.pages !== 1 || flyer.source == "" || flyer.visible)
                    break
                if (flyer.source != "file://" + info(0).view || !info(0).found)
                    return win.fail("cut-out page flew: " + flyer.source)
                var button = named(win.camera, "pagesButton")
                var end = button.mapToItem(win.camera, 0, 0)
                if (flyer.x !== end.x || flyer.y !== end.y || flyer.width !== button.width || flyer.height !== button.height)
                    return win.fail("flight ends on the pages button: " + [flyer.x, flyer.y, flyer.width, flyer.height])
                if (win.camera.backNavigation)
                    return win.fail("pages taken: no swipe drops them")
                if (named(win.camera, "pageCount").text !== "1")
                    return win.fail("count says 1: " + named(win.camera, "pageCount").text)
                win.camera.photoTaken(win.sheet)
                win.next()
                break
            case 3:
                if (win.camera.working || win.camera.pages !== 2 || named(win.camera, "flyer").visible)
                    break
                if (named(win.camera, "flyer").source != "file://" + info(1).view || !info(1).found)
                    return win.fail("second page flew: " + named(win.camera, "flyer").source)
                if (named(win.camera, "pageCount").text !== "2" || info(0).id !== 1 || info(1).id !== 2)
                    return win.fail("second page: " + win.camera.pages)
                win.camera.showPages()
                win.next()
                break
            case 4:
                if (!win.pageView) {
                    win.pageView = win.onTop("colourText")
                    if (!win.pageView)
                        break
                    if (win.pageView.scan !== win.scan || win.pageView.current !== 1)
                        return win.fail("pages open at the last one: " + win.pageView.current)
                    named(win.pageView, "pagedView").currentIndex = 0
                }
                if (win.pageView.current !== 0)
                    break
                win.pageView.retake()
                win.next()
                break
            case 5:
                win.retaker = win.onTop("photoTaken")
                if (!win.retaker)
                    break
                if (win.retaker === win.camera || !win.retaker.retakeOnly || win.retaker.retakeIndex !== 0)
                    return win.fail("a camera page for the one retake")
                if (!win.retaker.backNavigation)
                    return win.fail("retake can be swiped away")
                if (named(win.retaker, "pagesButton").visible || named(win.retaker, "doneButton").visible)
                    return win.fail("retake offers neither pages nor done")
                win.retaker.photoTaken(win.plain)
                win.next()
                break
            case 6:
                // Photo in: back to the page it was for.
                if (pageStack.busy || pageStack.currentPage !== win.pageView)
                    break
                if (win.scan.count !== 2 || info(0).found || info(0).id !== 1 || !info(1).found || win.pageView.current !== 0)
                    return win.fail("first page retaken, only that: " + JSON.stringify([info(0), info(1)]))
                if (win.ended.length !== 0)
                    return win.fail("leaving the retake keeps the scan")
                pageStack.pop()
                win.next()
                break
            case 7:
                if (!win.onTop("photoTaken"))
                    break
                if (pageStack.currentPage !== win.camera || named(win.camera, "flyer").visible)
                    return win.fail("back on the live camera page")
                win.camera.finish()
                win.next()
                break
            case 8:
                var dialog = win.onTop("addPage")
                if (!dialog)
                    break
                if (pageStack.depth !== 2)
                    return win.fail("save dialog takes the camera's place: depth " + pageStack.depth)
                if (win.ended.length !== 0 || win.scan === null || win.scan.count !== 2)
                    return win.fail("scan goes on into the save dialog")
                if (dialog.scan !== win.scan || dialog.owner !== owner || dialog.parentUri !== "lautta://user-documents/")
                    return win.fail("save dialog knows scan, owner and folder")
                dialog.addPage()
                win.next()
                break
            case 9:
                win.camera = win.onTop("photoTaken")
                if (!win.camera)
                    break
                if (win.ended.length !== 0 || win.camera.scan !== win.scan || win.camera.pages !== 2 || win.camera.retakeOnly)
                    return win.fail("plus tile: camera again, same scan")
                // No camera where the tests run: pages taken stay in reach all the same.
                if (!named(win.camera, "pagesButton").visible || !named(win.camera, "doneButton").visible)
                    return win.fail("pages and Done need no camera")
                var finder = named(win.camera, "finderLoader")
                if (!finder.active || win.camera.cameraFailed)
                    return win.fail("viewfinder built once the page shows")
                finder.item.broken()
                if (!win.camera.cameraFailed || finder.active)
                    return win.fail("a camera that fails is let go")
                named(win.camera, "dismissButton").clicked(null)
                win.next()
                break
            case 10:
                if (pageStack.busy || pageStack.depth !== 1 || win.scan !== null)
                    break
                if (JSON.stringify(win.ended) !== "[false]")
                    return win.fail("closing the camera drops the scan, once: " + JSON.stringify(win.ended))
                win.reachedEnd = true
                win.scenarioRunning = false
                break
            }
        }
    }
}
