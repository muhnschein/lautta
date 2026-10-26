// SPDX-License-Identifier: LGPL-2.1-or-later
// The grid on the target (SPEC §15.3, BRW-4, board DirectoryGrid): under
// every tile the name on up to two lines, a long one
// cut in the middle so that its end stays, and caption lines: a file's
// modified time and size, a folder's item count on this device (hidden files
// only when they are shown), a folder's modified time where counting would
// list it over the network (an archive stands in for a server here, both
// are not local); the glyph is about half the tile, at a multiple of 28 px
// (FileIcons.js). The View options
// preview draws the same cells at the same size. Fails when a caption line,
// the count, the cut name or the glyph size is missing or wrong, when cells
// differ in height or when the preview lays a cell out differently.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components/directory"
import "../../qml/components/directory/DirFormat.js" as DirFormat

ApplicationWindow {
    id: window

    readonly property string base: "lautta://user-downloads/GridCaptions/"
    readonly property string longName: "Minutes of the department meeting 2026-10-14 final.pdf"
    property string archiveRoot

    ViewerTools { id: tools }
    GridMetrics { id: metrics }
    FontMetrics {
        id: letter

        font {
            family: Theme.fontFamily
            pixelSize: Theme.fontSizeExtraSmall
        }
    }

    DirectoryModel { id: downloads; uri: "lautta://user-downloads/" }
    DirectoryModel { id: top }
    DirectoryModel { id: inner }

    Connections {
        target: Operations
        onJobFinished: if (ok) Operations.openArchive(resultUri)
        onArchiveOpened: window.archiveRoot = rootUri
    }

    function expect(ok, what) {
        if (!ok)
            console.error(what)
    }

    function same(a, b, what) {
        expect(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    // Pumps events until `done()` holds, ten seconds at most.
    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        expect(done(), "timed out waiting for " + what)
    }

    function install(name, folder) {
        expect(tools.installFixture(Qt.resolvedUrl("host/fixtures/notes-crlf.txt"), (folder || base) + name),
               "fixture " + name)
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
        return null
    }

    // The cells of the grid on screen, which are those that hold captions.
    // Laid out by hand: headless, no render loop polishes the view.
    function cells(view) {
        var out = []
        view.flickable.forceLayout()
        var kids = view.flickable.contentItem.children
        for (var i = 0; i < kids.length; ++i) {
            if (kids[i].captions !== undefined && kids[i].itemName !== undefined)
                out.push(kids[i])
        }
        return out
    }

    function cellNamed(view, name) {
        var all = cells(view)
        for (var i = 0; i < all.length; ++i) {
            if (all[i].itemName === name)
                return all[i]
        }
        expect(false, "no cell called " + name)
        return null
    }

    function textOf(cell, name) {
        var label = find(cell, name)
        return label ? label.text : "no " + name
    }

    function push(uri) {
        pageStack.push(pageComponent, { "folderUri": uri }, PageStackAction.Immediate)
        var view = pageStack.currentPage.view
        waitFor(function () { return !view.model.loading && view.model.count > 0 }, "the folder " + uri)
        tools.pump(100)
        return view
    }

    function makeFolders() {
        downloads.createFolder("GridCaptions")
        waitFor(function () { return downloads.indexOf("lautta://user-downloads/GridCaptions") >= 0 }, "the folder")
        top.uri = base
        top.createFolder("Inner")
        top.createFolder("Empty")
        waitFor(function () { return top.count === 2 }, "its folders")
        inner.uri = base + "Inner/"
        inner.createFolder("deeper")
        waitFor(function () { return inner.count === 1 }, "the folder inside")
        install("a.txt", base + "Inner/")
        install("b.txt", base + "Inner/")
        install(".hidden", base + "Inner/")
        install("notes.txt")
        install(longName)
        top.refresh()
        waitFor(function () { return top.count === 4 }, "the files")
    }

    function checkFileCell(view) {
        var notes = cellNamed(view, "notes.txt")
        same(notes.captions.length, 2, "a file has two caption lines")
        same(textOf(notes, "gridName"), metrics.nameLines("notes.txt", notes.width - 2 * notes.gap).join("\n"),
             "the name is cut as GridName.js cuts it")
        same(textOf(notes, "gridCaption0"), notes.captions[0], "the first line is the modified time")
        expect(notes.captions[0].length > 0, "and says something")
        same(textOf(notes, "gridCaption1"), Format.formatFileSize(20), "the second line is the size")
        var name = find(notes, "gridName")
        expect(name.font.pixelSize === Theme.fontSizeExtraSmall && name.horizontalAlignment === Text.AlignHCenter
               && String(name.color) === String(Theme.primaryColor), "the name is small, centred, in the text colour")
        for (var i = 0; i < 2; ++i) {
            var line = find(notes, "gridCaption" + i)
            expect(line.font.pixelSize === Theme.fontSizeTiny && line.horizontalAlignment === Text.AlignHCenter
                   && String(line.color) === String(Theme.secondaryColor) && line.truncationMode === TruncationMode.Elide,
                   "caption line " + i + " is tiny, centred, secondary and cut at the end")
        }
        expect(!notes.counting, "a file is not counted")
    }

    function checkFolderCells(view) {
        var folder = cellNamed(view, "Inner")
        expect(folder.counting, "a folder here is counted")
        waitFor(function () { return folder.items === 3 }, "the count of Inner")
        same(folder.captions, [qsTrId("lautta-dir-grid-items", 3)], "a folder here has its item count, no hidden file")
        same(textOf(folder, "gridCaption0"), folder.captions[0], "shown on the first line")
        same(textOf(folder, "gridCaption1"), "", "and nothing on the second")
        var empty = cellNamed(view, "Empty")
        waitFor(function () { return empty.items === 0 }, "the count of Empty")
        same(empty.captions, [qsTrId("lautta-dir-grid-items", 0)], "an empty folder has a count too")
    }

    // Hidden files count when the view shows them.
    function checkHiddenCount(view) {
        var folder = cellNamed(view, "Inner")
        App.setSetting("show_hidden", JSON.stringify(true))
        waitFor(function () { return folder.items === 4 }, "the count with the hidden file")
        App.setSetting("show_hidden", JSON.stringify(false))
        waitFor(function () { return folder.items === 3 }, "the count without it")
    }

    // A folder of more than the counted entries says "More than".
    function checkHugeFolder() {
        var huge = "lautta://user-downloads/GridCaptionsHuge/"
        downloads.createFolder("GridCaptionsHuge")
        waitFor(function () { return downloads.indexOf("lautta://user-downloads/GridCaptionsHuge") >= 0 }, "the huge folder")
        top.uri = huge
        top.createFolder("Many")
        waitFor(function () { return top.count === 1 }, "the folder of many")
        for (var i = 0; i <= 1000; ++i)
            install(String(i), huge + "Many/")
        var view = push(huge)
        var many = cellNamed(view, "Many")
        waitFor(function () { return many.items === 1000 }, "the capped count")
        same(many.captions, [qsTrId("lautta-dir-grid-items-more", 1000)], "a huge folder has more than its count")
        same(textOf(many, "gridCaption0"), many.captions[0], "shown on the first line")
        pageStack.pop(null, PageStackAction.Immediate)
    }

    // The icon of a cell: the item that knows whether its thumbnail failed.
    function iconIn(item) {
        if (!item)
            return null
        if (item.thumbnailFailed !== undefined)
            return item
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i) {
            var found = iconIn(kids[i])
            if (found)
                return found
        }
        return null
    }

    // Under a tile the name and the lines stay whether a PDF's
    // first page covers the tile or the page could not be made (encrypted).
    // The PDFs are in an archive, where the app draws them: the system
    // thumbnailer, which draws local ones, draws nothing here (no scene graph).
    function checkPdfThumbnails() {
        var pdfs = "lautta://user-downloads/GridCaptionsPdf/"
        downloads.createFolder("GridCaptionsPdf")
        waitFor(function () { return downloads.indexOf("lautta://user-downloads/GridCaptionsPdf") >= 0 }, "the PDF folder")
        expect(tools.installFixture(Qt.resolvedUrl("host/fixtures/two-pages.pdf"), pdfs + "readable.pdf"), "readable PDF")
        expect(tools.installFixture(Qt.resolvedUrl("host/fixtures/secret.pdf"), pdfs + "secret.pdf"), "encrypted PDF")
        App.setSetting("thumbnails_local", JSON.stringify(true))
        window.archiveRoot = ""
        Operations.compress(JSON.stringify([pdfs + "readable.pdf", pdfs + "secret.pdf"]), "lautta://user-downloads/",
                            "grid-captions-pdf", "zip")
        waitFor(function () { return window.archiveRoot.length > 0 }, "the archive of PDFs")
        var view = push(window.archiveRoot)
        window.archiveRoot = ""
        waitFor(function () { return cells(view).length === 2 }, "both PDFs")
        var good = cellNamed(view, "readable.pdf")
        var locked = cellNamed(view, "secret.pdf")
        waitFor(function () { return iconIn(good).thumbnailReady }, "the first page of the readable PDF")
        waitFor(function () { return iconIn(locked).thumbnailFailed }, "the encrypted PDF to fail")
        var both = [good, locked]
        for (var i = 0; i < both.length; ++i) {
            var cell = both[i]
            var label = find(cell, "gridName")
            expect(label.visible && label.text === metrics.nameLines(cell.itemName, cell.width - 2 * cell.gap).join("\n"),
                   "the name of " + cell.itemName + " is under its tile: " + label.text)
            same(cell.captions.length, 2, "and two lines under " + cell.itemName)
            same(textOf(cell, "gridCaption1"), cell.captions[1], "shown")
        }
        pageStack.pop(null, PageStackAction.Immediate)
        App.setSetting("thumbnails_local", JSON.stringify(false))
    }

    // A file added to the folder changes its modified time: the cell counts again.
    function checkChangedFolder(view) {
        var folder = cellNamed(view, "Inner")
        install("c.txt", base + "Inner/")
        view.model.refresh()
        waitFor(function () { return folder.items === 4 }, "the count after a file was added")
        same(folder.captions, [qsTrId("lautta-dir-grid-items", 4)], "and says so")
    }

    // The screen of this check is 240 pixels wide and its font a box per
    // letter, too small for a name to show: the cut is checked in a caption
    // as wide as sixteen of the widest letter.
    function checkNames() {
        var caption = captionComponent.createObject(window.contentItem,
                                                    { "width": 16 * letter.advanceWidth("m"), "metrics": metrics })
        caption.name = "notes.txt"
        same(find(caption, "gridName").text, "notes.txt", "a short name is whole")
        caption.name = longName
        var label = find(caption, "gridName")
        var lines = label.text.split("\n")
        same(lines.length, 2, "a long name takes two lines")
        expect(lines[0].indexOf("Minutes") === 0, "it starts on the first: " + lines[0])
        expect(lines[1].indexOf("\u2026") > 0 && lines[1].lastIndexOf(".pdf") === lines[1].length - 4,
               "and ends on the second behind an ellipsis: " + lines[1])
        expect(!label.truncated && label.contentWidth <= label.width,
               "the lines fit the cell: " + label.contentWidth + " of " + label.width)
        expect(label.height >= label.contentHeight, "and the room: " + label.contentHeight + " of " + label.height)
        caption.name = "bold"
        var word = label.contentWidth
        caption.name = "<b>bold</b>"
        expect(label.contentWidth > 2 * word, "a name is text, its tags shown: " + label.contentWidth + " for " + word)
        caption.destroy()
    }

    function checkCells(view) {
        var grid = view.flickable
        var all = cells(view)
        expect(all.length === 4, "four cells: " + all.length)
        for (var i = 0; i < all.length; ++i)
            expect(all[i].height === grid.cellHeight, "every cell is as tall as the view says")
        var tile = grid.cellWidth - 2 * all[0].gap
        var glyph = find(all[0], "fileGlyph")
        expect(glyph.width % 28 === 0 && Math.abs(glyph.width - tile / 2) <= 14,
               "the glyph is about half the tile, at a multiple of 28 px: " + glyph.width + " in " + tile)
        expect(grid.cellHeight > grid.cellWidth, "cells are taller than wide: " + grid.cellHeight + " " + grid.cellWidth)
        // The last caption line ends close to the cell's end, which is the
        // height of the tile, two name lines and two caption lines with the
        // space around them: no line is cut, no room wasted.
        var last = find(all[0], "gridCaption1")
        var room = all[0].height - last.mapToItem(all[0], 0, last.height).y
        expect(Math.abs(room - (all[0].gap + Theme.paddingSmall / 2)) <= 1,
               "the cell ends a gap and half a padding below the last line: " + room)
    }

    // A folder that is not on this device shows its modified time, not a count.
    function checkArchive() {
        Operations.compress(JSON.stringify([base + "Inner"]), "lautta://user-downloads/", "grid-captions", "zip")
        waitFor(function () { return window.archiveRoot.length > 0 }, "the archive")
        var view = push(window.archiveRoot)
        expect(!view.model.folderCounts, "an archive has no item counts")
        var folder = cellNamed(view, "Inner")
        same(folder.items, -1, "its folder has no count")
        same(folder.captions.length, 1, "but a line")
        expect(folder.captions[0].length > 0 && folder.captions[0] !== qsTrId("lautta-dir-grid-items", 0),
               "which is its modified time: " + folder.captions[0])
        same(textOf(folder, "gridCaption0"), folder.captions[0], "shown under the name")
        pageStack.pop(null, PageStackAction.Immediate)
    }

    function checkCaptionText() {
        var now = new Date()
        var yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1, 21, 40).getTime()
        var older = new Date(now.getFullYear() - 1, 9, 12, 10, 2).getTime()
        var time = function (ms) { return Format.formatDate(new Date(ms), Formatter.TimeValue) }
        same(DirFormat.modifiedWithTime(older),
             qsTrId("lautta-dir-date-time").arg(DirFormat.modified(older)).arg(time(older)), "a date and its time")
        same(DirFormat.modifiedWithTime(yesterday),
             qsTrId("lautta-dir-date-time").arg(DirFormat.modified(yesterday)).arg(time(yesterday)),
             "yesterday and its time")
        same(DirFormat.modifiedWithTime(now.getTime()), DirFormat.modified(now.getTime()), "today as the list says it")
        same(DirFormat.modifiedWithTime(-1), "", "no time known")
        same(DirFormat.items(-1, false), "", "no count")
        same(DirFormat.items(1, false), qsTrId("lautta-dir-grid-items", 1), "one item goes through the plural string")
        expect(DirFormat.items(14, false) === qsTrId("lautta-dir-grid-items", 14)
               && DirFormat.items(1000, true) === qsTrId("lautta-dir-grid-items-more", 1000)
               && DirFormat.items(14, false) !== DirFormat.items(14, true), "a capped count says more than")
        same(DirFormat.gridCaptions(false, 2400000, older, true, -1, false),
             [DirFormat.modifiedWithTime(older), Format.formatFileSize(2400000)], "a file: time and size")
        same(DirFormat.gridCaptions(false, -1, -1, true, -1, false), [], "a file of unknown time and size")
        same(DirFormat.gridCaptions(true, -1, older, true, 14, false), [DirFormat.items(14, false)],
             "a counted folder: its items")
        same(DirFormat.gridCaptions(true, -1, older, true, -1, false), [], "a folder not counted yet, or unreadable")
        same(DirFormat.gridCaptions(true, -1, older, false, -1, false), [DirFormat.modifiedWithTime(older)],
             "a folder that is not counted: its time")
        same(DirFormat.gridCaptions(true, -1, -1, false, -1, false), [], "and none if that is unknown")
    }

    // The preview of View options has the cell size and the lines of the folder.
    function checkPreview(view) {
        var preview = previewComponent.createObject(window.contentItem,
                                                    { "screenWidth": view.width, "screenHeight": view.height })
        tools.pump(300)
        expect(preview.gridMode, "the preview is a grid")
        same(preview.cellWidth, view.flickable.cellWidth, "its cell is as wide")
        same(preview.cellHeight, view.flickable.cellHeight, "and as tall")
        var rows = preview.shown()
        expect(rows.length === 6, "the sample folder has six items: " + rows.length)
        for (var i = 0; i < rows.length; ++i) {
            same(rows[i].captions.length, rows[i].isDir ? 1 : 2,
                 "lines of " + rows[i].name + ": " + JSON.stringify(rows[i].captions))
        }
        expect(rows[0].isDir && rows[0].captions[0] === qsTrId("lautta-dir-grid-items", 24)
               && preview.itemsOf(rows[0].name) === 24,
               "a sample folder has its items: " + JSON.stringify(rows[0].captions))
        same(find(preview, "gridCaption0").text, rows[0].captions[0], "shown under the first tile")
        var glyph = find(view, "fileGlyph")
        expect(Math.abs(find(preview, "fileGlyph").width - glyph.width) <= 1,
               "its glyph is as large as the view's: " + find(preview, "fileGlyph").width + " and " + glyph.width)
        preview.destroy()
    }

    Component.onCompleted: {
        var settings = { "view_mode": "grid", "show_hidden": false, "thumbnails_local": false }
        for (var key in settings)
            App.setSetting(key, JSON.stringify(settings[key]))
        makeFolders()
        var view = push(base)
        waitFor(function () { return cells(view).length === 4 }, "the cells")
        checkFileCell(view)
        checkFolderCells(view)
        checkHiddenCount(view)
        checkChangedFolder(view)
        checkHugeFolder()
        checkPdfThumbnails()
        checkNames()
        checkCells(view)
        checkPreview(view)
        checkArchive()
        checkCaptionText()
        var restore = { "view_mode": "list", "thumbnails_local": true }
        for (var again in restore)
            App.setSetting(again, JSON.stringify(restore[again]))
    }

    Component {
        id: pageComponent

        Page {
            id: page

            property string folderUri
            property alias view: folder

            DirectoryView {
                id: folder

                anchors.fill: parent
                uri: page.folderUri
            }
        }
    }

    Component {
        id: captionComponent

        GridCaption { }
    }

    Component {
        id: previewComponent

        ViewPreview { }
    }
}
