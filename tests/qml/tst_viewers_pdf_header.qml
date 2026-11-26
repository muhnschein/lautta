// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/viewers"

ApplicationWindow {
    id: window
    property bool reachedEnd

    ViewerTools { id: tools }

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
        return out
    }

    function named(root, name) {
        var all = findAll(root, function (x) { return x.objectName === name }, [])
        expect(all.length === 1, "one " + name + ": " + all.length)
        return all[0]
    }

    function isInside(ancestor, item) {
        for (var p = item; p; p = p.parent) {
            if (p === ancestor)
                return true
        }
        return false
    }

    function dragOn(item, x, from, to) {
        tools.mouse(item, "press", x, from)
        for (var i = 1; i <= 12; ++i)
            tools.mouse(item, "move", x, from + (to - from) * i / 12)
    }

    function letGo(item, x, at) {
        tools.mouse(item, "move", x, at)
        tools.mouse(item, "release", x, at)
    }

    function stop(flick) {
        flick.cancelFlick()
        flick.returnToBounds()
        waitFor(function () { return !flick.moving }, "the flickable to stop")
    }

    function closeMenu(host) {
        host.pullDownMenu.close(true)
        waitFor(function () { return !host.pullDownMenu.active && host.contentY === 0 }, "the pulley to close")
    }

    function checkLayout(viewer, label) {
        var host = named(viewer, "pdfPulleyHost")
        var header = named(viewer, "pdfHeader")
        var view = named(viewer, "pdfPages")
        var menus = findAll(viewer, function (x) { return x._isPullDownMenu === true }, [])
        if (!expect(menus.length === 1, label + ": one pulley: " + menus.length) || !host || !header || !view)
            return
        var menu = menus[0]
        expect(menu.flickable === host && host.pullDownMenu === menu, label + ": the pulley is the host's")
        expect(!view.pullDownMenu && view.boundsBehavior === Flickable.StopAtBounds,
               label + ": the pages have none and stop at their top: " + view.boundsBehavior)
        expect(isInside(host.contentItem, header) && isInside(host.contentItem, view),
               label + ": header and pages are in the host")
        expect(host.contentHeight === host.height && host.contentY === 0 && host.height === viewer.height,
               label + ": the host fills the page and does not scroll: " + host.contentHeight + " " + host.height)
        expect(Math.abs(menu.mapToItem(viewer, 0, menu.height).y) < 1,
               label + ": the pulley comes out above the header: " + menu.mapToItem(viewer, 0, menu.height).y)
        var titles = findAll(header, function (x) { return x.title !== undefined && x.description !== undefined }, [])
        expect(titles.length === 1 && titles[0].title === viewer.fileName && titles[0].description === "",
               label + ": the header names the file, nothing more: "
               + (titles.length ? "'" + titles[0].title + "' '" + titles[0].description + "'" : titles.length))
        expect(Math.abs(view.mapToItem(viewer, 0, 0).y - header.height) < 0.5,
               label + ": the pages start under the header: " + view.mapToItem(viewer, 0, 0).y + " vs " + header.height)
    }

    function checkScrolled(viewer, label) {
        var host = named(viewer, "pdfPulleyHost")
        var header = named(viewer, "pdfHeader")
        var view = named(viewer, "pdfPages")
        if (!host || !header || !view)
            return
        var x = view.width / 2
        var headerAt = header.mapToItem(viewer, 0, 0).y
        view.positionViewAtEnd()
        expect(view.contentY > view.originY + view.height && viewer.currentPage === viewer.document.pageCount,
               label + ": at the last page: " + view.contentY + ", page " + viewer.currentPage)
        expect(Math.abs(header.mapToItem(viewer, 0, 0).y - headerAt) < 0.5 && header.visible,
               label + ": the header stays where it was: " + header.mapToItem(viewer, 0, 0).y)
        var scrolled = view.contentY
        dragOn(view, x, view.height * 0.2, view.height * 0.5)
        expect(view.contentY < scrolled - Theme.paddingSmall && host.contentY === 0,
               label + ": a drag down on the pages turns them back, no pulley: " + view.contentY + " vs " + scrolled
               + ", host " + host.contentY)
        letGo(view, x, view.height * 0.2)
        stop(view)
        scrolled = view.contentY
        var y = header.height / 2
        dragOn(header, x, y, y + Theme.itemSizeLarge)
        var pulled = -host.contentY
        expect(pulled > Theme.paddingLarge && Math.abs(view.contentY - scrolled) < 0.5,
               label + ": a drag down on the header pulls the host on the last page: " + host.contentY
               + ", pages " + view.contentY + " vs " + scrolled)
        expect(Math.abs(header.mapToItem(viewer, 0, 0).y - headerAt - pulled) < 0.5,
               label + ": the header comes down with it: " + header.mapToItem(viewer, 0, 0).y)
        expect(host.pullDownMenu.active, label + ": the pulley is out")
        letGo(header, x, y)
        waitFor(function () { return !host.moving }, label + ": the host to settle")
        expect(pageStack.currentPage === viewer && !pageStack.busy, label + ": letting go where it started runs no entry")
        closeMenu(host)
        expect(Math.abs(view.contentY - scrolled) < 0.5, label + ": the pages stay on the last page: " + view.contentY)

        view.positionViewAtBeginning()
        stop(view)
        var top = view.contentY
        expect(view.atYBeginning, label + ": back at the first page's top: " + top + " vs " + view.originY)
        dragOn(view, x, view.height * 0.2, view.height * 0.2 + Theme.itemSizeLarge)
        expect(-host.contentY > Theme.paddingLarge && Math.abs(view.contentY - top) < 0.5,
               label + ": a drag down at the first page's top pulls the host: " + host.contentY + ", pages " + view.contentY)
        letGo(view, x, view.height * 0.2)
        waitFor(function () { return !host.moving }, label + ": the host to settle")
        closeMenu(host)
    }

    function checkZoomed(viewer, label) {
        var header = named(viewer, "pdfHeader")
        var view = named(viewer, "pdfPages")
        if (!header || !view)
            return
        var headerAt = header.mapToItem(viewer, 0, 0)
        viewer.zoomTo(2.5, view.width / 2, view.height / 2)
        view.contentX = view.contentWidth - view.width
        expect(view.contentX > 0 && Math.abs(header.mapToItem(viewer, 0, 0).x - headerAt.x) < 0.5
               && Math.abs(header.mapToItem(viewer, 0, 0).y - headerAt.y) < 0.5 && header.width === viewer.width,
               label + ": zoomed and panned, the header stays put: " + header.mapToItem(viewer, 0, 0).x + " "
               + header.width)
        viewer.zoomTo(1, 0, 0)
        waitFor(function () { return viewer.renderZoom === 1 }, label + ": zoom back")
    }

    function checkProblem() {
        var viewer = pageStack.push(lockedComponent, {}, PageStackAction.Immediate)
        waitFor(function () { return pageStack.currentPage.wrong !== undefined && !pageStack.busy }, "the password dialog")
        pageStack.currentPage.reject()
        waitFor(function () { return pageStack.currentPage === viewer && !pageStack.busy }, "the viewer again")
        var host = named(viewer, "pdfPulleyHost")
        var menus = findAll(viewer, function (x) { return x._isPullDownMenu === true }, [])
        var unlock = findAll(viewer, function (x) { return x.text === qsTrId("lautta-viewers-pdf-unlock") }, [])
        expect(menus.length === 1 && menus[0].flickable === host && unlock.length === 1 && isInside(menus[0], unlock[0]),
               "locked: Unlock is on the host's pulley: " + menus.length + " " + unlock.length)
        var hints = findAll(viewer, function (x) { return x.hintText !== undefined && x.enabled }, [])
        expect(hints.length === 1 && hints[0].hintText === qsTrId("lautta-viewers-pdf-pull-hint"),
               "locked: the placeholder says to pull down")
        pageStack.pop(null, PageStackAction.Immediate)
    }

    Component.onCompleted: {
        if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/twelve-pages.pdf"), "lautta://user-documents/header-long.pdf"))
            console.error("FAILED: fixture not installed")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/secret.pdf"), "lautta://user-documents/header-locked.pdf")
        pageStack.push(emptyPage, {}, PageStackAction.Immediate)
        var viewer = pageStack.push(longComponent, {}, PageStackAction.Immediate)
        waitFor(function () { return viewer.document.loaded }, "the PDF")
        named(viewer, "pdfPages").forceLayout()
        checkLayout(viewer, "portrait")
        checkScrolled(viewer, "portrait")
        checkZoomed(viewer, "portrait")
        window.deviceOrientation = Orientation.Landscape
        waitFor(function () { return viewer.isLandscape && !viewer.orientationTransitionRunning }, "landscape")
        named(viewer, "pdfPages").forceLayout()
        checkLayout(viewer, "landscape")
        checkScrolled(viewer, "landscape")
        window.deviceOrientation = Orientation.Portrait
        waitFor(function () { return !viewer.isLandscape && !viewer.orientationTransitionRunning }, "portrait again")
        pageStack.pop(null, PageStackAction.Immediate)
        checkProblem()
        reachedEnd = true
    }

    Component {
        id: emptyPage
        Page { }
    }

    Component {
        id: longComponent
        PdfViewer { uri: "lautta://user-documents/header-long.pdf" }
    }

    Component {
        id: lockedComponent
        PdfViewer { uri: "lautta://user-documents/header-locked.pdf" }
    }
}
