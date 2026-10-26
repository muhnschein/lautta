// SPDX-License-Identifier: LGPL-2.1-or-later
// The grid view (View options, BRW-4) has one size: 3 tiles per row in
// portrait, with the name and caption lines under every tile (their text is
// checked in tst_directory_grid_captions.qml). A number of tiles per row
// stored by an earlier version is no setting and changes nothing. Fails when
// the grid shows another number of tiles per row, tiles without captions or
// a setting for the size.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components/directory"

ApplicationWindow {
    id: window

    ViewerTools { id: tools }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
    }

    // The tiles on screen, which are those that hold captions.
    // Laid out by hand: headless, no render loop polishes the view.
    function tiles(grid) {
        var out = []
        grid.forceLayout()
        var kids = grid.contentItem.children
        for (var i = 0; i < kids.length; ++i) {
            if (kids[i].captions !== undefined && kids[i].itemName !== undefined)
                out.push(kids[i])
        }
        return out
    }

    function check(view, what) {
        var grid = view.flickable
        expect(view.gridMode, what + ": the folder is a grid")
        expect(grid.columns === 3, what + ": 3 tiles per row, not " + grid.columns)
        // A file's tile has its time and size under it; a folder's waits for its count.
        expect(tiles(grid).some(function (t) { return t.captions.length > 0 }), what + ": a tile has caption lines")
    }

    Component.onCompleted: {
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), "lautta://user-downloads/grid-photo.jpg")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"),
                             "lautta://user-downloads/harbour-salama-0.8.0-1.20261015.aarch64.txt")
        App.setSetting("view_mode", JSON.stringify("grid"))
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        tools.pump(500)
        var view = pageStack.currentPage.view
        check(view, "grid")
        expect(!App.setSetting("grid_columns", JSON.stringify(4)), "the number of tiles per row is no setting")
        tools.pump(200)
        check(view, "after a stored size")
        App.setSetting("view_mode", JSON.stringify("list"))
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
