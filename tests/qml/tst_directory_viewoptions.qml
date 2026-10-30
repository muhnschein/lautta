// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"
import "../../qml/components/directory"

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

    DirectoryModel {
        id: open
        uri: window.openUri
    }

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

    function textShown(rows) {
        return rows.some(function (r) { return r.text })
    }

    function sameSize(preview, size, what) {
        expect(preview.width === size.width && preview.height === size.height,
               "the preview keeps its size with " + what + ": " + preview.width + "x" + preview.height
               + ", was " + size.width + "x" + size.height)
    }

    function keysOf(page) {
        var out = []
        var walk = function (item) {
            if (!item)
                return
            if (typeof item.settingKey === "string" && out.indexOf(item.settingKey) < 0)
                out.push(item.settingKey)
            var kids = item.children || []
            for (var i = 0; i < kids.length; ++i)
                walk(kids[i])
            if (item.contentItem && item.contentItem !== item)
                walk(item.contentItem)
        }
        walk(page)
        return out
    }

    function rowOf(item) {
        var rows = item.parent.children
        for (var i = 0; i < rows.length; ++i) {
            if (rows[i] === item)
                return i
        }
        return -1
    }

    function checkLayout(page) {
        var header = find(page, "viewOptionsHeader")
        expect(header && header.description === "", "the header has no description: " + (header ? header.description : "none"))
        var section = find(page, "thumbnailsSection")
        var local = find(page, "thumbnailsSwitch")
        var remote = find(page, "remoteThumbnailsSwitch")
        expect(section && section.visible && rowOf(section) > rowOf(page.preview), "a Thumbnails section below the preview")
        expect(rowOf(local) > rowOf(section) && rowOf(remote) > rowOf(local),
               "both thumbnail switches are in the section: " + [rowOf(section), rowOf(local), rowOf(remote)])
        var keys = keysOf(page)
        var wanted = ["thumbnails_remote", "thumbnail_max_remote_mb", "thumbnail_cache_mb"]
        expect(JSON.stringify(keys) === JSON.stringify(wanted), "the thumbnail settings: " + keys)
        expect(local.description.length > 0 && remote.description.length > 0,
               "both thumbnail switches say what they cost")
    }

    function checkRemoteThumbnails(page) {
        var remote = find(page, "remoteThumbnailsSwitch")
        var limit = find(page, "remoteThumbnailLimitBox")
        expect(remote.enabled && limit.enabled, "server thumbnails can be set while thumbnails are on")
        tap(page, "remoteThumbnailsSwitch")
        expect(App.setting("thumbnails_remote") === false, "Thumbnails from servers applies at once")
        expect(!limit.enabled, "no size limit without them")
        tap(page, "remoteThumbnailsSwitch")
        expect(App.setting("thumbnails_remote") === true, "and on again")
        tap(page, "thumbnailsSwitch")
        expect(!remote.enabled && !limit.enabled, "nothing from servers without thumbnails")
        tap(page, "thumbnailsSwitch")
        expect(remote.enabled && limit.enabled, "back with thumbnails")
    }

    function makeFolders() {
        documents.createFolder("ViewOptionsTest")
        tools.pump(500)
        documents.createFolder("ViewOptionsPhotos")
        tools.pump(500)
        tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), base + "a.md")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), base + "b.jpg")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), "lautta://user-documents/ViewOptionsPhotos/a.jpg")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), "lautta://user-documents/ViewOptionsPhotos/b.jpg")
        openUri = base
        tools.pump(500)
        expect(open.count === 2 && open.indexOf(base + "a.md") === 0, "the open folder lists a.md first: " + open.count)
    }

    function checkControls(page) {
        var preview = page.preview
        var size = { "width": preview.width, "height": preview.height }
        expect(size.width > 0 && size.height > 0, "the preview has a size: " + size.width + "x" + size.height)
        var rows = preview.shown()
        expect(!preview.gridMode && rows.length === 6 && rows[0].isDir && rows[1].isDir,
               "the preview starts as a list, folders first, no hidden file: " + JSON.stringify(rows))
        expect(photoShown(rows), "the photo has its thumbnail")

        pick(page, "sortBox", "size")
        expect(App.setting("sort_key") === "size", "Sort by applies at once")
        expect(ordered(fileSizes(preview.shown()), false), "smallest file first: " + fileSizes(preview.shown()))
        sameSize(preview, size, "another order")

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
        sameSize(preview, size, "the hidden file")

        pick(page, "viewBox", "grid")
        expect(App.setting("view_mode") === "grid", "View applies at once")
        expect(preview.gridMode && preview.columns === 3 && preview.shown().length === 7,
               "the preview is a grid of 3 per row: " + preview.columns)
        expect(preview.shown().every(function (r) { return r.captions.length > 0 }),
               "with caption lines under every name: " + JSON.stringify(preview.shown()))
        expect(find(page, "viewBox").currentIndex === 1, "View shows Grid")
        expect(find(page, "gridBox") === null, "a grid has no size to choose")
        sameSize(preview, size, "a grid")

        expect(photoShown(preview.shown()), "the grid shows the photo")
        expect(textShown(preview.shown()), "and the notes' first lines")
        tap(page, "thumbnailsSwitch")
        expect(App.setting("thumbnails_local") === false, "Thumbnails applies at once")
        expect(!photoShown(preview.shown()) && !textShown(preview.shown()), "no thumbnail without thumbnails")
        tap(page, "hiddenSwitch")
        sameSize(preview, size, "no hidden file in the grid")
        tap(page, "hiddenSwitch")
    }

    function checkOpenFolderFollowed() {
        tools.pump(500)
        expect(open.sortKey === "size" && open.descending && !open.foldersFirst && open.showHidden,
               "an open folder follows the view options: " + open.sortKey + " " + open.descending)
        expect(open.viewMode === "grid" && !open.thumbnails, "its view too: " + open.viewMode)
        expect(open.indexOf(base + "b.jpg") === 0, "the larger file is first now")
    }

    function checkFixedSize() {
        var views = [
            { "show_hidden": false, "view_mode": "list" },
            { "show_hidden": true },
            { "view_mode": "grid" },
            { "show_hidden": false },
            { "sort_key": "size", "thumbnails_local": false }
        ]
        var size = null
        for (var i = 0; i < views.length; ++i) {
            for (var key in views[i])
                App.setSetting(key, JSON.stringify(views[i][key]))
            var preview = previewComponent.createObject(window.contentItem,
                                                        { "width": 540, "screenWidth": 540, "screenHeight": 960 })
            tools.pump(200)
            var what = JSON.stringify(views[i]) + " (" + preview.count + " rows, "
                       + (preview.gridMode ? preview.columns + " per row" : "a list") + ")"
            if (size === null) {
                size = { "width": preview.width, "height": preview.height }
                expect(size.height === 960 / 2, "half the page's height: " + size.height)
            }
            sameSize(preview, size, what)
            preview.destroy()
            tools.pump(100)
        }
        restoreDefaults()
    }

    function restoreDefaults() {
        var defaults = { "sort_key": "name", "sort_descending": false, "folders_first": true, "show_hidden": false,
                         "view_mode": "list", "thumbnails_local": true }
        for (var key in defaults)
            App.setSetting(key, JSON.stringify(defaults[key]))
        tools.pump(300)
    }

    function checkOwnFolder() {
        suggestedUri = "lautta://user-documents/ViewOptionsPhotos/"
        tools.pump(800)
        expect(suggested.suggestGrid && suggested.viewMode === "grid", "a folder of photos suggests a grid")
        pageStack.push(pageComponent, { "model": suggested }, PageStackAction.Immediate)
        tools.pump(300)
        var page = pageStack.currentPage
        expect(find(page, "thumbnailsSection").visible && find(page, "remoteThumbnailsSwitch").visible,
               "the thumbnails are there from a folder too")
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
        checkLayout(page)
        checkRemoteThumbnails(page)
        checkControls(page)
        checkOpenFolderFollowed()
        restoreDefaults()
        checkOwnFolder()
        checkFixedSize()
    }

    Component {
        id: previewComponent

        ViewPreview { }
    }

    Component {
        id: pageComponent

        ViewOptionsPage { }
    }
}
