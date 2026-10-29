// SPDX-License-Identifier: LGPL-2.1-or-later
// The image viewer's Delete on the target (PRV-4, OPS-8, real Silica and
// Lautta types): the overlay's Delete calls remove(); once its remorse has
// run, the image the viewer opened with is gone and the viewer shows its
// neighbour in the folder, not the deleted file on its own.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/viewers"

ApplicationWindow {
    id: window

    property bool scenarioRunning: true
    property int step: 0
    property int waited: 0
    property var viewer
    property bool cleaned
    readonly property string folder: "lautta://user-pictures/ViewerNeighbour/"

    ViewerTools { id: tools }
    Connections {
        target: Operations
        onRemoved: window.cleaned = true
    }
    // Makes the test's folder.
    DirectoryModel {
        id: pictures
        uri: "lautta://user-pictures/"
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

    function first(pred) {
        var found = findAll(viewer, pred, [])
        return found.length > 0 ? found[0] : null
    }

    function remorse() {
        return first(function(x) { return x.execute !== undefined && x.cancel !== undefined && x.active !== undefined })
    }

    Timer {
        interval: 100
        repeat: true
        running: window.scenarioRunning
        onTriggered: {
            if (!window.viewer)
                return
            if (++window.waited > 100)
                return window.fail("step " + window.step + " timed out on " + window.viewer.currentUri)
            switch (window.step) {
            case 0:
                if (!window.viewer.inFolder || window.viewer.currentUri !== window.folder + "a.jpg")
                    return
                window.expect(window.viewer.total === 2, "two images: " + window.viewer.total)
                var remove = window.first(function(x) { return x.objectName === "deleteButton" })
                if (!window.expect(remove !== null, "the overlay has Delete"))
                    return
                remove.clicked(null)
                break
            case 1:
                var popup = window.remorse()
                if (!window.expect(popup !== null && popup.active, "Delete runs remove(): its remorse shows"))
                    return
                popup.trigger()
                break
            case 2:
                // Deleted and listed again: on the neighbour, in the folder.
                if (window.viewer.total !== 1 || window.viewer.currentUri !== window.folder + "b.jpg")
                    return
                window.expect(window.viewer.inFolder, "still on the folder's listing")
                var view = window.first(function(x) { return x.objectName === "flickableView" })
                var shown = view && view.currentItem ? view.currentItem.item : null
                window.expect(shown !== null && String(shown.source).indexOf("b.jpg") >= 0,
                              "the neighbour is shown: " + (shown ? shown.source : "nothing"))
                // Leave Pictures as it was for the scenarios after this one.
                window.cleaned = false
                Operations.remove(JSON.stringify([window.folder.slice(0, -1)]))
                break
            case 3:
                if (!window.cleaned)
                    return
                window.scenarioRunning = false
                break
            }
            window.step++
            window.waited = 0
        }
    }

    Component.onCompleted: {
        pictures.createFolder("ViewerNeighbour")
        tools.pump(500)
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), folder + "a.jpg")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), folder + "b.jpg")
        viewer = pageStack.push(viewerComponent, {}, PageStackAction.Immediate)
    }

    Component {
        id: viewerComponent
        ImageViewer { uri: window.folder + "a.jpg"; folderUri: window.folder }
    }
}
