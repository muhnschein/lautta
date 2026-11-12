// SPDX-License-Identifier: LGPL-2.1-or-later Pages of a scan one at a time: place, colour, turn,
// remove behind a remorse, adjust.
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
    property var pageView
    property var before
    property QtObject scan

    Component {
        id: maker

        ScanModel { }
    }

    initialPage: Component {
        Page { }
    }

    function finish(what) {
        App.setSetting("remorse_seconds", JSON.stringify(5))
        if (what)
            console.error("FAILED: " + what)
        scenarioRunning = false
    }

    function fail(what) {
        finish(what)
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

    function remorse() {
        var found = findAll(win.contentItem, function(x) { return x.execute !== undefined && x.pending !== undefined }, [])
        return found.length > 0 ? found[0] : null
    }

    function info(index) {
        return JSON.parse(scan.pageJson(index))
    }

    // Without a catalogue the text is its id.
    function place(of, total) {
        var text = named(win.pageView, "placeLabel").text
        return text === of + " of " + total || text === "lautta-scan-page-of"
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
            var popup
            switch (win.step) {
            case 0:
                win.scan = maker.createObject(win)
                win.scan.add(win.sheet)
                win.scan.add(win.plain)
                win.scan.add(win.sheet)
                win.next()
                break
            case 1:
                if (win.scan.busy)
                    break
                // Longer than a step may take: a remorse that runs out by itself fails the test.
                App.setSetting("remorse_seconds", JSON.stringify(60))
                win.pageView = pageStack.push(Qt.resolvedUrl("../../qml/pages/ScanPageView.qml"), { "scan": win.scan, "index": 1 })
                win.next()
                break
            case 2:
                if (!win.onTop("colourText") || !named(win.pageView, "pagedView").currentItem)
                    break
                if (win.pageView.current !== 1 || win.pageView.pages !== 3 || !place(2, 3))
                    return win.fail("opens at the page asked for: " + win.pageView.current)
                if (named(win.pageView, "pagedView").currentItem.pageId !== 2)
                    return win.fail("shows the second page")
                if (win.pageView.colour !== "original" || !named(win.pageView, "colour-original").highlighted
                        || named(win.pageView, "colour-grayscale").highlighted || named(win.pageView, "colour-blackwhite").highlighted)
                    return win.fail("colour line marks Original")
                named(win.pageView, "colour-blackwhite").clicked(null)
                win.next()
                break
            case 3:
                if (win.scan.busy || win.pageView.colour !== "blackwhite")
                    break
                if (info(1).colour !== "blackwhite" || info(0).colour !== "original" || info(2).colour !== "original")
                    return win.fail("colour of the page shown only")
                if (!named(win.pageView, "colour-blackwhite").highlighted || named(win.pageView, "colour-original").highlighted)
                    return win.fail("colour line follows")
                named(win.pageView, "rotateButton").clicked(null)
                win.next()
                break
            case 4:
                if (win.scan.busy)
                    break
                if (info(1).turns !== 1 || info(0).turns !== 0 || info(2).turns !== 0)
                    return win.fail("page shown turned a quarter right: " + info(1).turns)
                named(win.pageView, "removeButton").clicked(null)
                win.next()
                break
            case 5:
                popup = win.remorse()
                if (!popup || !popup.pending)
                    break
                if (popup.text !== "Removing page" && popup.text !== "lautta-scan-removing-page")
                    return win.fail("remorse says what goes: " + popup.text)
                if (named(win.pageView, "overlay").enabled || named(win.pageView, "pagedView").interactive)
                    return win.fail("controls and swipes rest while the remorse runs")
                popup.cancel()
                win.next()
                break
            case 6:
                popup = win.remorse()
                if (popup.pending || !named(win.pageView, "overlay").enabled)
                    break
                if (win.scan.count !== 3)
                    return win.fail("cancelled remorse removes nothing")
                named(win.pageView, "removeButton").clicked(null)
                win.next()
                break
            case 7:
                popup = win.remorse()
                if (!popup || !popup.pending)
                    break
                popup.trigger()
                win.next()
                break
            case 8:
                // The view itself follows a removed page only once drawn: not in this headless run.
                if (win.scan.busy || win.scan.count !== 2)
                    break
                if (info(0).id !== 1 || info(1).id !== 3)
                    return win.fail("second page removed: " + info(0).id + ", " + info(1).id)
                if (pageStack.currentPage !== win.pageView || win.pageView.pages !== 2)
                    return win.fail("pages left: view stays")
                win.before = info(1)
                named(win.pageView, "adjustButton").clicked(null)
                win.next()
                break
            case 9:
                var dialog = win.onTop("corners")
                if (!dialog)
                    break
                if (dialog.scan !== win.scan || dialog.index !== 1)
                    return win.fail("adjust opens for the page shown: " + dialog.index)
                pageStack.pop()
                win.next()
                break
            case 10:
                if (!win.onTop("colourText"))
                    break
                if (JSON.stringify(info(1)) !== JSON.stringify(win.before))
                    return win.fail("adjust left without Done changes nothing")
                named(win.pageView, "removeButton").clicked(null)
                win.next()
                break
            case 11:
                popup = win.remorse()
                if (!popup || !popup.pending)
                    break
                popup.trigger()
                win.next()
                break
            case 12:
                if (win.scan.busy || win.scan.count !== 1)
                    break
                if (info(0).id !== 1 || pageStack.currentPage !== win.pageView)
                    return win.fail("one page left, still shown")
                named(win.pageView, "pagedView").currentIndex = 0
                named(win.pageView, "removeButton").clicked(null)
                win.next()
                break
            case 13:
                popup = win.remorse()
                if (!popup || !popup.pending)
                    break
                popup.trigger()
                win.next()
                break
            case 14:
                // No page left: nothing to show, back to where it came from.
                if (pageStack.busy || pageStack.depth !== 1)
                    break
                if (win.scan.count !== 0)
                    return win.fail("all pages removed")
                win.reachedEnd = true
                win.finish()
                break
            }
        }
    }
}
