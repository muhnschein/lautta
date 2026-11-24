// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: win

    property bool reachedEnd
    property var root
    readonly property string folder: "lautta://user-pictures/RecentsTileSize/"
    readonly property string photo: folder + "photo.jpg"

    ViewerTools { id: tools }
    DirectoryModel {
        id: pictures
        uri: "lautta://user-pictures/"
    }

    ListModel { id: homeRows }
    ListModel { id: recentRows }

    Component {
        id: loadingThumbnail
        Item {
            readonly property bool ready: false
            readonly property bool failed: false
        }
    }

    Component {
        id: shownThumbnail
        Item {
            readonly property bool ready: true
            readonly property bool failed: false
        }
    }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        return expect(done(), "timed out waiting for " + what)
    }

    function findAll(item, pred, out) {
        if (!item)
            return out
        if (pred(item) && out.indexOf(item) < 0)
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out)
        if (item.contentItem && item.contentItem !== item)
            findAll(item.contentItem, pred, out)
        return out
    }

    function layOut(page) {
        findAll(page, function (x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, []).forEach(function (v) {
            v.cacheBuffer = 100000
            v.contentY += 1
            v.contentY -= 1
            v.forceLayout()
        })
    }

    function named(page, name) {
        layOut(page)
        return findAll(page, function (x) { return x.objectName === name }, [])
    }

    function fileIcon(item) {
        return findAll(item, function (x) { return x.thumbnailSource !== undefined && x.thumbnailReady !== undefined }, [])[0] || null
    }

    function showThumbnail(icon, shown) {
        for (var i = 0; i < icon.children.length; ++i) {
            if (icon.children[i].sourceComponent !== undefined) {
                icon.children[i].sourceComponent = shown ? shownThumbnail : loadingThumbnail
                return
            }
        }
    }

    function recentRow() {
        return { "itemId": 1, "uri": photo, "name": "photo.jpg", "kind": "opened", "place": "Pictures", "at": 1,
                 "dateGroup": "today", "category": "image", "mimeType": "image/jpeg",
                 "thumbnailSource": App.localUrl(photo), "textThumb": false, "textMono": false, "typeIcon": "" }
    }

    function stripRow() {
        var tile = { "id": 1, "uri": photo, "name": "photo.jpg", "kind": "opened", "category": "image",
                     "mimeType": "image/jpeg", "thumbnailSource": App.localUrl(photo) }
        return { "section": "recents", "uri": "", "name": "", "kind": "recent-strip", "icon": "", "status": "ready",
                 "attention": "", "connection": "", "connectionError": "", "colour": "", "itemId": "", "place": "",
                 "provider": "", "host": "", "free": -1, "total": -1, "fs": "", "drive": "", "tiles": JSON.stringify([tile]) }
    }

    function openFrom(what, push, find, shown) {
        var page = push()
        var cell = null
        waitFor(function () { cell = find(page); return cell !== null && fileIcon(cell) !== null }, what + ": the photo")
        if (!cell)
            return
        var icon = fileIcon(cell)
        showThumbnail(icon, shown)
        waitFor(function () { return icon.thumbnailReady === shown }, what + ": thumbnail shown " + shown)
        var want = shown ? Math.round(icon.width) : 0
        cell.clicked(null)
        waitFor(function () { return pageStack.depth === 3 && !pageStack.busy }, what + ": the viewer")
        var viewer = pageStack.currentPage
        expect(icon.width > 0 && viewer.uri === photo && viewer.tileSize === want,
               what + ": viewer of " + viewer.uri + " gets size " + want + " (thumbnail shown " + shown + "): " + viewer.tileSize)
        pageStack.pop(root, PageStackAction.Immediate)
        waitFor(function () { return pageStack.depth === 1 && !pageStack.busy }, what + ": back at the start")
    }

    function pushHome() {
        return pageStack.push(Qt.resolvedUrl("../../qml/pages/HomePage.qml"), { "listModel": homeRows },
                              PageStackAction.Immediate)
    }

    function pushRecents() {
        return pageStack.push(Qt.resolvedUrl("../../qml/pages/RecentsPage.qml"), { "listModel": recentRows },
                              PageStackAction.Immediate)
    }

    function first(name) {
        return function (page) { return named(page, name)[0] || null }
    }

    function listRow(page) {
        var icon = named(page, "recentIcon")[0]
        for (var p = icon ? icon.parent : null; p; p = p.parent) {
            if (p.openMenu !== undefined && p.contentHeight !== undefined)
                return p
        }
        return null
    }

    function checkStrip() {
        homeRows.append(stripRow())
        for (var s = 0; s < 2; ++s)
            openFrom("Home Recents tile", pushHome, first("recentTile"), s === 0)
    }

    function checkRecents(mode) {
        App.setSetting("view_mode", JSON.stringify(mode))
        var find = mode === "grid" ? first("recentGridTile") : listRow
        for (var s = 0; s < 2; ++s)
            openFrom("Recents " + mode, pushRecents, find, s === 0)
    }

    Component.onCompleted: {
        pictures.createFolder("RecentsTileSize")
        waitFor(function () { return pictures.indexOf(folder.slice(0, -1)) >= 0 }, "the folder")
        if (!expect(tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), photo), "fixture installed"))
            return
        recentRows.append(recentRow())
        root = pageStack.push(rootComponent, {}, PageStackAction.Immediate)
        checkStrip()
        checkRecents("list")
        checkRecents("grid")
        App.setSetting("view_mode", JSON.stringify("list"))
        reachedEnd = true
    }

    Component {
        id: rootComponent
        Page {}
    }
}
