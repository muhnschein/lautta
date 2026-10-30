// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components/directory"

ApplicationWindow {
    id: window

    readonly property string base: "lautta://user-documents/TextThumbnails/"

    ViewerTools { id: tools }

    DirectoryModel {
        id: documents
        uri: "lautta://user-documents/"
    }

    function expect(ok, what) {
        if (!ok)
            console.error(what)
        return ok
    }

    function findAll(item, pred, out) {
        if (!item || out.indexOf(item) >= 0)
            return out
        if (pred(item))
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out)
        if (item.contentItem && item.contentItem !== item)
            findAll(item.contentItem, pred, out)
        return out
    }

    function named(item, name) {
        return findAll(item, function (x) { return x.objectName === name }, [])[0]
    }

    // No render loop headless: lay out by hand.
    function layOut(view) {
        var flicks = findAll(view, function (x) { return x.forceLayout !== undefined && x.cacheBuffer !== undefined }, [])
        for (var i = 0; i < flicks.length; ++i)
            flicks[i].forceLayout()
    }

    function iconsOf(view, category) {
        layOut(view)
        return findAll(view, function (x) { return x.previewShown !== undefined && x.category === category }, [])
    }

    function shownIcons(view) {
        layOut(view)
        return findAll(view, function (x) { return x.previewShown === true }, [])
    }

    function until(done) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        return done()
    }

    function checkTile(view, category, text, mono, type) {
        var icon = null
        until(function () {
            icon = iconsOf(view, category).filter(function (x) { return x.previewShown })[0]
            return !!icon
        })
        if (!expect(icon, category + ": a tile with text"))
            return
        var label = named(icon, "textPreview")
        if (!expect(label, category + ": its text label"))
            return
        expect(text(label.text), category + ": its first lines: " + JSON.stringify(label.text))
        expect((label.font.family === "monospace") === mono && (label.wrapMode === Text.NoWrap) === mono,
               category + ": font " + label.font.family + ", wrap " + label.wrapMode)
        expect(label.font.pixelSize === Theme.fontSizeTiny, category + ": at caption size")
        expect(!named(icon, "fileGlyph").visible, category + ": no glyph under the text")
        var typeIcon = named(icon, "typeIcon")
        expect(typeIcon && typeIcon.icon === type && typeIcon.status === Image.Ready,
               category + ": type icon " + (typeIcon ? typeIcon.icon + " " + typeIcon.status : "none"))
    }

    Component.onCompleted: {
        documents.createFolder("TextThumbnails")
        tools.pump(500)
        tools.installFixture(Qt.resolvedUrl("host/fixtures/notes-crlf.txt"), base + "notes.txt")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/script.sh"), base + "run.sh")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/doc.md"), base + "README.md")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/empty.txt"), base + "empty.txt")
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), base + "a.jpg")
        App.setSetting("view_mode", JSON.stringify("grid"))
        App.setSetting("thumbnails_local", JSON.stringify(false))
        pageStack.push(pageComponent, { "folderUri": base }, PageStackAction.Immediate)
        tools.pump(500)
        var view = pageStack.currentPage.view
        expect(view.gridMode, "a grid")
        expect(until(function () { return iconsOf(view, "code").length === 1 }) && shownIcons(view).length === 0,
               "thumbnails off: glyphs")
        App.setSetting("thumbnails_local", JSON.stringify(true))

        checkTile(view, "text", function (t) { return t === "line one\nline two" }, false, "types/text")
        checkTile(view, "code", function (t) { return t.indexOf("#!/bin/sh\n") === 0 }, true, "types/shell")
        checkTile(view, "markdown", function (t) { return t.length > 0 }, false, "types/markdown")
        expect(shownIcons(view).length === 3, "only those three: " + shownIcons(view).length)

        App.setSetting("view_mode", JSON.stringify("list"))
        expect(until(function () { return !view.gridMode }), "a list")
        expect(shownIcons(view).length === 0, "the list keeps glyphs")

        App.setSetting("view_mode", JSON.stringify("grid"))
        expect(until(function () { return view.gridMode && shownIcons(view).length === 3 }), "the grid again")
        App.setSetting("thumbnails_local", JSON.stringify(false))
        expect(until(function () { return shownIcons(view).length === 0 }), "without thumbnails no tile has text")

        App.setSetting("thumbnails_local", JSON.stringify(true))
        App.setSetting("view_mode", JSON.stringify("list"))
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
}
