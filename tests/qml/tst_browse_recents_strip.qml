// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"
import "../../qml/components/browse/BrowseText.js" as BrowseText
import "../../qml/components/viewers/ViewerImage.js" as ViewerImage

ApplicationWindow {
    id: win

    readonly property string file: "lautta://user-documents/strip-notes.md"
    readonly property string photo: "lautta://user-pictures/IMG_2041.jpg"
    property var prefetch: null
    property var fetched: []
    property bool scenarioRunning: true
    property bool reachedEnd
    property int step: 0
    property int waited: 0
    property var browse
    property var removed: []

    ViewerTools { id: tools }

    Connections {
        target: win.prefetch
        onSourceChanged: win.fetched.push(String(win.prefetch.source))
    }

    QtObject {
        id: store

        function remove(id) {
            win.removed.push(id)
        }
    }

    initialPage: browsePage

    Component {
        id: browsePage

        BrowsePage {
            id: page

            Component.onCompleted: win.browse = page
            recentsStore: store
            deviceFree: 42000000000
            deviceTotal: 128000000000
            listModel: ListModel {
                ListElement { section: "recents"; uri: ""; name: ""; kind: "recent-strip"; icon: ""; status: "ready"; attention: ""; connection: ""; connectionError: ""; colour: ""; itemId: ""; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: ""; drive: ""; tiles: "[{\"id\":7,\"uri\":\"lautta://user-pictures/IMG_2041.jpg\",\"name\":\"a\",\"kind\":\"previewed\",\"category\":\"image\",\"mimeType\":\"image/jpeg\",\"thumbnailSource\":\"\"},{\"id\":8,\"uri\":\"lautta://user-documents/strip-notes.md\",\"name\":\"strip-notes.md\",\"kind\":\"opened\",\"category\":\"text\",\"mimeType\":\"text/markdown\",\"thumbnailSource\":\"\",\"textThumb\":true,\"textMono\":false,\"typeIcon\":\"types/markdown\"},{\"id\":9,\"uri\":\"lautta://user-pictures/sent.jpg\",\"name\":\"A name far too long for one tile.jpg\",\"kind\":\"transferred\",\"category\":\"image\",\"mimeType\":\"image/jpeg\",\"thumbnailSource\":\"\"},{\"id\":10,\"uri\":\"lautta://user-pictures/older.jpg\",\"name\":\"older.jpg\",\"kind\":\"previewed\",\"category\":\"image\",\"mimeType\":\"image/jpeg\",\"thumbnailSource\":\"\"}]" }
                ListElement { section: "favourites"; uri: "lautta://user-documents/Uni"; name: "Thesis"; kind: "favourite"; icon: "image://theme/icon-m-favorite"; status: "ready"; attention: ""; connection: ""; connectionError: ""; colour: ""; itemId: "1"; place: "Documents › Uni"; provider: ""; host: ""; free: -1; total: -1; fs: ""; drive: ""; tiles: "" }
                ListElement { section: "device"; uri: "lautta://user-documents/"; name: "Documents"; kind: "folder"; icon: "file-folder"; status: "ready"; attention: ""; connection: ""; connectionError: ""; colour: ""; itemId: "user-documents"; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: ""; drive: ""; tiles: "" }
                ListElement { section: "volumes"; uri: "lautta://vol-SD/"; name: "SD card"; kind: "volume"; icon: "image://theme/icon-m-sd-card"; status: "ready"; attention: ""; connection: ""; connectionError: ""; colour: ""; itemId: "vol-SD"; place: ""; provider: ""; host: ""; free: 62500000000; total: 128000000000; fs: "exFAT"; drive: ""; tiles: "" }
            }
        }
    }

    function fail(what) {
        console.error("FAILED: " + what)
        scenarioRunning = false
    }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
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

    function list() {
        var views = findAll(browse, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
        if (views.length === 0)
            return null
        views[0].cacheBuffer = 100000
        views[0].forceLayout()
        return views[0]
    }

    function rowAt(index) {
        var view = list()
        if (!view)
            return null
        view.currentIndex = index
        return view.currentItem
    }

    function named(item, name) {
        return findAll(item, function(x) { return x.objectName === name }, [])
    }

    function left(item, within) {
        return Math.round(item.mapToItem(within, 0, 0).x)
    }

    function top(item, within) {
        return Math.round(item.mapToItem(within, 0, 0).y)
    }

    function label(text) {
        var found = findAll(list().contentItem, function(x) { return x.visible && x.font !== undefined && x.text === text }, [])
        return found.length === 1 ? found[0] : null
    }

    function checkTiles(row) {
        var tiles = named(row, "recentTile")
        var names = named(row, "recentTileName").map(function(x) { return x.text }).join("|")
        if (!expect(tiles.length === 3 && names === "a|strip-notes.md|A name far too long for one tile.jpg",
                    "of more recents three are tiles, with their names: " + tiles.length + " " + names))
            return
        var all = named(row, "allRecents")
        if (!expect(all.length === 1 && findAll(all[0], function(x) { return x.text === "lautta-browse-all-recents" }, []).length === 1,
                    "a fourth tile says All recents"))
            return
        var cells = tiles.concat(all)
        expect(left(cells[0], row) === Math.round(Theme.horizontalPageMargin), "tiles start at the page margin: " + left(cells[0], row))
        for (var i = 1; i < cells.length; ++i) {
            expect(cells[i].width === cells[0].width && cells[i].height === cells[0].height, "tile " + i + " is as large as the first")
            expect(left(cells[i], row) === Math.round(left(cells[i - 1], row) + cells[0].width + Theme.paddingSmall),
                   "tile " + i + " follows the one before it")
        }
        var side = Math.floor((Math.min(Screen.width, Screen.height) - 2 * Theme.horizontalPageMargin - 3 * Theme.paddingSmall) / 4)
        expect(cells[0].width === side && side > 0, "four tiles fill a portrait screen between its margins: " + cells[0].width + ", not " + side)
        expect(row.contentHeight >= cells[0].height && cells[0].height > cells[0].width, "the row holds tile and name")
        var longName = named(tiles[2], "recentTileName")[0]
        expect(longName.width === tiles[2].width && longName.implicitWidth > longName.width && longName.horizontalAlignment === Text.AlignLeft,
               "a long name is cut at its tile, starting at its left")
        var shortName = named(tiles[0], "recentTileName")[0]
        expect(shortName.implicitWidth <= shortName.width && shortName.horizontalAlignment === Text.AlignHCenter, "a short name sits in the middle")
        var glyphs = tiles.map(function(t) { return named(t, "fileGlyph")[0] })
        expect(glyphs.every(function(g) { return g && g.status === Image.Ready }) && glyphs[0].visible && glyphs[2].visible
               && String(glyphs[0].source) !== String(glyphs[1].source),
               "without thumbnail or first lines a tile draws its type's glyph: " + glyphs.map(function(g) { return g ? g.source : "none" }).join(", "))
        expect(row.menu !== null && !row.plain, "the tiles row is no plain row")
        var depth = pageStack.depth
        row.clicked(null)
        expect(pageStack.depth === depth && !pageStack.busy, "a tap beside the tiles opens nothing")
    }

    function checkSections() {
        var view = list()
        var recents = label("lautta-browse-recents")
        var favourites = label("lautta-browse-section-favourites")
        if (!expect(recents !== null && favourites !== null, "sections Recents and Favourites have their headers"))
            return
        expect(top(recents, view.contentItem) < top(rowAt(0), view.contentItem)
               && top(rowAt(0), view.contentItem) < top(favourites, view.contentItem),
               "Recents and its tiles come before Favourites")
    }

    function shownSpace() {
        return named(list().contentItem, "deviceSpace").filter(function(x) { return x.visible })
    }

    function checkSpace() {
        var view = list()
        var space = shownSpace()
        var device = label("lautta-browse-section-device")
        if (!expect(space.length === 1 && device !== null, "one section header tells free space: " + space.length))
            return
        expect(space[0].text === BrowseText.volumeLine(42000000000, 128000000000, "") && space[0].text.length > 0,
               "it reads as a volume's line: " + space[0].text)
        expect(left(space[0], view.contentItem) === Math.round(Theme.horizontalPageMargin), "it starts at the page margin")
        var sizes = [[104300000000, "97 GB"], [128000000000, "119 GB"], [10737418240, "10 GB"], [10694468567, "10 GB"],
                     [10400000000, "9.7 GB"], [9400000000, "8.8 GB"], [999000000, "953 MB"], [1500000, "1.4 MB"]]
        for (var i = 0; i < sizes.length; ++i)
            expect(BrowseText.space(sizes[i][0]) === sizes[i][1],
                   "space reads whole from 10 of its unit up, with a decimal below: " + BrowseText.space(sizes[i][0]) + " for " + sizes[i][1])
        expect(BrowseText.volumeLine(-1, 128000000000, "") === "119 GB" && BrowseText.volumeLine(-1, 128000000000, "exFAT") === "119 GB · exFAT",
               "a volume's size reads that way: " + BrowseText.volumeLine(-1, 128000000000, "exFAT"))
        expect(Math.round(space[0].mapToItem(view.contentItem, 0, space[0].baselineOffset).y)
               === Math.round(device.mapToItem(view.contentItem, 0, device.baselineOffset).y),
               "it stands on the line of On this device")
        expect(left(space[0], view.contentItem) + space[0].width < left(device, view.contentItem) + device.width - device.contentWidth,
               "it ends before the section's name")
        browse.deviceTotal = -1
        expect(shownSpace().length === 0, "unknown space shows nothing")
        browse.deviceTotal = 0
        expect(shownSpace().length === 0, "space not asked yet shows nothing")
        browse.deviceTotal = 128000000000
        expect(shownSpace().length === 1, "known space shows again")
    }

    function lit(row) {
        return named(row, "recentTile").map(function(t) { return t.lit ? t.tile.id : 0 }).join(",")
    }

    function checkPrefetch(row) {
        var tiles = named(row, "recentTile")
        prefetch = named(browse, "viewerPrefetch")[0] || null
        if (!expect(prefetch !== null, "Browse has a prefetch"))
            return
        touch(tiles[2], false)
        expect(fetched.length === 0, "a transferred photo's tile starts no decode: " + JSON.stringify(fetched))
        touch(tiles[0], false)
        expect(fetched.length === 2 && fetched[0] === String(ViewerImage.source(photo)) && fetched[1] === "",
               "finger down on a photo's tile starts its decode, a scroll drops it: " + JSON.stringify(fetched))
        fetched = []
        touch(tiles[0], true)
    }

    function touch(tile, stay) {
        tools.mouse(tile, "press", tile.width / 2, tile.height / 2)
        if (!stay)
            letGo(tile)
    }

    function letGo(tile) {
        var view = list()
        var at = view.contentY
        var y = tile.height / 2
        for (var i = 1; i <= 4; ++i)
            tools.mouse(tile, "move", tile.width / 2, y + i * Theme.startDragDistance)
        tools.mouse(tile, "release", tile.width / 2, y + 4 * Theme.startDragDistance)
        view.cancelFlick()
        view.contentY = at
    }

    function holdTile(row) {
        var tiles = named(row, "recentTile")
        expect(lit(row) === "0,0,0", "no tile is lit at rest: " + lit(row))
        tiles[1].pressAndHold(null)
        expect(fetched.length === 2 && fetched[1] === "" && prefetch.status !== Image.Ready,
               "a hold drops the decode: " + JSON.stringify(fetched))
        tools.mouse(tiles[0], "move", -Theme.paddingSmall, tiles[0].height / 2)
        tools.mouse(tiles[0], "release", -Theme.paddingSmall, tiles[0].height / 2)
        if (!expect(row.menuOpen, "a long press on a tile opens a menu"))
            return
        var items = findAll(row.openMenu(), function(x) { return x.__silica_menuitem !== undefined && x.visible }, [])
        if (!expect(items.length === 1 && items[0].text === "lautta-recents-remove",
                    "the menu removes from recents: " + items.map(function(x) { return x.text }).join(",")))
            return
        expect(lit(row) === "0,8,0", "the pressed tile alone is lit: " + lit(row))
        items[0].clicked()
        expect(win.removed.join(",") === "8", "the pressed tile's recent is removed: [" + win.removed.join(",") + "]")
        row.closeMenu()
    }

    function preview(row) {
        var tile = named(row, "recentTile")[1]
        var label = tile ? named(tile, "textPreview")[0] : null
        return label && label.visible ? label.text : ""
    }

    function checkTextThumb(row) {
        var tiles = named(row, "recentTile")
        expect(preview(row).indexOf("# Title\n") === 0, "a text tile shows its first lines: " + JSON.stringify(preview(row)))
        var type = named(tiles[1], "typeIcon")[0]
        expect(type && type.icon === "types/markdown" && type.status === Image.Ready, "with its type's icon")
        expect(!named(tiles[1], "fileGlyph")[0].visible, "and no glyph")
        expect(named(tiles[0], "textPreview").length === 0 && named(tiles[2], "textPreview").length === 0,
               "a tile that is no text has no lines")
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
            if (++win.waited > 100)
                return win.fail("step " + win.step + " timed out: depth " + pageStack.depth + (pageStack.busy ? ", busy" : "")
                                + ", page " + pageStack.currentPage)
            var row = win.browse && win.browse.status === PageStatus.Active && !pageStack.busy ? win.rowAt(0) : null
            var page = pageStack.currentPage
            switch (win.step) {
            case 0:
                if (!row || !row.strip || win.named(row, "recentTile").length === 0)
                    break
                if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), win.file)
                        || !tools.installFixture(Qt.resolvedUrl("host/fixtures/camera.jpg"), win.photo))
                    return win.fail("fixture")
                pageStack.replace(browsePage, {}, PageStackAction.Immediate)
                win.next()
                break
            case 1:
                if (!row || win.preview(row) === "")
                    break
                win.checkTextThumb(row)
                win.checkTiles(row)
                win.checkSections()
                win.checkSpace()
                win.checkPrefetch(row)
                win.holdTile(row)
                win.next()
                break
            case 2:
                if (!row || row.menuOpen)
                    break
                win.expect(win.lit(row) === "0,0,0", "the menu closed, no tile stays lit: " + win.lit(row))
                win.named(row, "recentTile")[1].clicked(null)
                win.next()
                break
            case 3:
                if (pageStack.busy || !page || page.uri !== win.file)
                    break
                win.expect(String(page).indexOf("MarkdownViewer") === 0, "an opened file's tile opens the file: " + page)
                pageStack.pop(null, PageStackAction.Immediate)
                win.next()
                break
            case 4:
                if (!row || pageStack.depth !== 1)
                    break
                win.named(row, "recentTile")[2].clicked(null)
                win.next()
                break
            case 5:
                if (pageStack.busy || !page || page.uri !== App.parentUri(win.photo))
                    break
                win.expect(String(page).indexOf("DirectoryPage") === 0, "a transferred file's tile opens its folder: " + page)
                pageStack.pop(null, PageStackAction.Immediate)
                win.next()
                break
            case 6:
                if (!row || pageStack.depth !== 1)
                    break
                win.named(row, "allRecents")[0].clicked(null)
                win.next()
                break
            case 7:
                if (pageStack.busy || !page || page.recentsOn === undefined)
                    break
                win.expect(win.removed.join(",") === "8", "nothing else was removed: [" + win.removed.join(",") + "]")
                win.reachedEnd = true
                win.scenarioRunning = false
                break
            }
        }
    }
}
