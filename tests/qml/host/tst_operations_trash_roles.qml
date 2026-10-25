// SPDX-License-Identifier: LGPL-2.1-or-later
// Host scenario of TrashModel's thumbnail roles (OPS-8, PRV-1): a trashed
// image has the stored copy as thumbnailSource and its type from the
// original name as mimeType, a trashed text file has the type and no
// thumbnail, and the Thumbnails setting (BRW-4) switches the thumbnails off
// at the next reload. The Trash page draws its rows from these roles.
import QtQuick 2.6
import QtQml 2.2
import Lautta 1.0

Item {
    id: test

    readonly property string docs: "lautta://user-documents/"
    property bool scenarioRunning: true
    property int step: 0
    property int waited: 0
    property bool removed
    property var photo
    property var note

    ViewerTools { id: tools }

    TrashModel { id: trash }

    // One object per row, with the roles the page reads.
    Instantiator {
        id: rows

        model: trash
        delegate: QtObject {
            readonly property string title: name
            readonly property int key: trashId
            readonly property string thumb: thumbnailSource
            readonly property string mime: mimeType
        }
    }

    function fail(what) {
        console.error("FAILED: " + what)
        finish()
    }

    function finish() {
        App.setSetting("thumbnails_local", JSON.stringify(true))
        for (var i = 0; i < rows.count; ++i) {
            var row = rows.objectAt(i)
            if (row.title === "trash-roles.jpg" || row.title === "trash-roles.md")
                trash.remove(row.key)
        }
        scenarioRunning = false
    }

    function rowNamed(title) {
        for (var i = 0; i < rows.count; ++i) {
            if (rows.objectAt(i).title === title)
                return rows.objectAt(i)
        }
        return undefined
    }

    function next() {
        step++
        waited = 0
    }

    Component.onCompleted: {
        Operations.removed.connect(function() { test.removed = true })
        Operations.failed.connect(function(kind) { test.fail("the delete failed: " + kind) })
    }

    Timer {
        interval: 50
        repeat: true
        running: test.scenarioRunning
        onTriggered: {
            test.waited++
            if (test.waited > 400)
                return test.fail("step " + test.step + " timed out")
            switch (test.step) {
            case 0:
                if (!tools.installFixture(Qt.resolvedUrl("fixtures/photo.jpg"), test.docs + "trash-roles.jpg")
                        || !tools.installFixture(Qt.resolvedUrl("fixtures/doc.md"), test.docs + "trash-roles.md"))
                    return test.fail("fixtures not installed")
                // The Trash is a device-folder feature: skip where the test
                // home is not a registered folder (under the SDK's sb2).
                if (App.locationName(test.docs) !== "Documents")
                    return test.finish()
                Operations.remove(JSON.stringify([test.docs + "trash-roles.jpg", test.docs + "trash-roles.md"]))
                test.next()
                break
            case 1:
                if (!test.removed)
                    break
                trash.reload()
                test.next()
                break
            case 2:
                test.photo = test.rowNamed("trash-roles.jpg")
                test.note = test.rowNamed("trash-roles.md")
                if (!test.photo || !test.note)
                    break
                // The stored copy has no extension: the type is the original's.
                if (test.photo.mime !== "image/jpeg")
                    return test.fail("the image's type: " + test.photo.mime)
                if (test.photo.thumb.indexOf("file://") !== 0 || test.photo.thumb.indexOf("trash-roles") >= 0)
                    return test.fail("the image's thumbnail is its stored copy: " + test.photo.thumb)
                if (test.note.mime !== "text/markdown" || test.note.thumb !== "")
                    return test.fail("a text file has a type and no thumbnail: " + test.note.mime + ", " + test.note.thumb)
                if (!App.setSetting("thumbnails_local", JSON.stringify(false)))
                    return test.fail("thumbnails_local not set")
                trash.reload()
                test.next()
                break
            case 3:
                // The reload replaces the rows: wait for the image's to be
                // the one without a thumbnail.
                var again = test.rowNamed("trash-roles.jpg")
                if (!again || again.thumb !== "")
                    break
                if (again.mime !== "image/jpeg")
                    return test.fail("the type stays with thumbnails off: " + again.mime)
                test.finish()
                break
            }
        }
    }
}
