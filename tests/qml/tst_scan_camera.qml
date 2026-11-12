// SPDX-License-Identifier: LGPL-2.1-or-later Camera page of a scan, photos handed in as the camera
// would: still with outline, retake, page count, going on to the pages and to saving.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

ApplicationWindow {
    id: win

    readonly property string sheet: Qt.resolvedUrl("host/fixtures/sheet.jpg")
    readonly property string plain: Qt.resolvedUrl("host/fixtures/large-photo.jpg")
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
            if (win.waited > 150)
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
                if (win.camera.stillPage !== null || win.camera.pages !== 0)
                    return win.fail("live and empty at first")
                win.camera.photoTaken(win.sheet)
                if (!win.camera.working)
                    return win.fail("busy while the photo is worked on")
                win.next()
                break
            case 2:
                if (win.camera.working || win.camera.stillPage === null)
                    break
                if (win.camera.pages !== 1 || win.camera.stillIndex !== 0)
                    return win.fail("first page, shown still: " + win.camera.pages + ", " + win.camera.stillIndex)
                var still = named(win.camera, "still")
                var outline = named(win.camera, "stillOutline")
                if (!still.visible || JSON.stringify(outline.corners) !== JSON.stringify(info(0).quad) || !info(0).found)
                    return win.fail("still shows the edges found: " + JSON.stringify(outline.corners))
                if (win.camera.backNavigation)
                    return win.fail("pages taken: no swipe drops them")
                if (named(win.camera, "pageCount").text !== "1")
                    return win.fail("count says 1: " + named(win.camera, "pageCount").text)
                named(win.camera, "retakeButton").clicked(null)
                if (win.camera.stillPage !== null || still.visible || win.camera.retakeIndex !== 0)
                    return win.fail("retake: live again, next shot is for this page")
                win.camera.photoTaken(win.plain)
                win.next()
                break
            case 3:
                if (win.camera.working || win.camera.stillPage === null)
                    break
                if (win.camera.pages !== 1 || win.camera.stillIndex !== 0 || info(0).found || info(0).id !== 1)
                    return win.fail("same page with the new photo: " + JSON.stringify(info(0)))
                if (win.camera.retakeIndex !== -1)
                    return win.fail("next shot is a new page again")
                // Shutter on a still only goes back to live.
                win.camera.shoot()
                if (win.camera.stillPage !== null || win.camera.taking)
                    return win.fail("shutter on a still returns to live")
                win.camera.photoTaken(win.sheet)
                win.next()
                break
            case 4:
                if (win.camera.working || win.camera.stillPage === null)
                    break
                if (win.camera.pages !== 2 || win.camera.stillIndex !== 1 || named(win.camera, "pageCount").text !== "2")
                    return win.fail("second page: " + win.camera.pages)
                named(win.camera, "still").clicked(null)
                if (win.camera.stillPage !== null)
                    return win.fail("tap puts the still away")
                win.camera.showPages()
                win.next()
                break
            case 5:
                win.pageView = win.onTop("colourText")
                if (!win.pageView)
                    break
                if (win.pageView.scan !== win.scan || win.pageView.current !== 1)
                    return win.fail("pages open at the last one: " + win.pageView.current)
                win.pageView.retake()
                win.next()
                break
            case 6:
                win.retaker = win.onTop("photoTaken")
                if (!win.retaker)
                    break
                if (win.retaker === win.camera || !win.retaker.retakeOnly || win.retaker.retakeIndex !== 1)
                    return win.fail("a camera page for the one retake")
                if (!win.retaker.backNavigation)
                    return win.fail("retake can be swiped away")
                if (named(win.retaker, "pagesButton").visible || named(win.retaker, "doneButton").visible)
                    return win.fail("retake offers neither pages nor done")
                win.retaker.photoTaken(win.plain)
                win.next()
                break
            case 7:
                // Photo in: back to the page it was for.
                if (pageStack.busy || pageStack.currentPage !== win.pageView)
                    break
                if (win.scan.count !== 2 || info(1).found || info(1).id !== 2 || win.pageView.current !== 1)
                    return win.fail("second page retaken: " + JSON.stringify(info(1)))
                if (win.ended.length !== 0)
                    return win.fail("leaving the retake keeps the scan")
                pageStack.pop()
                win.next()
                break
            case 8:
                if (!win.onTop("photoTaken"))
                    break
                if (pageStack.currentPage !== win.camera || win.camera.stillPage !== null)
                    return win.fail("back on the live camera page")
                win.camera.finish()
                win.next()
                break
            case 9:
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
            case 10:
                win.camera = win.onTop("photoTaken")
                if (!win.camera)
                    break
                if (win.ended.length !== 0 || win.camera.scan !== win.scan || win.camera.pages !== 2 || win.camera.retakeOnly)
                    return win.fail("plus tile: camera again, same scan")
                named(win.camera, "dismissButton").clicked(null)
                win.next()
                break
            case 11:
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
