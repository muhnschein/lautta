// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"
import "../../qml/viewers"

ApplicationWindow {
    id: win

    property bool reachedEnd

    readonly property string folder: "lautta://user-documents/"
    readonly property color tint: Theme.rgba(Theme.highlightBackgroundColor, 0.12)

    ViewerTools { id: tools }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
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

    function shownBands(root) {
        return findAll(root, function (x) {
            return x.visible && x.color !== undefined && x.width > 0 && x.height > 0
                    && String(x.color) === String(win.tint)
        }, [])
    }

    function headers(root) {
        return findAll(root, function (x) { return x._titleItem !== undefined && x.extraContent !== undefined }, [])
    }

    function isInside(ancestor, item) {
        for (var p = item; p; p = p.parent) {
            if (p === ancestor)
                return true
        }
        return false
    }

    function topOf(item, page) {
        return item.mapToItem(page, 0, 0).y
    }

    function bottomOf(item, page) {
        return item.mapToItem(page, 0, item.height).y
    }

    function within(item, band, page) {
        return topOf(item, page) >= topOf(band, page) - 0.5 && bottomOf(item, page) <= bottomOf(band, page) + 0.5
                && item.height > 0
    }

    function near(a, b) {
        return Math.abs(a - b) < 0.5
    }

    function settle(page) {
        var views = findAll(page, function (x) { return x.forceLayout !== undefined }, [])
        for (var i = 0; i < views.length; ++i)
            views[i].forceLayout()
        tools.pump(50)
    }

    function push(component, props) {
        pageStack.push(component, props || {}, PageStackAction.Immediate)
        var page = pageStack.currentPage
        settle(page)
        return page
    }

    function pop() {
        pageStack.pop(null, PageStackAction.Immediate)
    }

    function checkBand(page, label, end) {
        var bands = shownBands(page)
        if (!expect(bands.length === 1, label + ": one band: " + bands.length))
            return null
        var band = bands[0]
        var title = headers(page)
        expect(title.length === 1 && within(title[0], band, page), label + ": the title sits on the band")
        expect(near(band.width, page.width), label + ": the band is as wide as the page: " + band.width)
        expect(near(topOf(band, page), 0), label + ": the band starts at the top: " + topOf(band, page))
        var bottom = end ? bottomOf(end, page) : bottomOf(title[0], page)
        expect(near(bottomOf(band, page), bottom),
               label + ": the band ends with the header block: " + bottomOf(band, page) + " vs " + bottom)
        return band
    }

    function checkPages() {
        var simple = [
            [settings, "Settings"], [about, "About"], [trash, "Trash"], [transfers, "Activity"],
            [recents, "Recents"], [homeSettings, "Home page settings"], [storage, "Storage"],
            [deleting, "Deleting"], [searchSettings, "Search settings"], [recentsSettings, "Recents settings"],
            [transferSettings, "Transfer settings"]
        ]
        for (var i = 0; i < simple.length; ++i) {
            var page = push(simple[i][0])
            checkBand(page, simple[i][1])
            pop()
        }
    }

    function checkSearch() {
        var page = push(search, { "rootUri": "lautta://user-documents/" })
        var pills = findAll(page, function (x) { return x.objectName === "searchPills" }, [])
        if (expect(pills.length === 1, "Search: one pill strip: " + pills.length))
            checkBand(page, "Search", pills[0])
        pop()
    }

    function checkTextViewer() {
        expect(tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), folder + "notes.txt"), "a text file")
        var page = push(text, { "uri": folder + "notes.txt" })
        checkBand(page, "Text")
        pop()
    }

    function checkFolder() {
        var page = push(directory, { "uri": folder })
        var access = findAll(page, function (x) { return x.objectName === "folderAccessNotice" }, [])
        if (expect(access.length === 1, "Folder: one access line: " + access.length)) {
            var band = checkBand(page, "Folder", access[0])
            var views = findAll(page, function (x) { return x.startSelection !== undefined }, [])
            if (band && views.length === 1) {
                views[0].startSelection()
                checkBand(page, "Folder while selecting", access[0])
                views[0].endSelection()
            }
        }
        pop()
    }

    function stripRow(list) {
        for (var i = 0; i < list.model.count; ++i) {
            if (list.model.get(i).kind === "recent-strip") {
                list.currentIndex = i
                return list.currentItem
            }
        }
        return null
    }

    function homeList(page) {
        var views = findAll(page, function (x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
        return views.length === 1 ? views[0] : null
    }

    function checkHomeWithRecents() {
        var page = push(home, { "listModel": withRecents })
        var list = homeList(page)
        var row = list ? stripRow(list) : null
        if (!expect(row !== null, "Home: the Recents tiles"))
            return pop()
        list.positionViewAtBeginning()
        settle(page)
        var bands = shownBands(page)
        bands.sort(function (a, b) { return topOf(a, page) - topOf(b, page) })
        if (expect(bands.length === 3, "Home: title, Recents and its tiles on the band: " + bands.length)) {
            expect(near(topOf(bands[0], page), 0), "Home: the band starts at the top: " + topOf(bands[0], page))
            expect(near(bottomOf(bands[0], page), topOf(bands[1], page))
                   && near(bottomOf(bands[1], page), topOf(bands[2], page)),
                   "Home: the band runs on without gap or overlap: " + bands.map(function (b) {
                       return topOf(b, page) + "-" + bottomOf(b, page) }).join(" "))
            expect(within(headers(page)[0], bands[0], page), "Home: the title sits on the band")
            var named = findAll(bands[1].parent, function (x) { return x.visible && String(x.text) === "lautta-home-recents" }, [])
            expect(named.length === 1, "Home: the band takes in the Recents heading: " + named.length)
            expect(isInside(row, bands[2]) && near(bottomOf(bands[2], page), topOf(row, page) + row.contentHeight),
                   "Home: the band ends with the tiles: " + bottomOf(bands[2], page) + " vs "
                   + (topOf(row, page) + row.contentHeight))
            var next = findAll(page, function (x) { return x.visible && String(x.text) === "lautta-home-section-favourites" }, [])
            expect(next.length === 1 && topOf(next[0], page) >= bottomOf(bands[2], page) - 0.5,
                   "Home: Favourites stay under the band")
            row.heldId = 7
            row.openMenu()
            settle(page)
            expect(row.menuOpen && near(bands[2].height, row.contentHeight) && row.height > row.contentHeight + 1,
                   "Home: a tile menu opens under the band: " + bands[2].height + " vs " + row.height)
            row.closeMenu()
        }
        pop()
    }

    function checkHomeWithoutRecents() {
        var page = push(home, { "listModel": withoutRecents })
        var list = homeList(page)
        if (list) {
            list.positionViewAtBeginning()
            settle(page)
        }
        checkBand(page, "Home without Recents")
        pop()
    }

    Component.onCompleted: {
        checkPages()
        checkSearch()
        checkTextViewer()
        checkFolder()
        checkHomeWithRecents()
        checkHomeWithoutRecents()
        reachedEnd = true
    }

    ListModel {
        id: withRecents

        ListElement { section: "recents"; uri: ""; name: ""; kind: "recent-strip"; icon: ""; status: "ready"; attention: ""; colour: ""; itemId: ""; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: ""; tiles: "[{\"id\":7,\"uri\":\"lautta://user-pictures/IMG_2041.jpg\",\"name\":\"IMG_2041.jpg\",\"kind\":\"previewed\",\"category\":\"image\",\"mimeType\":\"image/jpeg\",\"thumbnailSource\":\"\"}]" }
        ListElement { section: "favourites"; uri: "lautta://user-documents/Uni"; name: "Thesis"; kind: "favourite"; icon: "image://theme/icon-m-favorite"; status: "ready"; attention: ""; colour: ""; itemId: "1"; place: "Documents › Uni"; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "device"; uri: "lautta://user-documents/"; name: "Documents"; kind: "folder"; icon: "image://theme/icon-m-file-folder"; status: "ready"; attention: ""; colour: ""; itemId: "user-documents"; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: "" }
    }

    ListModel {
        id: withoutRecents

        ListElement { section: "favourites"; uri: "lautta://user-documents/Uni"; name: "Thesis"; kind: "favourite"; icon: "image://theme/icon-m-favorite"; status: "ready"; attention: ""; colour: ""; itemId: "1"; place: "Documents › Uni"; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "device"; uri: "lautta://user-documents/"; name: "Documents"; kind: "folder"; icon: "image://theme/icon-m-file-folder"; status: "ready"; attention: ""; colour: ""; itemId: "user-documents"; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: "" }
    }

    Component { id: settings; SettingsPage { } }
    Component { id: about; AboutPage { } }
    Component { id: trash; TrashPage { } }
    Component { id: transfers; TransfersPage { } }
    Component { id: recents; RecentsPage { } }
    Component { id: homeSettings; HomeSettingsPage { } }
    Component { id: storage; StorageSettingsPage { } }
    Component { id: deleting; DeletingSettingsPage { } }
    Component { id: searchSettings; SearchSettingsPage { } }
    Component { id: recentsSettings; RecentsSettingsPage { } }
    Component { id: transferSettings; TransfersSettingsPage { } }
    Component { id: search; SearchPage { } }
    Component { id: text; TextViewer { } }
    Component { id: directory; DirectoryPage { } }
    Component { id: home; HomePage { } }
}
