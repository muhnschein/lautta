// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components"
import "../../qml/components/directory"
import "../../qml/components/FileIcons.js" as FileIcons

ApplicationWindow {
    id: window

    readonly property string base: "lautta://user-documents/GlyphsCheck/"
    property string archiveRoot
    property string thumb
    readonly property var kindOf: ({
        "folder": "folder", "markdown": "document", "archive": "archive", "video": "video",
        "package": "package", "database": "database"
    })

    ViewerTools { id: tools }

    DirectoryModel {
        id: documents
        uri: "lautta://user-documents/"
    }

    Connections {
        target: Operations
        onJobFinished: if (ok) Operations.openArchive(resultUri)
        onArchiveOpened: window.archiveRoot = rootUri
    }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    function same(a, b, what) {
        return expect(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        return expect(done(), "timed out waiting for " + what)
    }

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

    // No render loop headless: lay out by hand.
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
        checkOver(shown["clip.mp4"], "list view")
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
        checkOver(shown["clip.mp4"], "grid view")
        checkHighlight(view, "notes.md", function (g) {
            for (var cell = g; cell; cell = cell.parent) {
                if (cell.itemName !== undefined) {
                    cell.highlighted = true
                    return
                }
            }
        }, "grid")
    }

    function glyphBox(icon) {
        var glyph = named(icon, "fileGlyph")
        var centre = glyph.mapToItem(icon, glyph.width / 2, glyph.height / 2)
        return [glyph.width, glyph.height, centre.x, centre.y]
    }

    function overThumbnail(plain) {
        var props = { "category": "video", "thumbnailSource": window.thumb, "size": plain.width,
                      "glyphSize": plain.glyphSize }
        var icon = iconComponent.createObject(window.contentItem, props)
        waitFor(function () { return icon.thumbnailReady }, "the video's thumbnail")
        return icon
    }

    function checkOver(plain, what) {
        var icon = overThumbnail(plain)
        var glyph = named(icon, "fileGlyph")
        var disc = named(icon, "videoBadge")
        same(glyphBox(icon), glyphBox(plain), what + ": the glyph over the thumbnail is the plain glyph's size and centre")
        same(glyphBox(plain).slice(2), [plain.width / 2, plain.height / 2], what + ": the plain glyph is centred in its icon")
        same(String(glyph.source), String(named(plain, "fileGlyph").source), what + ": the same glyph file")
        expect(glyph.visible && glyph.status === Image.Ready, what + ": the glyph shows over the thumbnail")
        expect(disc.visible && disc.width === plain.glyphSize - 2 * Math.floor(plain.glyphSize / 10) && disc.width > plain.glyphSize * 0.79,
               what + ": a disc round the glyph's ring: " + disc.width + " for " + plain.glyphSize)
        var middle = disc.mapToItem(icon, disc.width / 2, disc.height / 2)
        expect(middle.x === glyphBox(plain)[2] && middle.y === glyphBox(plain)[3], what + ": the disc is centred on the glyph")
        expect(Math.abs(disc.color.a - Theme.opacityHigh) < 0.01 && Math.max(disc.color.r, disc.color.g, disc.color.b) < 0.35,
               what + ": the disc is dark and translucent: " + disc.color)
        same(String(glyph.color), String(Theme.lightPrimaryColor), what + ": the glyph is light over the disc")
        expect(glyph.opacity === Theme.opacityOverlay, what + ": the glyph is a little translucent: " + glyph.opacity)
        same(String(glyph.highlightColor), String(Theme.highlightFromColor(Theme.highlightColor, Theme.LightOnDark)),
             what + ": pressed, the highlight colour for a dark ground")
        icon.highlighted = true
        expect(glyph.highlighted, what + ": the glyph follows the item's highlight")
        icon.destroy()
    }

    function checkBadge() {
        Operations.compress(JSON.stringify([base + "Sub/page.pdf"]), "lautta://user-documents/", "glyphs-check", "zip")
        if (!waitFor(function () { return window.archiveRoot.length > 0 }, "the archive"))
            return
        var root = window.archiveRoot.charAt(window.archiveRoot.length - 1) === "/" ? window.archiveRoot : window.archiveRoot + "/"
        window.thumb = "image://lautta-thumb/" + root + "page.pdf"
        var list = Theme.iconSizeMedium
        var grid = 168
        var make = function (props) { return iconComponent.createObject(window.contentItem, props) }
        var plainList = make({ "category": "video", "size": list })
        var plainGrid = make({ "category": "video", "size": grid, "glyphSize": FileIcons.snap(grid / 2) })
        checkOver(plainList, "list")
        checkOver(plainGrid, "grid")
        var image = make({ "category": "image", "thumbnailSource": window.thumb, "size": list })
        var failed = make({ "category": "video", "thumbnailSource": "image://lautta-thumb/" + root + "none.mp4",
                           "size": list })
        waitFor(function () { return image.thumbnailReady }, "the image's thumbnail")
        waitFor(function () { return failed.thumbnailFailed }, "the missing file's thumbnail to fail")
        expect(!named(image, "videoBadge").visible && !named(image, "fileGlyph").visible,
               "an image's thumbnail has no disc and no glyph")
        expect(!named(plainList, "videoBadge").visible && named(plainList, "fileGlyph").visible,
               "a video without thumbnail shows its glyph, no disc")
        same(String(named(plainList, "fileGlyph").color), String(Theme.primaryColor), "the plain glyph is in the primary colour")
        expect(named(plainList, "fileGlyph").opacity === 1, "the plain glyph is opaque")
        expect(!named(failed, "videoBadge").visible && named(failed, "fileGlyph").visible,
               "a video whose thumbnail failed shows its glyph, no disc")
        var made = [plainList, plainGrid, image, failed]
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
        var settings = { "view_mode": "list", "show_hidden": false, "thumbnails_local": false,
                         "thumbnails_remote": true }
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
