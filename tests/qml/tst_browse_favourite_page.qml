// SPDX-License-Identifier: LGPL-2.1-or-later
// Scenario on the target (real Silica): the favourite page (ORG-1) is a
// page, not a dialog, without a pulley or an order section. Opened from a
// favourite's Edit favourite in Browse, each change is stored without
// anything to accept: an icon and a colour as soon as they are picked, the
// label once typing pauses and when the page is left while typing. Back in
// Browse the row shows the chosen icon in the chosen colour. Opened on a
// file that is no favourite yet (Add to favourites), the page makes it one
// at once, as a file, offering the file's own icon first as its default.
// Both favourites are removed again at the end, so later scenarios sharing
// the database see none of them.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

ApplicationWindow {
    id: win

    readonly property string folder: "lautta://user-documents/Holiday"
    readonly property string file: "lautta://user-documents/notes.md"
    property bool scenarioRunning: true
    property int step: 0
    property int waited: 0
    property var page

    FavouritesModel { id: favs }

    initialPage: Component {
        Loader {
            source: Qt.resolvedUrl("../../qml/pages/BrowsePage.qml")
        }
    }

    function fail(what) {
        console.error("FAILED: " + what)
        favs.removeUri(file)
        favs.removeUri(folder)
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

    function typeOf(item, name) {
        return String(item).indexOf(name) === 0
    }

    function favouriteRow(name) {
        // Headless, no render loop polishes the list: lay it out by hand.
        var views = findAll(win.contentItem, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
        for (var i = 0; i < views.length; ++i)
            views[i].forceLayout()
        var rows = findAll(win.contentItem, function(x) {
            return x.kind === "favourite" && x.openMenu !== undefined && x.subtitle !== undefined
        }, [])
        for (var j = 0; j < rows.length; ++j) {
            if (findAll(rows[j], function(x) { return x.text === name && x.truncationMode !== undefined }, []).length > 0)
                return rows[j]
        }
        return null
    }

    // "source colour" of a Browse row's icon, its first image.
    function rowIcon(row) {
        var icons = findAll(row, function(x) { return x.highlightColor !== undefined && x.source !== undefined }, [])
        return icons.length > 0 ? String(icons[0].source) + " " + String(icons[0].color) : "none"
    }

    // The stored favourite of `uri` as last loaded, {} when none; loads
    // again now and then, as the page's saves do not reach this model.
    function stored(uri) {
        if (win.waited % 3 === 0)
            favs.reload()
        var id = favs.idOf(uri)
        return id === -1 ? {} : JSON.parse(favs.get(id))
    }

    function favouritePage(uri) {
        var p = pageStack.currentPage
        return p && p.uri === uri && typeOf(p, "FavouritePage") && p.filled && !pageStack.busy ? p : null
    }

    function iconCells() {
        return findAll(page, function(x) { return x.chosen !== undefined && x.name !== undefined && x.pressed !== undefined }, [])
    }

    function iconCell(name) {
        var cells = iconCells()
        for (var i = 0; i < cells.length; ++i) {
            if (cells[i].name === name)
                return cells[i]
        }
        return null
    }

    function colourCell(colour) {
        var pickers = findAll(page, function(x) { return x.colours !== undefined && x.allowNone !== undefined }, [])
        var cells = pickers.length > 0 ? findAll(pickers[0], function(x) { return x.pressed !== undefined }, []) : []
        for (var i = 0; i < cells.length; ++i) {
            if (String(cells[i].children[0].color) === colour)
                return cells[i]
        }
        return null
    }

    function labelField() {
        var fields = findAll(page, function(x) { return x.placeholderText !== undefined && x.echoMode !== undefined }, [])
        return fields.length > 0 ? fields[0] : null
    }

    // A page that saves as it goes: no dialog to accept, no pulley, and
    // no order section: its texts are the header, the label field, the
    // folder line and the colour and icon sections, nothing else.
    function checkShape() {
        if (page.accept !== undefined || page.canAccept !== undefined)
            return fail("the favourite is a dialog: " + page)
        var menus = findAll(page, function(x) { return typeOf(x, "PullDownMenu") || typeOf(x, "PushUpMenu") }, [])
        if (menus.length > 0)
            return fail("the page has a pulley: " + menus[0])
        var texts = []
        findAll(page, function(x) { return x.text !== undefined && x.visible }, []).forEach(function(x) {
            var text = String(x.text)
            if (text.indexOf("lautta-favourite-") === 0 && texts.indexOf(text) < 0)
                texts.push(text.replace("lautta-favourite-", ""))
        })
        texts.sort()
        if (texts.join(",") !== "colour,edit-title,folder,icon,label")
            return fail("the page shows " + texts.join(","))
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
                favs.save(win.folder, "Holiday", "", "", true)
                win.next()
                break
            case 1:
                var row = win.favouriteRow("Holiday")
                if (!row || pageStack.busy)
                    break
                if (win.rowIcon(row).indexOf("image://theme/icon-m-favorite ") !== 0)
                    return win.fail("a favourite's default icon is the star: " + win.rowIcon(row))
                var menu = row.openMenu()
                var edit = win.findAll(menu, function(x) {
                    return x.text === "lautta-browse-edit-favourite" || x.text === "Edit favourite"
                }, [])
                if (edit.length === 0)
                    break
                edit[0].clicked(null)
                win.next()
                break
            case 2:
                win.page = win.favouritePage(win.folder)
                if (!win.page)
                    break
                if (win.page.added)
                    return win.fail("an existing favourite counts as added")
                if (win.checkShape())
                    win.next()
                break
            case 3:
                var star = win.iconCell("favorite")
                var cells = win.iconCells()
                if (!star || cells.length !== favs.iconChoices().length || cells[0] !== star || !star.chosen)
                    return win.fail("a folder offers the icons, the star first and chosen: " + cells.length)
                win.iconCell("home").clicked(null)
                win.next()
                break
            case 4:
                // Saved by the pick itself, the page still open.
                if (win.stored(win.folder).icon !== "home")
                    break
                if (!win.iconCell("home").chosen || win.iconCell("favorite").chosen)
                    return win.fail("the picked icon is the chosen one")
                win.colourCell("#4fa3e5").clicked(null)
                win.next()
                break
            case 5:
                var f = win.stored(win.folder)
                if (f.colour !== "#4fa3e5")
                    break
                if (f.icon !== "home" || f.label !== "Holiday")
                    return win.fail("the colour pick kept icon and label: " + favs.get(favs.idOf(win.folder)))
                win.labelField().text = "Trips"
                win.next()
                break
            case 6:
                // Once typing pauses.
                if (win.stored(win.folder).label !== "Trips")
                    break
                win.labelField().text = "Trips 2026"
                pageStack.pop(null, PageStackAction.Immediate)
                win.next()
                break
            case 7:
                // Left while typing: saved on the way out.
                if (win.stored(win.folder).label !== "Trips 2026")
                    break
                var shown = win.favouriteRow("Trips 2026")
                if (!shown)
                    break
                if (win.rowIcon(shown) !== "image://theme/icon-m-home #4fa3e5")
                    return win.fail("the row shows the chosen icon in the chosen colour: " + win.rowIcon(shown))
                pageStack.push(Qt.resolvedUrl("../../qml/pages/FavouritePage.qml"),
                               { "uri": win.file, "isDir": false }, PageStackAction.Immediate)
                win.next()
                break
            case 8:
                win.page = win.favouritePage(win.file)
                if (!win.page)
                    break
                var added = win.stored(win.file)
                if (added.uri !== win.file)
                    break
                if (!win.page.added || added.isDir !== false || added.label !== "notes.md" || added.icon !== "")
                    return win.fail("opening the page adds the file as it is: " + favs.get(favs.idOf(win.file)))
                var first = win.iconCells()[0]
                if (!first || first.name !== "" || !first.chosen || win.iconCells().length !== favs.iconChoices().length + 1)
                    return win.fail("a file offers its own icon first, chosen")
                var image = win.findAll(first, function(x) { return x.source !== undefined }, [])
                if (image.length === 0 || String(image[0].source) !== "image://theme/icon-m-document")
                    return win.fail("the file's own icon: " + (image.length > 0 ? image[0].source : "none"))
                win.iconCell("favorite").clicked(null)
                win.next()
                break
            case 9:
                if (win.stored(win.file).icon !== "favorite")
                    break
                pageStack.pop(null, PageStackAction.Immediate)
                win.next()
                break
            case 10:
                var fileRow = win.favouriteRow("notes.md")
                if (!fileRow || win.rowIcon(fileRow).indexOf("image://theme/icon-m-favorite ") !== 0)
                    break
                favs.removeUri(win.file)
                favs.removeUri(win.folder)
                win.next()
                break
            case 11:
                if (favs.idOf(win.file) === -1 && favs.idOf(win.folder) === -1)
                    win.scenarioRunning = false
                else if (win.waited % 10 === 0)
                    favs.reload()
                break
            }
        }
    }
}
