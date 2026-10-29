// SPDX-License-Identifier: LGPL-2.1-or-later
// The folder's header on the target (real Silica, SPEC §15.3, boards
// Directory, DirectoryGrid, DirectoryLandscape, DirectoryPulley,
// DirectorySelect): the header bar with its progress line, path menu and
// notices stays at the top of the page while the list or grid scrolls under
// it, in portrait and landscape; the list or grid has no header of its own,
// so a notice that comes after the rows cannot scroll it away from its top.
// Selecting keeps the header's height (count in place of the name, no extra
// line). The pulley is the page's, on a flickable that holds header and
// content and never scrolls: it comes out above the header, and pulling
// moves header and content together. With a finger sent through the window
// (ViewerTools.mouse): a drag on the content scrolls the content, not the
// host; a drag down at the content's top pulls the host. The empty-folder
// placeholder sits in the area below the header. Opening a folder starts at
// its very top, also while it still loads and when a notice comes after the
// rows (a photo folder shown as grid); going back shows the folder where it
// was left, the same item at its top, also in a grid whose rows differ in
// height (long names take two lines). Fails when the header scrolls,
// overlaps, leaves a gap or changes height with selection, when the pulley
// sits on the content or under the header, when a drag goes to the wrong
// flickable, when a view starts anywhere but its top or when going back
// loses the position. Headless there is no render loop, so how the header's
// height settles and how a pull feels on a device are not seen here.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: win

    readonly property string base: "lautta://user-documents/FolderHeader/"
    // Names of one line in the grid, and every fifth one again with a name
    // of two lines: grid rows of different heights all down the folder.
    readonly property int files: 72

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

    // a0 to f9, short enough for one line on the checks' small screen, then
    // a0, a5 to f5 again with a long name.
    function nameOf(i) {
        var n = i < 60 ? i : (i - 60) * 5
        var name = String.fromCharCode(97 + Math.floor(n / 10)) + n % 10
        return i < 60 ? name : name + " and a name long enough for two lines in the grid.md"
    }

    // The heights of the grid's rows made, each once.
    function rowHeights(f) {
        var out = []
        var kids = f.contentItem.children
        for (var i = 0; i < kids.length; ++i) {
            if (kids[i].regroup !== undefined && out.indexOf(kids[i].height) < 0)
                out.push(kids[i].height)
        }
        return out
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

    // Selecting changes the title, not the height: no description line comes
    // or goes, so nothing under the header jumps.
    function checkSelectionHeight(view, label) {
        var f = view.flickable
        var header = named(view, "folderHeader")
        var bar = named(view, "folderPageHeader")
        if (!header || !bar)
            return
        var before = { "bar": bar.height, "header": header.height, "y": f.y }
        var same = function (when) {
            expect(bar.description === "", label + " " + when + ": no description: " + bar.description)
            expect(bar.height === before.bar && header.height === before.header,
                   label + " " + when + ": the header keeps its height: " + bar.height + " vs " + before.bar
                   + ", " + header.height + " vs " + before.header)
            expect(f.y === before.y && f.contentY === f.originY,
                   label + " " + when + ": the content stays put at its top: " + f.y + " vs " + before.y)
        }
        view.startSelection()
        expect(bar.title.indexOf("lautta-dir-selected") === 0, label + ": selecting shows the count: " + bar.title)
        same("selecting")
        view.endSelection()
        expect(bar.title === view.model.title, label + ": the name comes back: " + bar.title)
        same("after selecting")
    }

    // The pulley is the page's: one, on the host that holds header and
    // content; none on the list or grid, which stop at their top. It ends at
    // the top of the page, above the header. The host never scrolls; pulled,
    // header and content move together and the content stays at its top.
    function checkPulley(view, label) {
        var f = view.flickable
        var host = named(view, "folderPulleyHost")
        var header = named(view, "folderHeader")
        var menus = findAll(view, function (x) { return x._isPullDownMenu === true }, [])
        if (!expect(menus.length === 1, label + ": one pulley: " + menus.length) || !host || !header)
            return
        var menu = menus[0]
        expect(menu.flickable === host && host.pullDownMenu === menu, label + ": the pulley is the host's")
        expect(!f.pullDownMenu && f.boundsBehavior === Flickable.StopAtBounds,
               label + ": the content has none and stops at its top: " + f.boundsBehavior)
        expect(isInside(host.contentItem, header) && isInside(host.contentItem, f),
               label + ": header and content are in the host")
        expect(host.contentHeight === host.height && host.contentY === 0 && host.height === view.height,
               label + ": the host fills the page and does not scroll: " + host.contentHeight + " " + host.height)
        var bottom = menu.mapToItem(view, 0, menu.height).y
        expect(Math.abs(bottom) < 1, label + ": the pulley comes out above the header: " + bottom)
        var by = Theme.itemSizeMedium
        var headerAt = header.mapToItem(view, 0, 0).y
        var contentAt = f.mapToItem(view, 0, 0).y
        host.contentY = -by
        expect(Math.abs(header.mapToItem(view, 0, 0).y - headerAt - by) < 0.5,
               label + ": pulled, the header moves: " + header.mapToItem(view, 0, 0).y)
        expect(Math.abs(f.mapToItem(view, 0, 0).y - contentAt - by) < 0.5,
               label + ": and the content with it: " + f.mapToItem(view, 0, 0).y)
        expect(f.contentY === f.originY, label + ": the content stays at its top: " + f.contentY)
        host.contentY = 0
    }

    // A finger from (x, from) to (x, to) of `item` in steps, still down.
    function dragOn(item, x, from, to) {
        tools.mouse(item, "press", x, from)
        for (var i = 1; i <= 12; ++i)
            tools.mouse(item, "move", x, from + (to - from) * i / 12)
    }

    // The finger goes back to where it started before it lets go: let go on
    // a pulled pulley, an entry under the highlight would run.
    function letGo(item, x, at) {
        tools.mouse(item, "move", x, at)
        tools.mouse(item, "release", x, at)
    }

    function stop(flick) {
        flick.cancelFlick()
        flick.returnToBounds()
        waitFor(function () { return !flick.moving }, "the flickable to stop")
    }

    // Real drags: up and down on the content scroll the content and leave
    // the host; down at the content's top pulls the host, header and all.
    function checkGestures(view, label) {
        var f = view.flickable
        var host = named(view, "folderPulleyHost")
        var header = named(view, "folderHeader")
        if (!host || !header)
            return
        var page = pageStack.currentPage
        var x = f.width / 2
        f.contentY = f.originY
        dragOn(f, x, f.height * 0.8, f.height * 0.2)
        expect(f.contentY > f.originY + Theme.paddingSmall && host.contentY === 0,
               label + ": a drag up scrolls the content, not the host: " + f.contentY + " " + host.contentY)
        tools.mouse(f, "release", x, f.height * 0.2)
        stop(f)
        var scrolled = f.contentY
        // Less far than it scrolled: the content does not reach its top.
        dragOn(f, x, f.height * 0.2, f.height * 0.5)
        expect(f.contentY < scrolled - Theme.paddingSmall && f.contentY > f.originY && host.contentY === 0,
               label + ": a drag down on scrolled content scrolls it back, not the host: "
               + f.contentY + " vs " + scrolled + ", " + host.contentY)
        letGo(f, x, f.height * 0.2)
        stop(f)
        f.contentY = f.originY
        var headerAt = header.mapToItem(view, 0, 0).y
        dragOn(f, x, f.height * 0.2, f.height * 0.2 + Theme.itemSizeLarge)
        var pulled = -host.contentY
        expect(pulled > Theme.paddingLarge && f.contentY === f.originY,
               label + ": a drag down at the content's top pulls the host: " + host.contentY + ", content " + f.contentY)
        expect(Math.abs(header.mapToItem(view, 0, 0).y - headerAt - pulled) < 0.5,
               label + ": the header comes down with it: " + header.mapToItem(view, 0, 0).y + " vs " + pulled)
        expect(host.pullDownMenu && host.pullDownMenu.active, label + ": the pulley is out")
        letGo(f, x, f.height * 0.2)
        tools.pump(100)
        expect(pageStack.currentPage === page && !pageStack.busy, label + ": letting go where it started runs no entry")
        // Let go out of bounds, the pulley's own animation opens it in full.
        // Closed at once as Silica closes it, which stops that animation;
        // contentY set by hand leaves it running into the next check.
        host.pullDownMenu.close(true)
        tools.pump(100)
        expect(!host.pullDownMenu.active && host.contentY === 0, label + ": the pulley closes and stays closed: " + host.contentY)
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
        checkSelectionHeight(view, label)
        checkPulley(view, label)
        checkGestures(view, label)
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
        f.forceLayout()
        var left = f.contentY
        var topItem = f.itemIndexAt(f.width / 2, left + 1)
        expect(left > f.originY + f.height && topItem > 0, mode + ": the parent is scrolled: " + left + " " + topItem)
        if (mode === "grid")
            expect(rowHeights(f).length > 1, "grid rows of different heights: " + rowHeights(f))
        var child = openAtTop(parentView, base + "Child/", files, mode + " child")
        pageStack.pop()
        waitFor(settled, "going back")
        waitFor(function () { return pageStack.currentPage === parentPage && !parentView.model.loading }, "the parent again")
        tools.pump(200)
        f.forceLayout()
        expect(Math.abs(f.contentY - left) < 1, mode + ": back at the old position: " + f.contentY + " vs " + left)
        var again = f.itemIndexAt(f.width / 2, f.contentY + 1)
        expect(again === topItem, mode + ": the same item at the top: " + again + " vs " + topItem)
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
