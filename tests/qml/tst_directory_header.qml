// SPDX-License-Identifier: LGPL-2.1-or-later
// The folder's header on the target (real Silica, SPEC §15.3, boards
// Directory, DirectoryGrid, DirectoryLandscape): the header bar with its
// progress line, path menu and notices stays at the top of the page while
// the list or grid scrolls under it, in portrait and landscape; the list or
// grid has no header of its own, so a notice or the selection line that
// comes after the rows cannot scroll it away from its top; the pulley is the
// content's and shows under the header; the empty-folder placeholder sits in
// the area below it. Opening a folder starts at its very top, also while it
// still loads and when a notice comes after the rows (a photo folder shown
// as grid); going back shows the folder where it was left. Fails when the
// header scrolls, overlaps or leaves a gap, when a view starts anywhere but
// its top or when going back loses the position. Headless there is no render
// loop, so how the header's height settles on a device is not seen here.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: win

    readonly property string base: "lautta://user-documents/FolderHeader/"
    readonly property int files: 60

    ViewerTools { id: tools }

    DirectoryModel { id: documents; uri: "lautta://user-documents/" }
    DirectoryModel { id: inside }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    // Pumps events until `done()` holds, ten seconds at most.
    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        return expect(done(), "timed out waiting for " + what)
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

    function named(root, name) {
        var all = findAll(root, function (x) { return x.objectName === name }, [])
        return expect(all.length === 1, "one item called " + name + ": " + all.length) ? all[0] : null
    }

    function viewOf(page) {
        var views = findAll(page, function (x) { return x.runAction !== undefined && x.startSelection !== undefined }, [])
        return expect(views.length === 1, "one folder view: " + views.length) ? views[0] : null
    }

    function isInside(ancestor, item) {
        for (var p = item; p; p = p.parent) {
            if (p === ancestor)
                return true
        }
        return false
    }

    function nameOf(i) {
        return "f" + (i < 10 ? "00" : (i < 100 ? "0" : "")) + i + ".md"
    }

    function makeFolders() {
        documents.createFolder("FolderHeader")
        waitFor(function () { return documents.indexOf(base.replace(/\/$/, "")) >= 0 }, "the folder")
        inside.uri = base
        inside.createFolder("Child")
        inside.createFolder("Empty")
        inside.createFolder("Photos")
        waitFor(function () { return inside.count === 3 }, "its folders")
        for (var i = 0; i < files; ++i) {
            tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), base + nameOf(i))
            tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), base + "Child/" + nameOf(i))
        }
        for (var j = 0; j < 12; ++j)
            tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), base + "Photos/p" + j + ".jpg")
        inside.refresh()
        waitFor(function () { return inside.count === files + 3 }, "the files")
    }

    // A folder page on the stack, listed.
    function push(uri, immediate) {
        pageStack.push(pageComponent, { "uri": uri }, immediate ? PageStackAction.Immediate : PageStackAction.Animated)
        return pageStack.currentPage
    }

    function listed(view, count) {
        return !view.model.loading && view.model.count === count
    }

    function settled() {
        return !pageStack.busy
    }

    // The header at the top, the list or grid right under it, whole.
    function checkFixed(view, label) {
        var f = view.flickable
        var header = named(view, "folderHeader")
        if (!header)
            return
        f.forceLayout()
        expect(header.height > 0, label + ": the header has a height")
        expect(f.headerItem === null, label + ": the list or grid has no header of its own")
        expect(header.mapToItem(view, 0, 0).y === 0, label + ": the header is at the top of the page")
        expect(Math.abs(f.y - header.height) < 0.5, label + ": the content starts under the header: " + f.y + " vs " + header.height)
        expect(Math.abs(f.y + f.height - view.height) < 0.5, label + ": and runs to the bottom: " + (f.y + f.height) + " vs " + view.height)
        expect(f.clip, label + ": and is cut at the header")
        expect(f.contentY === f.originY, label + ": it starts at its top: " + f.contentY + " vs " + f.originY)
        expect(!isInside(f.contentItem, header), label + ": the header is not part of the content")
        f.contentY = f.originY + f.height * 2
        expect(f.contentY > f.originY + f.height, label + ": the content scrolls: " + f.contentY)
        expect(header.mapToItem(view, 0, 0).y === 0, label + ": the header stays when the content scrolls")
        f.contentY = f.originY
    }

    // The selection line grows the header; the content follows and stays at its top.
    function checkSelectionLine(view, label) {
        var f = view.flickable
        var header = named(view, "folderHeader")
        var bar = named(view, "folderPageHeader")
        if (!header || !bar)
            return
        expect(bar.description === "", label + ": no description outside selection")
        var before = header.height
        view.startSelection()
        expect(bar.description.length > 0 && bar.description === view.model.title,
               label + ": selecting shows the folder name as description: " + bar.description)
        expect(header.height >= before, label + ": the header holds the line")
        expect(Math.abs(f.y - header.height) < 0.5, label + ": the content follows the header: " + f.y + " vs " + header.height)
        expect(f.contentY === f.originY, label + ": and stays at its top: " + f.contentY + " vs " + f.originY)
        view.endSelection()
        expect(bar.description === "", label + ": the description goes with selection")
        expect(Math.abs(f.y - header.height) < 0.5, label + ": the content follows back")
    }

    // The pulley is the content's: its menu ends where the content begins, so
    // pulled it shows right under the header. (Pulling it from here moves
    // contentY, which Silica resets when a grid re-estimates its height.)
    function checkPulley(view, label) {
        var f = view.flickable
        var header = named(view, "folderHeader")
        var menus = findAll(view, function (x) { return x._isPullDownMenu === true && x.flickable === f }, [])
        if (!expect(menus.length === 1, label + ": one pulley on the content: " + menus.length) || !header)
            return
        expect(menus[0].parent === f.contentItem, label + ": the pulley is in the content")
        var bottom = menus[0].mapToItem(view, 0, menus[0].height).y
        expect(Math.abs(bottom - header.height) < 1, label + ": the pulley comes out under the header: " + bottom + " vs " + header.height)
    }

    // The path menu is capped to leave the content a strip, and scrolls inside
    // the cap. (Headless, no polish lays its rows out, so its height is not seen.)
    function checkPathMenu(view, label) {
        var menu = named(view, "folderPathMenu")
        var header = named(view, "folderHeader")
        if (!menu || !header)
            return
        expect(menu.y > 0 && menu.y <= header.height, label + ": the path menu sits in the header: " + menu.y)
        expect(Math.abs(menu.maxHeight - (view.height - menu.y - Theme.itemSizeLarge)) < 0.5 && menu.maxHeight > 0,
               label + ": the open menu leaves a strip of the page: " + menu.maxHeight + " of " + view.height + " from " + menu.y)
        var scrollers = findAll(menu, function (x) { return x.objectName === "pathMenuScroller" }, [])
        expect(scrollers.length === 1 && scrollers[0].anchors.fill === menu, label + ": the menu scrolls inside its cap")
    }

    function checkLayout(page, label) {
        var view = viewOf(page)
        if (!view)
            return
        checkFixed(view, label)
        checkSelectionLine(view, label)
        checkPulley(view, label)
        checkPathMenu(view, label)
    }

    function checkOrientations(mode) {
        App.setSetting("view_mode", JSON.stringify(mode))
        var page = push(base, true)
        waitFor(function () { return listed(viewOf(page), files + 3) }, "the folder")
        tools.pump(100)
        expect(viewOf(page).gridMode === (mode === "grid"), mode + " is shown")
        checkLayout(page, mode + " portrait")
        win.deviceOrientation = Orientation.Landscape
        waitFor(function () { return page.isLandscape && !page.orientationTransitionRunning }, "landscape")
        tools.pump(100)
        checkLayout(page, mode + " landscape")
        win.deviceOrientation = Orientation.Portrait
        waitFor(function () { return !page.isLandscape && !page.orientationTransitionRunning }, "portrait again")
        pageStack.pop(null, PageStackAction.Immediate)
    }

    // The empty-folder placeholder sits in the area below the header.
    function checkPlaceholder() {
        var page = push(base + "Empty/", true)
        var view = viewOf(page)
        var layer = named(view, "stateLayer")
        var note = named(view, "folderPlaceholder")
        waitFor(function () { return !view.model.loading && note && note.visible }, "the placeholder")
        if (layer && note) {
            var f = view.flickable
            var header = named(view, "folderHeader")
            var corner = layer.mapToItem(view, 0, 0)
            expect(Math.abs(corner.y - header.height) < 0.5 && Math.abs(layer.height - f.height) < 0.5,
                   "the states fill the area under the header: " + corner.y + " " + layer.height)
            var y = note.mapToItem(view, 0, 0).y
            expect(y >= header.height && y + note.height <= view.height,
                   "the placeholder is below the header: " + y + " " + note.height)
        }
        pageStack.pop(null, PageStackAction.Immediate)
    }

    // Opens `uri` from `parent`, watching the new page's position until it is listed.
    function openAtTop(from, uri, count, label) {
        from.openFolder(uri)
        var page = pageStack.currentPage
        var view = viewOf(page)
        var f = view.flickable
        for (var i = 0; i < 200 && !listed(view, count); ++i) {
            expect(f.contentY === f.originY, label + ": at the top while it loads: " + f.contentY + " vs " + f.originY)
            tools.pump(10)
        }
        waitFor(settled, "the page to settle")
        tools.pump(100)
        expect(listed(view, count), label + ": listed")
        expect(f.contentY === f.originY && f.originY === 0, label + ": at the top once listed: " + f.contentY + " vs " + f.originY)
        return page
    }

    // Down into a folder from a scrolled one, and back.
    function checkNavigation(mode) {
        App.setSetting("view_mode", JSON.stringify(mode))
        var parentPage = push(base, true)
        var parentView = viewOf(parentPage)
        waitFor(function () { return listed(parentView, files + 3) }, "the folder")
        var f = parentView.flickable
        f.forceLayout()
        f.contentY = f.originY + f.height * 3
        tools.pump(100)
        var left = f.contentY
        expect(left > f.originY + f.height, mode + ": the parent is scrolled: " + left)
        var child = openAtTop(parentView, base + "Child/", files, mode + " child")
        pageStack.pop()
        waitFor(settled, "going back")
        waitFor(function () { return pageStack.currentPage === parentPage && !parentView.model.loading }, "the parent again")
        tools.pump(200)
        expect(Math.abs(f.contentY - left) < 1, mode + ": back at the old position: " + f.contentY + " vs " + left)
        // And down again: the folder starts at its top once more.
        openAtTop(parentView, base + "Child/", files, mode + " child again")
        pageStack.pop(null, PageStackAction.Immediate)
        pageStack.pop(null, PageStackAction.Immediate)
    }

    // A photo folder in list view becomes a grid with a notice once listed.
    function checkPhotos() {
        App.setSetting("view_mode", JSON.stringify("list"))
        var parentPage = push(base, true)
        var parentView = viewOf(parentPage)
        waitFor(function () { return listed(parentView, files + 3) }, "the folder")
        var page = openAtTop(parentView, base + "Photos/", 12, "photos")
        var view = viewOf(page)
        var header = named(view, "folderHeader")
        var notices = findAll(view, function (x) {
            return x.actionText !== undefined && String(x.text).indexOf("lautta-dir-grid-suggested") === 0 && x.visible
        }, [])
        expect(view.gridMode && notices.length === 1, "the photos are a grid with its notice: " + view.gridMode + " " + notices.length)
        if (notices.length === 1 && header) {
            var bottom = notices[0].mapToItem(view, 0, notices[0].height).y
            expect(Math.abs(view.flickable.y - bottom) < 0.5 && Math.abs(header.height - bottom) < 0.5,
                   "the grid starts under the notice: " + view.flickable.y + " " + bottom + " " + header.height)
        }
        pageStack.pop(null, PageStackAction.Immediate)
        pageStack.pop(null, PageStackAction.Immediate)
    }

    Component.onCompleted: {
        makeFolders()
        checkOrientations("list")
        checkOrientations("grid")
        checkPlaceholder()
        checkNavigation("list")
        checkNavigation("grid")
        checkPhotos()
        App.setSetting("view_mode", JSON.stringify("list"))
    }

    Component {
        id: pageComponent

        DirectoryPage { }
    }
}
