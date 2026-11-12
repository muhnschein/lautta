// SPDX-License-Identifier: LGPL-2.1-or-later Add from the folder pulley: a choice of folder or
// scan, nothing preset; scan opens the camera page and a scan left there is dropped.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: win

    readonly property string folder: "lautta://user-documents/"
    property bool scenarioRunning: true
    property bool reachedEnd
    property int step: 0
    property int waited: 0
    property var folderPage
    property var view
    property var dialog
    property var camera
    // Null once the scan is destroyed.
    property QtObject scan

    DirectoryModel {
        id: docs
        uri: win.folder
    }

    initialPage: Component {
        DirectoryPage {
            uri: win.folder
        }
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

    function text(got, english, id) {
        return got === english || got === id
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
            if (win.waited > 100)
                return win.fail("step " + win.step + " timed out")
            var folderSwitch, scanSwitch
            switch (win.step) {
            case 0:
                win.folderPage = pageStack.currentPage
                var views = findAll(win.contentItem, function(x) { return x.add !== undefined && x.endScan !== undefined }, [])
                if (views.length === 0 || views[0].model.loading)
                    break
                win.view = views[0]
                win.view.add()
                win.next()
                break
            case 1:
                win.dialog = win.onTop("choice")
                if (!win.dialog)
                    break
                folderSwitch = named(win.dialog, "folderSwitch")
                scanSwitch = named(win.dialog, "scanSwitch")
                if (!folderSwitch || !scanSwitch)
                    return win.fail("two choices")
                if (!text(folderSwitch.text, "New folder", "lautta-add-folder") || !text(scanSwitch.text, "Scan document", "lautta-add-scan"))
                    return win.fail("choices named: " + folderSwitch.text + ", " + scanSwitch.text)
                if (win.dialog.choice !== "" || folderSwitch.checked || scanSwitch.checked || win.dialog.canAccept)
                    return win.fail("nothing preset")
                scanSwitch.clicked(null)
                if (win.dialog.choice !== "scan" || !scanSwitch.checked || folderSwitch.checked || !win.dialog.canAccept)
                    return win.fail("scan picked")
                folderSwitch.clicked(null)
                if (win.dialog.choice !== "folder" || scanSwitch.checked || !folderSwitch.checked || !win.dialog.canAccept)
                    return win.fail("folder picked instead: one at a time")
                folderSwitch.clicked(null)
                if (win.dialog.choice !== "" || folderSwitch.checked || win.dialog.canAccept)
                    return win.fail("tapped again: no choice")
                folderSwitch.clicked(null)
                win.next()
                break
            case 2:
                // Silica has the page for the choice ready before it lets the dialog go on.
                if (!win.dialog.acceptDestinationInstance || win.dialog.acceptDestinationInstance.itemName === undefined)
                    break
                win.dialog.accept()
                win.next()
                break
            case 3:
                win.dialog = win.onTop("itemName")
                if (!win.dialog)
                    break
                if (pageStack.depth !== 2)
                    return win.fail("name dialog takes the choice's place: depth " + pageStack.depth)
                var fields = findAll(win.dialog, function(x) { return x.errorHighlight !== undefined && x.visible }, [])
                if (fields.length !== 1)
                    return win.fail("one name field: " + fields.length)
                fields[0].text = "From add"
                win.next()
                break
            case 4:
                if (!win.dialog.canAccept)
                    break
                win.dialog.accept()
                win.next()
                break
            case 5:
                if (win.waited % 5 === 0)
                    docs.refresh()
                if (docs.nameKind("From add") !== "folder" || pageStack.busy || pageStack.currentPage !== win.folderPage)
                    break
                win.view.add()
                win.next()
                break
            case 6:
                win.dialog = win.onTop("choice")
                if (!win.dialog)
                    break
                named(win.dialog, "scanSwitch").clicked(null)
                win.next()
                break
            case 7:
                if (!win.dialog.acceptDestinationInstance || win.dialog.acceptDestinationInstance.photoTaken === undefined)
                    break
                win.dialog.accept()
                win.next()
                break
            case 8:
                win.camera = win.onTop("photoTaken")
                if (!win.camera)
                    break
                if (pageStack.depth !== 2)
                    return win.fail("camera takes the choice's place: depth " + pageStack.depth)
                win.scan = win.camera.scan
                if (!win.scan || win.camera.owner !== win.view || win.camera.parentUri !== win.folder)
                    return win.fail("camera page knows its scan, folder view and folder")
                if (win.scan.count !== 0 || win.camera.retakeOnly)
                    return win.fail("a new, empty scan")
                // No camera where the tests run.
                var placeholder = named(win.camera, "cameraPlaceholder")
                if (!placeholder || !placeholder.enabled
                        || !text(placeholder.text, "Camera not available", "lautta-scan-no-camera")
                        || !text(placeholder.hintText, "Scanning needs the camera", "lautta-scan-no-camera-hint"))
                    return win.fail("without a camera the page says so")
                if (named(win.camera, "finderLoader").active)
                    return win.fail("no viewfinder without a camera")
                if (named(win.camera, "shutterButton").visible || named(win.camera, "flashButton").visible)
                    return win.fail("no camera controls without a camera")
                if (!win.camera.backNavigation)
                    return win.fail("nothing scanned: page can be swiped away")
                named(win.camera, "dismissButton").clicked(null)
                win.next()
                break
            case 9:
                if (pageStack.busy || pageStack.currentPage !== win.folderPage)
                    break
                if (win.scan !== null)
                    break
                win.reachedEnd = true
                win.scenarioRunning = false
                break
            }
        }
    }
}
