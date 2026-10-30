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
    readonly property var view: viewer ? viewer.slideshow : null
    onViewChanged: {
        if (step === 0 && view && view.moving)
            fail("new view slides, to index " + view.currentIndex)
    }
    property bool cleaned
    readonly property string folder: "lautta://user-pictures/ViewerOpenIndex/"
    readonly property string opened: folder + "c.jpg"

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

    function expect(ok, what) {
        if (!ok)
            fail(what)
        return ok
    }

    Connections {
        target: window.viewer
        ignoreUnknownSignals: true
        onCurrentUriChanged: {
            if (window.step === 0)
                window.expect(window.viewer.currentUri === window.opened, "showed another image first: " + window.viewer.currentUri)
        }
    }
    Connections {
        target: window.view
        ignoreUnknownSignals: true
        onMovingChanged: {
            if (window.step === 0 && window.view.moving)
                window.fail("slid while opening, to index " + window.view.currentIndex)
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
                return window.fail("step " + window.step + " timed out on " + window.viewer.currentUri)
            switch (window.step) {
            case 0:
                if (!window.viewer.inFolder || pageStack.busy)
                    return
                window.expect(window.viewer.total === 3, "three images: " + window.viewer.total)
                window.expect(window.view.currentIndex === 2 && window.viewer.currentUri === window.opened,
                              "on the opened image: " + window.view.currentIndex + " " + window.viewer.currentUri)
                window.expect(!window.view.moving, "settled")
                window.cleaned = false
                Operations.remove(JSON.stringify([window.folder.slice(0, -1)]))
                break
            case 1:
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
        pictures.createFolder("ViewerOpenIndex")
        tools.pump(500)
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), folder + "a.jpg")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), folder + "b.jpg")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), opened)
        viewer = pageStack.push(viewerComponent)
    }

    Component {
        id: viewerComponent
        ImageViewer { uri: window.opened; folderUri: window.folder }
    }
}
