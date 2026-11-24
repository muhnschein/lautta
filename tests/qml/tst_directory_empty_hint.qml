// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: window
    property bool reachedEnd

    readonly property string base: "lautta://user-documents/EmptyHint/"
    readonly property string empty: base + "empty/"
    readonly property string other: base + "other/"
    property string archiveRoot

    ViewerTools { id: tools }

    DirectoryModel { id: documents; uri: "lautta://user-documents/" }
    DirectoryModel { id: top }

    Connections {
        target: Operations
        onJobFinished: if (ok) Operations.openArchive(resultUri, "")
        onArchiveOpened: window.archiveRoot = rootUri
    }

    Component {
        id: pageComponent

        DirectoryPage { }
    }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        return expect(done(), "timed out waiting for " + (typeof what === "function" ? what() : what))
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

    function one(pred, what) {
        var found = findAll(window.contentItem, pred, [])
        expect(found.length === 1, "one " + what + ": " + found.length)
        return found.length === 1 ? found[0] : null
    }

    function shownHint() {
        var holder = one(function (x) { return x.objectName === "folderPlaceholder" }, "placeholder")
        return holder && holder.visible ? holder.hint : ""
    }

    function pulleyPaste() {
        var menu = one(function (x) { return x._isPullDownMenu === true && x.flickable && x.flickable.visible }, "folder pulley")
        if (!menu)
            return false
        menu.active = true
        var entries = findAll(menu._contentColumn, function (x) {
            return x.hasOwnProperty("__silica_menuitem") && x.visible && x.text.indexOf("lautta-dir-menu-paste") === 0
        }, [])
        menu.active = false
        return entries.length === 1
    }

    function panelPaste() {
        var panel = one(function (x) { return x.fromText !== undefined && x.open !== undefined }, "paste bar")
        if (!panel || !waitFor(function () { return panel.open && panel.visibleSize > 0 }, "the paste bar open"))
            return null
        var button = one(function (x) { return x.accent !== undefined && x.description === "lautta-dir-clip-paste" }, "Paste here")
        return button ? button.visible : null
    }

    // Page opened before the clipboard changes: what it offers follows the clipboard without a new page.
    function expectState(hint, paste, what) {
        waitFor(function () { return shownHint() === hint }, function () { return what + ": hint " + hint + ", not " + shownHint() })
        expect(pulleyPaste() === paste, what + ": pulley " + (paste ? "has" : "has no") + " Paste")
        if (App.clipboardCount > 0)
            expect(panelPaste() === paste, what + ": paste bar " + (paste ? "has" : "has no") + " Paste here")
    }

    function openEmpty(uri, replace) {
        if (replace)
            pageStack.replace(pageComponent, { "uri": uri }, PageStackAction.Immediate)
        else
            pageStack.push(pageComponent, { "uri": uri }, PageStackAction.Immediate)
        return waitFor(function () {
            var views = findAll(window.contentItem, function (x) { return x.placeholderKind !== undefined && x.runAction !== undefined }, [])
            return views.length === 1 && views[0].uri === uri && !views[0].model.loading && views[0].placeholderKind() === "empty"
        }, "empty folder " + uri + " open")
    }

    Component.onCompleted: {
        App.clearClipboard()
        documents.createFolder("EmptyHint")
        waitFor(function () { return documents.nameKind("EmptyHint") === "folder" }, "the folder")
        top.uri = base
        top.createFolder("empty")
        waitFor(function () { return top.nameKind("empty") === "folder" }, "the empty folder")
        top.createFolder("other")
        waitFor(function () { return top.nameKind("other") === "folder" }, "the other folder")
        openEmpty(empty, false)

        expectState("lautta-dir-empty-hint", false, "nothing copied")
        expect(App.copy(JSON.stringify([other])), "copy the other folder")
        expectState("lautta-dir-empty-hint-paste", true, "a folder copied that pastes here")
        expect(App.copy(JSON.stringify([empty])), "copy the open folder")
        expectState("lautta-dir-empty-hint", false, "the open folder copied: no paste into itself")
        App.clearClipboard()
        expectState("lautta-dir-empty-hint", false, "clipboard cleared")

        Operations.compress(JSON.stringify([empty]), base, "packed", "zip", "")
        waitFor(function () { return archiveRoot.length > 0 }, "the compressed folder")
        openEmpty(archiveRoot + "empty/", true)
        expect(App.copy(JSON.stringify([other])), "copy the other folder again")
        expectState("lautta-dir-empty-hint-readonly", false, "an empty folder in a compressed file: read-only")
        App.clearClipboard()
        reachedEnd = true
    }
}
