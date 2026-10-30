// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

ApplicationWindow {
    id: win

    readonly property string folder: "lautta://user-documents/RemoveMe"
    property bool scenarioRunning: true
    property int step: 0
    property int waited: 0
    property var browse
    property var row

    FavouritesModel {
        id: favs
    }

    initialPage: Component {
        Loader {
            source: Qt.resolvedUrl("../../qml/pages/BrowsePage.qml")
            onLoaded: win.browse = item
        }
    }

    function fail(what) {
        console.error("FAILED: " + what)
        scenarioRunning = false
    }

    function findAll(item, pred, out) {
        if (!item)
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

    function favouriteRow() {
        // No render loop headless: lay out by hand.
        var views = findAll(win.contentItem, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
        for (var i = 0; i < views.length; ++i)
            views[i].forceLayout()
        var rows = findAll(win.contentItem, function(x) { return x.kind === "favourite" && x.openMenu !== undefined }, [])
        return rows.length > 0 ? rows[0] : null
    }

    Timer {
        interval: 100
        repeat: true
        running: win.scenarioRunning
        onTriggered: {
            win.waited++
            if (win.waited > 150)
                return win.fail("step " + win.step + " timed out")
            switch (win.step) {
            case 0:
                favs.add(win.folder, "Remove me", "")
                win.step++
                win.waited = 0
                break
            case 1:
                win.row = favouriteRow()
                if (win.row) {
                    win.row.openMenu()
                    win.step++
                    win.waited = 0
                }
                break
            case 2:
                var items = findAll(win.row._menuItem, function(x) {
                    return x.text === "lautta-browse-remove-favourite" || x.text === "Remove from favourites"
                }, [])
                if (items.length === 0)
                    break
                items[0].clicked(null)
                win.browse.listModel = null
                win.step++
                win.waited = 0
                break
            case 3:
                favs.reload()
                win.step++
                break
            case 4:
                if (favs.loaded && favs.idOf(win.folder) === -1)
                    win.scenarioRunning = false
                else if (win.waited % 10 === 0)
                    favs.reload()
                break
            }
        }
    }
}
