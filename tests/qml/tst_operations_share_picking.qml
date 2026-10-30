// SPDX-License-Identifier: LGPL-2.1-or-later Headless: no finger, no render loop; looks and the
// dialog's swipe stay for the device run.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: win

    readonly property string base: "lautta://user-documents/SharePick/"
    readonly property string inner: base + "Inner"

    ViewerTools { id: tools }

    DirectoryModel { id: documents; uri: "lautta://user-documents/" }
    DirectoryModel { id: folder }

    Component {
        id: shareComponent
        ShareTargetPage { }
    }

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

    function one(root, pred, what) {
        var all = findAll(root, pred, [])
        return expect(all.length === 1, "one " + what + ": " + all.length) ? all[0] : null
    }

    function row(view, name) {
        view.flickable.forceLayout()
        return one(view, function (x) { return x.info !== undefined && x.info.uri === win.base + name }, "row " + name)
    }

    function pulleyTexts(page) {
        var menu = one(page, function (x) { return x._isPullDownMenu === true && x.flickable && x.flickable.visible },
                       "pulley")
        if (!menu)
            return []
        menu.active = true
        var kids = menu._contentColumn.children
        var out = []
        for (var i = 0; i < kids.length; ++i)
            if (kids[i].hasOwnProperty("__silica_menuitem") && kids[i].visible)
                out.push(kids[i].text)
        menu.active = false
        return out
    }

    function open(path) {
        pageStack.push(shareComponent, { "resources": [ App.localUrl(path) ] }, PageStackAction.Immediate)
        return pageStack.currentPage
    }

    function makeFolder() {
        documents.createFolder("SharePick")
        waitFor(function () { return documents.indexOf(base.replace(/\/$/, "")) >= 0 }, "the folder")
        folder.uri = base
        folder.createFolder("Inner")
        waitFor(function () { return folder.count === 1 }, "its folder")
        expect(tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), base + "a.md"), "the shared file")
        folder.refresh()
        waitFor(function () { return !folder.loading && folder.count === 2 }, "the file")
    }

    initialPage: Component {
        Page { }
    }

    Component.onCompleted: {
        makeFolder()
        var page = open(base + "a.md")
        var view = one(page, function (x) { return x.objectName === "shareFolder" }, "folder view")
        var places = one(page, function (x) { return x.objectName === "shareLocations" }, "locations")
        if (!view || !places)
            return
        expect(page.destUri === "lautta://user-downloads/", "first time: Downloads: " + page.destUri)
        expect(view.picking && view.visible && !places.visible, "the body is the folder view")
        expect(page.readableCount === 1 && page.note === qsTrId("lautta-share-count", 1), "note: " + page.note)

        page.destUri = base
        waitFor(function () { return view.uri === base && !view.model.loading && view.model.count === 2 }, "listing")
        var file = row(view, "a.md")
        var sub = row(view, "Inner")
        if (!file || !sub)
            return
        expect(!file.enabled && file.opacity < 1, "files are dimmed and do nothing")
        expect(sub.enabled && sub.opacity === 1, "folders open")
        expect(file.menu === null && sub.menu === null, "no item menus while saving")
        expect(JSON.stringify(pulleyTexts(page)) === JSON.stringify(["lautta-dir-menu-new-folder"]),
               "pulley: only New folder: " + JSON.stringify(pulleyTexts(page)))
        expect(waitFor(function () { return page.canAccept }, "Save here"), "a writable folder can take the file")

        App.copy(JSON.stringify([base + "a.md"]))
        var clip = one(page, function (x) { return String(x).indexOf("ClipboardPanel") === 0 }, "clipboard panel")
        expect(clip && !clip.open, "no clipboard bar while saving")
        App.clearClipboard()

        sub.clicked(null)
        expect(page.destUri === inner && view.uri === inner, "a folder opens in place: " + page.destUri)
        view.goTo(base)
        expect(page.destUri === base, "the path menu goes back in place: " + page.destUri)
        view.goTo("")
        expect(!view.visible && places.visible && !page.canAccept, "Locations: nothing to save into")
        page.destUri = inner
        waitFor(function () { return page.canAccept }, "Inner ready")
        page.accept()
        expect(page.recent[0] === inner, "remembered: " + page.recentJson)
        folder.uri = inner
        var tries = 0
        waitFor(function () {
            if (++tries % 10 === 0)
                folder.refresh()
            return folder.count === 1
        }, "the saved copy")
        waitFor(function () { return !pageStack.busy && pageStack.depth === 1 }, "dialog closed")

        // Stored value set by hand: no reliance on dconf reaching a second instance headless.
        var again = open(base + "a.md")
        again.recentJson = JSON.stringify([inner])
        again.load()
        expect(again.destUri === inner, "next time: last folder saved to: " + again.destUri)
        again.destUri = ""
        var lately = findAll(again, function (x) { return x.text === "Inner" }, [])
        expect(lately.length >= 1, "saved lately lists it")
        again.recentJson = "[]"
    }
}
