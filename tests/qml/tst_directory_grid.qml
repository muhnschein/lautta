// SPDX-License-Identifier: LGPL-2.1-or-later
// The grid view in both sizes (View options, BRW-4): 4 per row with icons
// only, 3 per row with name and caption lines under every tile (their text
// is checked in tst_directory_grid_captions.qml). Fails when the view does
// not follow the grid_columns setting.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components/directory"

ApplicationWindow {
    id: window

    ViewerTools { id: tools }

    function expect(columns) {
        App.setSetting("grid_columns", JSON.stringify(columns))
        tools.pump(200)
        var view = pageStack.currentPage.view
        if (!view.gridMode || view.gridColumns !== columns || view.gridCaptions !== (columns === 3))
            console.error("the grid must show " + columns + " per row, not " + view.gridColumns
                          + (view.gridCaptions ? " with captions" : ""))
    }

    Component.onCompleted: {
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), "lautta://user-downloads/grid-photo.jpg")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"),
                             "lautta://user-downloads/harbour-salama-0.8.0-1.20261015.aarch64.txt")
        App.setSetting("view_mode", JSON.stringify("grid"))
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        tools.pump(500)
        expect(3)
        expect(4)
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
