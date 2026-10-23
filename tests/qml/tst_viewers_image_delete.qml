// SPDX-License-Identifier: LGPL-2.1-or-later
// Scenario on the target (real Silica): Delete in the image viewer asks the
// same as the folder view (OPS-8). To the Trash it is the remorse popup,
// "Moving to trash", and no dialog. A permanent delete (here with the Trash
// off; on volumes and servers likewise) opens the Delete permanently? dialog
// instead, with no remorse: accepting it deletes the image at once, keeps the
// viewer while other images are left and leaves it with the last one.
// Before, an SD card's image said "Deleting" although it went for good.
// (Headless, the slideshow does not follow a reload, so each part starts
// from the image the viewer opened with.)
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

ApplicationWindow {
    id: win

    readonly property string folder: "lautta://user-pictures/DeleteTest/"
    readonly property string first: folder + "one.jpg"
    readonly property string second: folder + "two.jpg"
    property bool scenarioRunning: true
    property int step: 0
    property int waited: 0
    property bool removed
    property var below
    property var viewer
    property var dialog

    ViewerTools { id: tools }

    DirectoryModel {
        id: pictures
        uri: "lautta://user-pictures/"
    }

    DirectoryModel {
        id: images
        uri: win.folder
    }

    Connections {
        target: Operations
        onRemoved: win.removed = true
    }

    initialPage: Component {
        Page { }
    }

    function finish(what) {
        App.setSetting("recently_deleted", JSON.stringify(true))
        App.setSetting("remorse_seconds", JSON.stringify(5))
        if (what)
            console.error("FAILED: " + what)
        scenarioRunning = false
    }

    function fail(what) {
        finish(what)
    }

    // A flickable's content item is one of its children too: each item once.
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

    function remorse() {
        var found = findAll(win.contentItem, function(x) { return x.execute !== undefined && x.pending !== undefined }, [])
        return found.length > 0 ? found[0] : null
    }

    function dialogOnTop() {
        var page = pageStack.currentPage
        return page && page.uris !== undefined && page.status === PageStatus.Active ? page : null
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
                return win.fail("step " + win.step + " timed out: " + JSON.stringify({
                    "total": win.viewer ? win.viewer.total : -1,
                    "current": win.viewer ? win.viewer.currentUri : "",
                    "files": [images.nameKind("one.jpg"), images.nameKind("two.jpg")],
                    "removed": win.removed, "depth": pageStack.depth }))
            switch (win.step) {
            case 0:
                if (pictures.loading || win.waited < 3)
                    break
                if (pictures.nameKind("DeleteTest") !== "folder")
                    pictures.createFolder("DeleteTest")
                win.next()
                break
            case 1:
                if (win.waited % 5 === 0)
                    pictures.refresh()
                if (pictures.nameKind("DeleteTest") !== "folder")
                    break
                if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), win.first)
                        || !tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), win.second))
                    return win.fail("fixtures")
                App.setSetting("remorse_seconds", JSON.stringify(3))
                win.below = pageStack.currentPage
                win.viewer = pageStack.push(Qt.resolvedUrl("../../qml/viewers/ImageViewer.qml"),
                                            { "uri": win.first, "folderUri": win.folder })
                win.next()
                break
            case 2:
                if (win.viewer.total !== 2 || win.viewer.status !== PageStatus.Active)
                    break
                win.viewer.remove()
                win.next()
                break
            case 3:
                // To the Trash: the remorse popup, no dialog.
                if (win.dialogOnTop())
                    return win.fail("a delete to the Trash asks nothing")
                var popup = win.remorse()
                if (!popup || !popup.pending)
                    break
                if (popup.text !== "Moving to trash" && popup.text !== "lautta-viewers-trashing")
                    return win.fail("the remorse says where the image goes: " + popup.text)
                popup.cancel()
                App.setSetting("recently_deleted", JSON.stringify(false))
                win.next()
                break
            case 4:
                // Longer than the remorse timer: the cancelled delete is gone.
                if (win.waited < 40)
                    break
                images.refresh()
                win.next()
                break
            case 5:
                if (images.loading || win.waited < 3)
                    break
                if (images.nameKind("one.jpg") !== "file")
                    return win.fail("a cancelled remorse deleted the image")
                win.removed = false
                win.viewer.remove()
                win.next()
                break
            case 6:
                win.dialog = win.dialogOnTop()
                if (!win.dialog)
                    break
                var popup6 = win.remorse()
                if (popup6 && popup6.pending)
                    return win.fail("no remorse with the dialog")
                if (win.dialog.uris.length !== 1 || win.dialog.uris[0] !== win.first)
                    return win.fail("the dialog names the image: " + JSON.stringify(win.dialog.uris))
                win.dialog.accept()
                win.next()
                break
            case 7:
                // Deleted at once; another image is left, so the viewer stays.
                if (!win.removed || pageStack.busy)
                    break
                if (pageStack.currentPage !== win.viewer)
                    return win.fail("the viewer stays while images are left")
                if (win.waited > 20)
                    return win.fail("deleted only after a delay")
                images.refresh()
                win.next()
                break
            case 8:
                if (images.loading || win.waited < 3)
                    break
                if (images.nameKind("one.jpg") !== "" || images.nameKind("two.jpg") !== "file")
                    return win.fail("only the first image is deleted")
                pageStack.pop()
                win.next()
                break
            case 9:
                if (pageStack.busy || pageStack.depth !== 1)
                    break
                win.viewer = pageStack.push(Qt.resolvedUrl("../../qml/viewers/ImageViewer.qml"),
                                            { "uri": win.second, "folderUri": win.folder })
                win.next()
                break
            case 10:
                if (win.viewer.total !== 1 || win.viewer.status !== PageStatus.Active)
                    break
                win.removed = false
                win.viewer.remove()
                win.next()
                break
            case 11:
                win.dialog = win.dialogOnTop()
                if (!win.dialog)
                    break
                if (win.dialog.uris.length !== 1 || win.dialog.uris[0] !== win.second)
                    return win.fail("the dialog names the last image: " + JSON.stringify(win.dialog.uris))
                win.dialog.accept()
                win.next()
                break
            case 12:
                // The last image is gone: back past the viewer at once.
                if (!win.removed || pageStack.currentPage !== win.below || pageStack.busy)
                    break
                if (pageStack.depth !== 1)
                    return win.fail("the viewer is left: depth " + pageStack.depth)
                win.next()
                break
            case 13:
                if (win.waited % 5 === 0)
                    images.refresh()
                if (images.loading || images.nameKind("two.jpg") !== "")
                    break
                // Leave Pictures as it was for the scenarios after this one.
                win.removed = false
                Operations.remove(JSON.stringify([win.folder.slice(0, -1)]))
                win.next()
                break
            case 14:
                if (!win.removed)
                    break
                win.next()
                break
            case 15:
                if (win.waited % 5 === 0)
                    pictures.refresh()
                if (pictures.nameKind("DeleteTest") === "")
                    win.finish("")
                break
            }
        }
    }
}
