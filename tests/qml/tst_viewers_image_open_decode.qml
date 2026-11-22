// SPDX-License-Identifier: LGPL-2.1-or-later
// Viewer shows the opened image alone until the folder's listing has it, then makes the folder's
// view: that view takes the picture the first one decoded. Hidden image keeps it alone until the
// test shows hidden files; cache hit is synchronous, so ready the moment the view is made. Its
// neighbour, made while the shown image is already there, loads too.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/viewers"

ApplicationWindow {
    id: window

    property bool scenarioRunning: true
    property bool reachedEnd
    property int step: 0
    property int waited: 0
    property var viewer
    property string firstSource
    property var swapped
    property bool cleaned
    readonly property string folder: "lautta://user-pictures/ViewerOpenDecode/"
    readonly property string opened: folder + ".b.jpg"

    ViewerTools { id: tools }
    Connections {
        target: Operations
        onRemoved: window.cleaned = true
    }
    DirectoryModel {
        id: pictures
        uri: "lautta://user-pictures/"
    }

    function fail(what) {
        console.error("FAILED: " + what)
        scenarioRunning = false
    }

    function photos(item, out) {
        if (item.objectName === "zoomableImage")
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            photos(kids[i], out)
        if (item.contentItem && item.contentItem !== item)
            photos(item.contentItem, out)
        return out
    }

    function neighbour() {
        var found = photos(viewer.slideshow, []).filter(function (p) { return String(p.source).slice(-6) === "/a.jpg" })
        return found.length > 0 ? found[0] : null
    }

    function photo(view) {
        return view && view.currentItem && view.currentItem.item ? view.currentItem.item.photo : null
    }

    Connections {
        target: window.viewer
        ignoreUnknownSignals: true
        onSlideshowChanged: {
            if (window.step !== 1 || !window.viewer.inFolder)
                return
            var shown = window.photo(window.viewer.slideshow)
            window.swapped = { "source": shown ? String(shown.source) : "", "status": shown ? shown.status : -1 }
        }
    }

    Timer {
        interval: 100
        repeat: true
        running: window.scenarioRunning
        onTriggered: {
            if (!window.viewer)
                return
            if (++window.waited > 100)
                return window.fail("step " + window.step + " timed out, in folder " + window.viewer.inFolder)
            var shown = window.photo(window.viewer.slideshow)
            switch (window.step) {
            case 0:
                if (pageStack.busy || !shown || shown.status !== Image.Ready)
                    return
                if (window.viewer.inFolder)
                    return window.fail("hidden image not in the folder's listing yet")
                window.firstSource = String(shown.source)
                App.setSetting("show_hidden", JSON.stringify(true))
                window.cleaned = false
                Operations.remove(JSON.stringify([window.folder + "c.jpg"]))
                break
            case 1:
                if (!window.swapped)
                    return
                if (window.swapped.source !== window.firstSource)
                    return window.fail("folder view shows the opened image: " + window.swapped.source)
                if (window.swapped.status !== Image.Ready)
                    return window.fail("folder view decodes the opened image again: status " + window.swapped.status)
                break
            case 2:
                var next = window.neighbour()
                if (!next || next.status !== Image.Ready)
                    return
                window.cleaned = false
                Operations.remove(JSON.stringify([window.folder.slice(0, -1)]))
                break
            case 3:
                if (!window.cleaned)
                    return
                window.reachedEnd = true
                window.scenarioRunning = false
                break
            }
            window.step++
            window.waited = 0
        }
    }

    Component.onCompleted: {
        App.setSetting("show_hidden", JSON.stringify(false))
        pictures.createFolder("ViewerOpenDecode")
        for (var i = 0; i < 200 && pictures.indexOf(pictures.uri + "ViewerOpenDecode") < 0; ++i)
            tools.pump(50)
        if (!(tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), folder + "a.jpg")
              && tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), opened)
              && tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), folder + "c.jpg")))
            return fail("fixtures not installed")
        viewer = pageStack.push(viewerComponent, {}, PageStackAction.Immediate)
    }

    Component {
        id: viewerComponent
        ImageViewer { uri: window.opened; folderUri: window.folder }
    }
}
