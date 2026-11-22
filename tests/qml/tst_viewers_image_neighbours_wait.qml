// SPDX-License-Identifier: LGPL-2.1-or-later
// Engine without incubation controller makes the neighbours' slots at once: right after the view
// is made, the shown image is still loading and the neighbours exist. Same view again: shown
// image ready at once (kept in Qt's cache). Broken image shown: neighbours load after its error.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components/viewers/gallery"

ApplicationWindow {
    id: window

    property bool scenarioRunning: true
    property bool reachedEnd
    property bool cleaned
    property int step: 0
    property int waited: 0
    property var view
    property var current
    property var neighbours: []
    readonly property string folder: "lautta://user-pictures/ViewerNeighboursWait/"
    readonly property var opens: [2, 2, 7]
    readonly property var kept: [false, true, false]

    ViewerTools { id: tools }
    DirectoryModel {
        id: pictures
        uri: "lautta://user-pictures/"
    }
    ListModel { id: images }
    Connections {
        target: Operations
        onRemoved: window.cleaned = true
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

    function open(index) {
        if (view)
            view.destroy()
        view = viewComponent.createObject(window.contentItem, { "currentIndex": index })
        current = view.currentItem ? view.currentItem.item.photo : null
        var expected = kept[step] ? Image.Ready : Image.Loading
        if (!current || current.status !== expected)
            return fail("open " + step + ": shown image " + (kept[step] ? "kept" : "loading")
                        + " right after the view is made: " + (current ? current.status : "none"))
        neighbours = photos(view, []).filter(function (p) { return p !== current })
        if (neighbours.length < 2)
            return fail("open " + step + ": neighbours made with the view: " + neighbours.length)
        for (var k = 0; k < neighbours.length && !kept[step]; ++k)
            if (neighbours[k].status !== Image.Null)
                return fail("open " + step + ": neighbour loads before the shown image: status " + neighbours[k].status)
    }

    Component {
        id: viewComponent
        FlickableImageView {
            width: window.width
            height: window.height
            model: images
        }
    }

    Timer {
        interval: 50
        repeat: true
        running: window.scenarioRunning && window.view !== undefined
        onTriggered: {
            if (++window.waited > 400)
                return window.fail("step " + window.step + " timed out: current " + window.current.status)
            if (window.step === window.opens.length) {
                if (!window.cleaned)
                    return
                window.reachedEnd = true
                window.scenarioRunning = false
                return
            }
            for (var i = 0; i < window.neighbours.length; ++i)
                if (window.neighbours[i].status !== Image.Ready)
                    return
            var expected = window.opens[window.step] === 7 ? Image.Error : Image.Ready
            if (window.current.status !== expected)
                return window.fail("open " + window.step + ": shown image " + window.current.status + ", not " + expected)
            window.waited = 0
            if (++window.step < window.opens.length)
                return window.open(window.opens[window.step])
            window.view.destroy()
            Operations.remove(JSON.stringify([window.folder.slice(0, -1)]))
        }
    }

    Component.onCompleted: {
        pictures.createFolder("ViewerNeighboursWait")
        for (var i = 0; i < 200 && pictures.indexOf(pictures.uri + "ViewerNeighboursWait") < 0; ++i)
            tools.pump(50)
        for (var n = 0; n < 8; ++n) {
            var uri = folder + "p" + n + ".jpg"
            var fixture = n === 7 ? "host/fixtures/data.bin" : "host/fixtures/photo.jpg"
            if (!tools.installFixture(Qt.resolvedUrl(fixture), uri))
                return fail("fixture " + uri + " not installed")
            images.append({ "uri": uri })
        }
        open(opens[0])
    }
}
