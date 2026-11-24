// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"
import "../../qml/components/directory"
import "../../qml/components/directory/PathFold.js" as PathFold

ApplicationWindow {
    id: win
    property bool reachedEnd

    readonly property string docs: "lautta://user-documents/"
    readonly property string longName: "Delta" + new Array(11).join("AndOnAndOn")
    readonly property var names: ["Crumbs", "Alpha", "Beta", "Gamma", longName]
    readonly property string homeRow: "lautta-home-title"

    ViewerTools { id: tools }

    DirectoryModel { id: maker }

    PathModel { id: deep }

    FontMetrics {
        id: stockTitle

        font.pixelSize: Theme.fontSizeLarge
    }

    FontMetrics {
        id: lineText

        font.pixelSize: Theme.fontSizeMedium
        font.family: Theme.fontFamily
    }

    ListModel {
        id: plain

        ListElement { uri: "one"; name: "One" }
        ListElement { uri: "two"; name: "Two" }
        ListElement { uri: "three"; name: "Three" }
    }

    initialPage: Component {
        Page { }
    }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    function same(a, b, what) {
        return expect(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    function near(a, b, what) {
        return expect(Math.abs(a - b) < 0.01, what + ": " + a + " != " + b)
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

    function all(root, name) {
        return findAll(root, function (x) { return x.objectName === name }, [])
    }

    function named(root, name) {
        var found = all(root, name)
        return expect(found.length === 1, "one item called " + name + ": " + found.length) ? found[0] : null
    }

    function viewOf(page) {
        var views = findAll(page, function (x) { return x.runAction !== undefined && x.startSelection !== undefined }, [])
        return expect(views.length === 1, "one folder view: " + views.length) ? views[0] : null
    }

    function uriOf(depth) {
        return docs + names.slice(0, depth).join("/")
    }

    function settled() {
        return !pageStack.busy
    }

    function tap(item) {
        tools.mouse(item, "press", item.width / 2, item.height / 2)
        tools.mouse(item, "release", item.width / 2, item.height / 2)
    }

    function makeFolders() {
        for (var depth = 0; depth < names.length; ++depth) {
            maker.uri = uriOf(depth)
            maker.createFolder(names[depth])
            var made = uriOf(depth + 1)
            waitFor(function () { return maker.indexOf(made) >= 0 }, "folder " + made)
        }
    }

    function stepsOf(root) {
        return all(root, "crumb")
    }

    function widthsOf(steps) {
        var widths = []
        for (var i = 0; i < steps.length - 1; ++i)
            widths.push(steps[i].crumbWidth)
        return widths
    }

    function shown(steps) {
        var out = []
        for (var i = 0; i < steps.length; ++i) {
            if (steps[i].visible)
                out.push(steps[i].name)
        }
        return out
    }

    function laidOut(steps, row, start, what) {
        var x = start
        for (var i = 0; i < steps.length; ++i) {
            if (!steps[i].visible)
                continue
            same(steps[i].x, x, what + ": " + steps[i].name + " starts where the one before ends")
            x += steps[i].width
        }
        same(row.width, x, what + ": the path ends with its last folder")
    }

    function pulleyEntry(page, name) {
        var menus = findAll(page, function (x) { return x._isPullDownMenu === true }, [])
        if (!expect(menus.length === 1, "one pulley: " + menus.length))
            return null
        var menu = menus[0]
        menu.active = true
        var kids = menu._contentColumn.children
        var out = null
        for (var i = 0; i < kids.length; ++i) {
            if (kids[i].objectName === name)
                out = { "item": kids[i], "shown": kids[i].visible }
        }
        menu.active = false
        return out
    }

    function rowsOf(menu) {
        return findAll(menu, function (x) { return x.hasOwnProperty("__silica_menuitem") }, [])
    }

    function texts(rows) {
        return rows.map(function (row) { return row.text })
    }

    function tapRow(menu, row) {
        var at = row.mapToItem(menu, row.width / 2, row.height / 2)
        tools.mouse(menu, "press", at.x, at.y)
        tools.mouse(menu, "release", at.x, at.y)
    }

    function openFolded(root) {
        tap(named(root, "crumbDots"))
        var menu = named(root, "crumbMenu")
        if (menu)
            waitFor(function () { return menu.active && menu._expanded }, "the folded folders to open")
        return menu
    }

    function checkFolding() {
        deep.uri = uriOf(5)
        var page = pageStack.push(benchPage, {}, PageStackAction.Immediate)
        var host = named(page, "bench")
        var crumbs = host.crumbs
        var row = named(host, "crumbRow")
        var home = named(host, "crumbHome")
        var dots = named(host, "crumbDots")
        var separator = named(host, "crumbSeparator")
        var title = named(host, "crumbTitle")
        var steps = stepsOf(host)
        if (!same(steps.length, 6, "a crumb per step of the path"))
            return
        var setRoom = function (room) {
            crumbs.width = room + title.implicitWidth + crumbs.reserve
        }
        var widths = widthsOf(steps)
        var sep = separator.width
        var fitting = function (folded) {
            return PathFold.tailWidth(widths, sep, folded)
        }
        expect(home.width > dots.width + 1 && widths[0] > 0, "crumbs have a width, the house more than the dots: " + home.width + " " + dots.width + " " + widths)
        same(crumbs.reserve, dots.width + sep + Theme.paddingMedium, "kept free of the name: the dots and their separator")
        var house = named(host, "crumbHouse")
        expect(String(house.source).indexOf("/qml/icons/home.svg?stroke=") > 0, "the house is Lautta's own drawing: " + house.source)
        var houseAt = fitting(0) + home.width - dots.width

        same(title.text, "Here", "the line ends with the folder's name")
        same([title.font.pixelSize, title.font.family], [separator.font.pixelSize, separator.font.family], "name and path are one size")
        same(title.y, separator.y, "on one line")
        expect(Qt.colorEqual(title.color, Theme.highlightColor) && !Qt.colorEqual(separator.color, title.color), "its colour sets the name apart: " + title.color)
        same(title.truncationMode, TruncationMode.Fade, "a long name fades out")
        var capMiddle = Theme.paddingLarge + stockTitle.ascent - stockTitle.tightBoundingRect("H").height / 2
        expect(Math.abs(title.y + title.baselineOffset - lineText.tightBoundingRect("H").height / 2 - capMiddle) <= 0.5,
               "its capitals stand around the middle of a stock title's capitals: " + title.y + " " + title.baselineOffset + " " + capMiddle)
        expect(Math.abs(house.y + house.height / 2 - capMiddle) <= 0.5, "and so does the house: " + house.y + " " + house.height + " " + capMiddle)

        setRoom(houseAt + 0.5)
        same(title.width, title.implicitWidth, "the name shows whole")
        same(title.x + title.width, crumbs.width, "at the line's end")
        expect(home.visible && !dots.visible, "room for the house and every folder: the house leads, no dots")
        same(crumbs.folded, 0, "nothing folded")
        same(shown(steps), ["Documents", "Crumbs", "Alpha", "Beta", "Gamma"], "every folder above shows, the folder itself is the line's name")
        near(row.mapToItem(crumbs, row.width, 0).x, title.x - Theme.paddingMedium, "the path ends beside the name")
        near(row.mapToItem(crumbs, 0, 0).x, 0.5, "and starts where the line starts")
        same(separator.x, home.width, "a separator follows the house")
        laidOut(steps, row, home.width + sep, "all shown")
        tap(home)
        same(host.homes, 1, "a tap on the house goes home")
        same(host.picked.length, 0, "and picks no folder")

        setRoom(houseAt - 0.5)
        expect(!home.visible && dots.visible, "a little short: the dots take the house's place")
        same(crumbs.folded, 0, "every folder still fits")
        same(shown(steps), ["Documents", "Crumbs", "Alpha", "Beta", "Gamma"], "and shows")
        same(dots.x, 0, "the dots lead")
        same(separator.x, dots.width, "a separator follows them")
        laidOut(steps, row, dots.width + sep, "dots first")

        setRoom(houseAt + 100)
        crumbs.homeInset = Theme.paddingMedium + 99.5
        expect(home.visible && !dots.visible, "the house may stand where the part kept clear of it ends")
        crumbs.homeInset = Theme.paddingMedium + 100.5
        expect(!home.visible && dots.visible, "a little into it: no house")
        same(crumbs.folded, 0, "the folders stay")
        crumbs.homeInset = 0
        expect(home.visible, "nothing to keep clear: the house is back")

        setRoom(fitting(0) - 1)
        same(crumbs.folded, 1, "one pixel short for the folders: the farthest folds")
        expect(!home.visible && dots.visible, "behind the dots")
        same(shown(steps), ["Crumbs", "Alpha", "Beta", "Gamma"], "the rest shows")

        setRoom(fitting(3))
        same(crumbs.folded, 3, "narrower: more fold")
        same(shown(steps), ["Beta", "Gamma"], "the nearest stay")
        laidOut(steps, row, dots.width + sep, "folded")
        near(row.mapToItem(crumbs, 0, 0).x, 0, "the path starts where the line starts")

        setRoom(0)
        same(crumbs.folded, 5, "no room beside the name: all fold")
        same(shown(steps), [], "no folder shows")
        same(row.width + Theme.paddingMedium, crumbs.reserve, "the dots fill exactly what was kept free")
        same(title.width, title.implicitWidth, "the name still shows whole")

        crumbs.width = crumbs.reserve + title.implicitWidth / 2
        near(title.width, title.implicitWidth / 2, "a name too long for the line is cut, the dots keep their room")
        near(row.mapToItem(crumbs, 0, 0).x, 0, "at the line's start")
        same(crumbs.folded, 5, "and every folder stays folded")

        crumbs.pathShown = false
        expect(!row.visible, "no path asked for: none shows")
        same(crumbs.reserve, 0, "and no room is kept for it")
        near(title.width, Math.min(title.implicitWidth, crumbs.width), "the name takes the line")
        crumbs.pathShown = true
        expect(row.visible, "the path comes back")

        setRoom(fitting(3))
        tap(steps[4].children[0])
        same(host.picked, [uriOf(4)], "a tap on a folder picks it")
        same(host.homes, 1, "and does not go home")

        var bare = host.height
        var menu = openFolded(host)
        if (!menu)
            return
        expect(menu.parent === host, "the menu hangs under the header")
        same(menu.height, 2 * Theme.itemSizeSmall, "no more rows than fit below")
        same(host.height, bare + menu.height, "the header grows by the menu")
        var rows = rowsOf(menu)
        same(texts(rows), ["Alpha", homeRow], "the nearest folded folder, then the way home")
        tapRow(menu, rows[0])
        same(host.picked.length, 1, "nothing is picked while the menu is still open")
        waitFor(function () { return host.picked.length === 2 }, "the pick once the menu closed")
        same(host.picked[1], uriOf(2), "the row's folder is picked")
        same(host.homes, 1, "not home")
        waitFor(function () { return host.height === bare && !menu.active }, "the header to shrink back")

        setRoom(houseAt + 0.5)
        deep.uri = uriOf(1)
        steps = stepsOf(host)
        same(steps.length, 2, "another path: its crumbs")
        same(crumbs.folded, 0, "folded anew for the new path")
        same(shown(steps), ["Documents"], "the one folder above")
        expect(home.visible, "with room, the house leads")
        deep.uri = docs
        same(shown(stepsOf(host)), [], "a place's top folder: no folder above")
        expect(home.visible && !dots.visible, "the house alone")
        setRoom(0)
        expect(!home.visible && dots.visible, "a top folder whose name takes the line: dots, no house")
        same(row.width + Theme.paddingMedium, crumbs.reserve, "in the room kept free")

        crumbs.pathModel = plain
        steps = stepsOf(host)
        widths = widthsOf(steps)
        setRoom(fitting(0))
        same(crumbs.folded, 0, "another model: room for all its folders")
        plain.setProperty(0, "name", "A longer name")
        expect(steps[0].crumbWidth > widths[0], "a longer name makes a wider crumb: " + steps[0].crumbWidth + " vs " + widths[0])
        same(crumbs.folded, 1, "and the path folds anew")
        same(shown(steps), ["Two"], "the nearest still shows")
        pageStack.pop(null, PageStackAction.Immediate)
    }

    function goHomeFrom(host, rowTexts, what) {
        var bare = host.height
        var menu = openFolded(host)
        if (!menu)
            return
        var rows = rowsOf(menu)
        if (!same(texts(rows), rowTexts, what))
            return
        tapRow(menu, rows[rows.length - 1])
        same(host.homes, 0, what + ": nothing happens while the menu is still open")
        waitFor(function () { return host.homes === 1 }, what + ": going home once the menu closed")
        same(host.picked, [], what + ": the home row picks no folder")
        waitFor(function () { return host.height === bare && !menu.active }, what + ": the header to shrink back")
    }

    function checkHomeRow() {
        deep.uri = uriOf(5)
        var page = pageStack.push(benchPage, { "most": 8 }, PageStackAction.Immediate)
        var host = named(page, "bench")
        host.crumbs.width = PathFold.tailWidth(widthsOf(stepsOf(host)), named(host, "crumbSeparator").width, 3)
                            + named(host, "crumbTitle").implicitWidth + host.crumbs.reserve
        same(host.crumbs.folded, 3, "three folders fold")
        goHomeFrom(host, ["Alpha", "Crumbs", "Documents", homeRow], "room below: every folded folder, nearest first, home last")
        pageStack.pop(null, PageStackAction.Immediate)

        deep.uri = docs
        page = pageStack.push(benchPage, { "most": 8 }, PageStackAction.Immediate)
        host = named(page, "bench")
        host.crumbs.width = host.crumbs.reserve
        expect(named(host, "crumbDots").visible, "a top folder whose name takes the line: dots")
        goHomeFrom(host, [homeRow], "their menu holds the way home alone")
        pageStack.pop(null, PageStackAction.Immediate)
    }

    function open(uri, immediate) {
        pageStack.push(folderPage, { "uri": uri }, immediate ? PageStackAction.Immediate : PageStackAction.Animated)
        waitFor(settled, "the page of " + uri)
        return pageStack.currentPage
    }

    function pickFolded(page, name) {
        var menu = openFolded(page)
        if (!menu)
            return
        var rows = rowsOf(menu).filter(function (row) { return row.text === name })
        if (!same(rows.length, 1, "one folded folder called " + name))
            return
        tapRow(menu, rows[0])
        waitFor(function () { return pageStack.currentPage !== page && settled() }, "the tap on " + name + " to leave the page")
    }

    function checkWalkedDown() {
        var start = pageStack.currentPage
        var pages = [start]
        for (var depth = 1; depth <= 5; ++depth)
            pages.push(open(uriOf(depth), depth === 1))
        same(pageStack.depth, 6, "walked down five folders")
        same(pages[5].trail, [docs, uriOf(1), uriOf(2), uriOf(3), uriOf(4), uriOf(5)], "a folder page knows its path")

        var view = viewOf(pages[5])
        var header = named(view, "folderHeader")
        var bar = named(view, "folderPageHeader")
        var crumbs = named(view, "folderCrumbs")
        same(named(view, "crumbTitle").text, longName, "the header names the folder")
        same(bar.leftMargin, Theme.itemSizeSmall, "the line starts right of where a tap goes a page back")
        same(crumbs.homeInset, view.width / 4 - Theme.itemSizeSmall, "the house keeps out of the page's left quarter")
        same(crumbs.folded, 5, "the name takes the whole header: every folder above folds")
        expect(!named(view, "crumbHome").visible, "and the house with them")
        var bare = header.height
        var listAt = view.flickable.y
        var menu = openFolded(pages[5])
        if (!menu)
            return
        same(texts(rowsOf(menu)), ["Gamma", homeRow], "the nearest folded folder that fits the page, then the way home")
        same(header.height, bare + menu.height, "the folder's header grows by the menu")
        same(view.flickable.y, listAt + menu.height, "and the content moves down with it")
        same(menu.mapToItem(view, 0, 0).y, bar.height, "the menu opens right under the name")
        same(menu.width, view.width, "over the page's whole width")
        tapRow(menu, rowsOf(menu)[0])
        waitFor(function () { return pageStack.currentPage !== pages[5] && settled() }, "the tap on Gamma to leave the page")
        expect(pageStack.currentPage === pages[4], "a tap on a folder walked through goes back to its page")
        same(pageStack.depth, 5, "the page above it is gone")

        named(pages[4], "folderCrumbs").picked(uriOf(1))
        waitFor(function () { return pageStack.currentPage !== pages[4] && settled() }, "the pick of Crumbs to leave the page")
        expect(pageStack.currentPage === pages[1], "further up: that folder's page")
        same(pageStack.depth, 2, "nothing left above it")

        for (depth = 2; depth <= 4; ++depth)
            open(uriOf(depth), false)
        var top = open(uriOf(5), false)
        same(pageStack.depth, 6, "walked down again")
        pickFolded(top, homeRow)
        expect(pageStack.currentPage === start, "the way home goes back to the first page")
        same(pageStack.depth, 1, "all folder pages are gone")
    }

    function checkEnteredDeep() {
        var start = pageStack.currentPage
        var entered = open(uriOf(5), true)
        pickFolded(entered, "Gamma")
        var gamma = pageStack.currentPage
        expect(gamma !== entered && gamma !== start, "a folder not walked through gets a page")
        same(gamma.trail, [docs, uriOf(1), uriOf(2), uriOf(3), uriOf(4)], "of that folder")
        same(pageStack.depth, 2, "in place of the page left")
        expect(pageStack.previousPage(gamma) === start, "back from it goes where the old page went back to")

        var top = open(uriOf(5), false)
        same(pageStack.depth, 3, "a folder further down")
        named(top, "folderCrumbs").picked(uriOf(1))
        waitFor(function () { return pageStack.currentPage !== top && settled() }, "the pick of Crumbs to leave the page")
        var crumbsPage = pageStack.currentPage
        same(crumbsPage.trail, [docs, uriOf(1)], "a folder above all open pages gets a page")
        same(pageStack.depth, 2, "in place of every page inside it")
        expect(pageStack.previousPage(crumbsPage) === start, "back goes to the first page")

        var only = open(uriOf(5), false)
        named(only, "folderCrumbs").home()
        waitFor(function () { return pageStack.currentPage !== only && settled() }, "going home from two pages up")
        expect(pageStack.currentPage === start, "home skips every page between")
    }

    function checkFavourite() {
        var page = open(uriOf(2), true)
        var view = viewOf(page)
        var entry = pulleyEntry(page, "folderMenuFavourite")
        if (!view || !entry)
            return
        var item = entry.item
        expect(entry.shown, "the folder pulley offers Add to favourites")
        same(item.text, "lautta-dir-act-favourite", "under its usual name")
        view.startSelection()
        expect(!pulleyEntry(page, "folderMenuFavourite").shown, "not while selecting")
        view.endSelection()
        item.clicked()
        waitFor(function () { return pageStack.currentPage !== page && settled() }, "the favourite page")
        var favourite = pageStack.currentPage
        same(favourite.uri, uriOf(2), "the favourite is the open folder")
        expect(favourite.isDir === true, "as a folder")
        pageStack.pop(null, PageStackAction.Immediate)
    }

    Component.onCompleted: {
        makeFolders()
        checkFolding()
        checkHomeRow()
        checkWalkedDown()
        checkEnteredDeep()
        checkFavourite()
        reachedEnd = true
    }

    Component {
        id: folderPage

        DirectoryPage { }
    }

    Component {
        id: benchPage

        Page {
            property int most: 2

            Item {
                id: host

                property alias crumbs: crumbs
                property var picked: []
                property int homes

                objectName: "bench"
                x: parent.width / 4
                y: parent.height / 2
                width: 10 * parent.width
                height: Theme.itemSizeLarge + crumbs.menuHeight
                scale: 0.05
                transformOrigin: Item.TopLeft

                PathCrumbs {
                    id: crumbs

                    anchors.right: parent.right
                    height: Theme.itemSizeLarge
                    pathModel: deep
                    title: "Here"
                    titleTop: Theme.paddingLarge
                    menuHost: host
                    menuMost: most
                    onPicked: host.picked = host.picked.concat([uri])
                    onHome: host.homes++
                }
            }
        }
    }
}
