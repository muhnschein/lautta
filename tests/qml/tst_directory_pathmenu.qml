// SPDX-License-Identifier: LGPL-2.1-or-later Headless: no animation or polish, so the menu keeps
// height 0 and positioners place nothing; rows are clicked by signal, layout checked by size.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components"
import "../../qml/pages"

ApplicationWindow {
    id: win

    readonly property string docs: "lautta://user-documents/"
    readonly property var names: ["Path", "Uni", "Thesis", "Chapters", "3 Methods", "Figures", "Source", "Run 12", "Plots"]

    ViewerTools { id: tools }

    DirectoryModel { id: maker }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    function same(a, b, what) {
        return expect(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        return expect(done(), "timed out waiting for " + what)
    }

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

    function shown(root, name) {
        return findAll(root, function (x) { return x.objectName === name && x.visible && x.height > 0 }, [])
    }

    // Menu rows: laid out, though the closed-looking menu hides them.
    function rows(menu, name) {
        return findAll(menu, function (x) { return x.objectName === name && x.height > 0 }, [])
    }

    function click(item) {
        item.clicked(null)
    }

    function tap(item) {
        tools.mouse(item, "press", item.width / 2, item.height / 2)
        tools.mouse(item, "release", item.width / 2, item.height / 2)
    }

    function uriAt(depth) {
        var u = docs
        for (var i = 0; i < depth; ++i)
            u += (i > 0 ? "/" : "") + encodeURIComponent(names[i])
        return u
    }

    function settled() {
        return !pageStack.busy
    }

    function makeFolders() {
        for (var depth = 0; depth < names.length; ++depth) {
            maker.uri = uriAt(depth)
            waitFor(function () { return !maker.loading }, "listing " + maker.uri)
            maker.createFolder(names[depth])
            waitFor(function () { return maker.indexOf(uriAt(depth + 1)) >= 0 }, "folder " + names[depth])
        }
    }

    function push(uri) {
        pageStack.push(pageComponent, { "uri": uri }, PageStackAction.Immediate)
        waitFor(settled, "push")
        return pageStack.currentPage
    }

    function entryNames(menu) {
        return rows(menu, "pathMenuEntry").map(function (e) {
            var labels = findAll(e, function (x) { return x.text !== undefined && x.truncationMode !== undefined }, [])
            return labels.length > 0 ? labels[0].text : "?"
        })
    }

    function entry(menu, name) {
        return rows(menu, "pathMenuEntry")[entryNames(menu).indexOf(name)]
    }

    function viewOf(page) {
        var views = findAll(page, function (x) { return x.runAction !== undefined && x.startSelection !== undefined }, [])
        return expect(views.length === 1, "one folder view: " + views.length) ? views[0] : null
    }

    function openMenu(page) {
        var menu = named(page, "folderPathMenu")
        var header = named(page, "folderHeader")
        tap(header.children[0])
        expect(menu.open, "tapping the header opens the menu")
        return menu
    }

    function depth() {
        return pageStack.depth
    }

    function checkSubtitle() {
        var page = push(uriAt(names.length))
        var subtitle = named(page, "folderPathSubtitle")
        var bar = named(page, "folderPageHeader")
        expect(bar.title === "Plots", "the folder's name is the title: " + bar.title)
        var full = ["Documents"].concat(names.slice(0, names.length - 1)).join(" › ")
        expect(subtitle.text.indexOf("Documents › ") === 0 && subtitle.text.slice(-" › Run 12".length) === " › Run 12",
               "the subtitle runs from the location to the parent: " + subtitle.text)
        expect(subtitle.text === full || subtitle.text.indexOf(" › … › ") > 0, "whole, or folded in the middle: " + subtitle.text)
        expect(subtitle.width <= page.width - 2 * Theme.horizontalPageMargin + 0.5, "inside the margins: " + subtitle.width)
        var header = named(page, "folderHeader")
        expect(subtitle.mapToItem(page, 0, subtitle.height).y <= header.height, "inside the header")
        expect(subtitle.y >= bar.titleHeight, "under the title: " + subtitle.y)
        var before = header.height
        viewOf(page).startSelection()
        expect(subtitle.visible && subtitle.text.length > 0 && header.height === before,
               "selecting keeps the subtitle and the header's height: " + header.height + " vs " + before)
        var menu = named(page, "folderPathMenu")
        tap(header.children[0])
        expect(!menu.open, "no path menu while selecting")
        viewOf(page).endSelection()
        pageStack.pop(null, PageStackAction.Immediate)

        page = push(docs)
        subtitle = named(page, "folderPathSubtitle")
        expect(!subtitle.visible && subtitle.text === "", "a location's root has no subtitle: " + subtitle.text)
        menu = openMenu(page)
        same(entryNames(menu), ["lautta-dir-path-browse"], "the root's menu: Browse only")
        pageStack.pop(null, PageStackAction.Immediate)
    }

    function checkEntriesAndActions() {
        var page = push(uriAt(names.length))
        var menu = openMenu(page)
        var want = names.slice(0, names.length - 1).reverse().concat(["Documents", "lautta-dir-path-browse"])
        same(entryNames(menu), want, "parent first, location, then Browse")
        var subtitle = named(page, "folderPathSubtitle")
        var actions = rows(menu, "pathMenuAction")
        expect(actions.length === 3, "three actions: " + actions.length)
        var first = entry(menu, "Run 12")
        var isIcon = function (x) { return x.iconSize !== undefined }
        var icons = findAll(first, isIcon, []).concat(findAll(actions[0], isIcon, []))
        var labels = findAll(actions[0], function (x) { return x.truncationMode !== undefined }, [])
        expect(icons.length === 2 && icons.every(function (i) { return i.iconSize === Theme.iconSizeMedium })
               && labels[0].font.pixelSize === Theme.fontSizeMedium, "list item sizes: medium icons and text")
        expect(actions.every(function (a) { return a.width === menu.width && a.height === Theme.itemSizeSmall }),
               "actions are full-width list items: " + actions[0].width + " " + actions[0].height)
        var lastEntry = entry(menu, "lautta-dir-path-browse")
        expect(actions[0].mapToItem(menu, 0, 0).y >= lastEntry.mapToItem(menu, 0, lastEntry.height).y
               && actions[2].mapToItem(menu, 0, 0).y > actions[1].mapToItem(menu, 0, 0).y,
               "actions under the folders, one under another")
        click(entry(menu, "Run 12"))
        expect(!menu.open, "a tap closes the menu")
        waitFor(settled, "going to Run 12")
        pageStack.pop(null, PageStackAction.Immediate)
    }

    // Hold the header, slide down, let go on a row: as Silica's context menu.
    function checkHoldAndSlide() {
        var bottom = depth()
        walk(names.length)
        var page = pageStack.currentPage
        var menu = named(page, "folderPathMenu")
        var press = named(page, "folderHeaderPress")
        var host = named(page, "folderPulleyHost")
        tools.mouse(press, "press", press.width / 2, press.height / 2)
        waitFor(function () { return menu.open }, "the menu opening on hold")
        expect(press.preventStealing && !host.interactive, "the finger stays with the menu: no pulley, no flick")
        var thesis = entry(menu, "Thesis")
        var at = thesis.mapToItem(press, thesis.width / 2, thesis.height / 2)
        tools.mouse(press, "move", at.x, at.y)
        expect(thesis.mapToItem(menu, 0, 0).y > menu.maxHeight && menu._dragRow === null,
               "a row past the menu's height is hidden: none lights up")
        // Test window is short.
        menu.maxHeight = -1
        tools.mouse(press, "move", at.x, at.y)
        expect(thesis.highlighted && menu._dragRow === thesis, "the row under the finger lights up")
        var goTo = rows(menu, "pathMenuAction")[1]
        var atAction = goTo.mapToItem(press, goTo.width / 2, goTo.height / 2)
        tools.mouse(press, "move", atAction.x, atAction.y)
        expect(goTo.highlighted && !thesis.highlighted, "and only that row: actions too")
        tools.mouse(press, "move", press.width / 2, press.height / 2)
        expect(menu._dragRow === null, "back on the header: no row")
        tools.mouse(press, "move", at.x, at.y)
        tools.mouse(press, "release", at.x, at.y)
        expect(!press.preventStealing && !menu.open, "letting go opens the row and closes the menu")
        waitFor(function () { return settled() && pageStack.currentPage.uri === uriAt(3) }, "back on Thesis")
        expect(depth() === bottom + 3, "as a tap on Thesis: " + depth())

        page = pageStack.currentPage
        menu = named(page, "folderPathMenu")
        press = named(page, "folderHeaderPress")
        tools.mouse(press, "press", press.width / 2, press.height / 2)
        waitFor(function () { return menu.open }, "the menu opening on hold")
        tools.mouse(press, "release", press.width / 2, press.height / 2)
        tools.pump(100)
        expect(menu.open && pageStack.currentPage === page, "let go on the header: menu stays open, rows tappable")
        menu.close()
        while (depth() > bottom)
            pageStack.pop(null, PageStackAction.Immediate)
    }

    // Open menu: rest of the view dims; a press there only closes the menu.
    function checkShade() {
        var page = push(uriAt(5))
        waitFor(function () { return !viewOf(page).model.loading && viewOf(page).model.count === 1 }, "Figures listed")
        var shade = named(page, "pathMenuShade")
        expect(!shade.visible, "no shade while the menu is closed")
        var menu = openMenu(page)
        expect(shade.visible, "shade under the open menu")
        expect(shade.mapToItem(page, 0, 0).y >= menu.mapToItem(page, 0, 0).y, "shade starts under the menu, header stays bright")
        var before = depth()
        tools.mouse(shade, "press", shade.width / 2, Theme.itemSizeSmall / 2)
        tools.mouse(shade, "release", shade.width / 2, Theme.itemSizeSmall / 2)
        tools.pump(200)
        expect(!menu.open, "a press beside the menu closes it")
        expect(depth() === before && pageStack.currentPage === page, "and opens nothing under it: " + depth())
        pageStack.pop(null, PageStackAction.Immediate)
    }

    function walk(to) {
        for (var d = 1; d <= to; ++d)
            push(uriAt(d))
    }

    function checkGoesBack() {
        var bottom = depth()
        walk(names.length)
        expect(depth() === bottom + names.length, "walked down: " + depth())
        var menu = openMenu(pageStack.currentPage)
        click(entry(menu, "Thesis"))
        waitFor(settled, "going back")
        expect(pageStack.currentPage.uri === uriAt(3), "back on Thesis's own page: " + pageStack.currentPage.uri)
        expect(depth() === bottom + 3, "the pages below it stay, those above are gone: " + depth())
        menu = openMenu(pageStack.currentPage)
        click(entry(menu, "Documents"))
        waitFor(settled, "up to the location")
        expect(pageStack.currentPage.uri === docs && pageStack.currentPage.folderPage,
               "the location, never opened on its own page, replaces the pages in it: " + pageStack.currentPage.uri)
        expect(depth() === bottom + 1, "nothing to swipe back into: " + depth())
        pageStack.pop(null, PageStackAction.Immediate)
        expect(depth() === bottom, "back at the start")

        push(uriAt(2))
        push(uriAt(names.length))
        menu = openMenu(pageStack.currentPage)
        click(entry(menu, "Source"))
        waitFor(settled, "up from a jump")
        expect(pageStack.currentPage.uri === uriAt(7) && depth() === bottom + 2,
               "the folder replaces the jumped-to page, above its ancestor: " + depth())
        pageStack.pop(null, PageStackAction.Immediate)
        expect(pageStack.currentPage.uri === uriAt(2), "swiping back goes up: " + pageStack.currentPage.uri)
        menu = openMenu(pageStack.currentPage)
        click(entry(menu, "lautta-dir-path-browse"))
        waitFor(settled, "Browse")
        expect(pageStack.currentPage === startPage, "Browse goes back to the first page")
    }

    function sections(dialog) {
        return shown(dialog, "goToRow").map(function (r) { return r.section }).join()
    }

    function checkGoToFolder() {
        var bottom = depth()
        var page = push(uriAt(2))
        var menu = openMenu(page)
        var goTo = rows(menu, "pathMenuAction")[1]
        click(goTo)
        waitFor(function () { return settled() && pageStack.currentPage !== page }, "Go to folder")
        var dialog = pageStack.currentPage
        var field = named(dialog, "goToField")
        expect(field.text === "~/Documents/Path/Uni" && field.selectedText === field.text,
               "opens with the address selected: " + field.text + " / " + field.selectedText)
        expect(!dialog.canAccept, "nothing to go to yet")
        expect(shown(dialog, "goToRow").length === 0, "nothing under the field until typing")
        field.text = "~/Documents/Path/Uni/Thesis/Chap"
        waitFor(function () { return dialog.fresh && shown(dialog, "goToRow").length === 1 }, "the path's folders")
        expect(dialog.chosenUri === uriAt(4), "Go opens the top row: " + dialog.chosenUri)
        shown(dialog, "goToRow")[0].clicked()
        expect(field.text === "~/Documents/Path/Uni/Thesis/Chapters/", "a folder of the path drills in: " + field.text)
        waitFor(function () { return dialog.fresh && dialog.chosenUri === uriAt(4) }, "Go to Chapters")
        expect(sections(dialog) === ",lautta-goto-in", "In header under Go to: " + sections(dialog))
        field.text = "~/D"
        waitFor(function () { return dialog.fresh && shown(dialog, "goToRow").length === 2 }, "user folders in home")
        same(shown(dialog, "goToRow").map(function (r) { return r.hit.name }), ["Documents", "Downloads"], "home lists none")
        expect(sections(dialog) === ",", "and no In header between them: " + sections(dialog))
        field.text = "~/Documents/Path/Nope"
        waitFor(function () { return dialog.fresh }, "a path that names nothing")
        expect(dialog.noFolder && field.errorHighlight && field.label === "lautta-goto-none" && !dialog.canAccept,
               "No such folder: " + field.label)
        field.text = "figur"
        waitFor(function () { return dialog.fresh && shown(dialog, "goToRow").length > 0 }, "a name")
        expect(dialog.chosenUri === uriAt(6), "found by name among listed folders: " + dialog.chosenUri)
        expect(shown(dialog, "goToSearch").length === 1, "and Search is offered")
        dialog.accept()
        waitFor(function () { return settled() && pageStack.currentPage.folderPage && pageStack.currentPage.uri === uriAt(6) },
                "the picked folder")
        expect(depth() === bottom + 2, "a folder below opens on top: " + depth())
        pageStack.pop(null, PageStackAction.Immediate)
        pageStack.pop(null, PageStackAction.Immediate)
    }

    function checkLandscape() {
        var page = push(uriAt(names.length))
        win.deviceOrientation = Orientation.Landscape
        waitFor(function () { return page.isLandscape && !page.orientationTransitionRunning }, "landscape")
        var menu = openMenu(page)
        expect(menu.wide, "wide in landscape")
        var entries = rows(menu, "pathMenuEntry")
        var columns = []
        entries.forEach(function (e) {
            if (columns.indexOf(e.parent) < 0)
                columns.push(e.parent)
        })
        expect(columns.length === 2 && Math.abs(entries[0].width - menu.width / 2) < 0.5,
               "two columns: " + columns.length + ", " + entries[0].width + " of " + menu.width)
        same(entryNames(menu).slice(0, 2), ["Run 12", "Source"], "read down first")
        expect(entries[0].parent === entries[4].parent && entries[5].parent !== entries[0].parent,
               "five in the first column, five in the second")
        expect(Math.abs(entries[5].mapToItem(menu, 0, 0).x - menu.width / 2) < 0.5
               && entries[5].mapToItem(menu, 0, 0).y === entries[0].mapToItem(menu, 0, 0).y,
               "second column beside the first: " + entries[5].mapToItem(menu, 0, 0).x)
        menu.close()
        win.deviceOrientation = Orientation.Portrait
        waitFor(function () { return !page.isLandscape && !page.orientationTransitionRunning }, "portrait")
        pageStack.pop(null, PageStackAction.Immediate)
    }

    function checkPicking() {
        var menu = pickingMenu.createObject(win.contentItem, { "pathModel": pickingPath, "open": true })
        var asked = []
        menu.navigate.connect(function (uri) { asked.push("navigate:" + uri) })
        menu.browse.connect(function () { asked.push("browse") })
        menu.goToRequested.connect(function () { asked.push("goto") })
        same(entryNames(menu), ["Path", "Documents", "lautta-dir-path-locations"], "picking: Locations last")
        var actions = rows(menu, "pathMenuAction")
        expect(actions.length === 1, "picking: one action: " + actions.length)
        click(entry(menu, "lautta-dir-path-locations"))
        click(entry(menu, "Path"))
        click(actions[0])
        same(asked, ["navigate:", "navigate:" + uriAt(1), "goto"], "picking: Locations is the empty folder")
        menu.destroy()
    }

    Component.onCompleted: {
        makeFolders()
        maker.uri = uriAt(5)
        waitFor(function () { return !maker.loading && maker.count === 1 }, "Figures listed for Go to")
        checkSubtitle()
        checkEntriesAndActions()
        checkGoesBack()
        checkHoldAndSlide()
        checkShade()
        checkGoToFolder()
        checkLandscape()
        checkPicking()
    }

    PathModel {
        id: pickingPath

        uri: uriAt(2)
    }

    Component {
        id: pickingMenu

        PathMenu { picking: true }
    }

    initialPage: Page {
        id: startPage

        readonly property bool browsePage: true
    }

    Component {
        id: pageComponent

        DirectoryPage { }
    }
}
