// SPDX-License-Identifier: LGPL-2.1-or-later
// Scenario on the target (real Silica): the pulley of a folder (SPEC §15.3,
// board DirectoryPulley) holds Activity at the top, then Select, New folder,
// Search and View options (Paste only with a clipboard); Activity opens the
// Activity page as the Browse pulley does. While selecting it holds only
// Select all and Clear selection. Fails when an entry is missing, extra or
// out of order, or Activity leads elsewhere. On the target the entries show
// their ids (no translation is loaded).
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: win

    property bool scenarioRunning: true
    property int step: 0
    property int waited: 0
    property var folderPage

    initialPage: Component {
        DirectoryPage {
            uri: "lautta://user-documents/"
        }
    }

    function fail(what) {
        console.error("FAILED: " + what)
        scenarioRunning = false
    }

    // A flickable's content item is one of its children too: each item once.
    function findAll(item, pred, out, seen) {
        seen = seen || []
        if (!item || seen.indexOf(item) >= 0)
            return out
        seen.push(item)
        if (pred(item))
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out, seen)
        if (item.contentItem)
            findAll(item.contentItem, pred, out, seen)
        return out
    }

    function folderView() {
        var views = findAll(win.folderPage, function(x) { return x.runAction !== undefined && x.startSelection !== undefined }, [])
        return views.length === 1 ? views[0] : null
    }

    // The entries of the pulley of the view shown, as the user sees them.
    function entries() {
        var menus = findAll(win.folderPage, function(x) {
            return x._isPullDownMenu === true && x.flickable && x.flickable.visible
        }, [])
        if (menus.length !== 1)
            return []
        // The entries show only while the pulley is pulled (active).
        var menu = menus[0]
        menu.active = true
        var kids = menu._contentColumn.children
        var out = []
        for (var i = 0; i < kids.length; ++i) {
            if (kids[i].hasOwnProperty("__silica_menuitem") && kids[i].visible)
                out.push(kids[i])
        }
        menu.active = false
        return out
    }

    function texts(items) {
        var out = []
        for (var i = 0; i < items.length; ++i)
            out.push(items[i].text)
        return out
    }

    function expectTexts(want, what) {
        var got = texts(entries())
        // The live count of Browse's Activity too, while transfers run.
        if (got.length > 0 && got[0].indexOf("lautta-dir-menu-activity") === 0)
            got[0] = "lautta-dir-menu-activity"
        if (JSON.stringify(got) !== JSON.stringify(want))
            return fail(what + ": " + JSON.stringify(got) + " != " + JSON.stringify(want))
        return true
    }

    function next() {
        step++
        waited = 0
    }

    Timer {
        interval: 100
        repeat: true
        running: win.scenarioRunning
        onTriggered: {
            win.waited++
            if (win.waited > 100)
                return win.fail("step " + win.step + " timed out")
            var view
            switch (win.step) {
            case 0:
                win.folderPage = pageStack.currentPage
                view = win.folderView()
                if (!view || view.model.loading || !view.model.writable || App.clipboardCount > 0)
                    break
                if (!win.expectTexts(["lautta-dir-menu-activity", "lautta-dir-menu-select", "lautta-dir-menu-new-folder",
                                      "lautta-dir-menu-search", "lautta-dir-menu-view-options"], "folder pulley"))
                    break
                view.startSelection()
                win.next()
                break
            case 1:
                if (!win.expectTexts(["lautta-dir-menu-select-all", "lautta-dir-menu-clear-selection"], "pulley while selecting"))
                    break
                win.folderView().endSelection()
                win.next()
                break
            case 2:
                var items = win.entries()
                if (items.length === 0 || items[0].text.indexOf("lautta-dir-menu-activity") !== 0)
                    break
                items[0].clicked()
                win.next()
                break
            case 3:
                var page = pageStack.currentPage
                if (pageStack.busy || page === win.folderPage)
                    break
                if (page.objectName !== "transfersPage" || pageStack.previousPage(page) !== win.folderPage)
                    return win.fail("Activity opens the Activity page on the folder: " + page)
                win.scenarioRunning = false
                break
            }
        }
    }
}
