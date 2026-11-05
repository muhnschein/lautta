// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components/directory"

ApplicationWindow {
    id: window
    property bool reachedEnd

    ViewerTools { id: tools }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    // No render loop headless: lay out by hand.
    function tiles(grid) {
        var out = []
        grid.forceLayout()
        var rows = grid.contentItem.children
        for (var i = 0; i < rows.length; ++i) {
            var kids = rows[i].children
            for (var j = 0; j < kids.length; ++j) {
                if (kids[j].captions !== undefined && kids[j].itemName !== undefined)
                    out.push(kids[j])
            }
        }
        return out
    }

    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        return expect(done(), "timed out waiting for " + what)
    }

    function check(view, what) {
        var grid = view.flickable
        expect(view.gridMode, what + ": the folder is a grid")
        expect(grid.columns === 3, what + ": 3 tiles per row, not " + grid.columns)
        expect(tiles(grid).some(function (t) { return t.captions.length > 0 }), what + ": a tile has caption lines")
    }

    Component.onCompleted: {
        expect(tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), "lautta://user-downloads/grid-photo.jpg")
               && tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"),
                                      "lautta://user-downloads/harbour-salama-0.8.0-1.20261015.aarch64.txt"),
               "fixtures installed")
        App.setSetting("view_mode", JSON.stringify("grid"))
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        var view = pageStack.currentPage.view
        waitFor(function () { return !view.model.loading && view.model.count >= 2 && tiles(view.flickable).length > 0 }, "the folder")
        check(view, "grid")
        expect(!App.setSetting("grid_columns", JSON.stringify(4)), "the number of tiles per row is no setting")
        App.setSetting("view_mode", JSON.stringify("list"))
        reachedEnd = true
    }

    Component {
        id: pageComponent

        Page {
            property alias view: folder

            DirectoryView {
                id: folder
                anchors.fill: parent
                uri: "lautta://user-downloads/"
            }
        }
    }
}
