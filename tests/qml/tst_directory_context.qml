// SPDX-License-Identifier: LGPL-2.1-or-later
// Scenario on the target (real Silica): the long-press menu of a file ends
// with More actions (SPEC §15.3, OPS-12); tapping it closes the menu and
// opens the More actions page, its header naming the file and one row per
// action with its icon (Compress and Move to new folder the app's archive and
// folder glyphs). Add to favourites opens the favourite page
// in the page's place, which makes the file itself a favourite at once, as a
// file;
// Compress opens the compress dialog for the file in the page's place, ready
// to accept. Fails when More actions changes the menu instead, a row is
// missing or has no icon, a dialog stacks on the page (leaving it would
// land on the page again), or an action gets the wrong item. The file's
// favourite is removed again at the end, so later scenarios sharing the
// database see none of it.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"
import "../../qml/components/directory/ContextActions.js" as Actions
import "../../qml/components/FileIcons.js" as FileIcons

ApplicationWindow {
    id: win

    readonly property string folder: "lautta://user-documents/"
    readonly property string file: folder + "report.md"
    property bool scenarioRunning: true
    property int step: 0
    property int waited: 0
    property var row
    property var menu
    property var folderPage
    // The folder's favourite before (an earlier scenario's, or -1).
    property int folderBefore: -2

    ViewerTools { id: tools }

    FavouritesModel { id: favs }

    initialPage: Component {
        DirectoryPage {
            uri: win.folder
        }
    }

    function fail(what) {
        console.error("FAILED: " + what)
        favs.removeUri(file)
        scenarioRunning = false
    }

    // A flickable's content item is one of its children too: each item once.
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

    // Headless, no render loop polishes the lists: lay them out by hand.
    function layOut(root) {
        var views = findAll(root, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
        for (var i = 0; i < views.length; ++i)
            views[i].forceLayout()
    }

    function fileRow() {
        layOut(win.contentItem)
        var rows = findAll(win.contentItem, function(x) {
            return x.openMenu !== undefined && x.info !== undefined && x.info.uri === win.file
        }, [])
        return rows.length > 0 ? rows[0] : null
    }

    function menuRows() {
        return findAll(win.menu, function(x) { return x.hasOwnProperty("__silica_menuitem") && x.visible }, [])
    }

    function texts(items) {
        var out = []
        for (var i = 0; i < items.length; ++i)
            out.push(items[i].text)
        return out
    }

    function expectTexts(ids, what) {
        var want = []
        for (var i = 0; i < ids.length; ++i)
            want.push(Actions.text(ids[i]))
        var got = texts(menuRows())
        if (JSON.stringify(got) !== JSON.stringify(want))
            return fail(what + ": " + JSON.stringify(got) + " != " + JSON.stringify(want))
        return true
    }

    // The More actions page on top, settled, or null.
    function morePage() {
        var page = pageStack.currentPage
        if (!page || page.actions === undefined || page.choose === undefined || pageStack.busy
                || page.status !== PageStatus.Active)
            return null
        layOut(page)
        return page
    }

    // The app's glyph of `kind` at a medium icon's size (FileIcons.js).
    function glyph(kind) {
        return String(Qt.resolvedUrl("../../qml/icons/" + FileIcons.glyph(kind, false, Theme.iconSizeMedium) + ".svg"))
    }

    // The rows of the More actions page: [{ row, icon, text }].
    function pageRows(page) {
        var icons = findAll(page, function(x) {
            return x.sourceSize !== undefined && x.highlightColor !== undefined && x.visible
        }, [])
        var out = []
        for (var i = 0; i < icons.length; ++i) {
            // Icon in the row's content item, in the row.
            var row = icons[i].parent.parent
            var labels = findAll(row, function(x) { return x.truncationMode !== undefined && x.text !== undefined }, [])
            out.push({ "row": row, "icon": String(icons[i].source), "text": labels.length > 0 ? labels[0].text : "" })
        }
        return out
    }

    function expectPage(page) {
        var header = findAll(page, function(x) { return x.title !== undefined && x.description !== undefined }, [])
        if (header.length !== 1 || header[0].title !== "report.md")
            return fail("the header names the file: " + (header.length ? header[0].title : "no header"))
        var rows = pageRows(page)
        var got = JSON.stringify(rows.map(function(r) { return [r.text, r.icon] }))
        var want = JSON.stringify([[Actions.text("favourite"), "image://theme/icon-m-favorite"],
                                   [Actions.text("compress"), glyph("archive")],
                                   [Actions.text("moveToNewFolder"), glyph("folder")]])
        if (got !== want)
            return fail("More actions rows: " + got + " != " + want)
        return rows
    }

    // Taps the row `text` of the More actions page.
    function tapRow(text) {
        var page = morePage()
        if (!page)
            return false
        var rows = expectPage(page)
        if (!rows)
            return false
        for (var i = 0; i < rows.length; ++i) {
            if (rows[i].text === text) {
                rows[i].row.clicked(null)
                return true
            }
        }
        return false
    }

    // The dialog in the More actions page's place: on the folder, settled.
    function dialogInPlace(pred) {
        var dialog = pageStack.currentPage
        if (!dialog || pageStack.busy || !pred(dialog))
            return null
        if (pageStack.depth !== 2 || pageStack.previousPage(dialog) !== win.folderPage) {
            fail("the dialog stacks on More actions: depth " + pageStack.depth)
            return null
        }
        return dialog
    }

    function openMore() {
        win.row = win.fileRow()
        if (!win.row || pageStack.busy || pageStack.currentPage !== win.folderPage)
            return false
        win.menu = win.row.openMenu()
        return true
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
            win.waited++
            if (win.waited > 100)
                return win.fail("step " + win.step + " timed out")
            switch (win.step) {
            case 0:
                if (!favs.loaded) {
                    if (win.waited % 10 === 1)
                        favs.reload()
                    break
                }
                win.folderBefore = favs.idOf(win.folder)
                win.folderPage = pageStack.currentPage
                if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), win.file))
                    return win.fail("fixture")
                win.next()
                break
            case 1:
                if (win.openMore())
                    win.next()
                break
            case 2:
                if (!win.menu._expanded)
                    break
                if (win.expectTexts(["share", "copy", "cut", "rename", "delete", "more"], "file menu, More actions last"))
                    win.next()
                break
            case 3:
                var items = win.menuRows()
                // As a tap does: Silica's own activation, closeOnActivation included.
                win.menu._activatedMenuItem(items[items.length - 1])
                win.next()
                break
            case 4:
                var page = win.morePage()
                if (!page)
                    break
                if (pageStack.previousPage(page) !== win.folderPage)
                    return win.fail("More actions opens a page on the folder")
                if (win.row.menuOpen)
                    break
                if (win.expectPage(page))
                    win.next()
                break
            case 5:
                if (win.tapRow(Actions.text("favourite")))
                    win.next()
                break
            case 6:
                var page = win.dialogInPlace(function(d) { return d.uri === win.file && d.isDir !== undefined })
                if (!page)
                    break
                if (page.isDir !== false)
                    return win.fail("the favourite is a file, not a folder")
                // Nothing to accept: the page adds the favourite itself.
                if (!page.filled)
                    break
                pageStack.pop()
                win.next()
                break
            case 7:
                favs.reload()
                win.next()
                break
            case 8:
                var id = favs.idOf(win.file)
                if (id === -1) {
                    if (win.waited % 10 === 0)
                        favs.reload()
                    break
                }
                var saved = JSON.parse(favs.get(id))
                if (saved.isDir !== false)
                    return win.fail("stored as a folder: " + favs.get(id))
                if (favs.idOf(win.folder) !== win.folderBefore)
                    return win.fail("the folder became a favourite")
                favs.removeUri(win.file)
                win.next()
                break
            case 9:
                // Back on the folder once the page is gone.
                if (win.openMore())
                    win.next()
                break
            case 10:
                if (!win.menu._expanded)
                    break
                var rows = win.menuRows()
                win.menu._activatedMenuItem(rows[rows.length - 1])
                win.next()
                break
            case 11:
                if (win.tapRow(Actions.text("compress")))
                    win.next()
                break
            case 12:
                var compress = win.dialogInPlace(function(d) { return d.uris !== undefined && d.formats !== undefined })
                if (!compress)
                    break
                if (JSON.stringify(compress.uris) !== JSON.stringify([win.file]))
                    return win.fail("Compress gets the file: " + JSON.stringify(compress.uris))
                if (!compress.canAccept)
                    return win.fail("the compress dialog can be accepted: into " + compress.destUri)
                compress.reject()
                win.next()
                break
            case 13:
                if (pageStack.busy || pageStack.currentPage !== win.folderPage)
                    break
                if (pageStack.depth !== 1)
                    return win.fail("leaving the dialog goes back to the folder: depth " + pageStack.depth)
                win.next()
                break
            case 14:
                if (favs.idOf(win.file) === -1)
                    win.scenarioRunning = false
                else if (win.waited % 10 === 0)
                    favs.reload()
                break
            }
        }
    }
}
