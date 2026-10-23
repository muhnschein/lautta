// SPDX-License-Identifier: LGPL-2.1-or-later
// The bars at the bottom on the target (real Silica, UI-9, SPEC §15.3): the
// selection panel, the paste bar, the activity bar, the undo notice and the
// search progress have an opaque backdrop, not Silica's see-through panel
// gradient alone. The selection panel shows its icons in the long-press
// menu's order with the archive icon on Compress; with one item selected it
// offers Rename and Add to favourites when pushed up, and Rename opens the
// rename dialog for that item. Archives show the same archive icon in a
// folder, in search, in the operations pages and in Recents. Fails when a
// bar lets content through, an action of the long-press menu is missing or
// an archive shows another icon.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components"
import "../../qml/components/browse"
import "../../qml/components/directory"
import "../../qml/components/operations"
import "../../qml/components/search"
import "../../qml/components/transfers"
import "../../qml/components/FileIcons.js" as FileIcons
import "../../qml/pages"

ApplicationWindow {
    id: win

    readonly property string base: "lautta://user-documents/SelectionPanel/"
    property var view: pageStack.depth > 0 && pageStack.currentPage.view ? pageStack.currentPage.view : null

    ViewerTools { id: tools }

    DirectoryModel {
        id: documents
        uri: "lautta://user-documents/"
    }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    function same(a, b, what) {
        return expect(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    // Every item under `item` that `pred` takes; a flickable's content item
    // is one of its children too: each item once.
    function findAll(item, pred, out, seen) {
        out = out || []
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

    function named(item, name) {
        return findAll(item, function (x) { return x.objectName === name })
    }

    // The bar's backdrop is opaque and covers it, and nothing between it and
    // the bar makes it see-through (the activity bar and the undo notice fade
    // as a whole when they go).
    function expectOpaque(bar, what) {
        var grounds = named(bar, "barBackground")
        if (!expect(grounds.length === 1, what + " has the opaque backdrop: " + grounds.length))
            return
        var ground = grounds[0]
        expect(ground.color.a === 1, what + ": its backdrop's alpha is 1: " + ground.color.a)
        expect(ground.visible || !bar.visible, what + ": the backdrop shows with the bar")
        expect(ground.width >= bar.width - 1 && ground.height >= bar.height - 1,
               what + ": the backdrop covers the bar: " + ground.width + "x" + ground.height)
        for (var item = ground; item && item !== bar; item = item.parent)
            expect(item.opacity === 1, what + ": nothing over the backdrop is translucent: " + item)
    }

    // The docked panel of the folder view with `prop`.
    function panel(prop) {
        var kids = view.children
        for (var i = 0; i < kids.length; ++i) {
            if (kids[i].dock !== undefined && kids[i][prop] !== undefined)
                return kids[i]
        }
        return null
    }

    function checkSelectionPanel() {
        var sel = panel("buttons")
        view.startSelection()
        view.model.toggle(base + "a.md")
        tools.pump(700)
        expect(sel.open && sel.visibleSize > 0, "the selection panel is open")
        expectOpaque(sel, "selection panel")
        same(sel.buttons.map(function (b) { return b.id }), ["share", "copy", "cut", "delete", "compress", "done"],
             "the long-press menu's order, then leaving")
        var buttons = findAll(sel, function (x) { return x.description !== undefined && x.icon !== undefined })
        same(buttons.map(function (b) { return b.icon }),
             ["image://theme/icon-m-share", "dir-copy", "dir-cut", "image://theme/icon-m-delete",
              FileIcons.archive, "image://theme/icon-m-dismiss"], "the panel's icons")
        expect(buttons.every(function (b) { return Math.abs(b.width - sel.width / 6) < 1 }),
               "the icons share the width evenly")

        var pulley = findAll(sel, function (x) { return x._activationPermitted !== undefined })
        var more = findAll(sel, function (x) { return x.hasOwnProperty("__silica_menuitem") })
        same(more.map(function (m) { return m.text }), [qsTrId("lautta-dir-act-rename"), qsTrId("lautta-dir-act-favourite")],
             "one item: the push-up menu holds the rest of its long-press menu")
        expect(pulley.length === 1 && pulley[0].visible, "the panel has a push-up menu")

        more[0].clicked()
        tools.pump(500)
        var dialog = pageStack.currentPage
        expect(dialog.newName !== undefined && dialog.uri === base + "a.md" && dialog.isDir === false,
               "Rename opens the rename dialog for the selected file")
        pageStack.pop(null, PageStackAction.Immediate)
        tools.pump(300)
        expect(!view.selecting, "Rename leaves selection")

        view.model.selectAll()
        tools.pump(300)
        more = findAll(sel, function (x) { return x.hasOwnProperty("__silica_menuitem") })
        expect(more.length === 0 && !pulley[0].visible, "several items: no single-item actions: " + more.length)
        view.endSelection()
        tools.pump(300)
    }

    function checkClipboardPanel() {
        App.copy(JSON.stringify([base + "a.md"]))
        tools.pump(700)
        var clip = panel("fromText")
        expect(clip.open, "the paste bar is open")
        expectOpaque(clip, "paste bar")
        App.clearClipboard()
        tools.pump(300)
    }

    function checkOtherBars() {
        expectOpaque(activityBar, "activity bar")
        expectOpaque(undoBanner.children[0], "undo notice")
        pageStack.push(Qt.resolvedUrl("../../qml/pages/SearchPage.qml"), {}, PageStackAction.Immediate)
        tools.pump(300)
        var searchPanels = findAll(pageStack.currentPage, function (x) { return x.dock !== undefined })
        if (expect(searchPanels.length === 1, "the search page has its progress panel"))
            expectOpaque(searchPanels[0], "search progress")
    }

    function checkArchiveIcons() {
        // Headless, no render loop polishes the list: lay it out by hand.
        findAll(view, function (x) { return x.forceLayout !== undefined }).forEach(function (v) { v.forceLayout() })
        var rows = findAll(view, function (x) { return x.category === "archive" && x.thumbnailReady !== undefined })
        if (expect(rows.length === 1, "the archive's row has a file icon"))
            same(findAll(rows[0], function (x) { return x.themed !== undefined })[0].icon, FileIcons.archive,
                 "a folder shows archives with the archive icon")
        same([fileIcon.iconFor("archive", false), opIcon.icon, resultIcon.icon, String(itemIcon.source)],
             [FileIcons.archive, FileIcons.archive, FileIcons.archive, FileIcons.archive],
             "folders, operations, search and Recents show the same archive icon")
        expect(FileIcons.archive === "image://theme/icon-m-file-archive-folder",
               "the platform's archive icon, not the off-centre zipper badge")
    }

    Component.onCompleted: {
        documents.createFolder("SelectionPanel")
        tools.pump(500)
        tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), base + "a.md")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), base + "b.md")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/data.bin"), base + "c.tar.gz")
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        tools.pump(800)
        if (expect(view && view.model.count === 3, "the folder lists its files")) {
            checkSelectionPanel()
            checkClipboardPanel()
            checkArchiveIcons()
        }
        checkOtherBars()
    }

    Item {
        width: 400
        height: 800

        ActivityBar { id: activityBar }
        UndoBanner { id: undoBanner }
        FileIcon { id: fileIcon; category: "archive" }
        OpIcon { id: opIcon; category: "archive" }
        ResultIcon { id: resultIcon; category: "archive" }
        ItemIcon { id: itemIcon; name: "backup.tar.gz" }
    }

    Component {
        id: pageComponent

        Page {
            property alias view: folder

            DirectoryView {
                id: folder
                anchors.fill: parent
                uri: win.base
            }
        }
    }
}
