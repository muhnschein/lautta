// SPDX-License-Identifier: LGPL-2.1-or-later
// Scenario on the target (real Silica): the Trash page draws a trashed image
// with the thumbnail of its stored copy and its type from the original name
// (OPS-8, PRV-1), a trashed text file with its glyph, and no thumbnail at all
// once the Thumbnails setting is off (BRW-4). Whether the system thumbnailer
// makes the picture, and how it looks, only a phone shows.
import QtQuick 2.6
import QtQml 2.2
import Sailfish.Silica 1.0
import Lautta 1.0

ApplicationWindow {
    id: win

    readonly property string folder: "lautta://user-documents/"
    readonly property string photo: "trash-thumb.jpg"
    readonly property string note: "trash-thumb.md"
    property bool scenarioRunning: true
    property int step: 0
    property int waited: 0
    property bool removed
    property var page

    ViewerTools { id: tools }

    // The ids of the rows, to clean up with.
    TrashModel { id: ids }

    Instantiator {
        id: stored

        model: ids
        delegate: QtObject {
            readonly property string title: name
            readonly property int key: trashId
        }
    }

    Connections {
        target: Operations
        onRemoved: win.removed = true
        onFailed: win.fail("the delete failed: " + kind)
    }

    initialPage: Component {
        Page { }
    }

    function fail(what) {
        console.error("FAILED: " + what)
        finish()
    }

    function finish() {
        App.setSetting("thumbnails_local", JSON.stringify(true))
        for (var i = 0; i < stored.count; ++i) {
            var row = stored.objectAt(i)
            if (row.title === photo || row.title === note)
                ids.remove(row.key)
        }
        scenarioRunning = false
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

    // The delegate showing `title`, undefined while it is not there.
    function rowOf(title) {
        var names = findAll(page, function(x) { return x.text === title && x.truncationMode !== undefined }, [])
        var row = names.length === 1 ? names[0] : undefined
        while (row && row.openMenu === undefined)
            row = row.parent
        return row
    }

    // What the row's icon is made of: the thumbnail icon and the glyph.
    function iconsOf(row) {
        return {
            "thumbnail": findAll(row, function(x) { return x.thumbnailSource !== undefined && x.thumbnailReady !== undefined }, [])[0],
            "glyph": findAll(row, function(x) { return x.themeIcon !== undefined }, [])[0]
        }
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
            switch (win.step) {
            case 0:
                if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), win.folder + win.photo)
                        || !tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), win.folder + win.note))
                    return win.fail("fixtures")
                Operations.remove(JSON.stringify([win.folder + win.photo, win.folder + win.note]))
                win.next()
                break
            case 1:
                if (!win.removed)
                    break
                ids.reload()
                win.page = pageStack.push(Qt.resolvedUrl("../../qml/pages/TrashPage.qml"))
                win.next()
                break
            case 2:
                var views = win.findAll(win.page, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
                if (views.length !== 1 || !views[0].model.loaded || views[0].model.busy)
                    break
                views[0].forceLayout()
                var photoRow = win.rowOf(win.photo)
                var noteRow = win.rowOf(win.note)
                if (!photoRow || !noteRow)
                    break
                var picture = win.iconsOf(photoRow)
                if (!picture.thumbnail || !picture.thumbnail.visible || picture.glyph.visible)
                    return win.fail("the image shows its thumbnail icon, not the glyph")
                if (picture.thumbnail.thumbnailSource.indexOf("file://") !== 0 || picture.thumbnail.thumbnailSource.indexOf("trash-thumb") >= 0)
                    return win.fail("the thumbnail is the stored copy: " + picture.thumbnail.thumbnailSource)
                if (picture.thumbnail.mimeType !== "image/jpeg")
                    return win.fail("the type comes from the original name: " + picture.thumbnail.mimeType)
                var text = win.iconsOf(noteRow)
                if (text.thumbnail.visible || !text.glyph.visible)
                    return win.fail("a text file keeps its glyph")
                App.setSetting("thumbnails_local", JSON.stringify(false))
                views[0].model.reload()
                win.next()
                break
            case 3:
                // The reload replaces the rows: wait for the image's new row.
                var again = win.rowOf(win.photo)
                if (!again)
                    break
                var shown = win.iconsOf(again)
                if (shown.thumbnail.thumbnailSource !== "") {
                    if (win.waited > 50)
                        return win.fail("with thumbnails off the image still has a thumbnail source: " + shown.thumbnail.thumbnailSource)
                    break
                }
                if (shown.thumbnail.visible || !shown.glyph.visible)
                    return win.fail("with thumbnails off the image keeps its glyph: " + shown.thumbnail.thumbnailSource)
                win.finish()
                break
            }
        }
    }
}
