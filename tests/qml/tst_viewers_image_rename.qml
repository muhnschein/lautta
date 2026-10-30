// SPDX-License-Identifier: LGPL-2.1-or-later
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
    readonly property string folder: "lautta://user-pictures/ViewerRename/"

    ViewerTools { id: tools }
    Connections {
        target: Operations
        onRemoved: window.cleaned = true
    }
    Connections {
        target: window.viewer
        ignoreUnknownSignals: true
        onCurrentUriChanged: window.expect(window.viewer.currentUri !== window.folder + "a.jpg",
                                           "showed the other image: step " + window.step)
    }
    DirectoryModel {
        id: listing
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

    function overlayButton(name) {
        var found = findAll(viewer, function(x) { return x.objectName === name }, [])
        return found.length > 0 ? found[0] : null
    }

    Timer {
        interval: 100
        repeat: true
        running: window.scenarioRunning
        onTriggered: {
            if (!window.viewer)
                return
            if (++window.waited > 100)
                return window.fail("step " + window.step + " timed out: " + pageStack.depth + " pages")
            var top = pageStack.currentPage
            switch (window.step) {
            case 0:
                if (!window.viewer.inFolder || pageStack.busy)
                    return
                var more = window.overlayButton("moreButton")
                if (!window.expect(more !== null, "the overlay has More"))
                    return
                more.clicked(null)
                break
            case 1:
                if (pageStack.busy || top === window.viewer)
                    return
                if (!window.expect(top.choose !== undefined, "More opened its page"))
                    return
                top.choose("rename")
                break
            case 2:
                if (pageStack.busy || top.newName === undefined)
                    return
                window.expect(pageStack.depth === 2, "the dialog replaced the More page: " + pageStack.depth + " pages")
                window.expect(top.oldName === "before.jpg", "renaming the image shown: " + top.oldName)
                var field = window.findAll(top, function(x) { return x.errorHighlight !== undefined && x.visible }, [])
                if (!window.expect(field.length === 1, "the dialog has its name field"))
                    return
                field[0].text = "after.jpg"
                if (!window.expect(top.canAccept, "the new name can be accepted"))
                    return
                top.accept()
                break
            case 3:
                if (pageStack.busy || top !== window.viewer || window.viewer.currentUri !== window.folder + "after.jpg"
                        || !window.viewer.inFolder)
                    return
                window.expect(window.viewer.total === 2, "two images in the folder: " + window.viewer.total)
                listing.uri = window.folder
                break
            case 4:
                if (listing.loading || listing.count === 0)
                    return
                window.expect(listing.indexOf(window.folder + "after.jpg") >= 0, "the file has its new name")
                window.expect(listing.indexOf(window.folder + "before.jpg") < 0, "the old name is gone")
                window.cleaned = false
                Operations.remove(JSON.stringify([window.folder.slice(0, -1)]))
                break
            case 5:
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
        listing.createFolder("ViewerRename")
        tools.pump(500)
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), folder + "a.jpg")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), folder + "before.jpg")
        viewer = pageStack.push(viewerComponent, {}, PageStackAction.Immediate)
    }

    Component {
        id: viewerComponent
        ImageViewer { uri: window.folder + "before.jpg"; folderUri: window.folder }
    }
}
