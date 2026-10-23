// SPDX-License-Identifier: LGPL-2.1-or-later
// Scenario on the target (real Silica): in an empty folder, pulling the
// pulley moves the whole view, the placeholder with the header, as it does
// in any Silica view (SPEC §15.3, boards StateEmpty, StateError). Checked in
// the list and the grid and for a folder that cannot be opened; the loading
// indicator sits with them. Fails when the placeholder or the loading
// indicator stays put while the content moves.
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

    // Pulls the shown list or grid down by `by` as a finger does on the
    // pulley, and checks that the placeholder of `kind` moves with it.
    function checkPull(view, kind, by) {
        if (view.placeholderKind() !== kind)
            return false
        var flicks = findAll(view, function(x) { return x.cacheBuffer !== undefined && x.visible }, [])
        var holders = findAll(view, function(x) { return x.buttons !== undefined && x.hint !== undefined && x.visible }, [])
        if (flicks.length !== 1 || holders.length !== 1)
            return fail(kind + ": one view and one placeholder shown, not " + flicks.length + " and " + holders.length)
        var flick = flicks[0]
        var holder = holders[0]
        var start = flick.contentY
        var before = holder.mapToItem(view, 0, 0).y
        flick.contentY = start - by
        var moved = holder.mapToItem(view, 0, 0).y - before
        flick.contentY = start
        if (Math.abs(moved - by) > 1)
            return fail(kind + (view.gridMode ? " in the grid" : " in the list") + ": the placeholder moves "
                        + moved + " when the view is pulled " + by)
        // The loading indicator is the placeholder's neighbour.
        var busy = []
        var kids = holder.parent.children
        for (var i = 0; i < kids.length; ++i) {
            if (String(kids[i]).indexOf("BusyIndicator") === 0)
                busy.push(kids[i])
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
