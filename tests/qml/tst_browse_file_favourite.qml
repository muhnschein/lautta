// SPDX-License-Identifier: LGPL-2.1-or-later
// Scenario on the target (real Silica): a file favourite (ORG-1) shows in
// Browse with its file icon and the folder it is in, and tapping it opens
// the file (here the Markdown viewer), not a folder page. Folder
// favourites still open their folder. Both favourites are removed again at
// the end, so later scenarios sharing the database see none of them.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components/FileIcons.js" as FileIcons

ApplicationWindow {
    id: win

    readonly property string folder: "lautta://user-documents/Uni"
    readonly property string file: folder + "/notes.md"
    property bool scenarioRunning: true
    property int step: 0
    property int waited: 0

    ViewerTools { id: tools }

    FavouritesModel { id: favs }

    DirectoryModel {
        id: docs
        uri: "lautta://user-documents/"
    }

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

    function favouriteRow(name) {
        // Headless, no render loop polishes the list: lay it out by hand.
        var views = findAll(win.contentItem, function(x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
        for (var i = 0; i < views.length; ++i)
            views[i].forceLayout()
        var rows = findAll(win.contentItem, function(x) {
            return x.kind === "favourite" && x.openMenu !== undefined && x.subtitle !== undefined
        }, [])
        for (var j = 0; j < rows.length; ++j) {
            var labels = findAll(rows[j], function(x) { return x.text === name && x.truncationMode !== undefined }, [])
            if (labels.length > 0)
                return rows[j]
        }
        return null
    }

    // The app's document glyph at a row icon's size (FileIcons.js).
    function documentGlyph() {
        return String(Qt.resolvedUrl("../../qml/icons/" + FileIcons.glyph("markdown", false, Theme.iconSizeMedium) + ".svg"))
    }

    // The row's icon is its first image (the page is not shown, headless).
    function icons(row) {
        return findAll(row, function(x) { return x.highlightColor !== undefined && x.source !== undefined }, [])
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
                docs.createFolder("Uni")
                win.next()
                break
            case 1:
                if (docs.nameKind("Uni") !== "folder") {
                    if (win.waited % 5 === 0)
                        docs.refresh()
                    break
                }
                if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), win.file))
                    return win.fail("fixture")
                favs.save(win.file, "Lecture notes", "", "", false)
                favs.save(win.folder, "Uni", "", "", true)
                win.next()
                break
            case 2:
                var row = win.favouriteRow("Lecture notes")
                if (!row || !win.favouriteRow("Uni"))
                    break
                if (row.subtitle !== "Documents › Uni")
                    return win.fail("the folder the file is in: " + row.subtitle)
                var icon = win.icons(row)
                if (icon.length === 0 || String(icon[0].source) !== win.documentGlyph())
                    return win.fail("the file's icon: " + (icon.length > 0 ? icon[0].source : "none"))
                row.clicked(null)
                win.next()
                break
            case 3:
                var page = pageStack.currentPage
                if (!page || page.uri !== win.file)
                    break
                if (String(page).indexOf("MarkdownViewer") !== 0)
                    return win.fail("a file favourite opens the file: " + page)
                pageStack.pop(null, PageStackAction.Immediate)
                win.next()
                break
            case 4:
                var folderRow = win.favouriteRow("Uni")
                if (!folderRow || pageStack.depth !== 1)
                    break
                folderRow.clicked(null)
                win.next()
                break
            case 5:
                var folderPage = pageStack.currentPage
                if (!folderPage || folderPage.uri !== win.folder)
                    break
                if (String(folderPage).indexOf("DirectoryPage") !== 0)
                    return win.fail("a folder favourite opens the folder: " + folderPage)
                favs.removeUri(win.file)
                favs.removeUri(win.folder)
                win.next()
                break
            case 6:
                if (favs.idOf(win.file) === -1 && favs.idOf(win.folder) === -1)
                    win.scenarioRunning = false
                else if (win.waited % 10 === 0)
                    favs.reload()
                break
            }
        }
    }
}
