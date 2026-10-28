// SPDX-License-Identifier: LGPL-2.1-or-later
// Scenario on the target (real Silica): in an empty folder, pulling the
// pulley moves the whole view, the placeholder with the header, as it does
// in any Silica view (SPEC §15.3, boards StateEmpty, StateError). Checked in
// the list and the grid and for a folder that cannot be opened, with the
// page's pulley host pulled and with a finger dragged down on the empty
// content (ViewerTools.mouse); the loading indicator sits with them. Fails
// when the placeholder, the header or the loading indicator stays put while
// the page is pulled, or a drag on the empty content does not pull it.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: win

    readonly property string empty: "lautta://user-documents/EmptyPulley/"
    readonly property string missing: "lautta://user-documents/NoSuchFolder/"
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

    Component {
        id: pageComponent

        DirectoryPage { }
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

    function isAncestor(ancestor, item) {
        for (var p = item; p; p = p.parent) {
            if (p === ancestor)
                return true
        }
        return false
    }

    // The folder view of the page on top, once it is settled.
    function folderView() {
        var page = pageStack.currentPage
        if (!page || pageStack.busy || page.status !== PageStatus.Active)
            return null
        var views = findAll(page, function(x) { return x.placeholderKind !== undefined && x.runAction !== undefined }, [])
        return views.length === 1 ? views[0] : null
    }

    function named(view, name) {
        var all = findAll(view, function(x) { return x.objectName === name }, [])
        return all.length === 1 ? all[0] : null
    }

    // How far down the view `item` is.
    function at(view, item) {
        return item.mapToItem(view, 0, 0).y
    }

    // Pulls the page down by `by` as a finger does on the pulley, and checks
    // that the placeholder of `kind` and the header move with it; then pulls
    // it with a finger on the empty content.
    function checkPull(view, kind, by) {
        if (view.placeholderKind() !== kind)
            return false
        var flicks = findAll(view, function(x) { return x.cacheBuffer !== undefined && x.visible }, [])
        var holders = findAll(view, function(x) { return x.buttons !== undefined && x.hint !== undefined && x.visible }, [])
        var host = named(view, "folderPulleyHost")
        var header = named(view, "folderHeader")
        if (flicks.length !== 1 || holders.length !== 1 || !host || !header)
            return fail(kind + ": one view, one placeholder, the pulley host and the header, not " + flicks.length
                        + " and " + holders.length + ", " + host + ", " + header)
        var flick = flicks[0]
        var holder = holders[0]
        var where = kind + (view.gridMode ? " in the grid" : " in the list")
        var before = at(view, holder)
        var top = at(view, header)
        host.contentY = -by
        var moved = at(view, holder) - before
        var headerMoved = at(view, header) - top
        host.contentY = 0
        if (Math.abs(moved - by) > 1 || Math.abs(headerMoved - by) > 1)
            return fail(where + ": the placeholder moves " + moved + " and the header " + headerMoved
                        + " when the page is pulled " + by)
        // A finger down on the empty content, above the placeholder, and
        // back up before it lets go (so no pulley entry runs).
        var x = flick.width / 2
        var y = flick.height * 0.2
        tools.mouse(flick, "press", x, y)
        for (var i = 1; i <= 12; ++i)
            tools.mouse(flick, "move", x, y + by * i / 12)
        var pulled = -host.contentY
        var followed = at(view, holder) - before
        tools.mouse(flick, "move", x, y)
        tools.mouse(flick, "release", x, y)
        // Headless no animation runs: the host would stay out. Closed by hand.
        host.cancelFlick()
        host.contentY = 0
        if (pulled < Theme.paddingLarge || Math.abs(followed - pulled) > 1)
            return fail(where + ": a finger pulls the page " + pulled + ", the placeholder follows " + followed)
        // The loading indicator is the placeholder's neighbour.
        var busy = []
        var kids = holder.parent.children
        for (var k = 0; k < kids.length; ++k) {
            if (String(kids[k]).indexOf("BusyIndicator") === 0)
                busy.push(kids[k])
        }
        if (busy.length !== 1 || !isAncestor(flick.contentItem, busy[0]))
            return fail("the loading indicator sits in the content of the view: " + busy.length)
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
                documents.createFolder("EmptyPulley")
                win.next()
                break
            case 1:
                if (!win.made)
                    break
                pageStack.push(pageComponent, { "uri": win.empty }, PageStackAction.Immediate)
                win.next()
                break
            case 2:
                view = win.folderView()
                if (!view || view.model.loading || view.gridMode)
                    break
                if (win.checkPull(view, "empty", 150)) {
                    view.model.setViewPrefs(JSON.stringify({ "viewMode": "grid" }))
                    win.next()
                }
                break
            case 3:
                view = win.folderView()
                if (!view || !view.gridMode)
                    break
                if (win.checkPull(view, "empty", 120)) {
                    pageStack.push(pageComponent, { "uri": win.missing }, PageStackAction.Immediate)
                    win.next()
                }
                break
            case 4:
                view = win.folderView()
                if (!view || view.model.loading || view.model.errorKind.length === 0)
                    break
                if (win.checkPull(view, "error", 150))
                    win.scenarioRunning = false
                break
            }
        }
    }
}
