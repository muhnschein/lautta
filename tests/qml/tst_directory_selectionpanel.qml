// SPDX-License-Identifier: LGPL-2.1-or-later
// The bars at the bottom on the target (real Silica, UI-10, SPEC §15.3): the
// selection panel, the paste bar, the activity bar, the undo notice and the
// search progress have an opaque backdrop, not Silica's see-through panel
// gradient alone. The selection panel shows its icons in the long-press
// menu's order, then More actions, whatever the number of items, with no
// push-up menu. More actions opens a page: for one item Rename (opening the
// rename dialog), Add to favourites and Compress, for several Compress, the
// header naming the item or counting them; a dialog it opens takes the
// page's place and ends selection, leaving the page keeps it. Archives show
// the same archive icon in a folder, in search, in the operations pages and
// in Recents. Ending selection, by Stop selecting or by deselecting the last
// item, leaves the panel with the buttons it had while it fades out, not the
// ones of no selection (just Stop selecting); waiting for the first item it
// has only that one. Fails when a bar lets content through, an action of the
// long-press menu is missing, an archive shows another icon or the panel
// changes its buttons on its way out.
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

    // Waits out the page stack's transition: Silica ignores a pop pressed
    // while one runs, and on the emulated target an animated push can take
    // longer than any fixed wait.
    function settle() {
        for (var i = 0; i < 100 && pageStack.busy; ++i)
            tools.pump(50)
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

    // The panel's button called `text`.
    function panelButton(sel, text) {
        var found = findAll(sel, function (x) { return x.description === text && x.icon !== undefined })
        return found.length === 1 ? found[0] : null
    }

    // The rows of the More actions page `page`, top down: { text, icon }
    // with the row to tap.
    function pageRows(page) {
        // Headless, no render loop polishes the list: lay it out by hand.
        findAll(page, function (x) { return x.forceLayout !== undefined }).forEach(function (v) { v.forceLayout() })
        var rows = named(page, "moreRow").sort(function (a, b) { return a.y - b.y })
        return rows.map(function (row) {
            return {
                "text": findAll(row, function (x) { return x.truncationMode !== undefined })[0].text,
                "icon": String(findAll(row, function (x) { return x.sourceSize !== undefined })[0].source),
                "row": row
            }
        })
    }

    function pageTitle(page) {
        var headers = findAll(page, function (x) { return x.extraContent !== undefined && x.title !== undefined })
        return headers.length === 1 ? headers[0].title : "no single header: " + headers.length
    }

    // Selects `uris`, taps More actions on the panel and gives the page it
    // opens, or null.
    function openMore(sel, uris) {
        var folderView = view
        folderView.startSelection()
        uris.forEach(function (uri) { folderView.model.toggle(uri) })
        tools.pump(500)
        var more = panelButton(sel, qsTrId("lautta-dir-act-more"))
        if (!expect(more !== null, "the panel has its More actions button"))
            return null
        more.clicked(null)
        tools.pump(300)
        settle()
        var page = pageStack.currentPage
        if (!expect(page.selection === true && page.actions !== undefined, "More actions opens the selection's page"))
            return null
        expect(folderView.selecting, "opening the page keeps the selection")
        return page
    }

    // Closes the dialog that took the More actions page's place: the folder
    // is under it, with the page gone.
    function closeDialog(depth, what) {
        expect(pageStack.depth === depth, what + ": the dialog took the page's place: depth " + pageStack.depth)
        pageStack.pop(null, PageStackAction.Immediate)
        tools.pump(300)
        settle()
        expect(view !== null && !view.selecting, what + " ends selection")
    }

    function checkSelectionPanel() {
        var sel = panel("buttons")
        view.startSelection()
        view.model.toggle(base + "a.md")
        tools.pump(700)
        expect(sel.open && sel.visibleSize > 0, "the selection panel is open")
        expectOpaque(sel, "selection panel")
        same(sel.buttons.map(function (b) { return b.id }), ["share", "copy", "cut", "delete", "more", "done"],
             "the long-press menu's order, More actions, then leaving")
        var buttons = findAll(sel, function (x) { return x.description !== undefined && x.icon !== undefined })
        same(buttons.map(function (b) { return b.icon }),
             ["image://theme/icon-m-share", "dir-copy", "dir-cut", "image://theme/icon-m-delete",
              "more", "image://theme/icon-m-dismiss"], "the panel's icons")
        expect(buttons.every(function (b) { return Math.abs(b.width - sel.width / 6) < 1 }),
               "the icons share the width evenly")
        same(buttons.map(function (b) { return named(b, "dockKey")[0].visible }),
             [false, false, false, false, false, true], "only Stop selecting sits on a key")
        same(String(named(buttons[5], "dockKey")[0].color),
             String(Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity)),
             "the key is the ambience's highlight background")
        same(findAll(buttons[5], function (x) { return x.themed !== undefined })[0].highlighted, true,
             "Stop selecting shows in the highlight colour")
        var menus = findAll(sel, function (x) { return x._activationPermitted !== undefined || x.hasOwnProperty("__silica_menuitem") })
        expect(menus.length === 0, "one item: no push-up menu on the panel: " + menus.length)
        view.endSelection()
        tools.pump(300)
    }

    function ids(buttons) {
        return buttons.map(function (b) { return b.id })
    }

    // The texts of the buttons the panel draws.
    function drawn(sel) {
        return findAll(sel, function (x) { return x.description !== undefined && x.icon !== undefined })
            .map(function (b) { return b.description })
    }

    // Ending selection zeroes the count while the panel is still on its way
    // out: from the first moment of the fade-out to its end the buttons stay.
    function checkLeavingKeepsButtons(sel) {
        var full = ["share", "copy", "cut", "delete", "more", "done"]
        view.startSelection()
        tools.pump(300)
        same(ids(sel.buttons), ["done"], "waiting for the first item: just Stop selecting")
        view.model.toggle(base + "a.md")
        tools.pump(700)
        same(ids(sel.buttons), full, "one item selected")
        var before = drawn(sel)
        panelButton(sel, qsTrId("lautta-dir-sel-done")).clicked(null)
        expect(!view.selecting && view.model.selectedCount === 0, "Stop selecting ends it")
        for (var i = 0; i < 20 && (i === 0 || sel.visibleSize > 0); ++i) {
            if (i === 0)
                expect(sel.visibleSize > 0, "the panel is still on screen as it leaves")
            same(ids(sel.buttons), full, "fading out after Stop selecting, step " + i)
            same(drawn(sel), before, "and it draws them, step " + i)
            tools.pump(20)
        }
        tools.pump(500)
        expect(!sel.open, "the panel is gone")

        view.startSelection()
        view.model.toggle(base + "a.md")
        tools.pump(700)
        same(ids(sel.buttons), full, "selecting again")
        view.model.toggle(base + "a.md")
        expect(!view.selecting, "deselecting the last item ends selection")
        same(ids(sel.buttons), full, "the buttons stay as the panel leaves after the last item")
        same(drawn(sel), before, "and it draws them")
        tools.pump(700)

        view.startSelection()
        tools.pump(300)
        same(ids(sel.buttons), ["done"], "waiting again: just Stop selecting")
        view.endSelection()
        tools.pump(500)
    }

    function checkMoreOfOne(sel) {
        var page = openMore(sel, [base + "a.md"])
        if (!page)
            return
        same(pageTitle(page), "a.md", "the header names the one item")
        same(pageRows(page).map(function (r) { return [r.text, r.icon] }),
             [[qsTrId("lautta-dir-act-rename"), "image://theme/icon-m-edit"],
              [qsTrId("lautta-dir-act-favourite"), "image://theme/icon-m-favorite"],
              [qsTrId("lautta-dir-act-compress"), FileIcons.archive]],
             "one item: Rename, Add to favourites, Compress, each with its icon")
        pageStack.pop(null, PageStackAction.Immediate)
        tools.pump(300)
        settle()
        expect(view !== null && view.selecting && view.model.selectedCount === 1, "leaving the page keeps the selection")

        page = openMore(sel, [])
        pageRows(page)[0].row.clicked(null)
        tools.pump(500)
        settle()
        var dialog = pageStack.currentPage
        expect(dialog.newName !== undefined && dialog.uri === base + "a.md" && dialog.isDir === false,
               "Rename opens the rename dialog for the selected file")
        closeDialog(2, "Rename")

        page = openMore(sel, [base + "a.md"])
        pageRows(page)[1].row.clicked(null)
        tools.pump(500)
        settle()
        var favourite = pageStack.currentPage
        expect(favourite.added !== undefined && favourite.uri === base + "a.md" && favourite.isDir === false,
               "Add to favourites opens the favourite page for the selected file")
        closeDialog(2, "Add to favourites")
    }

    function checkMoreOfSeveral(sel) {
        var folderView = view
        folderView.startSelection()
        folderView.model.selectAll()
        tools.pump(500)
        var page = openMore(sel, [])
        if (!page)
            return
        same(pageTitle(page), qsTrId("lautta-dir-selected", 3), "the header counts the selected items")
        same(pageRows(page).map(function (r) { return [r.text, r.icon] }),
             [[qsTrId("lautta-dir-act-compress"), FileIcons.archive]],
             "several items: Compress only")
        // A dialog takes the page's place while selection ends: the header
        // and the rows stay as they were.
        folderView.endSelection()
        tools.pump(300)
        same(pageTitle(page), qsTrId("lautta-dir-selected", 3), "the header keeps its count when selection ends")
        same(pageRows(page).length, 1, "the rows stay when selection ends")
        pageStack.pop(null, PageStackAction.Immediate)
        tools.pump(300)
        settle()

        folderView.model.selectAll()
        page = openMore(sel, [])
        pageRows(page)[0].row.clicked(null)
        tools.pump(500)
        settle()
        var dialog = pageStack.currentPage
        expect(dialog.formats !== undefined, "Compress opens the compress dialog")
        same(dialog.uris.slice().sort(), [base + "a.md", base + "b.md", base + "c.tar.gz"], "for every selected item")
        closeDialog(2, "Compress")
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
            var sel = panel("buttons")
            checkLeavingKeepsButtons(sel)
            checkMoreOfOne(sel)
            checkMoreOfSeveral(sel)
            // Grid view holds to select: the same panel and page there.
            App.setSetting("view_mode", JSON.stringify("grid"))
            tools.pump(300)
            if (expect(view.gridMode, "the folder shows its files as a grid"))
                checkMoreOfOne(sel)
            App.setSetting("view_mode", JSON.stringify("list"))
            tools.pump(300)
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
