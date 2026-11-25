// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"
import "../../qml/components/viewers/ViewerImage.js" as ViewerImage

ApplicationWindow {
    id: window

    property bool reachedEnd
    property var log: []
    property var prefetch
    readonly property string folder: "lautta://user-pictures/PagesPrefetch/"

    ViewerTools { id: tools }
    DirectoryModel {
        id: pictures
        uri: "lautta://user-pictures/"
    }
    DirectoryModel { id: inner }

    Connections {
        target: window.prefetch
        ignoreUnknownSignals: true
        onSourceChanged: window.log.push({ "source": String(window.prefetch.source), "status": window.prefetch.status })
    }

    ListModel {
        id: recentRows
        ListElement { itemId: 1; uri: "lautta://user-pictures/PagesPrefetch/recent.jpg"; name: "recent.jpg"; kind: "opened"; place: "Pictures"; at: 1; dateGroup: "today" }
        ListElement { itemId: 2; uri: "lautta://user-pictures/PagesPrefetch/sent.jpg"; name: "sent.jpg"; kind: "transferred"; place: "Pictures"; at: 2; dateGroup: "today" }
    }

    ListModel {
        id: homeRows
        ListElement { section: "favourites"; uri: "lautta://user-pictures/PagesPrefetch/fav.jpg"; name: "fav.jpg"; kind: "favourite"; isFile: true; icon: "image://theme/icon-m-favorite"; status: "ready"; attention: ""; colour: ""; itemId: "1"; place: "Pictures"; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "favourites"; uri: "lautta://user-pictures/PagesPrefetch/album.jpg"; name: "album.jpg"; kind: "favourite"; isFile: false; icon: "image://theme/icon-m-favorite"; status: "ready"; attention: ""; colour: ""; itemId: "2"; place: "Pictures"; provider: ""; host: ""; free: -1; total: -1; fs: "" }
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
        if (pred(item))
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out)
        if (item.contentItem && item.contentItem !== item)
            findAll(item.contentItem, pred, out)
        return out
    }

    function row(page, name) {
        findAll(page, function (x) { return x.forceLayout !== undefined }, []).forEach(function (v) { v.forceLayout() })
        var labels = findAll(page, function (x) {
            return typeof x.text === "string" && x.text.replace(/<[^>]*>/g, "") === name && x.visible
        }, [])
        for (var p = labels.length > 0 ? labels[0] : null; p; p = p.parent) {
            if (p.openMenu !== undefined && p.contentHeight !== undefined)
                return p
        }
        return null
    }

    function down(page, name) {
        var r = row(page, name)
        if (!expect(r !== null, name + " row found"))
            return null
        var list = r.parent
        while (list && list.positionViewAtIndex === undefined)
            list = list.parent
        list.contentY = r.mapToItem(list.contentItem, 0, 0).y
        log = []
        tools.mouse(r, "press", r.width / 2, Math.min(r.height, list.height) / 2)
        return { "row": r, "list": list }
    }

    function press(page, name) {
        var at = down(page, name)
        if (!at)
            return
        tools.mouse(at.row, "release", at.row.width / 2, at.row.height + at.list.height)
        expect(pageStack.currentPage === page && !pageStack.busy, name + ": letting go off the row opens nothing")
    }

    function starts(page, name, what) {
        press(page, name)
        expect(log.length === 2 && log[0].source === ViewerImage.source(folder + name)
               && log[1].source === "" && log[1].status !== Image.Ready,
               what + ": finger down starts its decode, let go off the row drops it: " + JSON.stringify(log))
    }

    function shown(page) {
        return !pageStack.busy && pageStack.currentPage === page && page.status === PageStatus.Active
    }

    function back() {
        var below = pageStack.previousPage()
        pageStack.pop(null, PageStackAction.Immediate)
        waitFor(function () { return shown(below) }, "the page below")
    }

    function holdDrops(page, name, what) {
        var at = down(page, name)
        if (!at)
            return
        at.row.pressAndHold(null)
        expect(log.length === 2 && log[1].source === "" && log[1].status !== Image.Ready,
               what + ": hold drops it: " + JSON.stringify(log))
        tools.mouse(at.row, "release", at.row.width / 2, at.row.height * 4)
        if (at.row.menuOpen)
            at.row.closeMenu()
    }

    function startsNone(page, name, what) {
        press(page, name)
        expect(log.length === 0, what + ": no decode: " + JSON.stringify(log))
    }

    function open(url, props) {
        var page = pageStack.push(url, props, PageStackAction.Immediate)
        waitFor(function () { return shown(page) }, url + " shown")
        prefetch = findAll(page, function (x) { return x.objectName === "viewerPrefetch" }, [])[0] || null
        expect(prefetch !== null, url + ": the page has a prefetch")
        return page
    }

    function checkSearch() {
        var page = open(Qt.resolvedUrl("../../qml/pages/SearchPage.qml"), { "rootUri": folder.slice(0, -1) })
        var field = findAll(page, function (x) { return x.objectName === "searchField" }, [])[0]
        var list = findAll(page, function (x) { return x.objectName === "searchList" }, [])[0]
        var panel = findAll(page, function (x) { return x.dock === Dock.Bottom && x.visibleSize !== undefined }, [])[0]
        field.text = "a"
        waitFor(function () {
            return !list.model.running && !panel.expanded
                   && row(page, "search.jpg") !== null && row(page, "album.jpg") !== null
        }, "search results, the progress panel closed")
        starts(page, "search.jpg", "search, image")
        holdDrops(page, "search.jpg", "search")
        startsNone(page, "album.jpg", "search, folder")
        back()
    }

    function checkRecents() {
        var page = open(Qt.resolvedUrl("../../qml/pages/RecentsPage.qml"), { "listModel": recentRows })
        starts(page, "recent.jpg", "recents, image")
        holdDrops(page, "recent.jpg", "recents")
        startsNone(page, "sent.jpg", "recents, transferred")
        back()
    }

    function checkHome() {
        var page = open(Qt.resolvedUrl("../../qml/pages/HomePage.qml"), { "listModel": homeRows })
        starts(page, "fav.jpg", "home, image favourite")
        holdDrops(page, "fav.jpg", "home")
        startsNone(page, "album.jpg", "home, folder favourite")
    }

    Component.onCompleted: {
        pictures.createFolder("PagesPrefetch")
        waitFor(function () { return pictures.indexOf(folder.slice(0, -1)) >= 0 }, "the folder")
        inner.uri = folder
        inner.createFolder("album.jpg")
        waitFor(function () { return inner.indexOf(folder + "album.jpg") >= 0 }, "the subfolder")
        var names = ["search.jpg", "recent.jpg", "sent.jpg", "fav.jpg"]
        var installed = true
        for (var i = 0; i < names.length; ++i)
            installed = tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), folder + names[i]) && installed
        if (!expect(installed, "fixtures installed"))
            return
        var root = pageStack.push(rootComponent, {}, PageStackAction.Immediate)
        if (!waitFor(function () { return shown(root) }, "the root page"))
            return
        checkSearch()
        checkRecents()
        checkHome()
        reachedEnd = true
    }

    Component {
        id: rootComponent
        Page {}
    }
}
