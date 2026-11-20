// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: window
    property bool reachedEnd

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
            console.error("FAILED: " + what)
        return ok
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
        var view = find(page, "viewBox")
        expect(section && section.visible && rowOf(section) === rowOf(view) + 1,
               "Thumbnails right after View, no preview between: " + [rowOf(view), rowOf(section)])
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

    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        return expect(done(), "timed out waiting for " + what)
    }

    function makeFolders() {
        documents.createFolder("ViewOptionsTest")
        documents.createFolder("ViewOptionsPhotos")
        waitFor(function () { return documents.indexOf(base.replace(/\/$/, "")) >= 0
                                     && documents.indexOf("lautta://user-documents/ViewOptionsPhotos") >= 0 }, "the folders")
        expect(tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), base + "a.md")
               && tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), base + "b.jpg")
               && tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), "lautta://user-documents/ViewOptionsPhotos/a.jpg")
               && tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), "lautta://user-documents/ViewOptionsPhotos/b.jpg"),
               "fixtures installed")
        openUri = base
        waitFor(function () { return !open.loading && open.count === 2 && open.indexOf(base + "a.md") === 0 },
                "the open folder to list a.md first")
    }

    function checkControls(page) {
        pick(page, "sortBox", "size")
        expect(App.setting("sort_key") === "size", "Sort by applies at once")
        tap(page, "descendingSwitch")
        expect(App.setting("sort_descending") === true, "Descending applies at once")
        tap(page, "foldersFirstSwitch")
        expect(App.setting("folders_first") === false, "Folders first applies at once")
        tap(page, "hiddenSwitch")
        expect(App.setting("show_hidden") === true, "Show hidden files applies at once")
        pick(page, "viewBox", "grid")
        expect(App.setting("view_mode") === "grid", "View applies at once")
        expect(find(page, "viewBox").currentIndex === 1, "View shows Grid")
        expect(find(page, "gridBox") === null, "a grid has no size to choose")
        tap(page, "thumbnailsSwitch")
        expect(App.setting("thumbnails_local") === false, "Thumbnails applies at once")
    }

    function checkOpenFolderFollowed() {
        waitFor(function () { return open.sortKey === "size" && open.indexOf(base + "b.jpg") === 0 }, "the open folder to re-sort")
        expect(open.sortKey === "size" && open.descending && !open.foldersFirst && open.showHidden,
               "an open folder follows the view options: " + open.sortKey + " " + open.descending)
        expect(open.viewMode === "grid" && !open.thumbnails, "its view too: " + open.viewMode)
        expect(open.indexOf(base + "b.jpg") === 0, "the larger file is first now")
    }

    function restoreDefaults() {
        var defaults = { "sort_key": "name", "sort_descending": false, "folders_first": true, "show_hidden": false,
                         "view_mode": "list", "thumbnails_local": true }
        for (var key in defaults)
            App.setSetting(key, JSON.stringify(defaults[key]))
        waitFor(function () { return open.viewMode === "list" && open.sortKey === "name" && !open.showHidden }, "the defaults")
    }

    function checkOwnFolder() {
        suggestedUri = "lautta://user-documents/ViewOptionsPhotos/"
        waitFor(function () { return !suggested.loading && suggested.suggestGrid && suggested.viewMode === "grid" },
                "a folder of photos to suggest a grid")
        pageStack.push(pageComponent, { "model": suggested }, PageStackAction.Immediate)
        waitFor(function () { return !pageStack.busy && pageStack.currentPage.objectName === "viewOptionsPage" }, "the page")
        var page = pageStack.currentPage
        expect(find(page, "thumbnailsSection").visible && find(page, "remoteThumbnailsSwitch").visible,
               "the thumbnails are there from a folder too")
        pick(page, "sortBox", "modified")
        waitFor(function () { return suggested.sortKey === "modified" }, "the folder to sort by date")
        expect(suggested.viewMode === "grid", "sorting keeps the suggested grid")
        pick(page, "viewBox", "list")
        expect(App.setting("view_mode") === "list", "the global view was list already")
        expect(suggested.viewMode === "list" && !suggested.suggestGrid, "the folder takes List: " + suggested.viewMode)
        App.setSetting("sort_key", JSON.stringify("name"))
    }

    Component.onCompleted: {
        makeFolders()
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        waitFor(function () { return !pageStack.busy && pageStack.currentPage.objectName === "viewOptionsPage" }, "the page")
        var page = pageStack.currentPage
        expect(page.canAccept === undefined, "a page, not a dialog")
        checkLayout(page)
        checkRemoteThumbnails(page)
        checkControls(page)
        checkOpenFolderFollowed()
        restoreDefaults()
        checkOwnFolder()
        reachedEnd = true
    }

    Component {
        id: pageComponent

        ViewOptionsPage { }
    }
}
