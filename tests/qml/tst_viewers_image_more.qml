// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

ApplicationWindow {
    id: win

    readonly property string file: "lautta://user-documents/ImageMore/photo.jpg"
    property bool scenarioRunning: true
    property int step: 0
    property int waited: 0
    property bool made

    ViewerTools { id: tools }

    DirectoryModel {
        id: documents

        uri: "lautta://user-documents/"
        onCreated: win.made = true
        onCreateFailed: win.made = true
    }

    initialPage: Component {
        Page { }
    }

    function fail(what) {
        console.error("FAILED: " + what)
        scenarioRunning = false
    }

    Timer {
        interval: 100
        repeat: true
        running: win.scenarioRunning
        onTriggered: {
            win.waited++
            if (win.waited > 100)
                return win.fail("step " + win.step + " timed out")
            var page = pageStack.currentPage
            switch (win.step) {
            case 0:
                documents.createFolder("ImageMore")
                win.step++
                break
            case 1:
                if (!win.made)
                    break
                if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), win.file))
                    return win.fail("fixture")
                pageStack.push(Qt.resolvedUrl("../../qml/components/viewers/ImageMorePage.qml"), { "uri": win.file })
                win.step++
                break
            case 2:
                if (pageStack.busy || !page || page.choose === undefined || page.status !== PageStatus.Active)
                    break
                page.choose("compress")
                win.step++
                break
            case 3:
                if (pageStack.busy || !page || page.formats === undefined)
                    break
                if (JSON.stringify(page.uris) !== JSON.stringify([win.file]))
                    return win.fail("Compress gets the image: " + JSON.stringify(page.uris))
                if (!page.canAccept)
                    return win.fail("the compress dialog can be accepted: into " + page.destUri)
                if (pageStack.depth !== 2)
                    return win.fail("the dialog takes More's place: depth " + pageStack.depth)
                win.scenarioRunning = false
                break
            }
        }
    }
}
