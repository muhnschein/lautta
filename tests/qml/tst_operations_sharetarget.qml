// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"
import "../../qml/components/directory"

ApplicationWindow {
    id: test
    property bool reachedEnd

    readonly property string docs: "lautta://user-documents/"
    readonly property string downloads: "lautta://user-downloads/"
    readonly property string target: docs + "ShareHere"
    readonly property string inner: target + "/Deep"
    readonly property string deeper: inner + "/Deeper"
    readonly property string deepest: deeper + "/Deepest"
    property string packed
    readonly property string shared: docs + "shared.txt"

    ViewerTools { id: tools }

    DirectoryModel { id: maker }

    DirectoryModel { id: landed }

    Connections {
        target: Operations
        onJobFinished: if (ok && resultUri.indexOf("packed") >= 0) Operations.openArchive(resultUri, "")
        onArchiveOpened: test.packed = rootUri
    }

    PathModel { id: way }

    Component {
        id: bandBench

        Page {
            property alias band: benchBand

            PathBand {
                id: benchBand

                title: "Deepest"
                pathModel: way
            }
        }
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

    function waitFor(done, what) {
        for (var i = 0; i < 400 && !done(); ++i)
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

    function layOut(root) {
        var views = findAll(root, function (x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
        for (var i = 0; i < views.length; ++i)
            views[i].forceLayout()
    }

    function rows(view) {
        layOut(view)
        return findAll(view.flickable, function (x) { return x.info !== undefined && x.contentHeight !== undefined }, [])
    }

    function row(view, uri) {
        var found = rows(view).filter(function (x) { return x.info.uri === uri })
        return expect(found.length === 1, "one row of " + uri + ": " + found.length) ? found[0] : null
    }

    function tap(item) {
        tools.mouse(item, "press", item.width / 2, item.height / 2)
        tools.mouse(item, "release", item.width / 2, item.height / 2)
    }

    function tapRow(menu, row) {
        var at = row.mapToItem(menu, row.width / 2, row.height / 2)
        tools.mouse(menu, "press", at.x, at.y)
        tools.mouse(menu, "release", at.x, at.y)
    }

    function checkFolded(page, view, band) {
        var bar = named(page, "folderHeaderBar")
        var dots = named(band, "crumbDots")
        if (!bar || !dots || !expect(dots.visible, "a deep path folds in the band"))
            return
        var bare = bar.height
        dots.clicked(null)
        var menu = named(page, "crumbMenu")
        if (!menu)
            return
        waitFor(function () { return menu.active && menu._expanded }, "the folded folders to open")
        var rows = findAll(menu, function (x) { return x.hasOwnProperty("__silica_menuitem") }, [])
        expect(menu.parent === bar && menu.height > 0 && bar.height === bare + menu.height,
               "they open under the band, which makes room")
        var room = Math.floor((view.height - named(page, "shareLead").height) / Theme.itemSizeSmall) - 1
        var folded = ["Documents", "ShareHere", "Deep", "Deeper"].slice(0, named(band, "bandCrumbs").folded).reverse()
        expect(room > 0 && folded.length > room, "more folders folded than fit under the band: " + folded + ", " + room)
        same(rows.map(function (row) { return row.text }), folded.slice(0, room), "as many as fit, nearest first")
        if (rows.length === 0)
            return
        var far = rows[rows.length - 1]
        var there = { "Documents": docs, "ShareHere": target, "Deep": inner, "Deeper": deeper }[far.text]
        tapRow(menu, far)
        waitFor(listed(view, there), "the folder picked from them: " + far.text)
        waitFor(function () { return bar.height === bare }, "the band to shrink back")
    }

    function tile(view, name) {
        var grid = view.flickable
        grid.forceLayout()
        var found = findAll(grid.contentItem, function (x) { return x.captions !== undefined && x.itemName === name }, [])
        return expect(found.length === 1, "one tile of " + name + ": " + found.length) ? found[0] : null
    }

    function placed(page, objectName, name) {
        layOut(page)
        var found = all(page, objectName).filter(function (x) {
            return findAll(x, function (label) { return label.text === name && label.truncationMode !== undefined }, []).length > 0
        })
        return expect(found.length === 1, "one " + objectName + " called " + name + ": " + found.length) ? found[0] : null
    }

    function pulley(page) {
        var menus = findAll(page, function (x) { return x._isPullDownMenu === true }, [])
        if (!expect(menus.length === 1, "one pulley: " + menus.length))
            return []
        var menu = menus[0]
        if (!menu.visible)
            return null
        menu.active = true
        var kids = menu._contentColumn.children
        var shown = []
        for (var i = 0; i < kids.length; ++i) {
            if (kids[i].visible && kids[i].hasOwnProperty("__silica_menuitem"))
                shown.push(kids[i].text)
        }
        menu.active = false
        return shown
    }

    function listed(view, uri) {
        return function () { return view.uri === uri && !view.model.loading && view.model.errorKind === "" }
    }

    function share(resources) {
        var page = pageStack.push(Qt.resolvedUrl("../../qml/pages/ShareTargetPage.qml"), { "resources": resources },
                                  PageStackAction.Immediate)
        return page
    }

    function leave() {
        pageStack.pop(null, PageStackAction.Immediate)
    }

    function kept() {
        return JSON.parse(App.settingsJson).share_folders
    }

    function setUp() {
        maker.uri = docs
        maker.createFolder("ShareHere")
        waitFor(function () { return maker.indexOf(target) >= 0 }, "the folder to save to")
        expect(tools.installFixture(Qt.resolvedUrl("tst_operations_sharetarget.qml"), shared), "the file to share")
        maker.uri = target
        maker.createFolder("Deep")
        waitFor(function () { return maker.indexOf(inner) >= 0 }, "a folder inside it")
        maker.uri = inner
        maker.createFolder("Deeper")
        waitFor(function () { return maker.indexOf(deeper) >= 0 }, "a folder further in")
        maker.uri = deeper
        maker.createFolder("Deepest")
        waitFor(function () { return maker.indexOf(deepest) >= 0 }, "and one in that")
        maker.uri = target
        expect(tools.installFixture(Qt.resolvedUrl("tst_operations_sharetarget.qml"), target + "/old.txt"), "a file inside it")
        maker.refresh()
        waitFor(function () { return !maker.loading && maker.indexOf(target + "/old.txt") >= 0 }, "the file inside it")
    }

    function checkNothingReadable() {
        var page = share(["file:///home/user/Downloads/IMG_2041.jpg", { "filePath": "/home/user/Documents/b.txt" }])
        var view = named(page, "shareView")
        same([page.entries.length, page.readableCount, page.skippedCount], [2, 0, 2], "files that can't be read are left out")
        waitFor(listed(view, downloads), "Downloads")
        expect(!page.canAccept, "nothing readable, nothing to save")
        same(named(page, "shareCount").text, "lautta-share-count", "the count stands, at none")
        expect(named(page, "shareSkipped").visible, "the files left out are told")
        same(page.skippedText, "lautta-share-skipped", "several by their number")
        leave()
    }

    function checkFirstTime() {
        var page = share([App.localUrl(shared), "/nowhere/gone.pdf"])
        var view = named(page, "shareView")
        if (!view)
            return
        same(page.folderUri, downloads, "first time it opens Downloads")
        waitFor(listed(view, downloads), "Downloads")
        same([page.readableCount, page.skippedCount], [1, 1], "one file to save, one left out")
        same(named(page, "shareCount").text, "lautta-share-one", "a single file goes by its name")
        same(page.skippedText, "lautta-share-skipped-one", "a single file left out by its name")
        expect(page.canAccept, "Downloads takes the file")

        var band = named(page, "folderBand")
        expect(!named(page, "folderPageHeader").visible, "the path band stands in for the folder header")
        same(named(band, "bandTitle").text, view.model.title, "the band names the folder")
        expect(named(band, "bandCrumbs").visible, "with the way up before it")
        expect(named(page, "folderHeaderBar").y >= named(page, "shareCount").height, "under Save here and the count")
        same(pulley(page), ["lautta-dir-menu-new-folder"], "the pulley makes a folder, nothing else")
        named(page, "folderMenuNewFolder").clicked()
        waitFor(function () { return pageStack.currentPage !== page && !pageStack.busy }, "the page that names a folder")
        same(pageStack.currentPage.parentUri, downloads, "the new folder goes into the folder shown")
        pageStack.pop(page, PageStackAction.Immediate)
        expect(!named(page, "sharePlaces").visible, "no places while a folder shows")

        named(band, "crumbHome").clicked(null)
        expect(view.atRoot && page.folderUri === "", "the house goes to Locations")
        same(named(band, "bandTitle").text, "lautta-share-locations", "the band says so")
        expect(!named(band, "bandCrumbs").visible, "no way up from there")
        expect(!page.canAccept, "nothing is saved to Locations")
        same(view.placeholderKind(), "", "and nothing is told about a folder there")
        same(pulley(page), null, "no pulley there")
        var places = named(page, "sharePlaces")
        expect(places.visible && !view.flickable.visible, "places show in place of a folder")
        same(all(page, "shareRecent").length, 0, "nothing saved lately yet")
        expect(!named(page, "shareRecentTitle").visible, "nor a title for that")
        var documents = placed(page, "sharePlace", "Documents")
        if (!documents)
            return
        expect(documents.enabled, "a place that takes files can be picked")
        documents.clicked(null)
        same(page.folderUri, docs, "a place opens its top folder")
        waitFor(listed(view, docs), "Documents")

        var folder = row(view, target)
        var file = row(view, shared)
        if (!folder || !file)
            return
        expect(folder.enabled && folder.opacity === 1, "folders can be entered")
        expect(!file.enabled && file.opacity < 1, "files are only shown")
        expect(folder.menu === null && file.menu === null, "no item menus")
        folder.clicked(null)
        same(page.folderUri, target, "a folder opens in place")
        same(pageStack.currentPage === page, true, "on the same page")
        waitFor(listed(view, target), "the folder to save to")
        row(view, inner).clicked(null)
        waitFor(listed(view, inner), "the folder inside")
        row(view, deeper).clicked(null)
        waitFor(listed(view, deeper), "the folder further in")
        row(view, deepest).clicked(null)
        waitFor(listed(view, deepest), "the innermost folder")
        same([view.placeholderKind(), view.placeholderHint("empty")], ["empty", "lautta-dir-empty-hint-picking"],
             "an empty folder says it can be saved to")
        checkFolded(page, view, band)
        var above = all(band, "crumb").filter(function (x) { return x.name === "ShareHere" })
        if (page.folderUri !== target && same(above.length, 1, "the folder to save to is a crumb"))
            named(above[0], "crumbButton").clicked(null)
        waitFor(listed(view, target), "the folder to save to, by its crumb")
        same(page.folderUri, target, "a crumb goes up in place")

        landed.uri = target
        page.accept()
        waitFor(function () { return pageStack.currentPage !== page && !pageStack.busy }, "the page to close")
        waitFor(function () {
            if (!landed.loading && landed.indexOf(target + "/shared.txt") < 0)
                landed.refresh()
            return landed.indexOf(target + "/shared.txt") >= 0
        }, "the file to land in the folder")
        same(kept(), [target], "the folder saved to is kept")
    }

    function checkNextTime() {
        var page = share([App.localUrl(shared)])
        var view = named(page, "shareView")
        if (!view)
            return
        same(page.folderUri, target, "next time it opens where it saved last")
        expect(page.skippedText === "" && !named(page, "shareSkipped").visible, "nothing left out, nothing told")
        view.homePicked()
        var lately = placed(page, "shareRecent", "ShareHere")
        if (!lately)
            return
        expect(named(page, "shareRecentTitle").visible, "folders saved to lately have their title")
        lately.clicked(null)
        same(page.folderUri, target, "a folder saved to lately opens")
        leave()
    }

    function checkOtherViews() {
        var page = share([App.localUrl(shared)])
        var view = named(page, "shareView")
        if (!view)
            return
        waitFor(listed(view, target), "the folder saved to")
        App.copy(JSON.stringify([shared]))
        var docked = findAll(view, function (x) { return x.dock !== undefined && x.open !== undefined }, [])
        expect(docked.length > 0 && docked.every(function (x) { return !x.open }), "no panels over the folder")
        same(view.coveredHeight, 0, "none takes room")
        var bars = findAll(view, function (x) { return x.allowed !== undefined && x.shown !== undefined }, [])
        expect(bars.length === 1 && !bars[0].allowed, "nor does the activity bar show")
        App.clearClipboard()
        leave()
    }

    function checkGrid() {
        App.setSetting("view_mode", JSON.stringify("grid"))
        var page = share([App.localUrl(shared)])
        var view = named(page, "shareView")
        if (!view)
            return
        waitFor(function () { return listed(view, target)() && view.gridMode }, "the folder saved to, as a grid")
        var folder = tile(view, "Deep")
        var file = tile(view, "old.txt")
        if (folder && file) {
            expect(folder.enabled && folder.opacity === 1, "a grid follows View options: folders can be entered")
            expect(!file.enabled && file.opacity < 1, "and files are only shown")
            tools.mouse(folder, "press", folder.width / 2, folder.height / 2)
            tools.pump(1500)
            expect(!view.selecting, "a tile held for a second and a half selects nothing: there is no selection here")
            tools.mouse(folder, "release", folder.width / 2, folder.height / 2 - 2 * folder.height)
        }
        named(named(page, "folderBand"), "crumbHome").clicked(null)
        expect(view.atRoot && !view.flickable.visible, "Locations stand in for the grid too")
        App.setSetting("view_mode", JSON.stringify("list"))
        leave()
    }

    function checkReadOnly() {
        Operations.compress(JSON.stringify([inner]), target, "packed", "zip", "")
        waitFor(function () { return packed.length > 0 }, "a compressed file to look into")
        var page = share([App.localUrl(shared)])
        var view = named(page, "shareView")
        if (!view)
            return
        page.folderUri = packed
        waitFor(listed(view, packed), "the compressed file")
        expect(!view.model.writable && !page.canAccept, "nothing is saved where nothing can be added")
        same(pulley(page), null, "nor is a folder made there")
        leave()
    }

    function checkBandWidths() {
        way.uri = deepest
        var page = pageStack.push(bandBench, {}, PageStackAction.Immediate)
        var band = page.band
        var row = named(band, "crumbRow")
        var title = named(band, "bandTitle")
        var crumbs = named(band, "bandCrumbs")
        if (!row || !title || !crumbs || !waitFor(function () { return crumbs.widths.length === 4 }, "the folders above to be measured"))
            return
        var outside = []
        var folds = []
        for (var width = 2000; width > 2 * Theme.horizontalPageMargin + crumbs.reserve + title.implicitWidth; width -= 4) {
            band.width = width
            var left = row.mapToItem(band, 0, 0).x
            if (left < Theme.horizontalPageMargin - 0.5 || left + row.width > title.x + 0.5)
                outside.push(width)
            if (folds.indexOf(crumbs.folded) < 0)
                folds.push(crumbs.folded)
        }
        same(folds, [0, 1, 2, 3, 4], "from all folders shown to all folded")
        same(outside, [], "at every width house, dots and folders stay between the band's edge and the name")
        leave()
    }

    function checkPlaceStatus() {
        var page = share([App.localUrl(shared)])
        same([page.rootStatus("attention", "auth-failed", "NAS"), page.rootStatus("attention", "server-identity-changed", "NAS"),
              page.rootStatus("ready", "", "NAS")],
             ["lautta-attention-auth-title", "lautta-attention-identity-title", ""], "a place says what is wrong with it")
        expect(page.rootStatus("offline", "", "NAS").length > 0, "or that it can't be reached")
        leave()
    }

    Component.onCompleted: {
        setUp()
        checkNothingReadable()
        checkFirstTime()
        checkNextTime()
        checkOtherViews()
        checkGrid()
        checkPlaceStatus()
        checkBandWidths()
        checkReadOnly()
        reachedEnd = true
    }
}
