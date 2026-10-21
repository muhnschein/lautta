// SPDX-License-Identifier: LGPL-2.1-or-later
// Target test: SettingsPage with real Silica and the real Lautta types. It
// has no date format (dates follow the system, §18) and repeats none of the
// view options (BRW-4): a View options row opens that page instead. Fails
// when one of those settings is back or the row is missing.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: window

    ViewerTools { id: tools }

    // Items are reached both as children and as a content item: each once.
    function findAll(item, pred, out) {
        if (!item || out.indexOf(item) >= 0)
            return out
        if (pred(item))
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out)
        if (item.contentItem && item.contentItem !== item)
            findAll(item.contentItem, pred, out)
        return out
    }

    Component.onCompleted: {
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        tools.pump(300)
        var page = pageStack.currentPage
        var keys = findAll(page, function (x) { return x.settingKey !== undefined }, []).map(function (x) {
            return x.settingKey
        })
        var gone = ["date_format", "view_mode", "sort_key", "sort_descending", "folders_first", "show_hidden",
                    "thumbnails_local", "grid_columns"]
        for (var i = 0; i < gone.length; ++i) {
            if (keys.indexOf(gone[i]) >= 0)
                console.error("Settings must not show " + gone[i] + ": " + keys)
        }
        if (keys.indexOf("thumbnails_remote") < 0 || keys.indexOf("thumbnail_cache_mb") < 0)
            console.error("server thumbnails and the cache stay in Settings: " + keys)
        var rows = findAll(page, function (x) { return x.objectName === "viewOptionsRow" }, [])
        if (rows.length !== 1) {
            console.error("Settings needs one View options row, not " + rows.length)
            return
        }
        rows[0].clicked(null)
        tools.pump(1000)
        var options = pageStack.currentPage
        if (options.objectName !== "viewOptionsPage" || options.model)
            console.error("the row opens the view options page for every folder: " + options.objectName)
    }

    Component {
        id: pageComponent

        SettingsPage { }
    }
}
