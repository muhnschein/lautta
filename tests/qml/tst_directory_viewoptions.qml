// SPDX-License-Identifier: LGPL-2.1-or-later
// View options on the target (BRW-4, board ViewOptions): a settings page,
// not a dialog. Every control changes the global setting at once and the
// preview follows (order, folders first, hidden file, list or grid, grid
// size, photo thumbnail); a folder already open follows too, and the folder
// the page came from takes a choice that only differed there (List over a
// suggested grid). Fails when the page waits for an Apply, the preview or
// an open folder keeps the old view, or the folder keeps its suggestion.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: window

    readonly property string base: "lautta://user-documents/ViewOptionsTest/"
    property string openUri
    property string suggestedUri

    ViewerTools { id: tools }

    DirectoryModel {
        id: documents
        uri: "lautta://user-documents/"
    }

    // A folder open further down the page stack.
    DirectoryModel {
        id: open
        uri: window.openUri
    }

    // A folder of photos, shown as a suggested grid.
    DirectoryModel {
        id: suggested
        uri: window.suggestedUri
    }

    function expect(ok, what) {
        if (!ok)
            console.error(what)
    }

    function find(item, name) {
        if (!item)
            return null
        if (item.objectName === name)
            return item
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i) {
            var found = find(kids[i], name)
            if (found)
                return found
        }
        return item.contentItem && item.contentItem !== item ? find(item.contentItem, name) : null
    }

    // Taps `item` in the menu of the combo box `box`, as a finger does.
    function pick(page, box, item) {
        var menu = find(page, box).menu
        var kids = menu._contentColumn.children
        for (var i = 0; i < kids.length; ++i) {
            if (kids[i].objectName === item) {
                menu._activatedMenuItem(kids[i])
                return
            }
        }
        console.error("no " + item + " in " + box)
    }

    function tap(page, name) {
        find(page, name).clicked(null)
    }

    // The sizes of the files the preview shows, in order.
    function fileSizes(rows) {
        return rows.filter(function (r) { return !r.isDir }).map(function (r) { return r.size })
    }

    function ordered(list, descending) {
        for (var i = 1; i < list.length; ++i) {
            if (descending ? list[i] > list[i - 1] : list[i] < list[i - 1])
                return false
        }
        return list.length > 1
    }

    function photoShown(rows) {
        return rows.some(function (r) { return r.photo })
    }

    function makeFolders() {
        documents.createFolder("ViewOptionsTest")
        tools.pump(500)
        documents.createFolder("ViewOptionsPhotos")
        tools.pump(500)
        // The smaller file first by name.
        tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), base + "a.md")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), base + "b.jpg")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), "lautta://user-documents/ViewOptionsPhotos/a.jpg")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), "lautta://user-documents/ViewOptionsPhotos/b.jpg")
        openUri = base
        tools.pump(500)
        expect(open.count === 2 && open.indexOf(base + "a.md") === 0, "the open folder lists a.md first: " + open.count)
    }

    // Opened from Settings: no folder of its own.
    function checkControls(page) {
        var preview = page.preview
        var rows = preview.shown()
        expect(!preview.gridMode && rows.length === 6 && rows[0].isDir && rows[1].isDir,
               "the preview starts as a list, folders first, no hidden file: " + JSON.stringify(rows))
        expect(photoShown(rows), "the photo has its thumbnail")

        pick(page, "sortBox", "size")
        expect(App.setting("sort_key") === "size", "Sort by applies at once")
        expect(ordered(fileSizes(preview.shown()), false), "smallest file first: " + fileSizes(preview.shown()))

        tap(page, "descendingSwitch")
        expect(App.setting("sort_descending") === true, "Descending applies at once")
        expect(ordered(fileSizes(preview.shown()), true), "largest file first: " + fileSizes(preview.shown()))

        tap(page, "foldersFirstSwitch")
        rows = preview.shown()
        expect(App.setting("folders_first") === false, "Folders first applies at once")
        expect(!rows[0].isDir && rows[rows.length - 1].isDir, "folders, which have no size, go last: " + JSON.stringify(rows))

        tap(page, "hiddenSwitch")
        rows = preview.shown()
        expect(App.setting("show_hidden") === true, "Show hidden files applies at once")
        expect(rows.length === 7 && rows.some(function (r) { return r.name.charAt(0) === "." }),
               "the hidden file is shown: " + JSON.stringify(rows))

        pick(page, "viewBox", "grid")
        expect(App.setting("view_mode") === "grid", "View applies at once")
        expect(preview.gridMode && preview.columns === 4 && !preview.gridCaptions && preview.shown().length === 7,
               "the preview is a grid of 4 per row: " + preview.columns)
        expect(find(page, "viewBox").currentIndex === 1, "View shows Grid")

        pick(page, "gridBox", "3")
        expect(App.setting("grid_columns") === 3, "Grid size applies at once")
        expect(preview.columns === 3 && preview.gridCaptions, "3 per row with names: " + preview.columns)

        expect(photoShown(preview.shown()), "the grid shows the photo")
        tap(page, "thumbnailsSwitch")
        expect(App.setting("thumbnails_local") === false, "Thumbnails applies at once")
        expect(!photoShown(preview.shown()), "no thumbnail without thumbnails")
    }

    function checkOpenFolderFollowed() {
        tools.pump(500)
        expect(open.sortKey === "size" && open.descending && !open.foldersFirst && open.showHidden,
               "an open folder follows the view options: " + open.sortKey + " " + open.descending)
        expect(open.viewMode === "grid" && !open.thumbnails, "its view too: " + open.viewMode)
        expect(open.indexOf(base + "b.jpg") === 0, "the larger file is first now")
    }

    function restoreDefaults() {
        var defaults = { "sort_key": "name", "sort_descending": false, "folders_first": true, "show_hidden": false,
                         "view_mode": "list", "grid_columns": 4, "thumbnails_local": true }
        for (var key in defaults)
            App.setSetting(key, JSON.stringify(defaults[key]))
        tools.pump(300)
    }

    // Opened from a folder showing a suggested grid.
    function checkOwnFolder() {
        suggestedUri = "lautta://user-documents/ViewOptionsPhotos/"
        tools.pump(800)
        expect(suggested.suggestGrid && suggested.viewMode === "grid", "a folder of photos suggests a grid")
        pageStack.push(pageComponent, { "model": suggested }, PageStackAction.Immediate)
        tools.pump(300)
        var page = pageStack.currentPage
        pick(page, "sortBox", "modified")
        tools.pump(300)
        expect(suggested.sortKey === "modified" && suggested.viewMode === "grid", "sorting keeps the suggested grid")
        pick(page, "viewBox", "list")
        expect(App.setting("view_mode") === "list", "the global view was list already")
        expect(suggested.viewMode === "list" && !suggested.suggestGrid, "the folder takes List: " + suggested.viewMode)
        App.setSetting("sort_key", JSON.stringify("name"))
    }

    Component.onCompleted: {
        makeFolders()
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        tools.pump(300)
        var page = pageStack.currentPage
        expect(page.objectName === "viewOptionsPage" && page.canAccept === undefined, "a page, not a dialog")
        checkControls(page)
        checkOpenFolderFollowed()
        restoreDefaults()
        checkOwnFolder()
    }

    Component {
        id: pageComponent

        ViewOptionsPage { }
    }
}
