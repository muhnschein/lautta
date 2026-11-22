// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

ApplicationWindow {
    id: win

    readonly property string file: "lautta://user-documents/ImageMore/photo.jpg"
    property bool scenarioRunning: true
    property bool reachedEnd
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

    function rowIds(page) {
        var lists = []
        var walk = function (item) {
            if (item.model !== undefined && item.count !== undefined && item.contentItem !== undefined)
                lists.push(item)
            for (var i = 0; i < item.children.length; ++i)
                walk(item.children[i])
        }
        walk(page)
        return lists.length === 1 ? lists[0].model.map(function (row) { return row.id }) : ["lists: " + lists.length]
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
                    return win.fail("the compress dialog can be accepted: into " + page.folderUri)
                if (pageStack.depth !== 2)
                    return win.fail("the dialog takes More's place: depth " + pageStack.depth)
                pageStack.replace(Qt.resolvedUrl("../../qml/components/viewers/ImageMorePage.qml"),
                                  { "uri": win.file, "writable": false })
                win.step++
                break
            case 4:
                if (pageStack.busy || !page || page.choose === undefined || page.status !== PageStatus.Active)
                    break
                var ids = win.rowIds(page)
                if (ids.indexOf("favourite") < 0 || ids.indexOf("compress") >= 0)
                    return win.fail("no Compress where the folder takes no files: " + JSON.stringify(ids))
                win.reachedEnd = true
                win.scenarioRunning = false
                break
            }
        }
    }
}
