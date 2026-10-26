// SPDX-License-Identifier: LGPL-2.1-or-later
// The app's own file-type glyphs on the target (real Silica, UI-2): list rows
// and grid tiles draw qml/icons/file-<kind>-<set>.svg, the set picked for the
// glyph's pixel size, a grid glyph at a multiple of 28 px; a glyph takes the
// highlight colour with its name, pressed or selected. A video's thumbnail,
// once shown, keeps the video glyph at its bottom left on a dark gradient; a
// plain glyph, a thumbnail that failed and an image's thumbnail have none.
// Fails when a glyph is a theme icon or does not load, a size gets another
// set, a glyph keeps its colour in a highlighted row, or the badge shows
// where it should not or not where it should. How sharp glyphs and badge
// look, and how the badge reads on real videos, only a phone shows.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components"
import "../../qml/components/directory"
import "../../qml/components/FileIcons.js" as FileIcons

ApplicationWindow {
    id: window

    readonly property string base: "lautta://user-documents/GlyphsCheck/"
    // The glyph each item's category shows, as drawn on the board.
    readonly property var kindOf: ({
        "folder": "folder", "markdown": "document", "archive": "archive", "video": "video",
        "package": "package", "database": "database"
    })

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

    // Pumps events until `done()` holds, ten seconds at most.
    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        return expect(done(), "timed out waiting for " + what)
    }

    // A flickable's content item is one of its children too: each item once.
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
        return findAll(item, function (x) { return x.objectName === name })[0]
    }

    // The set whose strokes land on whole pixels at `side` px, as drawn.
    function setAt(side) {
        if (side % 28 === 0)
            return 28
        if (side % 32 === 0)
            return 32
        return side % 80 === 0 ? 80 : 28
    }

    function svg(kind, set) {
        return String(Qt.resolvedUrl("../../qml/icons/file-" + kind + "-" + set + ".svg"))
    }

    // The FileIcon of each item on screen, by name. Laid out by hand:
    // headless, no render loop polishes the view.
    function icons(view) {
        view.flickable.forceLayout()
        var out = {}
        findAll(view.flickable, function (x) { return x.thumbnailReady !== undefined && x.glyphSize !== undefined })
            .forEach(function (icon) {
                for (var row = icon.parent; row; row = row.parent) {
                    if (row.info !== undefined || row.itemName !== undefined) {
                        out[row.info !== undefined ? App.nameOf(row.info.uri) : row.itemName] = icon
                        return
                    }
                }
            })
        return out
    }

    // Every item's glyph is its kind's SVG of `set`, loaded and shown, no badge.
    function checkGlyphs(view, set, what) {
        var shown = icons(view)
        var names = Object.keys(shown)
        same(names.length, 6, what + ": every item has its icon")
        names.forEach(function (name) {
            var icon = shown[name]
            var glyph = named(icon, "fileGlyph")
            same(String(glyph.source), svg(window.kindOf[icon.category], set), what + ": the glyph of " + name)
            expect(glyph.status === Image.Ready && glyph.visible, what + ": " + name + "'s glyph loads and shows")
            expect(!named(icon, "videoBadge").visible, what + ": no badge on the plain glyph of " + name)
        })
        return shown
    }

    // The glyph of item `name` takes the highlight colour while the item is
    // selected or pressed (`press` sets its row's or cell's highlighted).
    function checkHighlight(view, name, press, what) {
        var glyph = named(icons(view)[name], "fileGlyph")
        expect(!glyph.highlighted, what + ": not highlighted at rest")
        view.startSelection()
        view.model.toggle(base + name)
        tools.pump(200)
        glyph = named(icons(view)[name], "fileGlyph")
        expect(glyph.highlighted && String(glyph.highlightColor) === String(Theme.highlightColor),
               what + ": a selected item's glyph is highlighted, as its name")
        view.endSelection()
        tools.pump(200)
        glyph = named(icons(view)[name], "fileGlyph")
        expect(!glyph.highlighted, what + ": not highlighted once deselected")
        press(glyph)
        expect(glyph.highlighted, what + ": a pressed item's glyph is highlighted")
    }

    function checkList(view) {
        var side = Theme.iconSizeMedium
        var shown = checkGlyphs(view, setAt(side), "list")
        expect(named(shown["notes.md"], "fileGlyph").width === side, "a list glyph is a medium icon")
        checkHighlight(view, "notes.md", function (glyph) {
            for (var row = glyph; row; row = row.parent) {
                if (row.info !== undefined) {
                    row.highlighted = true
                    return
                }
            }
        }, "list")
    }

    function checkGrid(view) {
        var shown = checkGlyphs(view, 28, "grid")
        var glyph = named(shown["notes.md"], "fileGlyph")
        var tile = shown["notes.md"].width
        expect(glyph.width % 28 === 0 && Math.abs(glyph.width - tile / 2) <= 14,
               "a grid glyph is about half its tile at a multiple of 28 px: " + glyph.width + " in " + tile)
        checkHighlight(view, "notes.md", function (g) {
            for (var cell = g; cell; cell = cell.parent) {
                if (cell.itemName !== undefined) {
                    cell.highlighted = true
                    return
                }
            }
        }, "grid")
    }

    // FileIcons on their own, given a thumbnail the app draws (the page of a
    // PDF): the system thumbnailer makes none on the target.
    function checkBadge() {
        var thumb = "image://lautta-thumb/" + base + "Sub/page.pdf"
        var list = Theme.iconSizeMedium
        var grid = 168
        var make = function (props) { return iconComponent.createObject(window.contentItem, props) }
        var video = make({ "category": "video", "thumbnailSource": thumb, "size": list })
        var videoGrid = make({ "category": "video", "thumbnailSource": thumb, "size": grid,
                               "glyphSize": FileIcons.snap(grid / 2) })
        var image = make({ "category": "image", "thumbnailSource": thumb, "size": list })
        var plain = make({ "category": "video", "size": list })
        var failed = make({ "category": "video", "thumbnailSource": "image://lautta-thumb/" + base + "none.mp4",
                           "size": list })
        waitFor(function () { return video.thumbnailReady && videoGrid.thumbnailReady && image.thumbnailReady }, "the thumbnails")
        waitFor(function () { return failed.thumbnailFailed }, "the missing file's thumbnail to fail")
        var pairs = [[video, list, "list"], [videoGrid, grid, "grid"]]
        pairs.forEach(function (p) {
            var badge = named(p[0], "videoBadge")
            var mark = named(p[0], "videoBadgeGlyph")
            var side = Math.round(Math.min(Theme.iconSizeSmall, p[1] * 3 / 8))
            expect(badge.visible, p[2] + ": a shown video thumbnail has the badge")
            expect(!named(p[0], "fileGlyph").visible, p[2] + ": over the thumbnail, not the glyph")
            same(String(mark.source), svg("video", setAt(side)), p[2] + ": the badge is the video glyph")
            expect(mark.status === Image.Ready && mark.width === side, p[2] + ": loaded, " + side + " px: " + mark.width)
            var corner = mark.mapToItem(p[0], 0, mark.height)
            expect(corner.x === Theme.paddingSmall && Math.abs(p[1] - corner.y - Theme.paddingSmall) < 1,
                   p[2] + ": at the bottom left: " + corner.x + "," + corner.y)
            var shade = findAll(badge, function (x) { return x.gradient !== undefined && x.gradient !== null })[0]
            expect(shade && shade.height === p[1] / 2 && shade.y === p[1] / 2 && shade.opacity === Theme.opacityOverlay,
                   p[2] + ": on a gradient over the thumbnail's lower half")
        })
        expect(!named(image, "videoBadge").visible, "an image's thumbnail has no badge")
        expect(!named(plain, "videoBadge").visible && named(plain, "fileGlyph").visible,
               "a video without thumbnail shows its glyph, no badge")
        expect(!named(failed, "videoBadge").visible && named(failed, "fileGlyph").visible,
               "a video whose thumbnail failed shows its glyph, no badge")
        video.highlighted = true
        expect(named(video, "videoBadgeGlyph").highlighted, "the badge is highlighted with its item")
        var made = [video, videoGrid, image, plain, failed]
        made.forEach(function (icon) { icon.destroy() })
    }

    function makeFolder() {
        documents.createFolder("GlyphsCheck")
        waitFor(function () { return documents.indexOf("lautta://user-documents/GlyphsCheck") >= 0 }, "the folder")
        folderModel.uri = base
        folderModel.createFolder("Sub")
        waitFor(function () { return folderModel.indexOf(base + "Sub") >= 0 }, "the folder inside")
        var files = ["notes.md", "backup.tar.gz", "clip.mp4", "app.rpm", "data.sqlite"]
        files.forEach(function (name) {
            expect(tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), base + name), "fixture " + name)
        })
        expect(tools.installFixture(Qt.resolvedUrl("host/fixtures/two-pages.pdf"), base + "Sub/page.pdf"),
               "fixture page.pdf")
        folderModel.refresh()
        waitFor(function () { return folderModel.count === 6 }, "the items")
    }

    DirectoryModel { id: folderModel }

    Component.onCompleted: {
        var settings = { "view_mode": "list", "show_hidden": false, "thumbnails_local": false }
        for (var key in settings)
            App.setSetting(key, JSON.stringify(settings[key]))
        makeFolder()
        checkBadge()
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        var view = pageStack.currentPage.view
        if (waitFor(function () { return !view.model.loading && Object.keys(icons(view)).length === 6 }, "the rows")) {
            checkList(view)
            App.setSetting("view_mode", JSON.stringify("grid"))
            if (waitFor(function () { return view.gridMode && Object.keys(icons(view)).length === 6 }, "the tiles"))
                checkGrid(view)
        }
        App.setSetting("view_mode", JSON.stringify("list"))
        App.setSetting("thumbnails_local", JSON.stringify(true))
    }

    Component {
        id: iconComponent

        FileIcon { }
    }

    Component {
        id: pageComponent

        Page {
            property alias view: folder

            DirectoryView {
                id: folder

                anchors.fill: parent
                uri: window.base
            }
        }
    }
}
