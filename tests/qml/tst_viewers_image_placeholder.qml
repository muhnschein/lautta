// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/components/viewers/gallery"

ApplicationWindow {
    id: test

    property bool scenarioRunning: true
    property bool reachedEnd
    property var shown
    property int step
    property int waited

    function fail(what) {
        console.error("FAILED: " + what)
        scenarioRunning = false
    }

    function named(item, name) {
        if (!item)
            return null
        if (item.objectName === name)
            return item
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i) {
            var found = named(kids[i], name)
            if (found)
                return found
        }
        return item.contentItem && item.contentItem !== item ? named(item.contentItem, name) : null
    }

    initialPage: Component {
        Page {
            GalleryImage {
                id: gallery

                anchors.fill: parent
                placeholder: Qt.resolvedUrl("host/fixtures/photo.jpg")
                Component.onCompleted: test.shown = gallery
            }
        }
    }

    Timer {
        interval: 100
        repeat: true
        running: test.scenarioRunning
        onTriggered: {
            if (++test.waited > 100)
                return test.fail("step " + test.step + " timed out")
            var placeholder = test.named(test.shown, "placeholderImage")
            var photo = test.named(test.shown, "zoomableImage")
            if (!placeholder || !photo)
                return
            if (test.step === 0) {
                if (placeholder.status !== Image.Ready)
                    return
                if (!placeholder.visible || placeholder.width !== test.shown.width
                        || placeholder.height !== test.shown.height || placeholder.paintedWidth <= 0)
                    return test.fail("photo loading: placeholder fills the view, not "
                                     + placeholder.visible + " " + placeholder.width + "x" + placeholder.height
                                     + " of " + test.shown.width + "x" + test.shown.height)
                test.shown.source = Qt.resolvedUrl("host/fixtures/large-photo.jpg")
                test.step = 1
                test.waited = 0
            } else if (photo.status === Image.Ready && photo.opacity === 1) {
                if (placeholder.visible)
                    return test.fail("photo shown: placeholder goes")
                test.reachedEnd = true
                test.scenarioRunning = false
            }
        }
    }
}
