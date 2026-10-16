// SPDX-License-Identifier: LGPL-2.1-or-later
// Settings › Browse page (BRW-12): every row of the Browse page with its
// switch, and a preview that follows the switches. Fails when the preview
// does not lose a hidden row or get it back on Restore defaults. The nearby
// switch stands for servers found on the network, of which there are none.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: window

    ViewerTools { id: tools }

    function expect(ok, what) {
        if (!ok)
            console.error(what)
    }

    // Settings rows the Browse page shows as rows of its own.
    function shownRows(layout) {
        var n = 0
        for (var i = 0; i < layout.count; ++i) {
            if (layout.sectionAt(i) !== "nearby")
                ++n
        }
        return n
    }

    Component.onCompleted: {
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        tools.pump(800)
        var page = pageStack.currentPage
        var rows = shownRows(page.layoutModel)
        expect(rows >= 3, "the device folders are listed: " + rows)
        expect(page.previewCount === rows, "the preview shows every row: " + page.previewCount + " of " + rows)
        page.layoutModel.setHidden("device", "user-documents", true)
        tools.pump(800)
        expect(shownRows(page.layoutModel) === rows, "a hidden row stays in the settings")
        expect(page.previewCount === rows - 1, "the preview drops a hidden row: " + page.previewCount)
        page.layoutModel.restoreDefaults()
        tools.pump(800)
        expect(page.previewCount === rows, "Restore defaults shows it again: " + page.previewCount)
    }

    Component {
        id: pageComponent
        BrowseSettingsPage { }
    }
}
