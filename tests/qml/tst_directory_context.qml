// SPDX-License-Identifier: LGPL-2.1-or-later
// Scenario on the target (real Silica): the long-press menu of a file ends
// with More actions (SPEC §15.3); tapping it keeps the same menu open with
// Add to favourites and Compress in its rows' place, and Add to favourites
// makes the file itself a favourite, as a file. Fails when More actions is
// missing or not last, when the tap closes the menu, or when the favourite
// is the folder. The file's favourite is removed again at the end, so later
// scenarios sharing the database see none of it.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"
import "../../qml/components/directory/ContextActions.js" as Actions

ApplicationWindow {
    id: win

    readonly property string folder: "lautta://user-documents/"
    readonly property string file: folder + "report.md"
    property bool scenarioRunning: true
    property int step: 0
    property int waited: 0
    property var row
    property var menu
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

    function fileRow() {
        // Headless, no render loop polishes the list: lay it out by hand.
        var views = findAll(win.contentItem, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
        for (var i = 0; i < views.length; ++i)
            views[i].forceLayout()
        var rows = findAll(win.contentItem, function(x) {
            return x.openMenu !== undefined && x.info !== undefined && x.info.uri === win.file
        }, [])
        return rows.length > 0 ? rows[0] : null
    }

    function rows() {
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
        var got = texts(rows())
        if (JSON.stringify(got) !== JSON.stringify(want))
            return fail(what + ": " + JSON.stringify(got) + " != " + JSON.stringify(want))
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
                if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), win.file))
                    return win.fail("fixture")
                win.next()
                break
            case 1:
                win.row = win.fileRow()
                if (!win.row)
                    break
                win.menu = win.row.openMenu()
                win.next()
                break
            case 2:
                if (!win.menu._expanded)
                    break
                if (win.expectTexts(["share", "copy", "cut", "rename", "delete", "more"], "file menu, More actions last"))
                    win.next()
                break
            case 3:
                var items = win.rows()
                // As a tap does: Silica's own activation, closeOnActivation included.
                win.menu._activatedMenuItem(items[items.length - 1])
                win.next()
                break
            case 4:
                // Past the menu's close delay and its animation. Headless,
                // nothing polishes the menu's column again, so the menu
                // never counts as expanded once its rows changed.
                if (win.waited < 5)
                    break
                if (!win.row.menuOpen || !win.menu || !win.menu.active)
                    return win.fail("More actions closed the menu")
                if (win.expectTexts(["favourite", "compress"], "More actions in place"))
                    win.next()
                break
            case 5:
                win.menu._activatedMenuItem(win.rows()[0])
                win.next()
                break
            case 6:
                var dialog = pageStack.currentPage
                if (!dialog || dialog.uri !== win.file || dialog.isDir === undefined)
                    break
                // The menu closes after its close delay; the timeout fails
                // a menu that stays.
                if (win.row.menuOpen)
                    break
                if (dialog.isDir !== false)
                    return win.fail("the favourite is a file, not a folder")
                if (!dialog.canAccept)
                    break
                dialog.accept()
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
                if (favs.idOf(win.file) === -1)
                    win.scenarioRunning = false
                else if (win.waited % 10 === 0)
                    favs.reload()
                break
            }
        }
    }
}
